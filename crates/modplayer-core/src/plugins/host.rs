// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginHost`: discovery, the request/event channels, and the
//! per-tick housekeeping every user story runs on top of (data-model.md
//! §3.2, contracts/plugin-host-service.md).

use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modplayer_capability_gateway::event::{HostEvent, UnloadReason};
use modplayer_capability_gateway::focus::FocusToken;
use modplayer_capability_gateway::grants::Grants;
use modplayer_capability_gateway::manifest::{self, PluginIdentifier};
use modplayer_capability_gateway::state::{PluginStatePaths, StateWriter};
use modplayer_engine::RtShared;
use modplayer_plugin_runtime::events::{PlaybackSnapshot, RuntimeEvent, SuspendCause};
use modplayer_plugin_runtime::handle::{
    Control, PluginHandle, PluginSnapshot, RpcEnvelope, RuntimeDeps, SpawnConfig,
};

use crate::effects::ChainModel;
use crate::i18n::tr;
use crate::markers::{Owner, TrackMarkers};
use crate::notifications::{
    KEY_PLUGIN_AUTO_DISABLED, KEY_PLUGIN_SUSPENDED, NotificationAction, NotificationCenter,
    Severity,
};

use super::fanout::FanOut;
use super::log::PluginLog;
use super::{
    BundledPackage, Lifecycle, PluginId, PluginIdTable, PluginRecord, Source, bundled,
    to_gateway_id,
};

/// L5 (FR-010): a `HandlerAborted` older than this is evicted from a
/// record's rolling window.
const ABORT_WINDOW: Duration = Duration::from_secs(60);
/// L5: the 3-in-60s `Health::Warning` window stays open this long once
/// opened.
const WARNING_DURATION: Duration = Duration::from_secs(5 * 60);

/// The `plugin-suspended`/`plugin-auto-disabled` `$plugin` argument: the
/// manifest's `name` when it parsed, else the raw identifier (mirrors
/// [`record_sort_key`]'s own fallback).
fn plugin_display_name(record: &PluginRecord) -> String {
    record
        .manifest
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|_| record.identifier.to_string())
}

/// L6's `$cause`: an already-localised sentence fragment, not a raw enum
/// name (`locales/en-US/plugins.ftl`'s `plugin-suspended-cause-*`).
fn cause_label(cause: SuspendCause) -> String {
    tr(match cause {
        SuspendCause::Hang => "plugin-suspended-cause-hang",
        SuspendCause::CpuShare => "plugin-suspended-cause-cpu-share",
        SuspendCause::Memory => "plugin-suspended-cause-memory",
        SuspendCause::DidNotStart => "plugin-suspended-cause-did-not-start",
    })
}

/// `RpcEnvelope`/`RuntimeEvent`'s own plugin-id type (research R10) — see
/// [`super::to_gateway_id`]/[`super::from_gateway_id`].
type GatewayPluginId = modplayer_capability_gateway::focus::PluginId;

/// Discovery + lifecycle housekeeping for every installed plugin
/// (data-model.md §3.2). Owns no real-time state; a running plugin's
/// `Lua` state lives entirely on its own `plugin:<identifier>` thread
/// (Constitution I).
pub struct PluginHost {
    records: Vec<PluginRecord>,
    ids: PluginIdTable,
    focus: FocusToken,
    paths: Option<PluginStatePaths>,
    writer: Option<StateWriter>,
    requests_tx: SyncSender<RpcEnvelope>,
    requests_rx: Receiver<RpcEnvelope>,
    events_tx: SyncSender<(GatewayPluginId, RuntimeEvent)>,
    events_rx: Receiver<(GatewayPluginId, RuntimeEvent)>,
    playback: Arc<PlaybackSnapshot>,
    snapshot: Arc<Mutex<PluginSnapshot>>,
    log: PluginLog,
    fan_out: FanOut,
    fixtures_enabled: bool,
    waker: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// `plugins/bundled/` + (if `fixtures_enabled`) `plugins/fixtures/`,
/// parsed into a `PluginRecord` per package (L1). Kept out of
/// `PluginHost::discover` itself only so it stays easy to unit-test
/// without constructing a whole host.
fn build_records(fixtures_enabled: bool, ids: &mut PluginIdTable) -> Vec<PluginRecord> {
    let mut packages: Vec<BundledPackage> = bundled::packages();
    if fixtures_enabled {
        packages.extend(bundled::fixtures());
    }

    let mut records: Vec<PluginRecord> = packages
        .into_iter()
        .filter_map(|package| {
            let Some(identifier) = PluginIdentifier::parse(package.identifier) else {
                // A malformed embedded identifier is a build-time bug, not
                // a runtime condition — there is no well-formed
                // `PluginRecord` to build without one, so this package is
                // skipped rather than listed (it cannot happen for a
                // package that ever passed review, since the identifier
                // is also validated as the manifest's own `identifier`
                // field before it ships).
                log::error!(target: "plugin", "bundled package '{}' has a malformed identifier", package.identifier);
                return None;
            };
            let manifest = manifest::parse_and_validate(package.manifest_toml, true);
            let id = ids.intern(identifier.clone());
            let grants = manifest.as_ref().map(Grants::from_bundled).unwrap_or_else(|_| Grants::none());
            let api_range = manifest
                .as_ref()
                .map(|m| m.api)
                .unwrap_or(modplayer_capability_gateway::manifest::ApiRange { major: 1, min_minor: 0 });
            let lifecycle = match &manifest {
                Ok(_) => Lifecycle::Disabled,
                Err(err) => Lifecycle::Invalid(err.clone()),
            };
            let enabled = manifest.is_ok();
            Some(PluginRecord {
                id,
                identifier,
                package,
                manifest,
                source: Source::Bundled,
                enabled,
                lifecycle,
                health: None,
                grants,
                suspensions_this_session: 0,
                abort_window: std::collections::VecDeque::new(),
                warning_until: None,
                handle: None,
                gauges: None,
                api_range,
                budgets: modplayer_capability_gateway::budgets::Budgets::DEFAULT,
                compatibility_mode: false,
            })
        })
        .collect();

    records.sort_by_key(record_sort_key);
    records
}

/// FR-023's sort key: the manifest's `name`, case-folded; an `Invalid`
/// record (no parsed name) falls back to its raw identifier.
fn record_sort_key(record: &PluginRecord) -> String {
    record
        .manifest
        .as_ref()
        .map(|m| m.name.as_str())
        .unwrap_or_else(|_| record.identifier.as_str())
        .to_lowercase()
}

impl PluginHost {
    /// L1: discover every bundled (and, if `fixtures_enabled`, fixture)
    /// package, validating each manifest; an invalid package still gets a
    /// row (`Lifecycle::Invalid`, `enabled = false`, `health = None`).
    #[must_use]
    pub fn discover(fixtures_enabled: bool) -> Self {
        let mut ids = PluginIdTable::new();
        let records = build_records(fixtures_enabled, &mut ids);
        let (requests_tx, requests_rx) = std::sync::mpsc::sync_channel(256);
        let (events_tx, events_rx) = std::sync::mpsc::sync_channel(1024);
        Self {
            records,
            ids,
            focus: FocusToken::new(),
            paths: PluginStatePaths::resolve(),
            writer: Some(StateWriter::spawn()),
            requests_tx,
            requests_rx,
            events_tx,
            events_rx,
            playback: Arc::new(PlaybackSnapshot::new()),
            snapshot: Arc::new(Mutex::new(PluginSnapshot::default())),
            log: PluginLog::new(),
            fan_out: FanOut::new(),
            fixtures_enabled,
            waker: None,
        }
    }

    #[must_use]
    pub fn records(&self) -> &[PluginRecord] {
        &self.records
    }

    pub fn records_mut(&mut self) -> &mut [PluginRecord] {
        &mut self.records
    }

    #[must_use]
    pub fn record(&self, id: PluginId) -> Option<&PluginRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    pub fn record_mut(&mut self, id: PluginId) -> Option<&mut PluginRecord> {
        self.records.iter_mut().find(|r| r.id == id)
    }

    #[must_use]
    pub fn id_table(&self) -> &PluginIdTable {
        &self.ids
    }

    /// Mutable access for a caller that needs to intern a fresh
    /// identifier (markers::store's owner-string round trip, L2).
    pub fn id_table_mut(&mut self) -> &mut PluginIdTable {
        &mut self.ids
    }

    #[must_use]
    pub fn focus(&self) -> &FocusToken {
        &self.focus
    }

    #[must_use]
    pub fn paths(&self) -> Option<&PluginStatePaths> {
        self.paths.as_ref()
    }

    #[must_use]
    pub fn fixtures_enabled(&self) -> bool {
        self.fixtures_enabled
    }

    #[must_use]
    pub fn plugin_log(&self) -> &PluginLog {
        &self.log
    }

    #[must_use]
    pub fn playback_snapshot(&self) -> &Arc<PlaybackSnapshot> {
        &self.playback
    }

    #[must_use]
    pub fn snapshot(&self) -> &Arc<Mutex<PluginSnapshot>> {
        &self.snapshot
    }

    pub fn fan_out_mut(&mut self) -> &mut FanOut {
        &mut self.fan_out
    }

    /// The UI's `ctx.request_repaint` (or equivalent): called after every
    /// admitted request is queued, so a UI blocked on `recv` wakes up
    /// promptly (contracts/plugin-host-service.md §1).
    pub fn set_waker(&mut self, waker: Arc<dyn Fn() + Send + Sync>) {
        self.waker = Some(waker);
    }

    fn wake(&self) {
        if let Some(waker) = &self.waker {
            waker();
        }
    }

    /// One queued `RpcEnvelope`, if any (non-blocking) — `drain_plugin_
    /// requests`'s (C1) own loop calls this until it returns `None`.
    pub fn try_recv_request(&self) -> Option<RpcEnvelope> {
        self.requests_rx.try_recv().ok()
    }

    /// Test-support hook (mirrors `PlaybackController::debug_inject_
    /// engine_event`): a clone of the request channel's own sender, for a
    /// test that wants to submit a synthetic `RpcEnvelope` straight to
    /// `drain_plugin_requests()` (C1) without a real running plugin
    /// thread — US2 T089's ownership/focus tests exercise `plugins::
    /// apply`'s own logic this way, deliberately bypassing the gateway's
    /// permission/focus/rate admission a real plugin thread would have
    /// already run (that path is `modplayer-capability-gateway`'s own
    /// `tests/gateway.rs`'s job, not this crate's). Never reached by any
    /// production code path — `PluginHandle::spawn` is the only real
    /// sender.
    #[must_use]
    pub fn debug_requests_sender(&self) -> SyncSender<RpcEnvelope> {
        self.requests_tx.clone()
    }

    /// One queued `(plugin, RuntimeEvent)`, if any (non-blocking).
    pub fn try_recv_event(&self) -> Option<(PluginId, RuntimeEvent)> {
        self.events_rx
            .try_recv()
            .ok()
            .map(|(id, event)| (super::from_gateway_id(id), event))
    }

    /// L3: spawn every `enabled`, `Disabled` record's `PluginHandle`
    /// (`launch()`'s job, and `plugin_enable`'s once it exists — a later
    /// task). `shared` is the audio engine's `RtShared` — this crate
    /// never touches it beyond handing plugins a clone (Constitution I).
    pub fn load_all_enabled(&mut self, shared: &Arc<RtShared>) {
        let ids: Vec<PluginId> = self
            .records
            .iter()
            .filter(|r| r.enabled && matches!(r.lifecycle, Lifecycle::Disabled))
            .map(|r| r.id)
            .collect();
        for id in ids {
            self.spawn(id, shared);
        }
    }

    /// Spawn one record's `PluginHandle` and move it to `Loading`. A
    /// no-op if the record is missing, already running, or its manifest
    /// never validated.
    pub fn spawn(&mut self, id: PluginId, shared: &Arc<RtShared>) {
        let requests = self.requests_tx.clone();
        let events = self.events_tx.clone();
        let playback = Arc::clone(&self.playback);
        let snapshot = Arc::clone(&self.snapshot);
        let writer = self.writer.as_ref().map(StateWriter::sender);
        let paths = self.paths.clone();
        let focus = self.focus.clone();
        let fixtures_enabled = self.fixtures_enabled;
        let shared = Arc::clone(shared);

        let Some(record) = self.record_mut(id) else {
            return;
        };
        if record.handle.is_some() {
            return;
        }
        let Ok(manifest) = &record.manifest else {
            return;
        };

        let config = SpawnConfig {
            id: to_gateway_id(record.id),
            identifier: record.identifier.to_string(),
            entry_source: record.package.entry.to_string(),
            api_range: manifest.api,
            grants: record.grants,
            budgets: record.budgets,
            focus,
            fixtures_enabled,
        };
        let deps = RuntimeDeps {
            shared,
            playback,
            requests,
            events,
            snapshot,
            writer,
            paths,
        };
        let handle = PluginHandle::spawn(config, deps);
        record.gauges = Some(Arc::clone(&handle.gauges));
        record.handle = Some(handle);
        record.lifecycle = Lifecycle::Loading;
    }

    /// Drain every queued `RuntimeEvent` (L4 Ready, L5 abort accounting,
    /// L6 suspension handling): keeps `lifecycle`/`health` current, feeds
    /// the console ring, and raises/dismisses the suspension notices.
    /// `notifications`/`chain`/`markers` are the caller's own fields
    /// (`PluginHost` owns no real-time or notification state itself) —
    /// `tick()` passes them through every call.
    pub fn drain_plugin_runtime_events(
        &mut self,
        notifications: &mut NotificationCenter,
        chain: &mut ChainModel,
        mut markers: Option<&mut TrackMarkers>,
        now: Instant,
    ) {
        let mut to_teardown: Vec<PluginId> = Vec::new();
        while let Some((id, event)) = self.try_recv_event() {
            let identifier = self.id_table().identifier_of(id).cloned();
            let Some(record) = self.record_mut(id) else {
                continue;
            };
            match event {
                RuntimeEvent::Ready => {
                    record.lifecycle = Lifecycle::Active;
                    record.health = Some(super::Health::Ok);
                    record.abort_window.clear();
                    record.warning_until = None;
                    if let Some(identifier) = &identifier {
                        notifications.dismiss_by_dedupe(&format!("plugin-suspended:{identifier}"));
                    }
                    // L4: re-adopt any node this plugin owned before a
                    // prior suspension/restart orphaned it (chain.md
                    // §41; ids unchanged, bumps `chain.revision` so
                    // `fan_out_revision_events()` republishes it).
                    chain.readopt(id);
                }
                RuntimeEvent::Suspended { cause } => {
                    record.lifecycle = Lifecycle::Suspended { cause };
                    record.health = Some(super::Health::Suspended);
                    record.suspensions_this_session =
                        record.suspensions_this_session.saturating_add(1);
                    let name = plugin_display_name(record);
                    let auto_disabled = record.suspensions_this_session >= 3;
                    if let Some(identifier) = &identifier {
                        if auto_disabled {
                            notifications.raise_keyed(
                                Severity::Warning,
                                KEY_PLUGIN_AUTO_DISABLED,
                                vec![("plugin", name)],
                                Vec::new(),
                                format!("plugin-auto-disabled:{identifier}"),
                            );
                        } else {
                            notifications.raise_keyed(
                                Severity::Warning,
                                KEY_PLUGIN_SUSPENDED,
                                vec![("plugin", name), ("cause", cause_label(cause))],
                                vec![
                                    NotificationAction::RestartPlugin(id),
                                    NotificationAction::DisablePlugin(id),
                                ],
                                format!("plugin-suspended:{identifier}"),
                            );
                        }
                    }
                    if auto_disabled {
                        record.enabled = false;
                    }
                    // L7's core-side teardown (focus/markers/chain) needs
                    // `&mut self` — deferred until `record`'s borrow ends.
                    to_teardown.push(id);
                }
                RuntimeEvent::Exited => {
                    // `reap_plugin_threads` does the actual join + the
                    // `Draining -> Disabled` transition (L8); nothing to
                    // do here beyond having drained the event.
                }
                RuntimeEvent::HandlerAborted { cause } => {
                    record.abort_window.push_back(now);
                    while record
                        .abort_window
                        .front()
                        .is_some_and(|t| now.saturating_duration_since(*t) > ABORT_WINDOW)
                    {
                        record.abort_window.pop_front();
                    }
                    if record.abort_window.len() >= 3 {
                        record.warning_until = Some(now + WARNING_DURATION);
                    }
                    if let Some(identifier) = &identifier {
                        log::error!(target: "plugin", "[{identifier}] handler aborted: {cause:?}");
                    }
                }
                RuntimeEvent::Log { level, message } => {
                    if let Some(identifier) = identifier {
                        self.log.push(identifier, level, message);
                    }
                }
            }
        }
        for id in to_teardown {
            // L7's teardown already ran on the plugin's own thread as
            // part of its own suspension (RT8's `run_unloading`) before
            // `Suspended` was ever sent, so `stop`'s `Control::Unloading`
            // send here is a harmless no-op (the thread is exiting or
            // gone) — only the host-side focus/markers/chain cleanup
            // matters for a suspension.
            self.stop(id, UnloadReason::Suspend, markers.as_deref_mut(), chain);
        }
        self.wake();
    }

    /// L7/L8 (FR-012): release this plugin's transport focus, disarm and
    /// clear any loop region/transient markers it owns, orphan its effect
    /// nodes, tell its thread to unload (a no-op if it has already
    /// exited, e.g. a suspension's own teardown), and move it to
    /// `Draining` if it was still running. `reap_plugin_threads()` (L8)
    /// finishes the job once the thread actually exits. `markers` is
    /// `None` while no track is loaded (nothing to own there yet).
    pub fn stop(
        &mut self,
        id: PluginId,
        reason: UnloadReason,
        markers: Option<&mut TrackMarkers>,
        chain: &mut ChainModel,
    ) {
        self.focus.release_if(to_gateway_id(id));
        if let Some(markers) = markers {
            // `loop_disarmed`/`marker_changed` fan-out for this teardown
            // lands with US2 (T087, `fan_out_revision_events`); the model
            // mutation and revision bump happen now regardless.
            markers.disarm_if_owned_by(Owner::Plugin(id));
            markers.remove_transient_owned_by(Owner::Plugin(id));
        }
        chain.orphan_owned_by(id);
        if let Some(record) = self.record_mut(id) {
            if let Some(handle) = &record.handle {
                handle.send_control(Control::Unloading { reason });
            }
            if matches!(record.lifecycle, Lifecycle::Loading | Lifecycle::Active) {
                record.lifecycle = Lifecycle::Draining;
            }
        }
    }

    /// C4: deliver `event` to every `Active` plugin holding the
    /// permission it requires (`FanOut::send`) — a thin wrapper so
    /// callers never need `self.records`/`self.fan_out` borrowed by hand
    /// at once.
    pub fn fan_out(&mut self, event: &HostEvent, now: Instant) {
        self.fan_out.send(&self.records, event, now);
    }

    /// FR-014 (`clear_for_sign_out()`): every running plugin drops its
    /// in-memory track scope — reusing the same `TrackChanged{None}`
    /// path `handle_event`'s own `restore_track_state` already
    /// flushes-then-clears through (RT11) — sent directly to each
    /// handle, bypassing the general fan-out's permission filter, since
    /// this must reach every running plugin regardless of whether it
    /// holds `playback.observe`. Every plugin's on-disk `tracks/`
    /// directory is deleted the same way, running or not; `plugin.json`
    /// (`state.plugin`) is never touched.
    pub fn clear_track_state_for_sign_out(&mut self) {
        for record in &self.records {
            if let Some(handle) = &record.handle {
                handle.send_event(HostEvent::TrackChanged { track: None });
            }
            if let Some(paths) = &self.paths {
                paths.clear_tracks(record.identifier.as_str());
            }
        }
    }

    /// `shutdown()`'s plugin half (contracts/plugin-host-service.md §1):
    /// every `Loading`/`Active` plugin gets `stop(id, Shutdown, ..)`;
    /// the caller then waits (by polling `reap_plugin_threads()`) up to
    /// `deadline` for every thread to actually exit.
    pub fn stop_all_for_shutdown(
        &mut self,
        mut markers: Option<&mut TrackMarkers>,
        chain: &mut ChainModel,
    ) {
        let ids: Vec<PluginId> = self
            .records
            .iter()
            .filter(|r| matches!(r.lifecycle, Lifecycle::Loading | Lifecycle::Active))
            .map(|r| r.id)
            .collect();
        for id in ids {
            self.stop(id, UnloadReason::Shutdown, markers.as_deref_mut(), chain);
        }
    }

    /// Whether any plugin's thread is still joined-to (used by
    /// `shutdown()`'s bounded wait loop).
    #[must_use]
    pub fn any_thread_running(&self) -> bool {
        self.records.iter().any(|r| r.handle.is_some())
    }

    /// L8: join every finished handle; a `Draining` record whose thread
    /// has exited becomes `Disabled`.
    pub fn reap_plugin_threads(&mut self) {
        for record in &mut self.records {
            let finished = record
                .handle
                .as_ref()
                .is_some_and(PluginHandle::is_finished);
            if !finished {
                continue;
            }
            if let Some(handle) = record.handle.take() {
                handle.join();
            }
            record.gauges = None;
            if matches!(record.lifecycle, Lifecycle::Draining) {
                record.lifecycle = Lifecycle::Disabled;
            }
        }
    }
}

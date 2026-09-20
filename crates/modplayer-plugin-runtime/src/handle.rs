// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginHandle`: what core sees of a running plugin — an inbox, a
//! gauges handle, and a `JoinHandle` (RT1). The `Lua` state itself never
//! leaves the plugin's own thread.

use std::sync::mpsc::{Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::event::{HostEvent, UnloadReason};
use modplayer_capability_gateway::focus::{FocusToken, PluginId};
use modplayer_capability_gateway::grants::Grants;
use modplayer_capability_gateway::manifest::ApiRange;
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{
    MarkerInfo, NodeInfo, QueueItemInfo, Request, Response,
};
use modplayer_capability_gateway::state::PluginStatePaths;
use modplayer_capability_gateway::state::writer::WriteJob;
use modplayer_engine::RtShared;

use crate::budget::PluginGauges;
use crate::events::{PlaybackSnapshot, RuntimeEvent};
use crate::scheduler;

/// One admitted request, in flight from a plugin thread to core (RT7,
/// research R3). The plugin thread blocks on `reply` for up to the
/// gateway's RPC timeout.
pub struct RpcEnvelope {
    pub plugin: PluginId,
    pub request: Request,
    pub reply: SyncSender<Result<Response, Refusal>>,
}

/// Why a plugin's thread is being told to unload, plus the two other
/// control messages core can send it directly (bypassing the gateway,
/// since these are host-initiated, not plugin requests).
#[derive(Debug, Clone)]
pub enum Control {
    Unloading {
        reason: UnloadReason,
    },
    Stop,
    /// Fixture-only (L10): forwards `debug_probe(name)`'s argument and
    /// expects one JSON reply.
    Probe {
        name: String,
        reply: SyncSender<serde_json::Value>,
    },
}

/// One item in a plugin's inbox (research R5).
#[derive(Debug, Clone)]
pub enum Inbound {
    Event(HostEvent),
    Control(Control),
}

/// The controller's last-published read model for a plugin's local,
/// no-RPC reads (`list_markers`/`list_chain`/`queue.list`, research R3).
/// Shaped with the gateway crate's own DTOs so this crate never depends
/// on `modplayer-core` (the dependency runs the other way).
#[derive(Debug, Clone, Default)]
pub struct PluginSnapshot {
    pub markers: Vec<MarkerInfo>,
    pub armed_region: Option<modplayer_capability_gateway::request::RegionId>,
    pub chain: Vec<NodeInfo>,
    pub queue: Vec<QueueItemInfo>,
    pub marker_revision: u64,
    pub chain_revision: u64,
}

/// Everything a plugin's scheduler thread needs from the host, beyond its
/// own gateway/grants (data-model.md §2).
pub struct RuntimeDeps {
    pub shared: Arc<RtShared>,
    pub playback: Arc<PlaybackSnapshot>,
    pub requests: SyncSender<RpcEnvelope>,
    pub events: SyncSender<(PluginId, RuntimeEvent)>,
    pub snapshot: Arc<Mutex<PluginSnapshot>>,
    /// `None` when the plugin-state directory isn't determinable — the
    /// plugin's store still works, just never persists (mirrors
    /// `AnalysisPaths`/`TrackStatePaths` degrading the same way).
    pub writer: Option<Sender<WriteJob>>,
    pub paths: Option<PluginStatePaths>,
}

/// What core holds for a running plugin (RT1): an inbox, live gauges, and
/// a `JoinHandle` — never the `Lua` state itself.
pub struct PluginHandle {
    pub id: PluginId,
    inbox: SyncSender<Inbound>,
    pub gauges: Arc<PluginGauges>,
    thread: Option<JoinHandle<()>>,
    pub started_at: Instant,
}

/// Everything about *this* plugin package needed to spawn it (mirrors a
/// validated `Manifest` plus its embedded source — core owns the
/// `BundledPackage`/`Manifest` types and passes their pieces through).
pub struct SpawnConfig {
    pub id: PluginId,
    pub identifier: String,
    pub entry_source: String,
    pub api_range: ApiRange,
    pub grants: Grants,
    pub budgets: Budgets,
    pub focus: FocusToken,
    pub fixtures_enabled: bool,
}

impl PluginHandle {
    /// RT1: spawn `plugin:<identifier>`, owning a fresh `Lua` state for
    /// the life of the thread.
    ///
    /// ```
    /// use std::sync::mpsc::sync_channel;
    /// use std::sync::{Arc, Mutex};
    ///
    /// use modplayer_capability_gateway::budgets::Budgets;
    /// use modplayer_capability_gateway::focus::{FocusToken, PluginId};
    /// use modplayer_capability_gateway::grants::Grants;
    /// use modplayer_capability_gateway::manifest::ApiRange;
    /// use modplayer_engine::RtShared;
    /// use modplayer_plugin_runtime::events::PlaybackSnapshot;
    /// use modplayer_plugin_runtime::handle::{
    ///     Control, PluginHandle, PluginSnapshot, RuntimeDeps, SpawnConfig,
    /// };
    ///
    /// let (requests_tx, _requests_rx) = sync_channel(16);
    /// let (events_tx, _events_rx) = sync_channel(16);
    /// let config = SpawnConfig {
    ///     id: PluginId(1),
    ///     identifier: "org.modplayer.example".to_string(),
    ///     entry_source: "api.ready()".to_string(),
    ///     api_range: ApiRange { major: 1, min_minor: 0 },
    ///     grants: Grants::none(),
    ///     budgets: Budgets::DEFAULT,
    ///     focus: FocusToken::new(),
    ///     fixtures_enabled: false,
    /// };
    /// let deps = RuntimeDeps {
    ///     shared: Arc::new(RtShared::new()),
    ///     playback: Arc::new(PlaybackSnapshot::new()),
    ///     requests: requests_tx,
    ///     events: events_tx,
    ///     snapshot: Arc::new(Mutex::new(PluginSnapshot::default())),
    ///     writer: None,
    ///     paths: None,
    /// };
    /// let handle = PluginHandle::spawn(config, deps);
    /// handle.send_control(Control::Stop);
    /// handle.join();
    /// ```
    #[must_use]
    pub fn spawn(config: SpawnConfig, deps: RuntimeDeps) -> PluginHandle {
        let id = config.id;
        let identifier = config.identifier.clone();
        let gauges = Arc::new(PluginGauges::default());
        let (inbox_tx, inbox_rx) = std::sync::mpsc::sync_channel(config.budgets.inbox);
        let thread_gauges = Arc::clone(&gauges);
        let thread = std::thread::Builder::new()
            .name(format!("plugin:{identifier}"))
            .spawn(move || {
                scheduler::run(config, deps, inbox_rx, thread_gauges);
            })
            .ok();
        PluginHandle {
            id,
            inbox: inbox_tx,
            gauges,
            thread,
            started_at: Instant::now(),
        }
    }

    /// `try_send` (research R5): a full inbox drops the event (only
    /// possible while a handler is stuck, and the plugin is about to be
    /// suspended anyway).
    pub fn send_event(&self, event: HostEvent) -> bool {
        self.inbox.try_send(Inbound::Event(event)).is_ok()
    }

    pub fn send_control(&self, control: Control) -> bool {
        self.inbox.try_send(Inbound::Control(control)).is_ok()
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Join the thread (L8/shutdown). Consumes `self`.
    pub fn join(mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

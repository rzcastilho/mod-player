// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `PlaybackController` end-to-end tests for the bundled Section Loop
//! plugin (012-section-loop-plugin, User Story 1, contracts/
//! section-loop-plugin.md), driving the real `org.modplayer.section-loop`
//! package (always bundled, `bundled::packages()`, research R5) through
//! `plugin_panel_interaction`/`invoke_plugin_action` — mirroring
//! `controller_plugin_ui.rs`'s own harness (research R10). A few tests
//! seed host state directly via the same gateway-bypassing `call()`
//! `controller_markers.rs` already uses for `SetLoopEndpoint`/
//! `SetLoopRepeat` (attributed to the *real* Section Loop `PluginId`, so
//! its own running thread still receives the resulting `marker_changed`
//! and relists exactly as it would for a script-driven edit) wherever a
//! test's own subject is the event/relist path rather than the button
//! click itself (T024's own job).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{
    LoopEndpoint, RegionId as GatewayRegionId, RepeatArg, Request, Response,
};
use modplayer_capability_gateway::ui::{OverlayPrimitive, UiId, WidgetValue};
use modplayer_core::PlaybackController;
use modplayer_core::actions::{ActionSource, Chord, PluginActionId};
use modplayer_core::markers::{CueSlot, LoopRegion, MarkerKind, Owner, RegionId, RepeatCount};
use modplayer_core::plugins::{FocusHolder, FocusPolicy, Lifecycle, PluginId, to_gateway_id};
use modplayer_core::settings::SettingsStore;
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_plugin_runtime::handle::RpcEnvelope;

const SECTION_LOOP: &str = "org.modplayer.section-loop";
const FOCUS_B: &str = "org.modplayer.fixture.focus-b";
const TRACK_RATE: u64 = 44_100;

fn ms_frames(ms: u64) -> u64 {
    ms * TRACK_RATE / 1000
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-section-loop-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fresh_store() -> (SettingsStore, TempDir) {
    let dir = TempDir::new();
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    (store, dir)
}

fn fake_device(id: &str, is_default: bool) -> FakeDevice {
    FakeDevice {
        id: DeviceId::new(id).unwrap_or_else(|| unreachable!()),
        name: id.to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2_048))),
        is_default,
    }
}

fn track(id: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!()),
        id,
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

/// `MODPLAYER_PLUGIN_STATE_DIR`/`MODPLAYER_TRACK_STATE_DIR` are
/// process-global, so every `PlaybackController::new` in this binary must
/// be serialized against every other's brief mutation of them (mirrors
/// `controller_plugin_ui.rs`'s own `PLUGIN_ENV_LOCK`). No
/// `MODPLAYER_PLUGIN_FIXTURES`: the bundled Section Loop package always
/// discovers regardless (research R5) — 013-key-and-tempo-plugin
/// (research R8) means Key & Tempo now discovers alongside it too, but
/// this file's own tests only ever look up Section Loop's id and never
/// assert an exact plugin count, so they stay unaffected by the second
/// bundled package (it declares no `markers.*`/`transport.control`
/// permission and touches none of this file's own subject matter).
static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

fn section_loop_controller() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let plugin_state_dir = TempDir::new();
    let track_state_dir = TempDir::new();
    let host = ScriptedHost::new();
    let devices = vec![fake_device("dev-1", true)];
    let controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: narrowly scopes each mutation to the one synchronous
        // read `PlaybackController::new` makes of it, serialized against
        // every other test in this binary via the lock above.
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    (controller, dir, plugin_state_dir, track_state_dir)
}

fn section_loop_id(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) -> PluginId {
    plugin_id_by(controller, SECTION_LOOP)
}

fn plugin_id_by(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    identifier: &str,
) -> PluginId {
    controller
        .plugins_mut()
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == identifier)
        .map(|r| r.id)
        .unwrap_or_else(|| unreachable!("{identifier} must be discovered"))
}

/// T045 (US3-1): Section Loop alongside the `focus-b` fixture, which
/// contends for the same `transport.control` permission — `MODPLAYER_
/// PLUGIN_FIXTURES=1` so `focus-b` discovers too (research R5: Section
/// Loop is bundled and discovers regardless).
fn section_loop_controller_with_fixtures() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let plugin_state_dir = TempDir::new();
    let track_state_dir = TempDir::new();
    let host = ScriptedHost::new();
    let devices = vec![fake_device("dev-1", true)];
    let controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: see `section_loop_controller`'s own note above.
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_FIXTURES", "1");
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_FIXTURES");
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    (controller, dir, plugin_state_dir, track_state_dir)
}

/// As [`ready_section_loop`], but with `focus-b` also discovered and
/// active — returns Section Loop's own id and `focus-b`'s.
fn ready_section_loop_with_fixtures() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
    PluginId,
    PluginId,
) {
    let (mut controller, dir, psd, tsd) = section_loop_controller_with_fixtures();
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    let id = section_loop_id(&mut controller);
    let focus_b = plugin_id_by(&mut controller, FOCUS_B);
    assert!(wait_active(&mut controller, id), "Section Loop must launch");
    assert!(wait_active(&mut controller, focus_b), "focus-b must launch");
    assert!(
        wait_panel_registered(&mut controller, id),
        "Section Loop's ready_ack handler must register its panel"
    );
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    (controller, dir, psd, tsd, id, focus_b)
}

/// Polls the engine's own position (a render pull, not the nominal seek
/// target — see `seek_ms`'s own doc comment) until it has reached at
/// least `at_least_ms`, driving both the controller and a tiny render
/// each iteration (mirrors `controller_transport_focus.rs`'s own
/// `wait_position_at_least`).
fn wait_position_ms_at_least(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    at_least_ms: u64,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        controller.tick();
        let _ = controller.backend_mut().render_buffers(1);
        if controller.position() >= Duration::from_millis(at_least_ms) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn pump_until(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    timeout: Duration,
    mut done: impl FnMut(&mut PlaybackController<FakeBackend, ScriptedHost>) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        controller.tick();
        if done(controller) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Everything worth knowing when a wait on Section Loop times out: the
/// record's lifecycle/health/abort count, its CPU-share gauge, the
/// panels/overlays it has registered, and its console (which the runtime
/// now feeds with every handler abort, suspension and RPC timeout, with
/// cost figures). Evaluated only inside a failing `assert!`'s message.
fn diagnose(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> String {
    let record = controller.plugins_mut().record(id).map(|r| {
        (
            r.lifecycle.clone(),
            r.health,
            r.abort_window.len(),
            r.gauges
                .as_ref()
                .map(|g| g.cpu_permille_of_share())
                .unwrap_or(0),
        )
    });
    let panels: Vec<String> = controller
        .plugins_mut()
        .ui()
        .panels()
        .for_plugin(id)
        .iter()
        .map(|p| p.id.to_string())
        .collect();
    let overlays = overlay_ids(controller, id);
    let console: Vec<String> = controller
        .plugin_log()
        .entries()
        .map(|e| format!("[{}] {}: {}", e.level, e.plugin, e.message))
        .collect();
    format!(
        "record(lifecycle, health, aborts, cpu‰)={record:?} panels={panels:?} overlays={overlays:?} console={console:#?}"
    )
}

fn wait_active(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> bool {
    pump_until(controller, Duration::from_secs(5), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )
    })
}

fn wait_panel_registered(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> bool {
    pump_until(controller, Duration::from_secs(5), |c| {
        c.plugins_mut()
            .ui()
            .panels()
            .for_plugin(id)
            .iter()
            .any(|p| p.id.as_str() == "main")
    })
}

/// A controller with Section Loop `Active`, its panel registered, and a
/// track loaded (`transport_enabled()`, so `SetLoopEndpoint{region: None,
/// ..}`'s own `require_track` passes) — the common starting point for
/// every test below.
fn ready_section_loop() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
    PluginId,
) {
    let (mut controller, dir, psd, tsd) = section_loop_controller();
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    let id = section_loop_id(&mut controller);
    assert!(
        wait_active(&mut controller, id),
        "Section Loop must launch: {}",
        diagnose(&mut controller, id)
    );
    assert!(
        wait_panel_registered(&mut controller, id),
        "Section Loop's ready_ack handler must register its panel: {}",
        diagnose(&mut controller, id)
    );
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    (controller, dir, psd, tsd, id)
}

/// US2 (T038/T041): simulates "close and relaunch the app" — a fresh
/// `PlaybackController` spawned against the *same* plugin-state/track-
/// state directories (and settings file) an earlier controller already
/// `shutdown()` through, so anything that earlier controller flushed to
/// disk is what this one loads. Active with its panel registered by the
/// time this returns; the caller still queues/plays a track (US2-1 needs
/// that to happen *after* relaunch, mirroring a real reopen).
fn reopen_section_loop(
    dir: &TempDir,
    plugin_state_dir: &TempDir,
    track_state_dir: &TempDir,
) -> (PlaybackController<FakeBackend, ScriptedHost>, PluginId) {
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    let host = ScriptedHost::new();
    let devices = vec![fake_device("dev-1", true)];
    let mut controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: see `section_loop_controller`'s own note above.
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    let id = section_loop_id(&mut controller);
    assert!(
        wait_active(&mut controller, id),
        "Section Loop must launch again after relaunch: {}",
        diagnose(&mut controller, id)
    );
    assert!(
        wait_panel_registered(&mut controller, id),
        "Section Loop must re-register its panel from a fresh ready_ack (G4/L2): {}",
        diagnose(&mut controller, id)
    );
    (controller, id)
}

fn wid(s: &str) -> UiId {
    UiId::parse(s).unwrap_or_else(|| unreachable!("{s:?} must be valid grammar"))
}

fn click(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
    widget: &str,
    value: WidgetValue,
) {
    controller.plugin_panel_interaction(id, &wid("main"), &wid(widget), value);
}

fn widget_value(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
    widget: &str,
) -> Option<WidgetValue> {
    controller
        .plugins_mut()
        .ui()
        .panels()
        .for_plugin(id)
        .iter()
        .find(|p| p.id.as_str() == "main")
        .and_then(|p| p.widgets.iter().find(|w| w.spec.id.as_str() == widget))
        .map(|w| w.value.clone())
}

fn overlay_ids(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> Vec<String> {
    controller
        .plugin_overlays()
        .into_iter()
        .find(|l| l.plugin == id)
        .map(|l| l.primitives.iter().map(primitive_id).collect())
        .unwrap_or_default()
}

fn primitive_id(p: &OverlayPrimitive) -> String {
    match p {
        OverlayPrimitive::Line { id, .. }
        | OverlayPrimitive::Region { id, .. }
        | OverlayPrimitive::Label { id, .. }
        | OverlayPrimitive::Glyph { id, .. } => id.as_str().to_string(),
    }
}

/// Submits `request` from `plugin` straight to `drain_plugin_requests()`
/// (mirrors `controller_markers.rs`'s own `call()`): applied through
/// `plugins::apply::dispatch` exactly as if `plugin`'s own real Lua
/// thread had made the call — its own `marker_changed` fan-out still
/// reaches that thread on the next `tick()`.
fn call(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    plugin: PluginId,
    request: Request,
) -> Result<Response, Refusal> {
    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    let envelope = RpcEnvelope {
        plugin: to_gateway_id(plugin),
        request,
        reply: reply_tx,
    };
    controller
        .plugins_mut()
        .debug_requests_sender()
        .send(envelope)
        .unwrap_or_else(|_| unreachable!("the request channel must accept a synthetic envelope"));
    controller.tick();
    reply_rx
        .try_recv()
        .unwrap_or_else(|_| unreachable!("drain_plugin_requests must always reply (C1)"))
}

/// Seeds a Section-Loop-owned endpoint directly (bypassing the real Lua
/// thread, see the module doc) at `ms`, returning the region's gateway id
/// (for a following `SetLoopEndpoint`/`SetLoopRepeat` call) — `region`
/// `None` creates a new one.
fn seed_endpoint(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    plugin: PluginId,
    region: Option<GatewayRegionId>,
    which: LoopEndpoint,
    ms: u64,
) -> GatewayRegionId {
    match call(
        controller,
        plugin,
        Request::SetLoopEndpoint {
            region,
            which,
            position_ms: ms,
        },
    ) {
        Ok(Response::LoopEndpoint { region, .. }) => region,
        other => unreachable!("seed_endpoint: expected Response::LoopEndpoint, got {other:?}"),
    }
}

/// This plugin's own region, found by ownership (never by numeric id —
/// `RegionId::from_raw`/`raw` are `pub(crate)`, unreachable from here).
fn own_region(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    plugin: PluginId,
) -> Option<RegionId> {
    let markers = controller.markers()?;
    markers
        .regions()
        .iter()
        .find(|r| {
            let owner =
                r.a.and_then(|a| markers.owner_of(a))
                    .or_else(|| r.b.and_then(|b| markers.owner_of(b)));
            owner == Some(Owner::Plugin(plugin))
        })
        .map(|r| r.id)
}

fn action_id(short: &str) -> PluginActionId {
    PluginActionId::parse(&format!("{SECTION_LOOP}.{short}")).unwrap_or_else(|| unreachable!())
}

// -- T023: registration (contract G1/G2/G3) --------------------------------

const ACTION_SHORT_IDS: &[&str] = &[
    "set_a",
    "set_b",
    "toggle_loop",
    "nudge_earlier",
    "nudge_later",
    "set_cue_1",
    "set_cue_2",
    "set_cue_3",
    "set_cue_4",
    "set_cue_5",
    "set_cue_6",
    "set_cue_7",
    "set_cue_8",
    "jump_cue_1",
    "jump_cue_2",
    "jump_cue_3",
    "jump_cue_4",
    "jump_cue_5",
    "jump_cue_6",
    "jump_cue_7",
    "jump_cue_8",
    "toggle_snap",
    "clear_markers",
];

/// G1/G2: one panel (widgets in data-model.md §3.3's order) and 23
/// trigger actions.
#[test]
fn registers_panel_and_23_actions() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();

    let panel = controller
        .plugins_mut()
        .ui()
        .panels()
        .for_plugin(id)
        .iter()
        .find(|p| p.id.as_str() == "main")
        .cloned()
        .unwrap_or_else(|| unreachable!("panel 'main' must be registered"));
    assert_eq!(panel.title, "Section Loop");
    let widget_ids: Vec<String> = panel
        .widgets
        .iter()
        .map(|w| w.spec.id.as_str().to_string())
        .collect();
    assert_eq!(
        widget_ids,
        vec![
            "set_a",
            "set_b",
            "loop",
            "repeat",
            "markers",
            "snap",
            "snap_note",
            "status"
        ]
    );

    assert_eq!(ACTION_SHORT_IDS.len(), 23);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        ACTION_SHORT_IDS
            .iter()
            .all(|short| c.actions().plugin_action_registered(&action_id(short)))
    }));
}

/// G3: on a fresh install `I`/`O`/`L` are flagged (host-vs-bundled
/// conflict, 006's own `SetA`/`SetB`/`ToggleLoop` already bind them);
/// `[`/`]` (`OpenBracket`/`CloseBracket`) are active.
#[test]
fn fresh_install_iol_flagged_brackets_active() {
    let (mut controller, _dir, _psd, _tsd, _id) = ready_section_loop();
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        [
            "set_a",
            "set_b",
            "toggle_loop",
            "nudge_earlier",
            "nudge_later",
        ]
        .iter()
        .all(|short| c.actions().plugin_action_registered(&action_id(short)))
    }));

    let i = Chord::parse("I").unwrap_or_else(|_| unreachable!());
    let o = Chord::parse("O").unwrap_or_else(|_| unreachable!());
    let l = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    let open = Chord::parse("OpenBracket").unwrap_or_else(|_| unreachable!());
    let close = Chord::parse("CloseBracket").unwrap_or_else(|_| unreachable!());

    assert!(controller.actions().is_conflicting(action_id("set_a"), i));
    assert!(controller.actions().is_conflicting(action_id("set_b"), o));
    assert!(
        controller
            .actions()
            .is_conflicting(action_id("toggle_loop"), l)
    );
    assert!(
        !controller
            .actions()
            .is_conflicting(action_id("nudge_earlier"), open)
    );
    assert!(
        !controller
            .actions()
            .is_conflicting(action_id("nudge_later"), close)
    );
}

// -- T024: Set A / Set B from the panel (contract A1/A2, EC) ---------------

/// `seek_frames` (frames), not `seek` (ms): the controller's own ms->frame
/// conversion for `seek` depends on `source_sample_rate`, only ever
/// captured from a real stream `attach()` — never populated in this
/// harness (no real audio device), so it stays `0` and would silently
/// seek to frame 0 regardless of `ms`.
fn seek_ms(controller: &mut PlaybackController<FakeBackend, ScriptedHost>, ms: u64) {
    controller.seek_frames(ms_frames(ms));
    let _ = controller.backend_mut().render_buffers(1);
}

/// `seek_ms` lands the playhead a fixed, small "one buffer" amount past
/// its exact target (the `render_buffers(1)` a seek needs to actually
/// take effect, per `write_anchor`, itself advances playback by one
/// buffer's worth of frames) — a click's own recorded position is
/// therefore always a little *past* the nominal seek, never before it and
/// never anywhere near a neighbouring assertion's own target as long as
/// this test's seeks are spaced out generously (seconds, not ms). Assert
/// "landed near", not frame-exact.
fn assert_near_ms(actual_frames: Option<u64>, target_ms: u64, tolerance_ms: u64) {
    let target = ms_frames(target_ms);
    let tolerance = ms_frames(tolerance_ms);
    let actual = actual_frames
        .unwrap_or_else(|| unreachable!("expected a position, got None (target {target_ms}ms)"));
    assert!(
        actual >= target && actual <= target + tolerance,
        "expected ~{target_ms}ms (+0..{tolerance_ms}ms) = {target}..{} frames, got {actual}",
        target + tolerance
    );
}

/// US1-1: clicking "Set A" then "Set B" from a fresh install creates one
/// Section-Loop-owned region with both endpoints, at the playhead each
/// click landed on.
#[test]
fn panel_set_a_then_set_b_creates_owned_region() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();

    seek_ms(&mut controller, 1_000);
    click(&mut controller, id, "set_a", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        own_region(c, id).is_some_and(|r| {
            c.markers()
                .and_then(|m| m.region(r))
                .is_some_and(|region| region.a.is_some() && region.b.is_none())
        })
    }));
    let region = own_region(&mut controller, id).unwrap_or_else(|| unreachable!());
    let a_id = controller
        .markers()
        .and_then(|m| m.region(region))
        .and_then(|r| r.a)
        .unwrap_or_else(|| unreachable!());
    assert_near_ms(
        controller
            .markers()
            .and_then(|m| m.marker(a_id))
            .map(|m| m.position),
        1_000,
        200,
    );

    seek_ms(&mut controller, 9_000);
    click(&mut controller, id, "set_b", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.b.is_some())
    }));

    let same_region = own_region(&mut controller, id);
    assert_eq!(
        same_region,
        Some(region),
        "Set B must complete the same region Set A created, not a second one"
    );
    let (a_id, b_id) = {
        let r = controller
            .markers()
            .and_then(|m| m.region(region))
            .unwrap_or_else(|| unreachable!());
        (
            r.a.unwrap_or_else(|| unreachable!()),
            r.b.unwrap_or_else(|| unreachable!()),
        )
    };
    let markers = controller.markers().unwrap_or_else(|| unreachable!());
    assert_near_ms(markers.marker(a_id).map(|m| m.position), 1_000, 200);
    assert_near_ms(markers.marker(b_id).map(|m| m.position), 9_000, 200);
    assert_eq!(markers.owner_of(a_id), Some(Owner::Plugin(id)));
    assert_eq!(markers.owner_of(b_id), Some(Owner::Plugin(id)));
}

/// Edge Cases: clicking "Set B" first, then "Set A" at a *later* playhead
/// than B, swaps — the host's own `maybe_swap_region_endpoints` keeps
/// `a.pos <= b.pos` (006 FR-006/FR-007), so the resulting region's `a`
/// ends up at B's click position and its `b` at A's.
#[test]
fn set_b_before_set_a_then_swap() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();

    seek_ms(&mut controller, 3_000);
    click(&mut controller, id, "set_b", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        own_region(c, id).is_some_and(|r| {
            c.markers()
                .and_then(|m| m.region(r))
                .is_some_and(|region| region.b.is_some())
        })
    }));
    let region = own_region(&mut controller, id).unwrap_or_else(|| unreachable!());

    seek_ms(&mut controller, 9_000);
    click(&mut controller, id, "set_a", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.is_complete())
    }));

    let markers = controller.markers().unwrap_or_else(|| unreachable!());
    let r = markers
        .region(region)
        .unwrap_or_else(|| unreachable!("region must still exist"));
    let a_id = r.a.unwrap_or_else(|| unreachable!());
    let b_id = r.b.unwrap_or_else(|| unreachable!());
    assert_near_ms(markers.marker(a_id).map(|m| m.position), 3_000, 200);
    assert_near_ms(markers.marker(b_id).map(|m| m.position), 9_000, 200);
    assert!(
        markers.marker(a_id).map(|m| m.position) < markers.marker(b_id).map(|m| m.position),
        "a must land before b after the swap"
    );
}

// -- T025: Loop toggle arm/disarm (contract A3/A4/A11) ---------------------

/// Seeds a complete, armable Section-Loop-owned region well within the
/// track (a 4 s gap, far past any crossfade/minimum-length threshold),
/// and waits for the real plugin thread's own `relist()` to have caught
/// up (its overlays reflect both endpoints) before a test drives its
/// panel — so the click's own precondition check (`S.region ~= nil`)
/// never races the seed.
fn seed_armable_region(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> RegionId {
    let gateway_region = seed_endpoint(controller, id, None, LoopEndpoint::A, 1_000);
    seed_endpoint(controller, id, Some(gateway_region), LoopEndpoint::B, 5_000);
    assert!(pump_until(controller, Duration::from_secs(5), |c| {
        let ids = overlay_ids(c, id);
        ids.contains(&"a_line".to_string()) && ids.contains(&"b_line".to_string())
    }));
    own_region(controller, id).unwrap_or_else(|| unreachable!())
}

/// US1-2/SC-004: flipping the Loop toggle on requests focus and arms the
/// same region in one user action (under the default auto-on-interaction
/// policy, with no other plugin contending).
#[test]
fn loop_toggle_requests_focus_and_arms_same_action() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let region = seed_armable_region(&mut controller, id);

    click(&mut controller, id, "loop", WidgetValue::Bool(true));

    // Both the model (`region.armed`, set synchronously inside the
    // `ArmLoop` apply arm) and the plugin's own `loop_armed`-driven toggle
    // widget (one more round trip: the fan-out event -> its handler ->
    // `update_widget`) must settle before either is a meaningful read.
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.armed)
            && widget_value(c, id, "loop") == Some(WidgetValue::Bool(true))
    }));
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id)
    );
}

/// US1-3: flipping it back off disarms the same region.
#[test]
fn loop_toggle_off_disarms() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let region = seed_armable_region(&mut controller, id);

    click(&mut controller, id, "loop", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.armed)
            && widget_value(c, id, "loop") == Some(WidgetValue::Bool(true))
    }));

    click(&mut controller, id, "loop", WidgetValue::Bool(false));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| !r.armed)
            && widget_value(c, id, "loop") == Some(WidgetValue::Bool(false))
    }));
}

// -- T026: finite repeat release, incomplete-region revert (A3/A9, E4) -----

/// US1-4/SC-005: a region armed with a finite repeat count releases (and
/// disarms host-side) after exactly that many wraps; the plugin's own
/// `loop_disarmed` handler follows suit and reverts the Loop toggle.
#[test]
fn finite_repeat_releases_and_toggle_follows() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    // A short (50 ms) region so a handful of buffer renders wrap it
    // several times quickly (positions are explicit RPC arguments here,
    // never a live playhead read, so this needs no seek dance).
    let gateway_region = seed_endpoint(&mut controller, id, None, LoopEndpoint::A, 1_000);
    seed_endpoint(
        &mut controller,
        id,
        Some(gateway_region),
        LoopEndpoint::B,
        1_050,
    );
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        let ids = overlay_ids(c, id);
        ids.contains(&"a_line".to_string()) && ids.contains(&"b_line".to_string())
    }));
    let region = own_region(&mut controller, id).unwrap_or_else(|| unreachable!());
    assert_eq!(
        call(
            &mut controller,
            id,
            Request::SetLoopRepeat {
                region: gateway_region,
                repeat: RepeatArg::Times(2),
            },
        ),
        Ok(Response::Ok)
    );

    controller.play();
    controller.tick();
    click(&mut controller, id, "loop", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.armed)
    }));

    let mut released = false;
    for _ in 0..500 {
        let _ = controller.backend_mut().render_buffers(1);
        if controller.shared().loop_state() == 0 {
            released = true;
            break;
        }
    }
    assert!(released, "a Times(2) region must release on its own");
    controller.tick(); // drains LoopReleased -> markers.disarm() + fan-out

    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| !r.armed)
            && widget_value(c, id, "loop") == Some(WidgetValue::Bool(false))
    }));
}

/// US1-5: with only one endpoint set, flipping the Loop toggle on reverts
/// it and shows the host's own refusal message (never the `no_focus`
/// hint — focus is granted; the region itself is what's refused).
#[test]
fn arm_incomplete_reverts_toggle_with_message() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    seed_endpoint(&mut controller, id, None, LoopEndpoint::A, 1_000);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        overlay_ids(c, id).contains(&"a_line".to_string())
    }));

    click(&mut controller, id, "loop", WidgetValue::Bool(true));

    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "loop") == Some(WidgetValue::Bool(false))
            && widget_value(c, id, "status") != Some(WidgetValue::Text(String::new()))
    }));
    assert_eq!(
        widget_value(&mut controller, id, "status"),
        Some(WidgetValue::Text(
            "This loop region has no A/B endpoints yet.".to_string()
        ))
    );
}

// -- T027: nudge (contract A5, FR-006) -------------------------------------

/// US1-6: with an endpoint the plugin itself just set as active, a nudge
/// action moves exactly that endpoint by 10 ms.
#[test]
fn nudge_moves_active_marker_10ms() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();

    seek_ms(&mut controller, 1_000);
    click(&mut controller, id, "set_a", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        overlay_ids(c, id).contains(&"a_line".to_string())
    }));
    seek_ms(&mut controller, 5_000);
    click(&mut controller, id, "set_b", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        overlay_ids(c, id).contains(&"b_line".to_string())
    }));

    let region = own_region(&mut controller, id).unwrap_or_else(|| unreachable!());
    let (a_id, b_id) = {
        let r = controller
            .markers()
            .and_then(|m| m.region(region))
            .unwrap_or_else(|| unreachable!());
        (
            r.a.unwrap_or_else(|| unreachable!()),
            r.b.unwrap_or_else(|| unreachable!()),
        )
    };
    // `nudge`'s own `markers.move(id, S.b_ms + 10)` round-trips through the
    // host's ms<->frames conversion (`relist()`'s `position_ms` field, then
    // back through the RPC's own `ms_to_frames`), so its delta from
    // whatever the *actual* current frame position is (rather than the
    // nominal seek target, itself already a "landed near" value per
    // `assert_near_ms`) may be off by a rounding frame or two — never by a
    // whole millisecond.
    let a_before = controller
        .markers()
        .and_then(|m| m.marker(a_id))
        .map(|m| m.position);
    let b_before = controller
        .markers()
        .and_then(|m| m.marker(b_id))
        .map(|m| m.position)
        .unwrap_or_else(|| unreachable!());

    controller.invoke_plugin_action(&action_id("nudge_later"), ActionSource::Keyboard);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.marker(b_id))
            .is_some_and(|m| m.position > b_before)
    }));

    let b_after = controller
        .markers()
        .and_then(|m| m.marker(b_id))
        .map(|m| m.position)
        .unwrap_or_else(|| unreachable!());
    let delta = b_after - b_before;
    let ten_ms = ms_frames(10);
    assert!(
        delta.abs_diff(ten_ms) <= ms_frames(2),
        "nudge_later must move B by ~10ms, moved by {delta} frames (10ms = {ten_ms})"
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(a_id))
            .map(|m| m.position),
        a_before,
        "nudging B must never move A"
    );
}

/// With no region at all, a nudge action is a silent no-op: no marker is
/// ever created.
#[test]
fn nudge_without_endpoints_noop() {
    let (mut controller, _dir, _psd, _tsd, _id) = ready_section_loop();

    controller.invoke_plugin_action(&action_id("nudge_earlier"), ActionSource::Keyboard);
    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(controller.markers().map(|m| m.markers().len()), Some(0));
}

/// FR-006's fallback order — "else B, else A" — for a region the plugin
/// never itself clicked into being active: seeded directly (see the
/// module doc), so the real thread's own `S.active` was never set by a
/// `set_a`/`set_b` call and stays `nil` throughout.
#[test]
fn nudge_default_b_then_a() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let gateway_region = seed_endpoint(&mut controller, id, None, LoopEndpoint::A, 1_000);
    let gateway_region = {
        let _ = seed_endpoint(
            &mut controller,
            id,
            Some(gateway_region),
            LoopEndpoint::B,
            5_000,
        );
        gateway_region
    };
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        let ids = overlay_ids(c, id);
        ids.contains(&"a_line".to_string()) && ids.contains(&"b_line".to_string())
    }));
    let region = own_region(&mut controller, id).unwrap_or_else(|| unreachable!());
    let (a_id, b_id) = {
        let r = controller
            .markers()
            .and_then(|m| m.region(region))
            .unwrap_or_else(|| unreachable!());
        (
            r.a.unwrap_or_else(|| unreachable!()),
            r.b.unwrap_or_else(|| unreachable!()),
        )
    };

    // No active pointer was ever set (this region was seeded, not
    // clicked into being) -> nudge falls back to B.
    controller.invoke_plugin_action(&action_id("nudge_later"), ActionSource::Keyboard);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers().and_then(|m| m.marker(b_id)).map(|m| m.position)
            == Some(ms_frames(5_000) + ms_frames(10))
    }));
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(a_id))
            .map(|m| m.position),
        Some(ms_frames(1_000)),
        "the B fallback must never move A"
    );

    // Delete B host-side (allowed at any time, FR-013): only A remains,
    // and the region is incomplete but the plugin's own `S.active` is
    // still nil (it never pointed at B directly) -> nudge now falls
    // back to A.
    controller
        .delete_marker(b_id)
        .unwrap_or_else(|e| unreachable!("delete_marker(b): {e}"));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        !overlay_ids(c, id).contains(&"b_line".to_string())
    }));
    let _ = gateway_region;

    controller.invoke_plugin_action(&action_id("nudge_earlier"), ActionSource::Keyboard);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers().and_then(|m| m.marker(a_id)).map(|m| m.position)
            == Some(ms_frames(1_000) - ms_frames(10))
    }));
}

// -- T028: overlays mirror the markers (contract O1) -----------------------

/// An A-only region draws exactly `a_line`/`a_label`; completing it with
/// B (in ordered position) adds `b_line`/`b_label`/`ab_region`; a set cue
/// slot (Phase 5, T052) adds exactly its own `cue_<n>_dot`/`cue_<n>_
/// label` pair, on top of that A/B/region subset — never anything else.
#[test]
fn overlay_set_matches_markers() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let gateway_region = seed_endpoint(&mut controller, id, None, LoopEndpoint::A, 1_000);

    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        !overlay_ids(c, id).is_empty()
    }));
    let mut ids = overlay_ids(&mut controller, id);
    ids.sort();
    let mut expected = vec!["a_line".to_string(), "a_label".to_string()];
    expected.sort();
    assert_eq!(
        ids, expected,
        "an A-only region must draw only a_line/a_label"
    );

    seed_endpoint(
        &mut controller,
        id,
        Some(gateway_region),
        LoopEndpoint::B,
        5_000,
    );
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        overlay_ids(c, id).contains(&"ab_region".to_string())
    }));
    let mut ids = overlay_ids(&mut controller, id);
    ids.sort();
    let mut expected = vec![
        "a_line".to_string(),
        "a_label".to_string(),
        "b_line".to_string(),
        "b_label".to_string(),
        "ab_region".to_string(),
    ];
    expected.sort();
    assert_eq!(ids, expected);

    let layer = controller
        .plugin_overlays()
        .into_iter()
        .find(|l| l.plugin == id)
        .unwrap_or_else(|| unreachable!());
    let region_prim = layer
        .primitives
        .iter()
        .find(|p| matches!(p, OverlayPrimitive::Region { id, .. } if id.as_str() == "ab_region"))
        .unwrap_or_else(|| unreachable!());
    match region_prim {
        OverlayPrimitive::Region { from_ms, to_ms, .. } => {
            assert_eq!(*from_ms, 1_000);
            assert_eq!(*to_ms, 5_000);
        }
        other => unreachable!("expected a Region primitive, got {other:?}"),
    }

    // T052: a set cue slot adds exactly its own dot/label pair, on top of
    // the A/B/region subset above (contract O1 complete).
    assert!(matches!(
        call(
            &mut controller,
            id,
            Request::SetCue {
                slot: 3,
                position_ms: 2_500,
            },
        ),
        Ok(Response::MarkerId(_))
    ));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        overlay_ids(c, id).contains(&"cue_3_dot".to_string())
    }));
    let mut ids = overlay_ids(&mut controller, id);
    ids.sort();
    let mut expected = vec![
        "a_line".to_string(),
        "a_label".to_string(),
        "b_line".to_string(),
        "b_label".to_string(),
        "ab_region".to_string(),
        "cue_3_dot".to_string(),
        "cue_3_label".to_string(),
    ];
    expected.sort();
    assert_eq!(
        ids, expected,
        "a set cue slot must add only its own dot/label pair"
    );
}

// == Phase 4: User Story 2 -- markers outlive the session and the plugin ===
// (contracts/section-loop-plugin.md P4/L1/L2, spec.md US2)

// -- T038: persistence across a full relaunch (US2-1, SC-002) --------------

/// US2-1/SC-002: A, B, a cue and a finite repeat count set in one session
/// are exactly where they were the next time the same track is opened,
/// *including after a full app restart* -- and Section Loop's own
/// `relist()` (driven by the fresh thread's `ready_ack` -> `track_changed`)
/// shows them without the plugin ever having written anything itself
/// (FR-013: the host's per-track store is the only persistence).
#[test]
fn markers_survive_reload_and_relist() {
    let (mut controller, dir, psd, tsd, id) = ready_section_loop();

    let gateway_region = seed_endpoint(&mut controller, id, None, LoopEndpoint::A, 1_000);
    seed_endpoint(
        &mut controller,
        id,
        Some(gateway_region),
        LoopEndpoint::B,
        5_000,
    );
    assert_eq!(
        call(
            &mut controller,
            id,
            Request::SetLoopRepeat {
                region: gateway_region,
                repeat: RepeatArg::Times(4),
            },
        ),
        Ok(Response::Ok)
    );
    assert!(matches!(
        call(
            &mut controller,
            id,
            Request::SetCue {
                slot: 1,
                position_ms: 2_000,
            },
        ),
        Ok(Response::MarkerId(_))
    ));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "repeat") == Some(WidgetValue::Number(4.0))
    }));

    // "Close the app": flushes the debounced marker-state save and joins
    // its writer thread (mirrors `controller_markers.rs`'s own use of
    // `shutdown()` to make a following on-disk read deterministic).
    controller.shutdown();

    // "Reopen the same track tomorrow": a brand-new controller against the
    // same track-state directory.
    let (mut controller2, id2) = reopen_section_loop(&dir, &psd, &tsd);
    controller2.set_playback_permitted(true, None);
    controller2.queue_replace(vec![track("a")]);
    controller2.play();
    // FR-11.3.2, deterministically: `play()` is the `dispatch()` that
    // makes the track current, loads its markers and fans out
    // `track_changed` in one go — and the plugin's `markers.list()` in
    // that handler is a read of the shared snapshot. The snapshot must
    // therefore already carry the restored region *now*, before any
    // `tick()` has run; previously it was only republished at the end
    // of the next tick, so a plugin thread that won the race relisted
    // the previous (empty) track and never saw a later `marker_changed`.
    {
        let snapshot = controller2.plugins_mut().snapshot().clone();
        let guard = snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(
            guard.regions.len(),
            1,
            "the plugin-visible snapshot must hold the restored region before the first tick: {:?}",
            guard.regions
        );
        assert_eq!(
            guard.markers.len(),
            3,
            "A, B and the cue must all be in the snapshot before the first tick: {:?}",
            guard.markers
        );
    }
    controller2.tick();

    // The host model itself restores A, B and the cue before Section
    // Loop's own `track_changed` handler ever runs (FR-11.3.2).
    assert!(pump_until(&mut controller2, Duration::from_secs(5), |c| {
        own_region(c, id2).is_some_and(|r| {
            c.markers()
                .and_then(|m| m.region(r))
                .is_some_and(LoopRegion::is_complete)
        })
    }));
    let region2 = own_region(&mut controller2, id2).unwrap_or_else(|| unreachable!());
    let markers2 = controller2.markers().unwrap_or_else(|| unreachable!());
    let r2 = markers2
        .region(region2)
        .unwrap_or_else(|| unreachable!("region must exist"));
    assert_eq!(
        markers2
            .marker(r2.a.unwrap_or_else(|| unreachable!()))
            .map(|m| m.position),
        Some(ms_frames(1_000)),
        "A must be exactly where it was left"
    );
    assert_eq!(
        markers2
            .marker(r2.b.unwrap_or_else(|| unreachable!()))
            .map(|m| m.position),
        Some(ms_frames(5_000)),
        "B must be exactly where it was left"
    );
    assert_eq!(
        r2.repeat,
        RepeatCount::Times(4),
        "the repeat count must survive the restart"
    );
    let cue1 = markers2
        .markers()
        .iter()
        .find(|m| matches!(m.kind, MarkerKind::Cue { slot } if slot.get() == 1))
        .unwrap_or_else(|| unreachable!("cue slot 1 must survive the restart"));
    assert_eq!(cue1.position, ms_frames(2_000));
    assert_eq!(markers2.owner_of(cue1.id), Some(Owner::Plugin(id2)));

    // Section Loop's own view (relisted from `markers.list()` on its fresh
    // `ready_ack` and the ensuing `track_changed`) agrees: its repeat
    // slider shows 4 and its overlays are rebuilt from the restored
    // region.
    let relisted = pump_until(&mut controller2, Duration::from_secs(5), |c| {
        let ids = overlay_ids(c, id2);
        widget_value(c, id2, "repeat") == Some(WidgetValue::Number(4.0))
            && ids.contains(&"a_line".to_string())
            && ids.contains(&"b_line".to_string())
            && ids.contains(&"ab_region".to_string())
    });
    assert!(
        relisted,
        "Section Loop must relist the restored region on its fresh track_changed: repeat={:?} {}",
        widget_value(&mut controller2, id2, "repeat"),
        diagnose(&mut controller2, id2)
    );
}

// -- T039: disable mid-loop (US2-2/US2-3, SC-003) ---------------------------

/// US2-2/US2-3/SC-003: disabling Section Loop while its loop is armed and
/// it holds transport focus releases the loop and returns focus to the
/// host within the same interaction (010 FR-007, no plugin code involved),
/// while A and B stay exactly where they were and remain editable from the
/// host side -- without re-enabling the plugin.
#[test]
fn disable_mid_loop_releases_and_keeps_markers() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let region = seed_armable_region(&mut controller, id);

    click(&mut controller, id, "loop", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.armed)
            && c.plugins_mut().arbiter().holder() == FocusHolder::Plugin(id)
    }));

    let (a_id, b_id) = {
        let r = controller
            .markers()
            .and_then(|m| m.region(region))
            .unwrap_or_else(|| unreachable!());
        (
            r.a.unwrap_or_else(|| unreachable!()),
            r.b.unwrap_or_else(|| unreachable!()),
        )
    };
    let a_before = controller
        .markers()
        .and_then(|m| m.marker(a_id))
        .map(|m| m.position);
    let b_before = controller
        .markers()
        .and_then(|m| m.marker(b_id))
        .map(|m| m.position);

    controller.plugin_disable(id);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Disabled)
        )
    }));

    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "transport focus must return to the host on disable (010 FR-007)"
    );
    assert!(
        controller
            .markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| !r.armed),
        "the loop must disarm on disable"
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(a_id))
            .map(|m| m.position),
        a_before,
        "A must stay exactly where it was"
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(b_id))
            .map(|m| m.position),
        b_before,
        "B must stay exactly where it was"
    );

    // US2-3: the host's own Markers panel can still edit these markers
    // without re-enabling Section Loop (FR-5.1.3/FR-014) -- exercised here
    // via the same core calls that panel drives.
    controller
        .rename_marker(a_id, "Verse")
        .unwrap_or_else(|e| unreachable!("rename_marker while disabled: {e}"));
    controller
        .move_marker(b_id, ms_frames(6_000))
        .unwrap_or_else(|e| unreachable!("move_marker while disabled: {e}"));
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(a_id))
            .map(|m| m.name.clone()),
        Some("Verse".to_string())
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(b_id))
            .map(|m| m.position),
        Some(ms_frames(6_000))
    );
}

// -- T040: host-side edits fan out to the plugin (US2-5) --------------------

/// US2-5: a host-side edit (e.g. dragging A on the waveform, or renaming
/// it in the host's Markers panel) reaches Section Loop as `marker_changed`
/// (actor `host`) and its own view -- here, the overlay line it drew for
/// A -- follows within the next UI update, with ownership unchanged.
#[test]
fn host_edit_reflected_via_marker_changed() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let gateway_region = seed_endpoint(&mut controller, id, None, LoopEndpoint::A, 1_000);
    seed_endpoint(
        &mut controller,
        id,
        Some(gateway_region),
        LoopEndpoint::B,
        5_000,
    );
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        let ids = overlay_ids(c, id);
        ids.contains(&"a_line".to_string()) && ids.contains(&"b_line".to_string())
    }));
    let region = own_region(&mut controller, id).unwrap_or_else(|| unreachable!());
    let a_id = controller
        .markers()
        .and_then(|m| m.region(region))
        .and_then(|r| r.a)
        .unwrap_or_else(|| unreachable!());

    // A host-side move -- never through the plugin's own RPC path.
    controller
        .move_marker(a_id, ms_frames(2_000))
        .unwrap_or_else(|e| unreachable!("move_marker: {e}"));

    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        controller_a_line_at_ms(c, id) == Some(2_000)
    }));
    assert_eq!(
        controller.markers().and_then(|m| m.owner_of(a_id)),
        Some(Owner::Plugin(id)),
        "a host-side move must never change ownership"
    );
}

fn controller_a_line_at_ms(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> Option<u64> {
    controller
        .plugin_overlays()
        .into_iter()
        .find(|l| l.plugin == id)?
        .primitives
        .iter()
        .find_map(|p| match p {
            OverlayPrimitive::Line { id, at_ms, .. } if id.as_str() == "a_line" => Some(*at_ms),
            _ => None,
        })
}

// -- T041: re-enable relists (US2-4) -----------------------------------------

/// US2-4: once Section Loop is disabled and re-enabled, its panel's own
/// marker list -- here, its overlay rebuild and repeat slider, both driven
/// solely by `relist()`'s `markers.list()` read -- shows the same A, B and
/// region, unchanged by the interruption (contract L2, FR-015).
#[test]
fn reenable_relists() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let gateway_region = seed_endpoint(&mut controller, id, None, LoopEndpoint::A, 1_000);
    seed_endpoint(
        &mut controller,
        id,
        Some(gateway_region),
        LoopEndpoint::B,
        5_000,
    );
    assert_eq!(
        call(
            &mut controller,
            id,
            Request::SetLoopRepeat {
                region: gateway_region,
                repeat: RepeatArg::Times(4),
            },
        ),
        Ok(Response::Ok)
    );
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        let ids = overlay_ids(c, id);
        widget_value(c, id, "repeat") == Some(WidgetValue::Number(4.0))
            && ids.contains(&"a_line".to_string())
            && ids.contains(&"b_line".to_string())
    }));
    let region = own_region(&mut controller, id).unwrap_or_else(|| unreachable!());

    controller.plugin_disable(id);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Disabled)
        )
    }));
    assert!(
        overlay_ids(&mut controller, id).is_empty(),
        "a disabled plugin's overlays must be cleared (011 FR-016)"
    );

    controller.plugin_enable(id);
    assert!(wait_active(&mut controller, id));
    assert!(wait_panel_registered(&mut controller, id));

    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        let ids = overlay_ids(c, id);
        widget_value(c, id, "repeat") == Some(WidgetValue::Number(4.0))
            && ids.contains(&"a_line".to_string())
            && ids.contains(&"b_line".to_string())
            && ids.contains(&"ab_region".to_string())
    }));
    assert_eq!(
        own_region(&mut controller, id),
        Some(region),
        "the same region, unchanged by the disable/re-enable cycle"
    );
}

// == Phase 5: User Story 3 -- the user's plugin choice always wins on
// transport focus (contracts/section-loop-plugin.md A6-A11, O1, spec.md
// US3) ========================================================================

// -- T045: focus contention (US3-1, SC-004) ----------------------------------

/// US3-1/SC-004: with `focus-b` (a real, contending `transport.control`
/// plugin, fixture) holding transport focus, flipping Section Loop's own
/// Loop toggle under the default Auto-on-interaction policy both takes
/// focus away from `focus-b` and arms the same region in one user action
/// (011 R16/FR-026: `request_focus()` called synchronously inside a
/// `panel_interaction` handler is flagged `UserInteraction`, which grants
/// immediately under Auto -- exactly like the user's own "Give focus").
#[test]
fn loop_toggle_takes_focus_from_other_plugin() {
    let (mut controller, _dir, _psd, _tsd, id, focus_b) = ready_section_loop_with_fixtures();
    let region = seed_armable_region(&mut controller, id);

    controller.focus_give(focus_b);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.plugins_mut().arbiter().holder() == FocusHolder::Plugin(focus_b)
    }));

    click(&mut controller, id, "loop", WidgetValue::Bool(true));

    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.plugins_mut().arbiter().holder() == FocusHolder::Plugin(id)
            && c.markers()
                .and_then(|m| m.region(region))
                .is_some_and(|r| r.armed)
            && widget_value(c, id, "loop") == Some(WidgetValue::Bool(true))
    }));
}

// -- T046: Manual policy refusal, no deferred arm (US3-2, SC-007) -----------

/// US3-2/SC-007: under Manual, `request_focus()` is only ever recorded,
/// never auto-granted -- so the same click that arms under Auto is
/// refused `no_focus`, the toggle reverts and Status shows the focus
/// hint. A later "Give focus" grant must never itself arm the loop the
/// click already reverted (contract A11/A12: `focus_granted` never arms
/// by itself).
#[test]
fn manual_policy_refuses_and_hints_no_deferred_arm() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    controller.set_focus_policy(FocusPolicy::Manual);
    let region = seed_armable_region(&mut controller, id);

    click(&mut controller, id, "loop", WidgetValue::Bool(true));

    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "loop") == Some(WidgetValue::Bool(false))
            && widget_value(c, id, "status") != Some(WidgetValue::Text(String::new()))
    }));
    assert_eq!(
        widget_value(&mut controller, id, "status"),
        Some(WidgetValue::Text(
            "Needs transport focus — give Section Loop focus in the Transport panel (T)"
                .to_string()
        ))
    );
    assert!(
        controller
            .markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| !r.armed),
        "the region must never arm under Manual with no user grant"
    );

    // A later grant must never itself arm the loop -- no deferred arm.
    controller.focus_give(id);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.plugins_mut().arbiter().holder() == FocusHolder::Plugin(id)
    }));
    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        controller
            .markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| !r.armed),
        "focus_granted must never itself arm the loop (A11/A12, no deferred arm)"
    );
    assert_eq!(
        widget_value(&mut controller, id, "loop"),
        Some(WidgetValue::Bool(false))
    );
}

// -- T047: host L disarms/rearms the plugin's own region (US3-4) ------------

/// US3-4: the host's own `L` (`toggle_current_loop`, bypassing the
/// gateway entirely -- the user's own override, Constitution X) disarms
/// Section Loop's armed region; the plugin's own `loop_disarmed` handler
/// (E4) follows suit and reverts the Loop toggle. A second `L` re-arms
/// the same region and the toggle follows again.
#[test]
fn host_l_disarm_then_rearm() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let region = seed_armable_region(&mut controller, id);

    click(&mut controller, id, "loop", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.armed)
            && widget_value(c, id, "loop") == Some(WidgetValue::Bool(true))
    }));

    controller
        .toggle_current_loop()
        .unwrap_or_else(|e| unreachable!("host L disarm: {e:?}"));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| !r.armed)
            && widget_value(c, id, "loop") == Some(WidgetValue::Bool(false))
    }));

    controller
        .toggle_current_loop()
        .unwrap_or_else(|e| unreachable!("host L rearm: {e:?}"));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.armed)
            && widget_value(c, id, "loop") == Some(WidgetValue::Bool(true))
    }));
}

// -- T048: jump_cue (US3-5, contract A7) -------------------------------------

/// US3-5: `jump_cue_n` requests focus, then seeks to the cue's position
/// -- any owner's cue is a valid jump target (contract A7).
#[test]
fn jump_cue_requests_focus_then_seeks() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    assert!(matches!(
        call(
            &mut controller,
            id,
            Request::SetCue {
                slot: 3,
                position_ms: 42_000,
            },
        ),
        Ok(Response::MarkerId(_))
    ));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        overlay_ids(c, id).contains(&"cue_3_dot".to_string())
    }));

    controller.invoke_plugin_action(&action_id("jump_cue_3"), ActionSource::Keyboard);
    assert!(
        wait_position_ms_at_least(&mut controller, 42_000, Duration::from_secs(5)),
        "jump_cue_3 must seek to the cue's position"
    );
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id),
        "jump_cue_3 must request focus first (contract A11)"
    );
}

/// With no cue set in that slot, `jump_cue_n` is a silent no-op: no
/// focus request, no seek.
#[test]
fn jump_cue_empty_slot_noop() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let _ = id;

    controller.invoke_plugin_action(&action_id("jump_cue_5"), ActionSource::Keyboard);
    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "an empty cue slot must never request focus"
    );
    assert_eq!(controller.position(), Duration::ZERO);
}

// -- T049: set_cue ownership (contract A6) -----------------------------------

/// A slot the host already occupies is refused `not_owner`; Status shows
/// the per-slot hint naming that slot (contract A6).
#[test]
fn set_cue_on_host_slot_refused_with_hint() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    seek_ms(&mut controller, 3_000);
    let host_slot = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    controller
        .set_cue(host_slot)
        .unwrap_or_else(|e| unreachable!("host set_cue: {e:?}"));

    controller.invoke_plugin_action(&action_id("set_cue_1"), ActionSource::Keyboard);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "status") != Some(WidgetValue::Text(String::new()))
    }));
    assert_eq!(
        widget_value(&mut controller, id, "status"),
        Some(WidgetValue::Text(
            "Cue 1 belongs to the host — move or delete it in the Markers panel".to_string()
        ))
    );
}

/// An empty slot creates the plugin's own cue; a second `set_cue_n` on
/// the same, still-own slot moves it (never refused).
#[test]
fn set_cue_own_slot_moves() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    seek_ms(&mut controller, 2_000);
    controller.invoke_plugin_action(&action_id("set_cue_4"), ActionSource::Keyboard);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers().is_some_and(|m| {
            m.markers()
                .iter()
                .any(|mk| matches!(mk.kind, MarkerKind::Cue { slot } if slot.get() == 4))
        })
    }));
    let cue_id = {
        let markers = controller.markers().unwrap_or_else(|| unreachable!());
        markers
            .markers()
            .iter()
            .find(|mk| matches!(mk.kind, MarkerKind::Cue { slot } if slot.get() == 4))
            .unwrap_or_else(|| unreachable!())
            .id
    };
    assert_near_ms(
        controller
            .markers()
            .and_then(|m| m.marker(cue_id))
            .map(|m| m.position),
        2_000,
        200,
    );

    seek_ms(&mut controller, 7_000);
    controller.invoke_plugin_action(&action_id("set_cue_4"), ActionSource::Keyboard);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.marker(cue_id))
            .is_some_and(|mk| mk.position >= ms_frames(7_000))
    }));
    assert_near_ms(
        controller
            .markers()
            .and_then(|m| m.marker(cue_id))
            .map(|m| m.position),
        7_000,
        200,
    );
    assert_eq!(
        controller.markers().and_then(|m| m.owner_of(cue_id)),
        Some(Owner::Plugin(id))
    );
}

// -- T050: clear_markers (contract A8) ---------------------------------------

/// `clear_markers` deletes only this plugin's own A/B and cues, never a
/// host-owned cue; deleting the armed region's own endpoints disarms it
/// host-side (`delete_marker`'s own `LoopDisarm` push) -- the script
/// itself never calls `disarm_loop` (contract A8).
#[test]
fn clear_markers_deletes_own_only_and_disarms_hostside() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();

    // Seed the host-owned cue *before* the loop is armed -- once armed,
    // a `seek` past the region's own B snaps straight back to A on the
    // very next render (the engine's own gapless-wrap check), which
    // would make this cue's captured position meaningless.
    seek_ms(&mut controller, 9_000);
    let host_slot = CueSlot::new(6).unwrap_or_else(|| unreachable!());
    let host_cue_id = controller
        .set_cue(host_slot)
        .unwrap_or_else(|e| unreachable!("host set_cue: {e:?}"));

    let region = seed_armable_region(&mut controller, id);
    click(&mut controller, id, "loop", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        c.markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.armed)
    }));

    assert!(matches!(
        call(
            &mut controller,
            id,
            Request::SetCue {
                slot: 5,
                position_ms: 2_000,
            },
        ),
        Ok(Response::MarkerId(_))
    ));
    // The real thread's own `S.cues[5]` must have caught up (its next
    // `relist()`, off the resulting `marker_changed`) before `clear_
    // markers` reads it -- otherwise it would see slot 5 as still empty.
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        overlay_ids(c, id).contains(&"cue_5_dot".to_string())
    }));

    controller.invoke_plugin_action(&action_id("clear_markers"), ActionSource::Keyboard);

    // `own_region`/`armed_region` alone would settle after just the A/B
    // deletes -- the handler's own cue-deletion RPC (a third, later call
    // in the same synchronous handler) can still be in flight on the
    // plugin's own thread at that point, so wait for the full effect
    // (the cue actually gone) before reading final state.
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        own_region(c, id).is_none()
            && c.markers().and_then(|m| m.armed_region()).is_none()
            && c.markers().is_some_and(|m| {
                !m.markers()
                    .iter()
                    .any(|mk| matches!(mk.kind, MarkerKind::Cue { slot } if slot.get() == 5))
            })
    }));
    let markers = controller.markers().unwrap_or_else(|| unreachable!());
    assert!(
        !markers
            .markers()
            .iter()
            .any(|m| matches!(m.kind, MarkerKind::Cue { slot } if slot.get() == 5)),
        "the plugin's own cue must be deleted"
    );
    assert_near_ms(markers.marker(host_cue_id).map(|m| m.position), 9_000, 200);
    assert_eq!(markers.owner_of(host_cue_id), Some(Owner::Host));
}

// -- T051: inert snap, status lifecycle --------------------------------------

/// The `snap` toggle (and the `toggle_snap` action) always reverts to
/// `false` -- it is permanently inert (contract A10, FR-005).
#[test]
fn snap_toggle_always_reverts() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();

    click(&mut controller, id, "snap", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "snap") == Some(WidgetValue::Bool(false))
    }));

    controller.invoke_plugin_action(&action_id("toggle_snap"), ActionSource::Keyboard);
    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        widget_value(&mut controller, id, "snap"),
        Some(WidgetValue::Bool(false))
    );
}

/// A refusal's message stays in Status until the next success clears it.
#[test]
fn status_clears_on_success() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    seek_ms(&mut controller, 3_000);
    let host_slot = CueSlot::new(7).unwrap_or_else(|| unreachable!());
    controller
        .set_cue(host_slot)
        .unwrap_or_else(|e| unreachable!("host set_cue: {e:?}"));

    controller.invoke_plugin_action(&action_id("set_cue_7"), ActionSource::Keyboard);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "status") != Some(WidgetValue::Text(String::new()))
    }));

    seek_ms(&mut controller, 6_000);
    controller.invoke_plugin_action(&action_id("set_cue_8"), ActionSource::Keyboard);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "status") == Some(WidgetValue::Text(String::new()))
    }));
}

// -- T052: overlays cleared on disable (O3) ----------------------------------

/// Disabling Section Loop clears every overlay it drew, cue primitives
/// included -- not just the A/B/region subset US2's own
/// `reenable_relists` already covers (011 FR-016).
#[test]
fn overlay_cleared_on_disable() {
    let (mut controller, _dir, _psd, _tsd, id) = ready_section_loop();
    let gateway_region = seed_endpoint(&mut controller, id, None, LoopEndpoint::A, 1_000);
    seed_endpoint(
        &mut controller,
        id,
        Some(gateway_region),
        LoopEndpoint::B,
        5_000,
    );
    assert!(matches!(
        call(
            &mut controller,
            id,
            Request::SetCue {
                slot: 2,
                position_ms: 3_000,
            },
        ),
        Ok(Response::MarkerId(_))
    ));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        let ids = overlay_ids(c, id);
        ids.contains(&"ab_region".to_string()) && ids.contains(&"cue_2_dot".to_string())
    }));

    controller.plugin_disable(id);
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Disabled)
        )
    }));
    assert!(
        overlay_ids(&mut controller, id).is_empty(),
        "disabling Section Loop must clear every overlay, cue primitives included (O3)"
    );
}

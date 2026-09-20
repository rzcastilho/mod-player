// SPDX-License-Identifier: MIT OR Apache-2.0

//! Transport-focus arbitration, end to end (010-transport-focus,
//! contracts/focus-arbitration.md §2, plan.md's `controller_transport_
//! focus.rs`). US1 (this file's first block, P1 — the single-holder
//! guarantee and the user's unconditional override) drives the real
//! `focus-a`/`focus-b` fixtures on a full `PlaybackController<FakeBackend,
//! ScriptedHost>` (`MODPLAYER_PLUGIN_FIXTURES=1`), reading state back
//! through `transport_focus_view()`/the raw arbiter, `markers()` and the
//! fixtures' own `debug_probe("log")`. US2 (this file's second block,
//! P2 — the three focus policies and policy persistence, tasks.md Phase
//! 4) extends this same file, mixing the same real-fixture harness (where
//! the scenario is about a real, `transport.control`-granted plugin — a
//! panel row disappearing on disable, a restarted plugin's own re-request)
//! with a `bare_controller_with_track()` + fabricated `PluginId`s (where
//! it is about the pure engine's ordering guarantees a racy real plugin
//! thread cannot deterministically prove — see `bare_controller`'s own
//! doc comment). US3 extends this same file next (tasks.md Phase 5) — not
//! this session's scope.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, RemoteCommand, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::event::{HostEvent, PlayState};
use modplayer_capability_gateway::refusal::{Refusal, RefusalCode};
use modplayer_capability_gateway::request::{Request, Response};
use modplayer_core::markers::RegionId;
use modplayer_core::plugins::{Lifecycle, PluginId, to_gateway_id};
use modplayer_core::settings::SettingsStore;
use modplayer_core::transport::Intent;
use modplayer_core::{FocusHolder, FocusPolicy, PlaybackController};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_plugin_runtime::handle::RpcEnvelope;

const FOCUS_A: &str = "org.modplayer.fixture.focus-a";
const FOCUS_B: &str = "org.modplayer.fixture.focus-b";

// -----------------------------------------------------------------------
// Harness (mirrors controller_plugins_permissions.rs / controller_plugins_
// lifecycle.rs's own patterns).
// -----------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-transport-focus-{}-{}",
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

/// `MODPLAYER_PLUGIN_FIXTURES`/`MODPLAYER_PLUGIN_STATE_DIR`/
/// `MODPLAYER_TRACK_STATE_DIR` are process-global, so every construction in
/// this binary is serialized against every other's brief mutation of them
/// (mirrors every other `controller_plugins_*.rs` file's own pattern).
static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

/// A controller that will discover every `plugins/fixtures/` package,
/// **not yet `launch()`ed** — a caller can override a fixture's `Budgets`
/// (via `plugins_mut().record_mut(id)`) before its thread ever spawns. Also
/// hands back a `ScriptedHostHandle` and the `SettingsStore`'s temp dir, for
/// tests that need to drive a remote command or reconstruct the settings.
fn fixture_controller() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let plugin_state_dir = TempDir::new();
    let track_state_dir = TempDir::new();
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device("dev-1", true)];
    let controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: narrowly scopes each mutation to the one synchronous
        // read `PlaybackController::new` makes of it, serialized against
        // every other test in this binary via the lock above.
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
    (controller, handle, dir, plugin_state_dir, track_state_dir)
}

fn controller_plugin_id(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    identifier: &str,
) -> PluginId {
    controller
        .plugins_mut()
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == identifier)
        .map(|r| r.id)
        .unwrap_or_else(|| unreachable!("fixture '{identifier}' must be discovered"))
}

fn pump_controller_until(
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

/// A controller launched with a device confirmed, playback permitted and a
/// single track already playing — every test in this file needs
/// `transport_enabled()`/`markers()` live for the focus-gated requests and
/// loop arm/disarm to have anything to act on. `launch()` is what actually
/// spawns every enabled fixture's thread (contracts/plugin-host-service.md
/// §1), so it must run before any budget override would need to (this
/// helper is for the tests that don't need one).
fn controller_with_track() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
    TempDir,
    TempDir,
) {
    let (mut controller, handle, dir, psd, tsd) = fixture_controller();
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    (controller, handle, dir, psd, tsd)
}

fn wait_active(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> bool {
    pump_controller_until(controller, Duration::from_secs(2), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )
    })
}

fn wait_holder(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    holder: FocusHolder,
    timeout: Duration,
) -> bool {
    pump_controller_until(controller, timeout, |c| {
        c.plugins_mut().arbiter().holder() == holder
    })
}

/// The armed region, once one exists (a fixture's own `arm_loop` RPC lands
/// asynchronously, a few ticks after its `focus_granted`).
fn wait_armed(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    timeout: Duration,
) -> Option<RegionId> {
    let deadline = Instant::now() + timeout;
    loop {
        controller.tick();
        if let Some(region) = controller.markers().and_then(|m| m.armed_region()) {
            return Some(region.id);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Submits `request` from `plugin` straight to `drain_plugin_requests()`
/// (C1), bypassing the gateway's own admission entirely (mirrors
/// `controller_plugins_permissions.rs`'s own `call()`) — used both for the
/// host-side re-check test (R12, an admitted-but-stale envelope) and for
/// every fixture's `debug_probe("log")` read.
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

/// `focus-a`/`focus-b`'s own accumulated event log (data-model.md §5),
/// read back through the fixture-only `debug_probe("log")` request — the
/// same path a manual scenario's own debug tooling would use.
fn probe_log(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> Vec<String> {
    match call(
        controller,
        id,
        Request::DebugProbe {
            name: "log".to_string(),
        },
    ) {
        Ok(Response::Probe(value)) => value
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn wait_for_log_contains(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
    needle: &str,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if probe_log(controller, id).iter().any(|l| l.contains(needle)) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// `focus-a`/`focus-b`'s own `play_state_changed` trigger (mirrors
/// `controller_plugins_lifecycle.rs`'s `trigger_hang_via_controller`) — a
/// direct `fan_out`, since neither fixture needs a real device render to
/// receive it.
fn fan_out_play_state(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    state: PlayState,
) {
    let now = controller.now();
    controller
        .plugins_mut()
        .fan_out(&HostEvent::PlayStateChanged { state }, now);
}

/// A controller with **no** discovered plugins at all (`MODPLAYER_PLUGIN_
/// FIXTURES` left unset) — Phase 4 (US2)'s own tests that need precise,
/// racy-thread-free control over *which* `PluginId` requests focus first
/// use this instead of `fixture_controller()`: `focus-a`/`focus-b`'s own
/// scripts self-`request_focus()` on `ready_ack`, and RT5 guarantees that
/// dispatch runs, on the plugin's own thread, strictly before it ever
/// posts `RuntimeEvent::Ready` — so by the time a real fixture's record is
/// observed `Active`, it has *already* requested. That is exactly right
/// for US1's tests; it makes a real fixture useless for proving "never
/// requested" (the never-requested edge case) or a deterministic A-then-B
/// request order across 20 tracks. `FocusArbiter::request`/`give`/
/// `release` and `PluginHost::apply_focus_changes` are pure/plumbing
/// (data-model.md §1.3-§1.4) and never look the `PluginId` up in the
/// plugin registry to validate it, so a fabricated id drives them exactly
/// like a real plugin's would — `apply_focus_changes`'s own handle lookup
/// (`self.record(plugin)`) is `None` for a fabricated id and is skipped
/// silently, precisely the documented "Fault" no-handle path.
fn bare_controller() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device("dev-1", true)];
    let controller = {
        // `MODPLAYER_PLUGIN_FIXTURES` is process-global (this file's own
        // `PLUGIN_ENV_LOCK` doc comment) — this construction reads it too
        // (indirectly, via `bundled::fixtures_enabled()`), so it must take
        // the same lock as `fixture_controller()`'s own brief mutation of
        // it, or a concurrently-running test's transient `"1"` could leak
        // in here and load real fixtures onto this file's fabricated
        // `PluginId`s.
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        PlaybackController::new(FakeBackend::new(devices), host, store)
    };
    (controller, handle, dir)
}

/// [`bare_controller`], `launch()`ed with a device confirmed, playback
/// permitted and a first track already playing.
fn bare_controller_with_track() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
) {
    let (mut controller, handle, dir) = bare_controller();
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("seed")]);
    controller.play();
    controller.tick();
    (controller, handle, dir)
}

/// Drive one arbiter call end to end exactly as production code must
/// (design note 4/C3): apply the returned `Vec<FocusChange>` through
/// `PluginHost::apply_focus_changes` immediately, never leaving a call's
/// changes un-applied.
fn request(controller: &mut PlaybackController<FakeBackend, ScriptedHost>, id: PluginId) {
    let changes = controller.plugins_mut().arbiter_mut().request(
        id,
        true,
        modplayer_core::plugins::RequestOrigin::Api,
    );
    controller.plugins_mut().apply_focus_changes(changes);
}

fn release(controller: &mut PlaybackController<FakeBackend, ScriptedHost>, id: PluginId) {
    let changes = controller.plugins_mut().arbiter_mut().release(id);
    controller.plugins_mut().apply_focus_changes(changes);
}

fn give(controller: &mut PlaybackController<FakeBackend, ScriptedHost>, id: PluginId) {
    let changes = controller.plugins_mut().arbiter_mut().give(id);
    controller.plugins_mut().apply_focus_changes(changes);
}

fn holder(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) -> FocusHolder {
    controller.plugins_mut().arbiter().holder()
}

fn request_order(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> Option<usize> {
    controller.plugins_mut().arbiter().request_order(id)
}

/// The engine anchor `position()` reads only republishes on an actual
/// render pull (contracts/transport-and-queue.md's own "position derived
/// from the audio clock"; `controller_streaming.rs`'s tests all render
/// after a `seek()` for the same reason) — poll a fixture's own RPC'd
/// `seek` landing by pumping both the controller and a tiny render.
fn wait_position_at_least(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    at_least: Duration,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        controller.tick();
        let _ = controller.backend_mut().render_buffers(1);
        if controller.position() >= at_least {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

// -----------------------------------------------------------------------
// Phase 3, User Story 1 (P1, MVP): exactly one party is in charge, and the
// user always wins.
// -----------------------------------------------------------------------

/// SC-001, US1-1: while focus-a holds transport focus, focus-b's own
/// `seek(0)` (on the next `play_state_changed`, per its own script) is
/// refused `no_focus` at its own thread's `Gateway::admit` — the playhead
/// never moves as a result, and the holder is unaffected.
#[test]
fn non_holder_seek_is_refused_no_side_effect() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    let id_b = controller_plugin_id(&mut controller, FOCUS_B);
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    assert!(
        wait_active(&mut controller, id_b),
        "focus-b must reach Active"
    );

    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );

    // focus-a's own `focus_granted` handler seeks to 1000ms — wait for that
    // to actually land before using it as this test's "no side effect"
    // baseline.
    assert!(
        wait_position_at_least(
            &mut controller,
            Duration::from_millis(1000),
            Duration::from_secs(2)
        ),
        "focus-a's own seek(1000) on grant must land"
    );

    // Trigger focus-b's own contention attempt: it is not the holder, so
    // its `seek(0)` must be refused by its own thread's Gateway before it
    // ever reaches the host.
    fan_out_play_state(&mut controller, PlayState::Playing);
    assert!(
        wait_for_log_contains(
            &mut controller,
            id_b,
            "seek: no_focus",
            Duration::from_secs(2)
        ),
        "focus-b must log its own no_focus refusal"
    );

    assert!(
        controller.position() >= Duration::from_millis(900),
        "focus-b's refused seek(0) must have had no side effect on the playhead, got {:?}",
        controller.position()
    );
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id_a),
        "the refused contention must not change the holder"
    );
}

/// R12 (C1): the host-side re-check ahead of the eight focus-gated
/// requests closes the admission→application race — an envelope for a
/// plugin that held focus a moment ago but was revoked (here, by the
/// user's own "Take back") before this synthetic, already-"admitted"
/// envelope is drained is still refused `no_focus`, not applied.
#[test]
fn host_side_recheck_refuses_after_revoke() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );

    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );
    assert!(
        wait_position_at_least(
            &mut controller,
            Duration::from_millis(1000),
            Duration::from_secs(2)
        ),
        "focus-a's own seek(1000) on grant must land"
    );
    let position_before = controller.position();

    // The user takes focus back — the same one UI frame's worth of race
    // this closes: a real plugin thread's `Gateway::admit` might have let
    // a `Seek` through a moment before this, still in flight when the
    // revoke landed.
    controller.focus_take_back();
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "take_back must revoke immediately"
    );

    let refusal = call(&mut controller, id_a, Request::Seek { position_ms: 9_999 }).expect_err(
        "a stale, already-non-holder envelope must be refused by the host's own re-check",
    );
    assert_eq!(refusal.code, RefusalCode::NoFocus);
    assert_eq!(refusal.reason, "no_focus");
    // Playback keeps advancing in real time between `position_before` and
    // here (the transport is still `Playing`) — the assertion that matters
    // is that the refused `Seek { position_ms: 9_999 }` never landed, not
    // that the clock stood still.
    let position_after = controller.position();
    assert!(
        position_after >= position_before
            && position_after < position_before + Duration::from_millis(500),
        "the host-side re-check must have had no side effect on the playhead: \
         before={position_before:?}, after={position_after:?}"
    );
    assert!(
        position_after < Duration::from_secs(5),
        "the refused seek to 9999ms must never have landed, got {position_after:?}"
    );
}

/// US1-2: the `L` path (`toggle_current_loop`) is a genuine local user
/// action — under the default Auto-on-interaction policy it both toggles
/// the region *and* returns focus to the host, with `focus_revoked`
/// delivered to the former holder before `loop_disarmed` (C3/FR-014).
#[test]
fn local_loop_toggle_applies_and_returns_focus_under_auto() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );

    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );
    assert!(
        wait_armed(&mut controller, Duration::from_secs(2)).is_some(),
        "focus-a must arm its own transient loop region on grant"
    );

    controller
        .toggle_current_loop()
        .unwrap_or_else(|e| unreachable!("toggling an armed region must succeed: {e:?}"));

    // `note_local_transport_action` runs synchronously inside
    // `disarm_loop`, so the revoke is immediate — no `tick()` needed to
    // observe it.
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "a local loop toggle must return focus to the host under Auto"
    );
    assert!(
        controller
            .markers()
            .and_then(|m| m.armed_region())
            .is_none(),
        "the region itself must actually be disarmed"
    );

    assert!(
        wait_for_log_contains(
            &mut controller,
            id_a,
            "loop_disarmed",
            Duration::from_secs(2)
        ),
        "focus-a must observe its own loop_disarmed"
    );
    let log = probe_log(&mut controller, id_a);
    let revoked_at = log
        .iter()
        .position(|l| l.contains("focus_revoked: holder=host"))
        .unwrap_or_else(|| unreachable!("focus-a's log must contain focus_revoked: {log:?}"));
    let disarmed_at = log
        .iter()
        .position(|l| l.contains("loop_disarmed"))
        .unwrap_or_else(|| unreachable!("focus-a's log must contain loop_disarmed: {log:?}"));
    assert!(
        revoked_at < disarmed_at,
        "focus_revoked must precede loop_disarmed (C3/FR-014), log: {log:?}"
    );
}

/// SC-006: a local host transport action never revokes under Manual or
/// First-request-wins — only Auto-on-interaction's own hook does that
/// (A6). The holder given by the user survives any number of local
/// transport commands under either of the other two policies.
#[test]
fn local_action_keeps_holder_under_manual_and_first_wins() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_b = controller_plugin_id(&mut controller, FOCUS_B);
    assert!(
        wait_active(&mut controller, id_b),
        "focus-b must reach Active"
    );

    controller.set_focus_policy(FocusPolicy::Manual);
    controller.focus_give(id_b);
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id_b)
    );

    controller.pause();
    controller.play();
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id_b),
        "a local transport action must never revoke under Manual"
    );

    controller.set_focus_policy(FocusPolicy::FirstRequestWins);
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id_b),
        "C7/A10: switching policy alone must never change the holder"
    );

    controller.pause();
    controller.play();
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id_b),
        "a local transport action must never revoke under First-request-wins"
    );
}

/// US1-3, EC-3.4: a remote controller's own command (`Input::
/// RemoteCommand`, never one of the six local transport inputs, C2)
/// applies to ModPlayer's own transport exactly like FR-017's existing
/// mirroring, but must never be mistaken for a *local* user action — the
/// holder is completely unaffected and the holder's own log shows only the
/// resulting `play_state_changed`, no `focus_revoked`.
#[test]
fn remote_pause_applies_without_focus_change() {
    let (mut controller, handle, _dir, _psd, _tsd) = controller_with_track();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );

    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    handle.remote(RemoteCommand::Pause);
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            c.transport_state().intent == Intent::Paused
        }),
        "a remote pause must apply to ModPlayer's own transport (FR-017)"
    );

    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id_a),
        "a remote command must never change transport focus (FR-002a, C2)"
    );
    let log = probe_log(&mut controller, id_a);
    assert!(
        !log.iter().any(|l| l.contains("focus_revoked")),
        "the holder must see no focus_revoked from a remote command, log: {log:?}"
    );
}

/// US1-4, SC-003: the holder is suspended (its own scripted hang on the
/// third `play_state_changed`, data-model.md §5) — within the same
/// suspension's own teardown (L7, C4), focus returns to the host and the
/// loop it had armed is released, with no user action required.
#[test]
fn suspended_holder_returns_to_host_and_disarms_loop() {
    let (mut controller, _handle, _dir, _psd, _tsd) = fixture_controller();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    // A tiny `share` budget makes the scripted hang's own suspension
    // deterministic and fast (mirrors every hang-fixture test in
    // controller_plugins_lifecycle.rs).
    if let Some(record) = controller.plugins_mut().record_mut(id_a) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );
    assert!(
        wait_armed(&mut controller, Duration::from_secs(2)).is_some(),
        "focus-a must arm its own transient loop region on grant"
    );

    // focus-a hangs on its *third* play_state_changed (data-model.md §5).
    for _ in 0..3 {
        fan_out_play_state(&mut controller, PlayState::Playing);
    }

    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            matches!(
                c.plugins_mut().record(id_a).map(|r| &r.lifecycle),
                Some(Lifecycle::Suspended { .. })
            )
        }),
        "the scripted hang must suspend focus-a"
    );

    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "a suspension's own teardown (C4) must return focus to the host"
    );
    assert!(
        controller
            .markers()
            .and_then(|m| m.armed_region())
            .is_none(),
        "the suspended holder's own armed (transient) region must be released"
    );
}

/// US1-5: disabling the holder runs the same fixed teardown (C4) as a
/// suspension — focus returns to the host and the armed loop is released,
/// without waiting for any runtime fault.
#[test]
fn disabled_holder_returns_to_host_and_disarms_loop() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );

    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );
    assert!(
        wait_armed(&mut controller, Duration::from_secs(2)).is_some(),
        "focus-a must arm its own transient loop region on grant"
    );

    controller.plugin_disable(id_a);
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            !matches!(
                c.plugins_mut().record(id_a).map(|r| &r.lifecycle),
                Some(Lifecycle::Active | Lifecycle::Loading)
            )
        }),
        "disable must run the plugin's teardown to completion"
    );

    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "disabling the holder (C4) must return focus to the host"
    );
    assert!(
        controller
            .markers()
            .and_then(|m| m.armed_region())
            .is_none(),
        "the disabled holder's own armed (transient) region must be released"
    );
}

/// FR-014: a "Give focus" from one plugin holder straight to another
/// delivers `focus_revoked` to the outgoing holder and `focus_granted` to
/// the incoming one — both real plugin threads actually observe their own
/// half of the same ordered `Vec<FocusChange>` (A11, unit-tested in
/// isolation by `focus_arbiter.rs`'s own `revoked_precede_granted`; this is
/// its end-to-end delivery proof).
#[test]
fn event_order_revoked_before_granted_on_plugin_to_plugin() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    let id_b = controller_plugin_id(&mut controller, FOCUS_B);
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    assert!(
        wait_active(&mut controller, id_b),
        "focus-b must reach Active"
    );

    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );
    assert!(
        wait_for_log_contains(
            &mut controller,
            id_a,
            "focus_granted",
            Duration::from_secs(2)
        ),
        "focus-a must observe its own focus_granted"
    );

    controller.focus_give(id_b);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_b),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-b"
    );

    assert!(
        wait_for_log_contains(
            &mut controller,
            id_a,
            &format!("focus_revoked: holder={FOCUS_B}"),
            Duration::from_secs(2)
        ),
        "the outgoing holder must observe focus_revoked{{holder=incoming}}"
    );
    assert!(
        wait_for_log_contains(
            &mut controller,
            id_b,
            &format!("focus_granted: holder={FOCUS_B}"),
            Duration::from_secs(2)
        ),
        "the incoming holder must observe its own focus_granted"
    );
}

/// FR-007 (second half), C5: a non-fault focus transition never touches
/// `TrackMarkers` — the user's own "Take back" (a voluntary revoke, not a
/// vacancy fault) leaves the former holder's armed loop exactly as it was.
#[test]
fn non_fault_transitions_keep_loop_armed() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );

    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );
    let region = wait_armed(&mut controller, Duration::from_secs(2))
        .unwrap_or_else(|| unreachable!("focus-a must arm its own transient loop region"));

    controller.focus_take_back();
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "take_back must revoke immediately"
    );

    let still_armed = controller
        .markers()
        .and_then(|m| m.armed_region())
        .map(|r| r.id);
    assert_eq!(
        still_armed,
        Some(region),
        "a non-fault focus transition (C5) must never touch TrackMarkers"
    );
}

// -----------------------------------------------------------------------
// Phase 4, User Story 2 (P2): the user picks how focus gets assigned.
// -----------------------------------------------------------------------

/// US2-1: under Manual, two plugins' `request_focus()` are both recorded
/// and neither is granted — the holder stays the host until the user
/// explicitly picks one via `focus_give`.
#[test]
fn manual_two_requests_both_pending_until_give() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    let id_b = controller_plugin_id(&mut controller, FOCUS_B);
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    assert!(
        wait_active(&mut controller, id_b),
        "focus-b must reach Active"
    );

    // RT5 (research R2): each fixture's own `ready_ack` dispatch (which
    // calls `request_focus()`) runs on its own thread strictly before it
    // ever posts the `Ready` runtime event this file's `wait_active`
    // polls for — so both fixtures have already requested by the time
    // both are observed `Active`, regardless of which policy governed
    // that request (A4: Auto never grants on request() either).
    controller.set_focus_policy(FocusPolicy::Manual);
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "US2-1: neither plugin is granted under Manual"
    );
    let view = controller.transport_focus_view();
    assert!(view.holder.is_none(), "the panel must show host as holder");
    for id in [id_a, id_b] {
        let row = view
            .rows
            .iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| unreachable!("{id:?} must be a Transport panel row"));
        assert!(
            row.request_order.is_some(),
            "US2-1: {id:?} must appear as requesting"
        );
        assert!(!row.holds);
    }

    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "the user's own Give focus must grant focus-a"
    );
    assert_eq!(
        controller.plugins_mut().arbiter().request_order(id_b),
        Some(1),
        "focus-b is still just listed as requesting, never auto-granted under Manual"
    );
}

/// US2-2: under Auto-on-interaction (the default) with no holder, the
/// user's own "Give focus" on a plugin's row grants it immediately.
#[test]
fn auto_give_focus_grants_immediately() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_b = controller_plugin_id(&mut controller, FOCUS_B);
    assert!(
        wait_active(&mut controller, id_b),
        "focus-b must reach Active"
    );
    assert_eq!(
        controller.focus_policy(),
        FocusPolicy::AutoOnInteraction,
        "US2-2 exercises the default policy"
    );
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "A4: request_focus() alone never grants under Auto, even by default"
    );

    controller.focus_give(id_b);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_b),
            Duration::from_secs(2)
        ),
        "US2-2: Give focus must grant immediately under Auto"
    );
    assert!(
        wait_for_log_contains(
            &mut controller,
            id_b,
            "focus_granted",
            Duration::from_secs(2)
        ),
        "focus-b must observe its own focus_granted"
    );
}

/// SC-005, US2-3/4: under First-request-wins, across 20 consecutive
/// "track loads" alternating which of two plugins requests first, the
/// first requester is granted every time and the other is recorded but
/// never granted while the first holds. Driven on
/// `bare_controller_with_track()` with fabricated `PluginId`s (see that
/// helper's own doc comment) so the request order is exact, not raced
/// against two real plugin threads' own concurrent re-requests.
#[test]
fn first_wins_a_then_b_over_twenty_tracks() {
    let (mut controller, _handle, _dir) = bare_controller_with_track();
    controller.set_focus_policy(FocusPolicy::FirstRequestWins);
    let id_a = PluginId(9_001);
    let id_b = PluginId(9_002);

    for i in 0..20u32 {
        controller.queue_replace(vec![track(&format!("sc005-{i}"))]);
        controller.play();
        controller.tick();
        assert_eq!(
            holder(&mut controller),
            FocusHolder::Host,
            "track {i}: a fresh track must reset the holder to host (FR-006)"
        );

        let (winner, loser) = if i % 2 == 0 {
            (id_a, id_b)
        } else {
            (id_b, id_a)
        };
        request(&mut controller, winner);
        assert_eq!(
            holder(&mut controller),
            FocusHolder::Plugin(winner),
            "track {i}: the first requester must be granted"
        );
        request(&mut controller, loser);
        assert_eq!(
            holder(&mut controller),
            FocusHolder::Plugin(winner),
            "track {i}: the later requester must never be granted while the first holds"
        );
        assert_eq!(
            request_order(&mut controller, loser),
            Some(1),
            "track {i}: the later requester is recorded, not granted"
        );
    }
}

/// US2-5, Clarifications: switching the active policy while a plugin
/// holds focus never touches the current holder; the new policy governs
/// only the next contention event on.
#[test]
fn policy_switch_leaves_holder() {
    let (mut controller, _handle, _dir) = bare_controller_with_track();
    let id_a = PluginId(1);
    let id_b = PluginId(2);

    controller.set_focus_policy(FocusPolicy::FirstRequestWins);
    request(&mut controller, id_a);
    assert_eq!(holder(&mut controller), FocusHolder::Plugin(id_a));

    controller.set_focus_policy(FocusPolicy::Manual);
    assert_eq!(
        holder(&mut controller),
        FocusHolder::Plugin(id_a),
        "A10/C7: switching policy alone must never change the holder"
    );

    // The *next* contention event is judged by the new (Manual) policy:
    // B's request is recorded, never auto-granted, and A keeps holding.
    request(&mut controller, id_b);
    assert_eq!(holder(&mut controller), FocusHolder::Plugin(id_a));
    assert_eq!(request_order(&mut controller, id_b), Some(1));

    // Switch once more, to Auto-on-interaction: a local host transport
    // action now revokes, proving this later policy governs from here.
    controller.set_focus_policy(FocusPolicy::AutoOnInteraction);
    assert_eq!(holder(&mut controller), FocusHolder::Plugin(id_a));
    controller.pause();
    assert_eq!(
        holder(&mut controller),
        FocusHolder::Host,
        "the newly active Auto policy's own local-action hook must now apply"
    );
}

/// US2-6: under First-request-wins, the user's own "Take back" locks the
/// host for the rest of the track — the earliest pending requester stays
/// listed as requesting, never auto-granted (contrast with a voluntary
/// `release`, proven by `first_wins_release_refills` right below).
#[test]
fn first_wins_take_back_no_refill() {
    let (mut controller, _handle, _dir) = bare_controller_with_track();
    controller.set_focus_policy(FocusPolicy::FirstRequestWins);
    let id_a = PluginId(1);
    let id_b = PluginId(2);

    request(&mut controller, id_a);
    assert_eq!(holder(&mut controller), FocusHolder::Plugin(id_a));
    request(&mut controller, id_b);
    assert_eq!(request_order(&mut controller, id_b), Some(1));

    controller.focus_take_back();
    assert_eq!(
        holder(&mut controller),
        FocusHolder::Host,
        "take_back must revoke immediately"
    );
    assert_eq!(
        request_order(&mut controller, id_b),
        Some(1),
        "US2-6: B stays listed as requesting, not auto-granted (no refill after take_back, A8)"
    );
}

/// US2-7, FR-006: under First-request-wins, the holder's own voluntary
/// `release_focus()` immediately refills from the earliest pending
/// requester, unlike a user "Take back" (`first_wins_take_back_no_
/// refill` above).
#[test]
fn first_wins_release_refills() {
    let (mut controller, _handle, _dir) = bare_controller_with_track();
    controller.set_focus_policy(FocusPolicy::FirstRequestWins);
    let id_a = PluginId(1);
    let id_b = PluginId(2);

    request(&mut controller, id_a);
    request(&mut controller, id_b);
    assert_eq!(request_order(&mut controller, id_b), Some(1));

    release(&mut controller, id_a);
    assert_eq!(
        holder(&mut controller),
        FocusHolder::Plugin(id_b),
        "US2-7: B must refill immediately on A's voluntary release"
    );
    assert_eq!(
        request_order(&mut controller, id_b),
        None,
        "B is the holder now, no longer merely pending"
    );
}

/// US2-8, SC-007, FR-012: the focus **policy** survives a relaunch (a
/// fresh controller built from the same `SettingsStore`'s file); the
/// holder and the pending queue never do — every relaunch starts with
/// the host holding focus and an empty queue.
#[test]
fn policy_persists_holder_does_not() {
    let dir = TempDir::new();
    let settings_path = dir.path().join("settings.toml");
    let devices = || vec![fake_device("dev-1", true)];

    // See `bare_controller`'s own doc comment: `PlaybackController::new`
    // reads the process-global `MODPLAYER_PLUGIN_FIXTURES`, so every
    // construction here must take this file's own `PLUGIN_ENV_LOCK`.
    let mut controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        PlaybackController::new(
            FakeBackend::new(devices()),
            ScriptedHost::new(),
            SettingsStore::with_path(&settings_path),
        )
    };
    controller.launch();
    controller.set_focus_policy(FocusPolicy::FirstRequestWins);
    request(&mut controller, PluginId(1));
    assert_eq!(
        holder(&mut controller),
        FocusHolder::Plugin(PluginId(1)),
        "give this session an actual holder, to prove it does NOT survive"
    );

    // "Relaunch": a brand-new controller reading the same settings.toml.
    let mut relaunched = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        PlaybackController::new(
            FakeBackend::new(devices()),
            ScriptedHost::new(),
            SettingsStore::with_path(&settings_path),
        )
    };
    relaunched.launch();

    assert_eq!(
        relaunched.focus_policy(),
        FocusPolicy::FirstRequestWins,
        "US2-8/SC-007: the chosen policy must be restored"
    );
    assert_eq!(
        holder(&mut relaunched),
        FocusHolder::Host,
        "US2-8/SC-007: the holder must never persist across a relaunch"
    );
    assert!(
        relaunched.plugins_mut().arbiter().pending().is_empty(),
        "US2-8/SC-007: the pending queue must never persist across a relaunch"
    );
}

/// Edge case (Clarifications): "Give focus" on a listed plugin that never
/// itself called `request_focus()` still grants it — `give()` never
/// requires a prior request.
#[test]
fn give_to_never_requested_plugin_delivers_granted() {
    let (mut controller, _handle, _dir) = bare_controller_with_track();
    let id = PluginId(5);
    assert_eq!(
        request_order(&mut controller, id),
        None,
        "this plugin must never have requested"
    );

    give(&mut controller, id);
    assert_eq!(
        holder(&mut controller),
        FocusHolder::Plugin(id),
        "give() must grant even a plugin that never requested"
    );
}

/// Edge case: `release_focus()` from a plugin that is merely pending (not
/// the holder) withdraws its request; it stops appearing as requesting
/// and the holder is unaffected (extends 009's no-op-when-not-holding
/// behavior).
#[test]
fn release_while_pending_withdraws_request() {
    let (mut controller, _handle, _dir) = bare_controller_with_track();
    controller.set_focus_policy(FocusPolicy::Manual);
    let id = PluginId(3);

    request(&mut controller, id);
    assert_eq!(request_order(&mut controller, id), Some(1));

    release(&mut controller, id);
    assert_eq!(
        request_order(&mut controller, id),
        None,
        "a pending release() must withdraw the request"
    );
    assert_eq!(holder(&mut controller), FocusHolder::Host);
}

/// FR-011: a pending requester that is disabled before ever being granted
/// has its request cleared and its row leaves the Transport panel; the
/// current holder is unaffected.
#[test]
fn pending_plugin_disabled_leaves_queue_and_panel() {
    let (mut controller, _handle, _dir, _psd, _tsd) = controller_with_track();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    let id_b = controller_plugin_id(&mut controller, FOCUS_B);
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    assert!(
        wait_active(&mut controller, id_b),
        "focus-b must reach Active"
    );

    // Give A the holder — B, already self-requested (RT5), remains
    // pending (this file's own `bare_controller`'s doc comment explains
    // why both are guaranteed to have already requested by now).
    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );
    assert_eq!(
        controller.plugins_mut().arbiter().request_order(id_b),
        Some(1),
        "focus-b must still be pending ahead of this test's own action"
    );

    controller.plugin_disable(id_b);
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            !matches!(
                c.plugins_mut().record(id_b).map(|r| &r.lifecycle),
                Some(Lifecycle::Active | Lifecycle::Loading)
            )
        }),
        "disable must run focus-b's teardown to completion"
    );

    assert_eq!(
        controller.plugins_mut().arbiter().request_order(id_b),
        None,
        "FR-011: the disabled pending requester's request must be cleared"
    );
    assert!(
        !controller
            .transport_focus_view()
            .rows
            .iter()
            .any(|r| r.id == id_b),
        "FR-011: the disabled plugin's row must leave the Transport panel"
    );
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id_a),
        "the unrelated holder must be unaffected"
    );
}

/// C10, Clarifications: `plugin_restart` never itself touches the
/// arbiter — a restarted former holder comes back as a plain observer
/// with no residual request, and is granted focus again only once it
/// calls `request_focus()` itself.
#[test]
fn restarted_plugin_is_observer() {
    let (mut controller, _handle, _dir, _psd, _tsd) = fixture_controller();
    let id_a = controller_plugin_id(&mut controller, FOCUS_A);
    // A tiny `share` budget makes the scripted hang's own suspension
    // deterministic and fast (mirrors `suspended_holder_returns_to_
    // host_and_disarms_loop` above).
    if let Some(record) = controller.plugins_mut().record_mut(id_a) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    controller.focus_give(id_a);
    assert!(
        wait_holder(
            &mut controller,
            FocusHolder::Plugin(id_a),
            Duration::from_secs(2)
        ),
        "focus_give must grant focus-a"
    );

    for _ in 0..3 {
        fan_out_play_state(&mut controller, PlayState::Playing);
    }
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            matches!(
                c.plugins_mut().record(id_a).map(|r| &r.lifecycle),
                Some(Lifecycle::Suspended { .. })
            )
        }),
        "the scripted hang must suspend focus-a"
    );
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "the suspension's own teardown (C4) must already have returned focus to the host"
    );

    controller.plugin_restart(id_a);
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "C10: plugin_restart itself must never touch the arbiter"
    );
    assert_eq!(
        controller.plugins_mut().arbiter().request_order(id_a),
        None,
        "C10: a freshly restarted plugin is a plain observer, no residual request"
    );

    assert!(
        wait_active(&mut controller, id_a),
        "restart must actually respawn focus-a"
    );
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            c.plugins_mut().arbiter().request_order(id_a).is_some()
        }),
        "the restarted plugin must re-request focus itself (Clarifications)"
    );
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "it is never re-granted automatically, only re-added to the queue"
    );
}

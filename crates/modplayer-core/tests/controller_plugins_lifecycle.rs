// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PlaybackController`/`PluginHost` lifecycle tests (US1,
//! contracts/plugin-host-service.md §2 "L" rules, §5, §6): a
//! hang/throw/leak/never-ready plugin is contained on its own thread,
//! the user is notified with Restart/Disable, three suspensions in a
//! session auto-disable it, and none of this ever touches audio or the
//! transport.
//!
//! Tests that exercise the controller *façade* itself
//! (`plugin_restart`/`plugin_disable`'s wiring, T074) drive a full
//! `PlaybackController<FakeBackend, ScriptedHost>`. Tests that only need
//! `PluginHost`'s own lifecycle/teardown/shutdown/sign-out mechanics (the
//! controller's own wrappers for these are simple pass-throughs, already
//! exercised by the façade tests above) construct a bare `PluginHost`
//! directly — no device, no queue, no env-var dance beyond the one
//! `MODPLAYER_PLUGIN_STATE_DIR` read `PluginStatePaths::resolve()` makes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::event::{HostEvent, PlayState, UnloadReason};
use modplayer_core::markers::{Owner, TrackMarkers};
use modplayer_core::notifications::{KEY_PLUGIN_AUTO_DISABLED, KEY_PLUGIN_SUSPENDED};
use modplayer_core::plugins::{Lifecycle, PluginHost, to_gateway_id};
use modplayer_core::settings::SettingsStore;
use modplayer_core::transport::Intent;
use modplayer_core::{ChainModel, Health, NotificationCenter, PlaybackController, PluginId};
use modplayer_effects::catalog::{NodeKind, NodeOwner};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_plugin_runtime::events::{RuntimeEvent, SuspendCause};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-plugins-lifecycle-{}-{}",
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

// -----------------------------------------------------------------------
// Bare `PluginHost` harness (no device, no env var beyond the plugin-state
// dir): used by every test that only needs lifecycle/teardown mechanics.
// -----------------------------------------------------------------------

/// `MODPLAYER_PLUGIN_STATE_DIR` is process-global (`std::env`), so every
/// `PluginHost::discover` in this binary must be serialized against every
/// other's brief mutation of it (mirrors `controller_markers.rs`'s own
/// `MODPLAYER_TRACK_STATE_DIR` pattern).
static PLUGIN_STATE_ENV_LOCK: Mutex<()> = Mutex::new(());

/// A `PluginHost` that has discovered every `plugins/fixtures/` package
/// (`fixtures_enabled = true`), backed by a private temp directory rather
/// than the real per-user plugin-state directory.
fn fixture_host() -> (PluginHost, TempDir) {
    let dir = TempDir::new();
    let host = {
        let _guard = PLUGIN_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: narrowly scopes the mutation to the one synchronous
        // read `PluginStatePaths::resolve()` makes inside `discover()`,
        // serialized against every other test in this binary via the
        // lock above.
        unsafe { std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", dir.path()) };
        let host = PluginHost::discover(true);
        unsafe { std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR") };
        host
    };
    (host, dir)
}

fn find_id(host: &PluginHost, identifier: &str) -> PluginId {
    host.records()
        .iter()
        .find(|r| r.identifier.as_str() == identifier)
        .map(|r| r.id)
        .unwrap_or_else(|| unreachable!("fixture '{identifier}' must be discovered"))
}

/// Spawn `id`'s `PluginHandle` against a fresh, throwaway `RtShared` —
/// none of this file's fixtures read real-time engine state.
fn spawn(host: &mut PluginHost, id: PluginId) {
    let shared = Arc::new(modplayer_engine::RtShared::new());
    host.spawn(id, &shared);
}

/// Deliver one `play_state_changed(playing)` straight to `host`'s
/// `playback.observe` holders — every US1 fixture's own trigger.
fn trigger_hang(host: &mut PluginHost) {
    host.fan_out(
        &HostEvent::PlayStateChanged {
            state: PlayState::Playing,
        },
        Instant::now(),
    );
}

/// Drain runtime events (L4/L5/L6) until `done` is true or `timeout`
/// elapses. `notifications`/`chain` are the caller's own state, threaded
/// through exactly like `PlaybackController::tick()` does.
fn pump_until(
    host: &mut PluginHost,
    notifications: &mut NotificationCenter,
    chain: &mut ChainModel,
    timeout: Duration,
    mut done: impl FnMut(&mut PluginHost) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        host.drain_plugin_runtime_events(notifications, chain, None, Instant::now());
        if done(host) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn is_active(host: &mut PluginHost, id: PluginId) -> bool {
    matches!(
        host.record(id).map(|r| &r.lifecycle),
        Some(Lifecycle::Active)
    )
}

fn is_suspended(host: &mut PluginHost, id: PluginId) -> bool {
    matches!(
        host.record(id).map(|r| &r.lifecycle),
        Some(Lifecycle::Suspended { .. })
    )
}

/// RT5's `ready_ack` is dispatched entirely on the plugin's own thread
/// (research R2) before it can process anything else — from the host's
/// side, the observable proof is that the very first `RuntimeEvent` a
/// freshly spawned plugin ever posts is `Ready`, never a stray `Log` or
/// abort ahead of it.
#[test]
fn ready_ack_is_first_event() {
    let (mut host, _dir) = fixture_host();
    let id = find_id(&host, "org.modplayer.fixture.hang");
    assert!(matches!(
        host.record(id).unwrap_or_else(|| unreachable!()).lifecycle,
        Lifecycle::Disabled
    ));
    spawn(&mut host, id);

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut first = None;
    while first.is_none() {
        if let Some((event_id, event)) = host.try_recv_event() {
            if event_id == id {
                first = Some(event);
            }
        } else if Instant::now() >= deadline {
            break;
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert!(
        matches!(first, Some(RuntimeEvent::Ready)),
        "the first event a freshly loaded plugin posts must be Ready, got {first:?}"
    );
}

/// L5/L6: a plugin that never calls `api.ready()` is suspended as
/// `DidNotStart` once its `ready_timeout` budget elapses, offered
/// Restart; `plugin_restart` (T074) moves it back to `Loading` and
/// dismisses the notice.
#[test]
fn never_ready_suspended_with_restart() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    let id = controller_plugin_id(&mut controller, "org.modplayer.fixture.noready");
    if let Some(record) = controller.plugins_mut().record_mut(id) {
        record.budgets = Budgets {
            ready_timeout: Duration::from_millis(80),
            ..Budgets::DEFAULT
        };
    }
    controller.launch();

    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            matches!(
                c.plugins_mut().record(id).map(|r| &r.lifecycle),
                Some(Lifecycle::Suspended { .. })
            )
        }),
        "a plugin that never calls ready() must eventually be suspended"
    );
    assert!(matches!(
        controller
            .plugins_mut()
            .record(id)
            .unwrap_or_else(|| unreachable!())
            .lifecycle,
        Lifecycle::Suspended {
            cause: SuspendCause::DidNotStart
        }
    ));
    assert!(
        controller
            .notifications()
            .visible()
            .any(|n| n.message_key == KEY_PLUGIN_SUSPENDED),
        "a suspension must raise plugin-suspended"
    );

    assert!(wait_for_reaped(&mut controller, id, Duration::from_secs(1)));
    controller.plugin_restart(id);
    assert!(matches!(
        controller
            .plugins_mut()
            .record(id)
            .unwrap_or_else(|| unreachable!())
            .lifecycle,
        Lifecycle::Loading
    ));
    assert!(
        !controller
            .notifications()
            .visible()
            .any(|n| n.message_key == KEY_PLUGIN_SUSPENDED),
        "restart must dismiss the plugin-suspended notice"
    );
}

/// L6: the third suspension within a session auto-disables the plugin
/// and replaces the `plugin-suspended` notice with `plugin-auto-
/// disabled` (no actions) — `share` is overridden tiny so a single
/// handler abort (≈4ms) already blows it, making each round
/// deterministic and fast.
#[test]
fn three_suspensions_auto_disable() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    let id = controller_plugin_id(&mut controller, "org.modplayer.fixture.hang");
    if let Some(record) = controller.plugins_mut().record_mut(id) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    controller.launch();

    for round in 1..=3u8 {
        assert!(
            pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
                matches!(
                    c.plugins_mut().record(id).map(|r| &r.lifecycle),
                    Some(Lifecycle::Active)
                )
            }),
            "round {round}: the plugin must be Active before it can be made to hang"
        );
        trigger_hang_via_controller(&mut controller);
        assert!(
            pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
                matches!(
                    c.plugins_mut().record(id).map(|r| &r.lifecycle),
                    Some(Lifecycle::Suspended { .. })
                )
            }),
            "round {round}: the hang must suspend the plugin"
        );
        assert_eq!(
            controller
                .plugins_mut()
                .record(id)
                .unwrap_or_else(|| unreachable!())
                .suspensions_this_session,
            round
        );
        if round < 3 {
            assert!(wait_for_reaped(&mut controller, id, Duration::from_secs(1)));
            controller.plugin_restart(id);
        }
    }

    let record = controller
        .plugins_mut()
        .record(id)
        .unwrap_or_else(|| unreachable!());
    assert!(
        !record.enabled,
        "the 3rd suspension must auto-disable the plugin"
    );
    assert!(
        controller
            .notifications()
            .visible()
            .any(|n| n.message_key == KEY_PLUGIN_AUTO_DISABLED),
        "the 3rd suspension must raise plugin-auto-disabled"
    );
    assert!(
        !controller
            .notifications()
            .visible()
            .any(|n| n.message_key == KEY_PLUGIN_SUSPENDED),
        "plugin-auto-disabled must replace plugin-suspended, not stack alongside it"
    );
}

/// `plugin_restart` moves `Suspended -> Loading` but must **not** reset
/// `suspensions_this_session` (contracts/plugin-host-service.md §1) — the
/// counter keeps accumulating across restarts within the same session.
#[test]
fn restart_keeps_session_counter() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    let id = controller_plugin_id(&mut controller, "org.modplayer.fixture.hang");
    if let Some(record) = controller.plugins_mut().record_mut(id) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    controller.launch();

    let suspend_once = |controller: &mut PlaybackController<FakeBackend, ScriptedHost>| {
        assert!(pump_controller_until(
            controller,
            Duration::from_secs(2),
            |c| matches!(
                c.plugins_mut().record(id).map(|r| &r.lifecycle),
                Some(Lifecycle::Active)
            )
        ));
        trigger_hang_via_controller(controller);
        assert!(pump_controller_until(
            controller,
            Duration::from_secs(2),
            |c| matches!(
                c.plugins_mut().record(id).map(|r| &r.lifecycle),
                Some(Lifecycle::Suspended { .. })
            )
        ));
    };

    suspend_once(&mut controller);
    assert_eq!(
        controller
            .plugins_mut()
            .record(id)
            .unwrap_or_else(|| unreachable!())
            .suspensions_this_session,
        1
    );

    assert!(wait_for_reaped(&mut controller, id, Duration::from_secs(1)));
    controller.plugin_restart(id);
    assert_eq!(
        controller
            .plugins_mut()
            .record(id)
            .unwrap_or_else(|| unreachable!())
            .suspensions_this_session,
        1,
        "restart must not reset the session suspension counter"
    );

    suspend_once(&mut controller);
    assert_eq!(
        controller
            .plugins_mut()
            .record(id)
            .unwrap_or_else(|| unreachable!())
            .suspensions_this_session,
        2,
        "the counter must keep accumulating across restarts"
    );
}

/// L7 (FR-012, fixed order; 010-transport-focus C4): `arbiter.vacate(id,
/// Fault)` (applied via `apply_focus_changes`) → `disarm_if_owned_by` →
/// `remove_transient_owned_by` → `orphan_owned_by`. Since `ArmLoop`/
/// `CreateMarker`/`CreateNode`'s own request-application lands with
/// US2/US3, this drives the same owned-mutation model methods directly
/// (the ones `apply.rs` will call later) to set the plugin up as the
/// owner of a loop, a transient marker and a node before tearing it down.
#[test]
fn disable_runs_teardown_in_order() {
    let (mut host, _dir) = fixture_host();
    let id = find_id(&host, "org.modplayer.fixture.hang");
    spawn(&mut host, id);
    let mut notifications = NotificationCenter::new();
    let mut chain = ChainModel::new(44_100);
    assert!(pump_until(
        &mut host,
        &mut notifications,
        &mut chain,
        Duration::from_secs(2),
        |h| is_active(h, id)
    ));

    // 010-transport-focus (research R1): the arbiter, not a CAS on the
    // token, is the only decision-maker now — grant this plugin focus
    // via a host "Give focus" (works under any policy, A5) so the L7a
    // assertion below has something real to release.
    let changes = host.arbiter_mut().give(id);
    host.apply_focus_changes(changes);
    assert_eq!(
        host.focus().holder(),
        Some(to_gateway_id(id)),
        "focus must be held by this plugin at the start of this test"
    );

    let mut markers = TrackMarkers::new(
        TrackId::new("spotify:track:a").unwrap_or_else(|_| unreachable!()),
        44_100,
        44_100 * 180,
    );
    let region = markers
        .new_loop_region_owned(1_000, 5_000, Owner::Plugin(id), false)
        .unwrap_or_else(|e| unreachable!("new_loop_region_owned: {e:?}"));
    markers
        .arm(region)
        .unwrap_or_else(|e| unreachable!("arm: {e:?}"));
    let transient = markers
        .add_point_owned(2_000, Owner::Plugin(id), true)
        .unwrap_or_else(|e| unreachable!("add_point_owned: {e:?}"));

    let (node_id, _) = chain
        .add(NodeKind::Gain, NodeOwner::Plugin(id))
        .unwrap_or_else(|e| unreachable!("chain.add: {e:?}"));

    host.stop(id, UnloadReason::Disable, Some(&mut markers), &mut chain);

    assert_eq!(
        host.focus().holder(),
        None,
        "L7a: focus must be released (arbiter.vacate(id, Fault) first)"
    );
    assert!(
        markers.armed_region().is_none(),
        "L7b: the loop region this plugin owned must be disarmed"
    );
    assert!(
        markers.owner_of(transient).is_none(),
        "L7c: the transient marker this plugin owned must be removed"
    );
    assert_eq!(
        chain
            .nodes()
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.orphaned),
        Some(true),
        "L7e: the plugin's node must be orphaned, not removed — it keeps running"
    );
}

/// L4: `chain.readopt(id)` re-adopts a plugin's own orphaned nodes (ids
/// and owner unchanged) the moment it becomes `Ready` again — the mirror
/// of `disable_runs_teardown_in_order`'s L7e orphan.
#[test]
fn suspended_readopts_on_ready() {
    let (mut host, _dir) = fixture_host();
    let id = find_id(&host, "org.modplayer.fixture.hang");
    if let Some(record) = host.record_mut(id) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    spawn(&mut host, id);
    let mut notifications = NotificationCenter::new();
    let mut chain = ChainModel::new(44_100);
    assert!(pump_until(
        &mut host,
        &mut notifications,
        &mut chain,
        Duration::from_secs(2),
        |h| is_active(h, id)
    ));

    let (node_id, _) = chain
        .add(NodeKind::Gain, NodeOwner::Plugin(id))
        .unwrap_or_else(|e| unreachable!("chain.add: {e:?}"));

    trigger_hang(&mut host);
    assert!(pump_until(
        &mut host,
        &mut notifications,
        &mut chain,
        Duration::from_secs(2),
        |h| is_suspended(h, id)
    ));
    assert_eq!(
        chain
            .nodes()
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.orphaned),
        Some(true),
        "the suspension's own L7e teardown must orphan the node first"
    );

    // Wait for the old thread to actually finish (L8) before respawning
    // — `spawn` is a no-op while a handle is still attached.
    let reaped = {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            host.reap_plugin_threads();
            if host.record(id).is_some_and(|r| r.handle.is_none()) {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    assert!(reaped, "the suspended plugin's thread must exit");

    // Mirrors `PlaybackController::plugin_restart`'s own dismiss+respawn
    // (T074, exercised directly by `restart_keeps_session_counter`) —
    // only the L4 readopt this test is about needs driving here.
    if let Some(record) = host.record_mut(id) {
        record.lifecycle = Lifecycle::Disabled;
    }
    spawn(&mut host, id);
    assert!(pump_until(
        &mut host,
        &mut notifications,
        &mut chain,
        Duration::from_secs(2),
        |h| is_active(h, id)
    ));

    assert_eq!(
        chain
            .nodes()
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.orphaned),
        Some(false),
        "L4: a plugin's own node must be re-adopted once it is Ready again"
    );
}

/// `shutdown()`'s plugin half (contracts/plugin-host-service.md §1):
/// every running plugin is stopped and the wait for every thread to
/// actually exit is bounded at `SHUTDOWN_WAIT` (250 ms) — a hung plugin
/// never blocks it.
#[test]
fn shutdown_delivers_unloading_and_bounds_wait() {
    let (mut host, _dir) = fixture_host();
    let id = find_id(&host, "org.modplayer.fixture.hang");
    spawn(&mut host, id);
    let mut notifications = NotificationCenter::new();
    let mut chain = ChainModel::new(44_100);
    assert!(pump_until(
        &mut host,
        &mut notifications,
        &mut chain,
        Duration::from_secs(2),
        |h| is_active(h, id)
    ));

    let start = Instant::now();
    host.stop_all_for_shutdown(None, &mut chain);
    let deadline = start + Duration::from_millis(250);
    loop {
        host.reap_plugin_threads();
        if !host.any_thread_running() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let elapsed = start.elapsed();

    assert!(
        !host.any_thread_running(),
        "every plugin thread must have exited within the bounded wait"
    );
    assert!(
        elapsed <= Duration::from_millis(250) + Duration::from_millis(150),
        "shutdown's own wait must be bounded at ~250ms, took {elapsed:?}"
    );
}

/// FR-014: sign-out flushes and drops every plugin's in-memory track
/// scope and deletes its on-disk `tracks/` directory; `plugin.json`
/// (`state.plugin`) survives untouched.
#[test]
fn sign_out_clears_track_state_only() {
    let (mut host, _dir) = fixture_host();
    let id = find_id(&host, "org.modplayer.fixture.hang");
    spawn(&mut host, id);
    let mut notifications = NotificationCenter::new();
    let mut chain = ChainModel::new(44_100);
    assert!(pump_until(
        &mut host,
        &mut notifications,
        &mut chain,
        Duration::from_secs(2),
        |h| is_active(h, id)
    ));

    let identifier = host
        .record(id)
        .unwrap_or_else(|| unreachable!())
        .identifier
        .to_string();
    let paths = host
        .paths()
        .unwrap_or_else(|| unreachable!("MODPLAYER_PLUGIN_STATE_DIR must resolve"));
    let plugin_file = paths.plugin_file(&identifier);
    let track_file = paths.track_file(&identifier, "spotify:track:a");
    std::fs::create_dir_all(plugin_file.parent().unwrap_or_else(|| unreachable!()))
        .unwrap_or_else(|e| unreachable!("{e}"));
    std::fs::write(&plugin_file, b"{}").unwrap_or_else(|e| unreachable!("{e}"));
    std::fs::create_dir_all(track_file.parent().unwrap_or_else(|| unreachable!()))
        .unwrap_or_else(|e| unreachable!("{e}"));
    std::fs::write(&track_file, b"{}").unwrap_or_else(|e| unreachable!("{e}"));

    host.clear_track_state_for_sign_out();

    assert!(
        !track_file.exists(),
        "FR-014: the per-track scope must be deleted on sign-out"
    );
    assert!(
        plugin_file.exists(),
        "FR-014: state.plugin must survive sign-out untouched"
    );
}

/// L5: three `HandlerAborted`s within the trailing 60 s open a 5-minute
/// `Health::Warning` window (never a suspension by themselves — the
/// default `share` budget easily absorbs 3×4ms) — and it clears again on
/// its own past 5 minutes.
#[test]
fn warning_after_three_aborts_and_clears_after_5min() {
    let (mut host, _dir) = fixture_host();
    let id = find_id(&host, "org.modplayer.fixture.hang");
    spawn(&mut host, id);
    let mut notifications = NotificationCenter::new();
    let mut chain = ChainModel::new(44_100);
    assert!(pump_until(
        &mut host,
        &mut notifications,
        &mut chain,
        Duration::from_secs(2),
        |h| is_active(h, id)
    ));

    for expected in 1..=3usize {
        trigger_hang(&mut host);
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            host.drain_plugin_runtime_events(&mut notifications, &mut chain, None, Instant::now());
            if host.record(id).map(|r| r.abort_window.len()).unwrap_or(0) >= expected {
                break;
            }
            assert!(Instant::now() < deadline, "abort #{expected} never arrived");
            std::thread::sleep(Duration::from_millis(2));
        }
        // Must not have suspended — the default share (100ms) absorbs
        // 3 aborts of ~4ms each comfortably.
        assert!(is_active(&mut host, id));
    }

    let now = Instant::now();
    assert_eq!(
        host.record(id)
            .unwrap_or_else(|| unreachable!())
            .derive_health(now),
        Some(Health::Warning),
        "the 3rd abort within 60s must open a 5-minute warning window"
    );

    let later = now + Duration::from_secs(5 * 60 + 1);
    assert_eq!(
        host.record(id)
            .unwrap_or_else(|| unreachable!())
            .derive_health(later),
        Some(Health::Ok),
        "the warning window must clear on its own once 5 minutes have passed"
    );
}

// -----------------------------------------------------------------------
// Full `PlaybackController` harness: only for tests that drive the
// façade methods (`plugin_restart`) or need real audio (`FakeBackend`).
// -----------------------------------------------------------------------

/// `MODPLAYER_PLUGIN_FIXTURES`/`MODPLAYER_PLUGIN_STATE_DIR`/
/// `MODPLAYER_TRACK_STATE_DIR` are all process-global, so every
/// `PlaybackController::new` in this binary must be serialized against
/// every other's brief mutation of them (mirrors `controller_markers.
/// rs`'s own pattern, extended to the two 009 variables it doesn't use).
static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

/// A controller that will discover every `plugins/fixtures/` package —
/// **not yet `launch()`ed**, so a caller can override a fixture's
/// `Budgets` (via `plugins_mut().record_mut(id)`) before its thread ever
/// spawns.
fn fixture_controller() -> (
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

fn trigger_hang_via_controller(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) {
    let now = controller.now();
    controller.plugins_mut().fan_out(
        &HostEvent::PlayStateChanged {
            state: PlayState::Playing,
        },
        now,
    );
}

/// `plugin_restart`/`plugin_enable`'s `spawn` is a no-op while a record
/// still has an attached (not-yet-reaped) handle (L8) — a suspended
/// plugin's own OS thread can take a tick or two past the `Suspended`
/// event itself to actually finish and be joined. Callers that want to
/// restart right after detecting a suspension wait for this first, or
/// risk a silent no-op that this file's own `never_ready_suspended_with_
/// restart` failure (fixed here) demonstrated directly.
fn wait_for_reaped(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
    timeout: Duration,
) -> bool {
    pump_controller_until(controller, timeout, |c| {
        c.plugins_mut()
            .record(id)
            .is_some_and(|r| r.handle.is_none())
    })
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

/// SC-001/SC-009 end to end: while the hang fixture is repeatedly aborted
/// and finally suspended, a second thread pulling `FakeBackend::
/// render_buffers` never misses a beat (each pull returns promptly) and
/// `transport_enabled()`/the transport `Intent` never change because of
/// it — the plugin's own thread is fully isolated from the audio path
/// (Constitution I).
#[test]
fn hang_fixture_suspended_audio_continues() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    let id = controller_plugin_id(&mut controller, "org.modplayer.fixture.hang");
    if let Some(record) = controller.plugins_mut().record_mut(id) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.tick();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    assert!(
        controller.transport_enabled(),
        "test setup must reach transport_enabled"
    );

    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(2),
        |c| matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )
    ));

    let controller = Arc::new(Mutex::new(controller));
    let render_ticks = Arc::new(AtomicU64::new(0));
    let stop_rendering = Arc::new(AtomicBool::new(false));

    let render_handle = {
        let controller = Arc::clone(&controller);
        let render_ticks = Arc::clone(&render_ticks);
        let stop_rendering = Arc::clone(&stop_rendering);
        std::thread::spawn(move || {
            while !stop_rendering.load(Ordering::SeqCst) {
                let iter_start = Instant::now();
                {
                    let mut c = controller.lock().unwrap_or_else(PoisonError::into_inner);
                    let _ = c.backend_mut().render_buffers(1);
                }
                render_ticks.fetch_add(1, Ordering::SeqCst);
                assert!(
                    iter_start.elapsed() <= Duration::from_millis(500),
                    "render_buffers(1) must never be blocked anywhere near this long by \
                     a plugin's own hang"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        })
    };

    {
        let mut c = controller.lock().unwrap_or_else(PoisonError::into_inner);
        let now = c.now();
        c.plugins_mut().fan_out(
            &HostEvent::PlayStateChanged {
                state: PlayState::Playing,
            },
            now,
        );
    }

    let suspended = {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut c = controller.lock().unwrap_or_else(PoisonError::into_inner);
            c.tick();
            let done = matches!(
                c.plugins_mut().record(id).map(|r| &r.lifecycle),
                Some(Lifecycle::Suspended { .. })
            );
            assert!(
                c.transport_enabled(),
                "transport must stay enabled while a plugin is being contained"
            );
            drop(c);
            if done {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    assert!(suspended, "the hang fixture must suspend");

    // Let the render thread keep making progress a little longer, past
    // the suspension itself, before joining it.
    std::thread::sleep(Duration::from_millis(50));
    stop_rendering.store(true, Ordering::SeqCst);
    render_handle
        .join()
        .unwrap_or_else(|_| unreachable!("the render thread must never panic"));

    let mut c = controller.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        c.transport_enabled(),
        "transport must remain enabled after the plugin is contained"
    );
    assert_eq!(c.transport_state().intent, Intent::Playing);
    let output = c.backend_mut().render_buffers(2);
    assert!(
        output.iter().any(|s| s.abs() > 1e-6),
        "audio must still be audible after the plugin suspended"
    );
    assert!(
        render_ticks.load(Ordering::SeqCst) > 0,
        "the render thread must have made progress throughout"
    );
}

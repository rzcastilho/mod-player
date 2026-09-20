// SPDX-License-Identifier: MIT OR Apache-2.0

//! Least-privilege enforcement tests (US2, spec.md priority P2,
//! contracts/plugin-api-v1.md §2 "Check order: permission → transport
//! focus → rate limit → per-call validation"; contracts/plugin-host-
//! service.md §3, §6): an ungranted request is refused with no host-side
//! effect, ownership is checked before existence is conflated with it,
//! transport focus is single-holder, and the rate limiter caps a
//! flooding plugin.
//!
//! Two harnesses:
//! - The structural/gateway-level checks (`matrix_…`,
//!   `arm_loop_permission_before_focus`) construct a bare
//!   `modplayer_capability_gateway::gateway::Gateway` from a real
//!   fixture's parsed `Grants` — the same object a plugin's own thread
//!   would hold — without spawning anything.
//! - The ownership/focus/rate-limit tests that exercise `plugins::apply`
//!   itself (`not_owner_vs_not_found`, `request_focus_contention_
//!   invalid_state`, `set_cue_ownership`) inject a synthetic
//!   `RpcEnvelope` straight into `PluginHost::debug_requests_sender()`
//!   (a test-support hook, mirrors `PlaybackController::debug_inject_
//!   engine_event`) so they can drive `apply.rs`'s own logic for two
//!   distinct plugin identities without a real running Lua thread on
//!   either side — permission/focus/rate admission is the gateway
//!   crate's own job (already covered by its `tests/gateway.rs`), not
//!   this crate's.
//! - `rate_limit_1000_seeks` is the one fully end-to-end test: it spawns
//!   the real `flood` fixture and reads the rate limiter's effect back
//!   through the plugin's own console log.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use modplayer_audio_io::FakeBackend;
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_capability_gateway::api::{Permission, RequestKind};
use modplayer_capability_gateway::focus::FocusToken;
use modplayer_capability_gateway::gateway::Gateway;
use modplayer_capability_gateway::refusal::RefusalCode;
use modplayer_capability_gateway::request::{
    MarkerId as GatewayMarkerId, ParamArg, ParamRef, QueueItemId as GatewayQueueItemId, Request,
    Response,
};
use modplayer_core::PlaybackController;
use modplayer_core::markers::Owner;
use modplayer_core::plugins::{Lifecycle, PluginHost, PluginId, to_gateway_id};
use modplayer_core::settings::SettingsStore;
use modplayer_core::transport::Intent;
use modplayer_effects::catalog::{self, NodeKind, NodeOwner};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_plugin_runtime::handle::RpcEnvelope;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-plugins-permissions-{}-{}",
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

fn fake_device(id: &str, is_default: bool) -> modplayer_audio_io::FakeDevice {
    modplayer_audio_io::FakeDevice {
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
// Bare `PluginHost` harness — for the gateway-level structural checks,
// which need a real fixture's parsed `Grants` but nothing else running.
// -----------------------------------------------------------------------

static PLUGIN_STATE_ENV_LOCK: Mutex<()> = Mutex::new(());

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

// -----------------------------------------------------------------------
// Full `PlaybackController` harness — for `apply.rs`'s own ownership/
// focus logic (via envelope injection) and the one end-to-end rate-limit
// test.
// -----------------------------------------------------------------------

static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

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

/// A controller with a current track (so `markers()` and `transport_
/// enabled()` are both live) — every test that exercises owned-marker or
/// transport-focus requests needs this.
fn controller_with_track() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
) {
    let (mut controller, dir, psd, tsd) = fixture_controller();
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    (controller, dir, psd, tsd)
}

/// Submits `request` from `plugin` straight to `drain_plugin_requests()`
/// (C1), bypassing the gateway's own admission entirely — the test-
/// support hook `PluginHost::debug_requests_sender()` this relies on is
/// never reached by any production code path.
fn call(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    plugin: PluginId,
    request: Request,
) -> Result<Response, modplayer_capability_gateway::refusal::Refusal> {
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

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

/// Spec US2 acceptance scenario 1: every `RequestKind` the observer
/// fixture (`playback.observe` only) was not granted is refused
/// `permission_denied`/`not_granted` at `Gateway::admit` — G1 means this
/// is a structural guarantee (no `RpcEnvelope` is ever sent for a
/// refused call, so it has no host-side effect by construction) — plus
/// one empirical, end-to-end proof for the one call the fixture's own
/// script actually makes.
#[test]
fn matrix_every_request_without_its_permission_is_denied_with_no_effect() {
    let (host, _dir) = fixture_host();
    let id = find_id(&host, "org.modplayer.fixture.observer");
    let grants = host.record(id).unwrap_or_else(|| unreachable!()).grants;
    let mut gateway = Gateway::new(to_gateway_id(id), grants, FocusToken::new());
    for kind in RequestKind::ALL {
        let Some(permission) = kind.requires() else {
            continue;
        };
        if grants.holds(permission) {
            continue;
        }
        let refusal = gateway
            .admit(kind, Instant::now())
            .expect_err(&format!("{kind:?} must be refused without its permission"));
        assert_eq!(
            refusal.code,
            RefusalCode::PermissionDenied,
            "{kind:?} must refuse permission_denied"
        );
        assert_eq!(refusal.reason, "not_granted", "{kind:?}");
    }

    // Empirical proof for the one call the fixture's own script makes
    // (`transport.seek(0)` on `ready_ack`): the real plugin thread's own
    // `Gateway` refuses it the same way, and no host-side model moved.
    let (mut controller, _dir3, _psd, _tsd) = fixture_controller();
    let id = controller_plugin_id(&mut controller, "org.modplayer.fixture.observer");
    controller.launch();
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            matches!(
                c.plugins_mut().record(id).map(|r| &r.lifecycle),
                Some(Lifecycle::Active)
            )
        }),
        "the observer fixture must reach Active"
    );
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(1), |c| {
            c.plugin_log()
                .entries()
                .any(|e| e.message.contains("seek refused"))
        }),
        "the observer fixture must log its own seek refusal"
    );
    assert_eq!(
        controller.transport_state().intent,
        Intent::Stopped,
        "the refused seek must have had no transport side effect"
    );
    assert!(
        controller.markers().is_none(),
        "the refused seek must have created no marker state"
    );
}

/// C2 (FR-008): an id that does not exist is `not_found` even for a
/// plugin that owns nothing; an id that exists but belongs to a
/// different plugin is `not_owner`.
#[test]
fn not_owner_vs_not_found() {
    let (mut controller, _dir, _psd, _tsd) = controller_with_track();
    let owner = controller_plugin_id(&mut controller, "org.modplayer.fixture.observer");
    let other = controller_plugin_id(&mut controller, "org.modplayer.fixture.flood");

    let response = call(
        &mut controller,
        owner,
        Request::SetCue {
            slot: 1,
            position_ms: 1_000,
        },
    )
    .unwrap_or_else(|e| unreachable!("owner's own SetCue must succeed: {e:?}"));
    let Response::MarkerId(marker_id) = response else {
        unreachable!("SetCue must return a MarkerId, got {response:?}");
    };

    let not_owner = call(
        &mut controller,
        other,
        Request::MoveMarker {
            id: marker_id,
            position_ms: 2_000,
        },
    )
    .expect_err("a different plugin must not be able to move this cue");
    assert_eq!(not_owner.code, RefusalCode::PermissionDenied);
    assert_eq!(not_owner.reason, "not_owner");

    let not_found = call(
        &mut controller,
        owner,
        Request::MoveMarker {
            id: GatewayMarkerId(999_999),
            position_ms: 0,
        },
    )
    .expect_err("an id that was never allocated must be not_found");
    assert_eq!(not_found.code, RefusalCode::NotFound);
    assert_eq!(not_found.reason, "unknown_id");
}

/// G2 (Gateway::admit's own fixed order), demonstrated end to end
/// against a real fixture's parsed `Grants`: `ArmLoop` requires
/// `markers.write`, which the observer fixture was never granted, so it
/// is refused `permission_denied` even though it also does not hold
/// transport focus — proving permission is checked first.
#[test]
fn arm_loop_permission_before_focus() {
    let (host, _dir) = fixture_host();
    let id = find_id(&host, "org.modplayer.fixture.observer");
    let grants = host.record(id).unwrap_or_else(|| unreachable!()).grants;
    let mut gateway = Gateway::new(to_gateway_id(id), grants, FocusToken::new());

    let refusal = gateway
        .admit(RequestKind::TransportArmLoop, Instant::now())
        .expect_err("ArmLoop without markers.write must be refused");
    assert_eq!(refusal.code, RefusalCode::PermissionDenied);
    assert_eq!(
        refusal.reason, "not_granted",
        "permission must be checked before focus (G2)"
    );
}

/// 010-transport-focus (C1, FR-003/FR-011, research R10 supersedes 009's
/// `request_focus_contention_invalid_state`): `request_focus()` is a
/// plain RPC to core — always `Ok`, recorded whether or not another
/// plugin already holds or is pending; the arbiter, not this dispatch
/// layer, decides who is actually granted. Under the default
/// `AutoOnInteraction` policy neither request auto-grants (A4);
/// `release_focus()` withdraws a pending request just as unconditionally.
#[test]
fn request_focus_is_recorded_never_refused() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    let first = controller_plugin_id(&mut controller, "org.modplayer.fixture.observer");
    let second = controller_plugin_id(&mut controller, "org.modplayer.fixture.flood");

    assert_eq!(
        call(
            &mut controller,
            first,
            Request::RequestFocus { interaction: false }
        ),
        Ok(Response::Ok)
    );
    assert_eq!(
        call(
            &mut controller,
            second,
            Request::RequestFocus { interaction: false }
        ),
        Ok(Response::Ok),
        "a second plugin's request_focus() is recorded, never refused"
    );

    // This harness never `launch()`es (no fixture thread is spawned, so
    // `transport_focus_view()`'s own `Loading|Active` filter would show
    // neither row) — read the arbiter's raw pending queue directly
    // instead, which is exactly what `apply.rs`'s `RequestFocus`/
    // `ReleaseFocus` arms drove above.
    let arbiter = controller.plugins_mut().arbiter();
    assert_eq!(
        arbiter.holder(),
        modplayer_core::plugins::FocusHolder::Host,
        "AutoOnInteraction never auto-grants on request (A4)"
    );
    assert_eq!(arbiter.request_order(first), Some(1));
    assert_eq!(arbiter.request_order(second), Some(2));

    assert_eq!(
        call(&mut controller, first, Request::ReleaseFocus),
        Ok(Response::Ok)
    );
    let arbiter = controller.plugins_mut().arbiter();
    assert_eq!(
        arbiter.request_order(first),
        None,
        "release_focus() withdraws a pending request"
    );
    assert_eq!(
        arbiter.request_order(second),
        Some(1),
        "the remaining pending request keeps its own relative order"
    );
}

/// contracts/plugin-api-v1.md §3 `markers.set_cue`: an empty slot
/// creates a plugin-owned cue; the same owner calling again on the same
/// slot moves it (same id); a different owner is refused `not_owner`.
#[test]
fn set_cue_ownership() {
    let (mut controller, _dir, _psd, _tsd) = controller_with_track();
    let owner = controller_plugin_id(&mut controller, "org.modplayer.fixture.observer");
    let other = controller_plugin_id(&mut controller, "org.modplayer.fixture.flood");

    let first = call(
        &mut controller,
        owner,
        Request::SetCue {
            slot: 3,
            position_ms: 500,
        },
    )
    .unwrap_or_else(|e| unreachable!("creating a cue on an empty slot must succeed: {e:?}"));
    let Response::MarkerId(first_id) = first else {
        unreachable!("SetCue must return a MarkerId, got {first:?}");
    };

    let moved = call(
        &mut controller,
        owner,
        Request::SetCue {
            slot: 3,
            position_ms: 999,
        },
    )
    .unwrap_or_else(|e| unreachable!("the same owner moving its own cue must succeed: {e:?}"));
    assert_eq!(
        moved,
        Response::MarkerId(first_id),
        "the same slot/owner must move the existing cue, not create a new one"
    );

    let refused = call(
        &mut controller,
        other,
        Request::SetCue {
            slot: 3,
            position_ms: 111,
        },
    )
    .expect_err("a different plugin must not steal another owner's cue slot");
    assert_eq!(refused.code, RefusalCode::PermissionDenied);
    assert_eq!(refused.reason, "not_owner");
}

/// G5 (contracts/gateway-and-runtime.md): the real `flood` fixture
/// requests focus, then issues 1 000 `seek` calls back to back on its
/// first `play_state_changed(playing)` — the rolling 1 s / 100-call
/// `transport` category cap must refuse the clear majority of them
/// `rate_limited`, read back through the plugin's own console log.
#[test]
fn rate_limit_1000_seeks() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    let id = controller_plugin_id(&mut controller, "org.modplayer.fixture.flood");
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);

    let became_active = pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )
    });
    assert!(
        became_active,
        "the flood fixture must reach Active before it can be triggered"
    );

    // 010-transport-focus (research R10): under the default
    // `AutoOnInteraction` policy nobody auto-grants on `request_focus()`
    // any more (A4), so `flood`'s own in-script request would otherwise
    // leave it un-granted and every one of its 1 000 seeks would be
    // refused `no_focus` before ever reaching the rate limiter — this
    // test's subject is the rate limiter, not arbitration (that is
    // `request_focus_is_recorded_never_refused`'s job). Switch to
    // `FirstRequestWins` and deterministically hand `flood` the holder
    // via the host's own "Give focus" (`focus_give`, C8) *before* the
    // trigger below, so it is uncontested regardless of whether any other
    // eagerly-requesting fixture (e.g. `wellbehaved`'s own `ready_ack`
    // cycle) got there first.
    controller.set_focus_policy(modplayer_core::plugins::FocusPolicy::FirstRequestWins);
    controller.focus_give(id);

    // `controller.play()` alone can be a no-op transition here — `launch()`
    // may already have auto-played the test tone before any fixture
    // reached `Active` to receive that first fan-out (T070 only fans out
    // an actual intent *change*) — so this drives the trigger directly,
    // exactly like `controller_plugins_lifecycle.rs`'s own `trigger_hang_
    // via_controller` does for the same reason.
    let now = controller.now();
    controller.plugins_mut().fan_out(
        &modplayer_capability_gateway::event::HostEvent::PlayStateChanged {
            state: modplayer_capability_gateway::event::PlayState::Playing,
        },
        now,
    );

    let deadline = Instant::now() + Duration::from_secs(30);
    let found = loop {
        controller.tick();
        if controller
            .plugin_log()
            .entries()
            .any(|e| e.message.ends_with(" rate_limited"))
        {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
    };
    assert!(found, "the flood fixture must log its own refusal count");

    let message = controller
        .plugin_log()
        .entries()
        .find(|e| e.message.ends_with(" rate_limited"))
        .map(|e| e.message.clone())
        .unwrap_or_else(|| unreachable!());
    let count: u32 = message
        .trim_start_matches("flood: ")
        .split(' ')
        .next()
        .unwrap_or_else(|| unreachable!())
        .parse()
        .unwrap_or_else(|_| unreachable!("expected a leading integer in {message:?}"));

    assert!(
        count >= 800,
        "the rolling 1s/100-call cap must refuse the clear majority of 1000 calls, got {count}"
    );
    assert!(
        count < 1000,
        "at least the first ~100 calls must have been admitted, got {count} refused"
    );
}

// -----------------------------------------------------------------------
// US3 subset (T100): the full behavior-cycle plugin's own limits and
// ownership rules — queue writes are never focus-gated (R21), the chain
// and marker limits are enforced with the spec's own refusal reasons,
// transient markers live only for their creator's session, and a host
// edit to a plugin-owned marker keeps its owner and still notifies.
// -----------------------------------------------------------------------

/// R21 (contracts/plugin-api-v1.md §3 "queue.write (never focus-gated)"):
/// every `Queue*` request kind is `needs_focus = false` at the schema
/// level, and — proven end to end — a plugin that never holds transport
/// focus (indeed, while a *different* plugin holds it) still has its
/// queue write actually applied by `drain_plugin_requests` (C1).
#[test]
fn queue_write_ignores_focus() {
    for kind in [
        RequestKind::QueueList,
        RequestKind::QueueMove,
        RequestKind::QueueRemove,
        RequestKind::QueuePlayNext,
        RequestKind::QueueAdd,
    ] {
        assert!(
            !kind.needs_focus(),
            "{kind:?} must not require transport focus (R21)"
        );
        assert_eq!(kind.requires(), Some(Permission::QueueWrite));
    }

    let (mut controller, _dir, _psd, _tsd) = controller_with_track();
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    let holder_candidate = controller_plugin_id(&mut controller, "org.modplayer.fixture.flood");
    let mover = controller_plugin_id(&mut controller, "org.modplayer.fixture.observer");

    // 010-transport-focus (research R10): `request_focus()` alone no
    // longer grants anything under the default `AutoOnInteraction`
    // policy (A4) — deterministically hand `holder_candidate` the holder
    // via the host's own "Give focus" (`focus_give`, C8) instead of
    // racing a plain `RequestFocus` CAS, so this test's actual holder is
    // never `mover` regardless of any other fixture's own request.
    // `focus_give` is a no-op for a row that isn't yet `Loading`/`Active`
    // (C8, `transport_focus_view()`'s own filter), so wait for it first.
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            matches!(
                c.plugins_mut()
                    .record(holder_candidate)
                    .map(|r| &r.lifecycle),
                Some(Lifecycle::Active)
            )
        }),
        "the flood fixture must reach Active before it can be given focus"
    );
    controller.focus_give(holder_candidate);
    let holder = controller.plugins_mut().focus().holder();
    assert_eq!(
        holder,
        Some(to_gateway_id(holder_candidate)),
        "focus_give must grant the requested holder"
    );
    assert_ne!(
        holder,
        Some(to_gateway_id(mover)),
        "the mover must not itself be the one holding focus"
    );

    let third = controller
        .queue()
        .effective_order()
        .get(2)
        .unwrap_or_else(|| unreachable!("queue_replace seeded 3 items"))
        .uid;
    let gateway_item = GatewayQueueItemId(u32::try_from(third.get()).unwrap_or(u32::MAX));

    let response = call(
        &mut controller,
        mover,
        Request::QueueMove {
            item: gateway_item,
            to: 0,
        },
    );
    assert_eq!(
        response,
        Ok(Response::Ok),
        "a queue write must succeed for a plugin that never requested focus"
    );
    // `to: 0` targets the front of the *upcoming* tail (the currently
    // playing item at effective index 0 is never reordered) — so the
    // moved item lands at effective index 1, right after it.
    assert_eq!(
        controller.queue().effective_order()[1].uid,
        third,
        "the move actually reordered the queue"
    );
}

/// G1/I1 (contracts/gateway-and-runtime.md, data-model.md §1.5/§2.3): the
/// effect chain refuses `chain_full` at `MAX_NODES` (16) and the marker
/// model refuses `marker_limit` at `MAX_MARKERS` (64), both through the
/// real `apply.rs` dispatch a plugin's `CreateNode`/`CreateMarker` calls
/// reach — not just the bare model's own `ChainError::Full`/
/// `MarkerError::LimitReached`.
#[test]
fn chain_full_and_marker_limit() {
    // Deliberately never `launch()`s: this test only exercises `apply.rs`'s
    // own dispatch through the synthetic-envelope `call()` helper, which
    // needs no running plugin thread at all (discovery already happened
    // in `PlaybackController::new`). Skipping `launch()` also means no
    // fixture — in particular the well-behaved one's own ready_ack cycle,
    // which independently creates a node and a loop region — ever runs,
    // so the chain/marker counts below start at exactly zero rather than
    // racing a background thread for the shared track's model.
    let (store, _dir) = fresh_store();
    let plugin_state_dir = TempDir::new();
    let track_state_dir = TempDir::new();
    let devices = vec![fake_device("dev-1", true)];
    let mut controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_FIXTURES", "1");
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller =
            PlaybackController::new(FakeBackend::new(devices), ScriptedHost::new(), store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_FIXTURES");
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    controller.queue_replace(vec![track("a")]);
    controller.tick();
    let plugin = controller_plugin_id(&mut controller, "org.modplayer.fixture.observer");

    assert_eq!(
        controller.chain().nodes().len(),
        0,
        "no plugin thread ran, so the chain starts empty"
    );
    for i in 0..modplayer_effects::consts::MAX_NODES {
        let response = call(
            &mut controller,
            plugin,
            Request::CreateNode {
                kind: "gain".to_string(),
                suggested: None,
            },
        );
        assert!(
            matches!(response, Ok(Response::NodeId(_))),
            "node {i}: {response:?}"
        );
    }
    let full = call(
        &mut controller,
        plugin,
        Request::CreateNode {
            kind: "gain".to_string(),
            suggested: None,
        },
    )
    .expect_err("the chain is already at its capacity");
    assert_eq!(full.code, RefusalCode::InvalidState);
    assert_eq!(full.reason, "chain_full");

    assert_eq!(
        controller
            .markers()
            .unwrap_or_else(|| unreachable!())
            .count(),
        0,
        "no plugin thread ran, so the marker model starts empty"
    );
    for i in 0..modplayer_core::markers::MAX_MARKERS {
        let response = call(
            &mut controller,
            plugin,
            Request::CreateMarker {
                position_ms: u64::try_from(i).unwrap_or(0) * 100,
                name: None,
                transient: false,
            },
        );
        assert!(
            matches!(response, Ok(Response::MarkerId(_))),
            "marker {i}: {response:?}"
        );
    }
    let limit = call(
        &mut controller,
        plugin,
        Request::CreateMarker {
            position_ms: 0,
            name: None,
            transient: false,
        },
    )
    .expect_err("the track already has the maximum number of markers");
    assert_eq!(limit.code, RefusalCode::InvalidState);
    assert_eq!(limit.reason, "marker_limit");
}

/// contracts/plugin-api-v1.md §6 ("transient ones vanish on track change
/// and on the plugin's unload"), L7c: a transient marker created by a
/// plugin is torn down by `PluginHost::stop` alongside every other part
/// of that plugin's teardown (L7), while a non-transient marker from the
/// same plugin survives; both are visible in `markers()` right up to
/// that point.
#[test]
fn transient_marker_lifetime() {
    let (mut controller, _dir, _psd, _tsd) = controller_with_track();
    let plugin = controller_plugin_id(&mut controller, "org.modplayer.fixture.observer");

    let transient = call(
        &mut controller,
        plugin,
        Request::CreateMarker {
            position_ms: 1_000,
            name: None,
            transient: true,
        },
    )
    .unwrap_or_else(|e| unreachable!("{e:?}"));
    let Response::MarkerId(_) = transient else {
        unreachable!("CreateMarker must return a MarkerId, got {transient:?}");
    };
    let permanent = call(
        &mut controller,
        plugin,
        Request::CreateMarker {
            position_ms: 2_000,
            name: None,
            transient: false,
        },
    )
    .unwrap_or_else(|e| unreachable!("{e:?}"));
    let Response::MarkerId(_) = permanent else {
        unreachable!("CreateMarker must return a MarkerId, got {permanent:?}");
    };

    let markers = controller
        .markers()
        .unwrap_or_else(|| unreachable!("current track has a marker model"));
    assert_eq!(
        markers
            .markers()
            .iter()
            .filter(|m| m.owner == Owner::Plugin(plugin))
            .count(),
        2,
        "both the transient and the permanent marker exist before teardown"
    );
    let transient_id = markers
        .markers()
        .iter()
        .find(|m| m.owner == Owner::Plugin(plugin) && m.transient)
        .map(|m| m.id)
        .unwrap_or_else(|| unreachable!());
    let permanent_id = markers
        .markers()
        .iter()
        .find(|m| m.owner == Owner::Plugin(plugin) && !m.transient)
        .map(|m| m.id)
        .unwrap_or_else(|| unreachable!());

    // L7c via `PluginHost::stop`'s own fixed order (L7): disabling the
    // plugin runs its full teardown, transient-marker removal included.
    controller.plugin_disable(plugin);
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            !matches!(
                c.plugins_mut().record(plugin).map(|r| &r.lifecycle),
                Some(Lifecycle::Active | Lifecycle::Loading)
            )
        }),
        "disable must run the plugin's teardown to completion"
    );

    let markers = controller
        .markers()
        .unwrap_or_else(|| unreachable!("track markers still exist"));
    assert!(
        markers.marker(transient_id).is_none(),
        "the transient marker must not survive the plugin's teardown"
    );
    assert!(
        markers.marker(permanent_id).is_some(),
        "the non-transient marker must survive the plugin's teardown"
    );
}

/// contracts/plugin-api-v1.md §6: "The host user may edit or delete
/// anything the plugin owns; the plugin then sees `marker_changed {
/// actor = \"host\" }`" — a host-initiated edit (the same
/// `current_markers_mut()`-routed path the UI itself uses) on a
/// plugin-owned marker is never ownership-gated (design note 9), bumps
/// the revision that drives the coalesced `marker_changed` fan-out (C3),
/// and a plugin holding `markers.read` is actually notified with
/// `actor = "host"`.
#[test]
fn host_edit_keeps_owner_and_notifies() {
    let (mut controller, _dir, _psd, _tsd) = controller_with_track();
    let owner_plugin = controller_plugin_id(&mut controller, "org.modplayer.fixture.observer");
    let watcher_plugin = controller_plugin_id(&mut controller, "org.modplayer.fixture.wellbehaved");

    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            matches!(
                c.plugins_mut().record(watcher_plugin).map(|r| &r.lifecycle),
                Some(Lifecycle::Active)
            )
        }),
        "the wellbehaved fixture (markers.read) must reach Active to observe the fan-out"
    );

    let created = call(
        &mut controller,
        owner_plugin,
        Request::CreateMarker {
            position_ms: 1_000,
            name: None,
            transient: false,
        },
    )
    .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert!(matches!(created, Response::MarkerId(_)));

    let marker_id = controller
        .markers()
        .unwrap_or_else(|| unreachable!())
        .markers()
        .iter()
        .find(|m| m.owner == Owner::Plugin(owner_plugin))
        .map(|m| m.id)
        .unwrap_or_else(|| unreachable!("the plugin's own marker must exist"));
    let revision_after_create = controller
        .markers()
        .unwrap_or_else(|| unreachable!())
        .revision();

    // The host's own edit path (e.g. a UI drag-commit), not a plugin
    // `Request` at all.
    controller
        .move_marker(marker_id, 2_000)
        .unwrap_or_else(|e| unreachable!("host edits are never ownership-gated: {e}"));

    assert_eq!(
        controller
            .markers()
            .unwrap_or_else(|| unreachable!())
            .owner_of(marker_id),
        Some(Owner::Plugin(owner_plugin)),
        "a host edit must not reassign the marker's owner"
    );
    assert!(
        controller
            .markers()
            .unwrap_or_else(|| unreachable!())
            .revision()
            > revision_after_create,
        "the edit must bump the revision that drives marker_changed fan-out (C3)"
    );

    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            c.plugin_log()
                .entries()
                .any(|e| e.message.contains("marker_changed: actor=host"))
        }),
        "a plugin holding markers.read must see marker_changed with actor = host"
    );
}

// -----------------------------------------------------------------------
// 013-key-and-tempo-plugin (API 1.4, contract plugin-api-v1.4.md §5,
// research R2): `SetParam`'s wire-name/enum/boolean widening resolves
// against the owned node's own kind, checked only *after* the ownership
// gate — proven through the real `apply.rs` dispatch via the synthetic-
// envelope `call()` helper, exactly like `not_owner_vs_not_found` above.
// -----------------------------------------------------------------------

const EFFECTS_OBSERVER: &str = "org.modplayer.fixture.effects-observer";

/// The catalog's positional index for `wire` within `kind`'s own
/// `NodeModel.params` — mirrors `controller_effects.rs`'s own `pos`
/// closure in `rate_change_rebuild_reclamps_eq`.
fn param_pos(kind: NodeKind, wire: &str) -> usize {
    let param = catalog::param_by_wire_name(kind, wire)
        .unwrap_or_else(|| unreachable!("'{wire}' must be a known {kind:?} parameter"));
    catalog::params(kind)
        .iter()
        .position(|p| p.id == param)
        .unwrap_or_else(|| unreachable!("'{wire}' must be in {kind:?}'s own param list"))
}

/// contract plugin-api-v1.4.md §5: `set_param` accepts a wire name
/// (`ParamRef::Name`) for `param`, and — shape-checked against that
/// parameter — a boolean or an enum name for `value`; both land on the
/// plugin's own owned node exactly as the numeric 1.0-1.3 form would.
#[test]
fn apply_set_param_by_name_and_enum() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    let plugin = controller_plugin_id(&mut controller, EFFECTS_OBSERVER);

    let created = call(
        &mut controller,
        plugin,
        Request::CreateNode {
            kind: "pitch_shift".to_string(),
            suggested: None,
        },
    )
    .unwrap_or_else(|e| unreachable!("create_node must succeed: {e:?}"));
    let Response::NodeId(node) = created else {
        unreachable!("CreateNode must return a NodeId, got {created:?}");
    };

    call(
        &mut controller,
        plugin,
        Request::SetParam {
            node,
            param: ParamRef::Name("formant".to_string()),
            value: ParamArg::Bool(true),
        },
    )
    .unwrap_or_else(|e| unreachable!("set_param(formant, name/bool) must succeed: {e:?}"));

    call(
        &mut controller,
        plugin,
        Request::SetParam {
            node,
            param: ParamRef::Name("quality_mode".to_string()),
            value: ParamArg::Name("quality".to_string()),
        },
    )
    .unwrap_or_else(|e| unreachable!("set_param(quality_mode, name/enum) must succeed: {e:?}"));

    let node_model = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.owner == NodeOwner::Plugin(plugin))
        .unwrap_or_else(|| unreachable!("the created node must exist"));
    assert_eq!(
        node_model.params[param_pos(NodeKind::PitchShift, "formant")],
        1.0,
        "the boolean wire value must land as 1.0"
    );
    assert_eq!(
        node_model.params[param_pos(NodeKind::PitchShift, "quality_mode")],
        catalog::QualityMode::Quality as u8 as f32,
        "the enum name 'quality' must resolve to its catalog index"
    );
}

/// contract plugin-api-v1.4.md §5: an unknown wire name is refused
/// `invalid_state`/`invalid_argument`, naming the parameter — never a
/// panic, never a silent no-op — checked only after the ownership gate
/// (`not_owner_vs_not_found` covers that ordering for the numeric form
/// already).
#[test]
fn apply_set_param_unknown_name_refused() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    let plugin = controller_plugin_id(&mut controller, EFFECTS_OBSERVER);

    let created = call(
        &mut controller,
        plugin,
        Request::CreateNode {
            kind: "gain".to_string(),
            suggested: None,
        },
    )
    .unwrap_or_else(|e| unreachable!("create_node must succeed: {e:?}"));
    let Response::NodeId(node) = created else {
        unreachable!("CreateNode must return a NodeId, got {created:?}");
    };

    let refusal = call(
        &mut controller,
        plugin,
        Request::SetParam {
            node,
            param: ParamRef::Name("not_a_real_param".to_string()),
            value: ParamArg::Number(1.0),
        },
    )
    .expect_err("an unknown wire name must be refused, not accepted");
    assert_eq!(refusal.code, RefusalCode::InvalidState);
    assert_eq!(refusal.reason, "invalid_argument");
    assert!(
        refusal.message.contains("not_a_real_param"),
        "the refusal must name the unknown parameter: {refusal:?}"
    );
}

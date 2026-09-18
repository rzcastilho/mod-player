// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PlaybackController` marker/loop wiring tests (006,
//! contracts/marker-service.md §1–§4).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, SourceCommand, SourceEvent, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::PlaybackController;
use modplayer_core::markers::store::{TrackStatePaths, load};
use modplayer_core::markers::{RegionId, RepeatCount, TrackMarkers};
use modplayer_core::notifications::{KEY_TRACK_STATE_NEWER_VERSION, KEY_TRACK_STATE_UNREADABLE};
use modplayer_core::settings::SettingsStore;
use modplayer_core::transport::Intent;
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};

/// A track's default duration in source-rate frames (`track()`'s
/// `180_000` ms at `fake_device`'s 44.1 kHz).
const TRACK_LEN_FRAMES: u64 = 44_100 * 180;
const TRACK_RATE: u32 = 44_100;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-markers-{}-{}",
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

/// Mirrors `controller_streaming.rs`'s harness: a controller over a
/// confirmed device, ready for `play()`.
///
/// Its track-state directory is `<settings temp dir>/track-state`, never
/// the real per-user one: every test here creates markers/regions, and
/// 006 US2's debounced flush runs on every `tick()`, so an unisolated
/// controller would write `spotify:track:a`'s state into the developer's
/// own `~/Library/Application Support/ModPlayer.ModPlayer/track-state/`
/// (contracts/marker-service.md §3). Nesting it under `dir` keeps the
/// returned tuple's shape and ties both lifetimes together.
fn ready_controller() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let track_state_dir = dir.path().join("track-state");
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device("dev-1", true)];
    // `MODPLAYER_TRACK_STATE_DIR` is process-global (`std::env`), so every
    // `PlaybackController::new` in this binary must be serialized against
    // every other's brief mutation of it, or a concurrently running test
    // could construct against the wrong directory (research R10/R11).
    let mut controller = {
        let _guard = TRACK_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Safety: narrowly scopes the mutation to the one synchronous
        // read `PlaybackController::new` does of this var, serialized
        // against every other test in this binary via the lock above.
        unsafe { std::env::set_var("MODPLAYER_TRACK_STATE_DIR", &track_state_dir) };
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe { std::env::remove_var("MODPLAYER_TRACK_STATE_DIR") };
        controller
    };
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    (controller, handle, dir)
}

/// Serializes every test in this binary that reads `MODPLAYER_TRACK_STATE_
/// DIR` — the critical section is only `PlaybackController::new`'s single
/// synchronous read of it (research R10/R11), never held any longer.
static TRACK_STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// As `ready_controller`, but with a real (temp-dir-backed) track-state
/// directory wired in, so persistence (006, contracts/marker-service.md
/// §3-§4) actually reads/writes files this test can inspect.
fn ready_controller_with_track_state() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let track_state_dir = TempDir::new();
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device("dev-1", true)];
    let mut controller = {
        let _guard = TRACK_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Safety: this narrowly scopes the mutation to the one synchronous
        // read `PlaybackController::new` does of this var, serialized
        // against every other test in this binary via the lock above.
        unsafe { std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path()) };
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe { std::env::remove_var("MODPLAYER_TRACK_STATE_DIR") };
        controller
    };
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    (controller, handle, dir, track_state_dir)
}

/// A fake, advancing clock (mirrors `controller_streaming.rs`'s
/// `fake_clock`), for the loop re-seek throttle (research R5).
fn fake_clock() -> (impl Fn() -> std::time::Instant, std::sync::Arc<AtomicU64>) {
    let base = std::time::Instant::now();
    let offset_ms = std::sync::Arc::new(AtomicU64::new(0));
    let clock_offset = std::sync::Arc::clone(&offset_ms);
    (
        move || base + Duration::from_millis(clock_offset.load(Ordering::SeqCst)),
        offset_ms,
    )
}

/// Create a small (one-buffer-wide) loop region at the playhead via
/// `set_loop_a`/`set_loop_b`, arm it, and seek back to its `A` so it is
/// immediately armed-active. Returns the region and its `A` frame.
fn arm_small_region(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> (RegionId, u64) {
    arm_small_region_with_repeat(controller, RepeatCount::Infinite)
}

fn arm_small_region_with_repeat(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    repeat: RepeatCount,
) -> (RegionId, u64) {
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));

    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!("region must exist after set_loop_a/b"));
    controller
        .set_loop_repeat(region, repeat)
        .unwrap_or_else(|e| unreachable!("set_loop_repeat: {e}"));
    let a = {
        let markers = controller
            .markers()
            .unwrap_or_else(|| unreachable!("markers must exist"));
        let r = markers
            .region(region)
            .unwrap_or_else(|| unreachable!("region must exist"));
        r.span(markers)
            .unwrap_or_else(|| unreachable!("region must be complete"))
            .0
    };
    controller
        .arm_loop(region)
        .unwrap_or_else(|e| unreachable!("arm_loop: {e}"));
    controller.seek_frames(a);
    let _ = controller.backend_mut().render_buffers(1); // let the setters/commit/seek land
    (region, a)
}

/// `arm_loop` derives and pushes the engine's setters + `LoopCommit`
/// (the region reaches an armed state) and sends `PrefetchHint` at (or
/// before) `A` (FR-008, contracts/marker-service.md §1–§2).
#[test]
fn arm_pushes_setters_then_commit_and_prefetch_hint() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    let (_region, a) = arm_small_region(&mut controller);

    assert_ne!(
        controller.shared().loop_state(),
        0,
        "the commit must have landed on the engine"
    );

    let hint = handle.record_commands().into_iter().find_map(|c| match c {
        SourceCommand::PrefetchHint { frame } => Some(frame),
        _ => None,
    });
    assert!(
        matches!(hint, Some(frame) if frame <= a),
        "hint={hint:?} a={a}"
    );

    let mut wrapped = false;
    for _ in 0..200 {
        let _ = controller.backend_mut().render_buffers(1);
        if controller.shared().loop_wraps() > 0 {
            wrapped = true;
            break;
        }
    }
    assert!(wrapped, "an armed, active region must wrap");
}

/// Editing an armed region (e.g. its crossfade) re-commits the engine
/// without resetting the wrap count (contracts/marker-service.md §1).
#[test]
fn edit_armed_region_recommits_without_resetting_wraps() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    let (region, _a) = arm_small_region(&mut controller);

    let mut wraps_before = 0;
    for _ in 0..200 {
        let _ = controller.backend_mut().render_buffers(1);
        wraps_before = controller.shared().loop_wraps();
        if wraps_before >= 3 {
            break;
        }
    }
    assert!(wraps_before >= 3, "wraps_before={wraps_before}");

    controller
        .set_loop_crossfade_ms(region, 5)
        .unwrap_or_else(|e| unreachable!("set_loop_crossfade_ms: {e}"));
    let _ = controller.backend_mut().render_buffers(1);

    assert!(
        controller.shared().loop_wraps() >= wraps_before,
        "an edit-while-armed must not reset the wrap count"
    );
    assert_ne!(
        controller.shared().loop_state(),
        0,
        "still armed after the edit"
    );
}

/// At least one `LoopWrapped` in a tick sends exactly one coalesced
/// `SourceCommand::Seek`; a second tick inside the 250 ms throttle window
/// sends none, and one after the window elapses sends one more
/// (research R5).
#[test]
fn loop_wrapped_reseeks_source_once_per_tick_then_throttled() {
    let (mut controller, handle, _dir) = ready_controller();
    let (clock, offset_ms) = fake_clock();
    controller.set_clock(clock);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    let _ = arm_small_region(&mut controller);

    let seeks_in = |handle: &ScriptedHostHandle, from: usize| -> usize {
        handle
            .record_commands()
            .into_iter()
            .skip(from)
            .filter(|c| matches!(c, SourceCommand::Seek(_)))
            .count()
    };

    for _ in 0..50 {
        let _ = controller.backend_mut().render_buffers(1);
    }
    let before = handle.record_commands().len();
    controller.tick();
    assert_eq!(
        seeks_in(&handle, before),
        1,
        "one coalesced Seek regardless of how many wraps landed since the last tick"
    );

    for _ in 0..50 {
        let _ = controller.backend_mut().render_buffers(1);
    }
    let before = handle.record_commands().len();
    controller.tick();
    assert_eq!(
        seeks_in(&handle, before),
        0,
        "throttled: < 250ms since the first Seek"
    );

    offset_ms.store(300, Ordering::SeqCst);
    for _ in 0..50 {
        let _ = controller.backend_mut().render_buffers(1);
    }
    let before = handle.record_commands().len();
    controller.tick();
    assert_eq!(
        seeks_in(&handle, before),
        1,
        "throttle window elapsed: one more coalesced Seek"
    );
}

/// research R5 rule 3: `EndOfTrack` is ignored while `loop_state == 2` —
/// the queue never advances mid-loop.
#[test]
fn end_of_track_ignored_while_loop_active() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();

    let (_region, _a) = arm_small_region(&mut controller);
    let mut active = false;
    for _ in 0..200 {
        let _ = controller.backend_mut().render_buffers(1);
        if controller.shared().loop_state() == 2 {
            active = true;
            break;
        }
    }
    assert!(active, "must reach armed-active before the assertion below");

    handle.emit(SourceEvent::EndOfTrack);
    controller.tick();

    assert_eq!(
        controller.current_track().map(|item| item.track.id.clone()),
        Some(track("a").id),
        "must not advance past the current track while the loop is armed-active"
    );
    assert_eq!(controller.transport_state().intent, Intent::Playing);
}

/// The converse: with no loop armed, `EndOfTrack` advances the queue as
/// usual (unchanged pre-006 behaviour).
#[test]
fn end_of_track_advances_while_loop_inactive() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();
    assert_eq!(controller.shared().loop_state(), 0);

    handle.emit(SourceEvent::EndOfTrack);
    controller.tick();

    assert_eq!(
        controller.current_track().map(|item| item.track.id.clone()),
        Some(track("b").id)
    );
}

/// Once the engine releases (repeat count reached), the *model* is
/// disarmed on the next `tick()` — the RT already stopped looping on its
/// own (FR-011).
#[test]
fn loop_released_disarms_model() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    let (region, _a) = arm_small_region_with_repeat(&mut controller, RepeatCount::Times(2));

    for _ in 0..200 {
        let _ = controller.backend_mut().render_buffers(1);
        if controller.shared().loop_state() == 0 {
            break;
        }
    }
    controller.tick(); // drains LoopReleased -> markers.disarm()

    let armed = controller
        .markers()
        .and_then(|m| m.region(region))
        .map(|r| r.armed);
    assert_eq!(
        armed,
        Some(false),
        "the model must be disarmed after release"
    );
}

/// A stream rebuild (device change) re-pushes the armed region's four
/// setters + `LoopCommit { reset_wraps: false }` (research R6): the loop
/// survives the rebuild and keeps wrapping.
#[test]
fn stream_rebuild_repushes_armed_region() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    let (_region, _a) = arm_small_region(&mut controller);
    let _ = controller.backend_mut().render_buffers(1);
    assert_ne!(
        controller.shared().loop_state(),
        0,
        "armed before the rebuild"
    );

    controller
        .backend_mut()
        .add_device(fake_device("dev-2", false));
    controller.tick(); // DeviceListChanged: dev-1 still active, no-op
    let lost = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.backend_mut().remove_device(&lost);
    controller.tick(); // DeviceLost -> falls back to dev-2, rebuilds the Processor

    assert!(controller.is_connected());
    let _ = controller.backend_mut().render_buffers(1);
    assert_ne!(
        controller.shared().loop_state(),
        0,
        "the armed region must have been re-pushed after the rebuild \
         (a fresh Processor starts disarmed, so this would read 0 otherwise)"
    );

    let mut wrapped = false;
    for _ in 0..200 {
        let _ = controller.backend_mut().render_buffers(1);
        if controller.shared().loop_wraps() > 0 {
            wrapped = true;
            break;
        }
    }
    assert!(wrapped, "the re-pushed region must still wrap");
}

/// `sync_marker_attachment` (006, contracts/marker-service.md §4): a
/// current-track change flushes the previous track's dirty state to disk
/// before loading the new one's (fresh/empty, since nothing was ever
/// saved for it).
#[test]
fn track_change_flushes_disarms_then_loads() {
    let (mut controller, handle, _dir, track_state_dir) = ready_controller_with_track_state();
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();

    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    assert_eq!(controller.markers().map(|m| m.count()), Some(2));

    handle.emit(SourceEvent::EndOfTrack);
    controller.tick();

    assert_eq!(
        controller.current_track().map(|item| item.track.id.clone()),
        Some(track("b").id)
    );
    assert_eq!(
        controller.markers().map(|m| m.count()),
        Some(0),
        "track b's model starts fresh"
    );

    // `shutdown()` joins the writer thread, so the `Save` job the track
    // change queued above is guaranteed to have landed on disk by the
    // time it returns (mirrors `AnalysisService::shutdown()`'s join).
    controller.shutdown();
    let paths = TrackStatePaths::with_dir(track_state_dir.path());
    let outcome = load(&paths, &track("a").id, TRACK_RATE, TRACK_LEN_FRAMES);
    assert_eq!(outcome.warning, None);
    assert_eq!(
        outcome.state.count(),
        2,
        "track a's markers were flushed on the track change, before track b was loaded"
    );
}

/// FR-016 "same-session reload": a `TrackStarted` for the *same* id the
/// queue is already on still flushes/reloads (clearing the armed state,
/// which is session-only and never persisted) — unlike an ordinary
/// dispatch for the same track (e.g. a volume change), which must not.
#[test]
fn same_track_restart_reloads_and_clears_armed() {
    let (mut controller, handle, _dir, _track_state_dir) = ready_controller_with_track_state();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    // The track's *initial* `TrackStarted` — the one that belongs to the
    // attachment `play()` just made. It is not a restart, so it does not
    // reload (that would load the same file twice for one play and raise
    // any load warning twice); the restart under test is the second one.
    handle.emit(SourceEvent::TrackStarted {
        track: track("a"),
        program: Some((0, 0)),
        position_ms: 0,
        playing: true,
    });
    controller.tick();

    let (_region, _a) = arm_small_region(&mut controller);
    assert_ne!(
        controller.shared().loop_state(),
        0,
        "armed before the restart"
    );

    handle.emit(SourceEvent::TrackStarted {
        track: track("a"),
        program: Some((0, 0)),
        position_ms: 0,
        playing: true,
    });
    controller.tick();
    // `tick()` only enqueues the `LoopDisarm` `Command`; the engine side
    // only reflects it once the `Processor` actually renders (mirrors
    // every other engine-state assertion in this file).
    let _ = controller.backend_mut().render_buffers(1);

    assert_eq!(
        controller.markers().and_then(TrackMarkers::armed_region),
        None,
        "a same-session reload starts disarmed (FR-016) — the model side"
    );
    assert_eq!(
        controller.shared().loop_state(),
        0,
        "the engine side was disarmed too"
    );
}

/// Starting a track is *one* load, so an unreadable state file raises its
/// warning once — not once when the track becomes current and again on the
/// `TrackStarted` that follows it (found driving quickstart.md's M9: the
/// live app stacked two identical `track-state-unreadable` warnings).
#[test]
fn track_start_loads_once_and_warns_once() {
    let (mut controller, handle, _dir, track_state_dir) = ready_controller_with_track_state();
    let paths = TrackStatePaths::with_dir(track_state_dir.path());
    std::fs::write(paths.file_for(&track("a").id), b"not json")
        .unwrap_or_else(|e| unreachable!("seed unreadable file: {e}"));

    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    handle.emit(SourceEvent::TrackStarted {
        track: track("a"),
        program: Some((0, 0)),
        position_ms: 0,
        playing: true,
    });
    controller.tick();

    let warnings = controller
        .notifications()
        .all()
        .filter(|n| n.message_key == KEY_TRACK_STATE_UNREADABLE)
        .count();
    assert_eq!(
        warnings, 1,
        "one play of one track raises the unreadable warning exactly once"
    );
}

/// contracts/marker-service.md §1: "Clear all markers" flushes the empty
/// state immediately rather than waiting out the debounce.
#[test]
fn clear_all_writes_empty_state_immediately() {
    let (mut controller, _handle, _dir, track_state_dir) = ready_controller_with_track_state();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    assert_eq!(controller.markers().map(|m| m.count()), Some(2));

    controller.clear_all_markers();

    assert_eq!(controller.markers().map(|m| m.count()), Some(0));

    controller.shutdown();
    let paths = TrackStatePaths::with_dir(track_state_dir.path());
    let outcome = load(&paths, &track("a").id, TRACK_RATE, TRACK_LEN_FRAMES);
    assert_eq!(outcome.warning, None);
    assert_eq!(
        outcome.state.count(),
        0,
        "the empty state reached disk immediately, not after the 250ms debounce"
    );
}

/// contracts/marker-service.md §3 rule 4 / §4: sign-out flushes and drops
/// the in-memory model, but — unlike `library`/`play_log` — never deletes
/// the file itself (markers are not account data).
#[test]
fn sign_out_flushes_and_drops_state_but_keeps_file() {
    let (mut controller, _handle, _dir, track_state_dir) = ready_controller_with_track_state();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    assert_eq!(controller.markers().map(|m| m.count()), Some(1));

    controller.clear_for_sign_out();

    assert_eq!(
        controller.markers(),
        None,
        "the in-memory state is dropped on sign-out"
    );

    controller.shutdown();
    let paths = TrackStatePaths::with_dir(track_state_dir.path());
    let outcome = load(&paths, &track("a").id, TRACK_RATE, TRACK_LEN_FRAMES);
    assert_eq!(outcome.warning, None);
    assert_eq!(
        outcome.state.count(),
        1,
        "the file itself is kept — sign-out only clears the in-memory copy"
    );
}

/// contracts/marker-service.md §6: `Unreadable`/`NewerSchema` are each
/// raised at most once per load — a later `tick()` with nothing changed
/// raises no more, and a *different* track's own warning is independent.
#[test]
fn warnings_raised_once_per_load() {
    let (mut controller, handle, _dir, track_state_dir) = ready_controller_with_track_state();
    let paths = TrackStatePaths::with_dir(track_state_dir.path());
    std::fs::write(paths.file_for(&track("a").id), b"not json")
        .unwrap_or_else(|e| unreachable!("seed unreadable file: {e}"));
    std::fs::write(
        paths.file_for(&track("b").id),
        format!(
            r#"{{"schema_version":99,"track_id":"{}","sample_rate":44100,"len_frames":100}}"#,
            track("b").id.as_str()
        ),
    )
    .unwrap_or_else(|e| unreachable!("seed newer-schema file: {e}"));

    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();

    let count = |controller: &PlaybackController<FakeBackend, ScriptedHost>, key: &str| {
        controller
            .notifications()
            .all()
            .filter(|n| n.message_key == key)
            .count()
    };
    assert_eq!(
        count(&controller, KEY_TRACK_STATE_UNREADABLE),
        1,
        "raised exactly once for track a's unreadable file"
    );

    controller.tick();
    assert_eq!(
        count(&controller, KEY_TRACK_STATE_UNREADABLE),
        1,
        "ticking again with nothing changed raises no more"
    );

    handle.emit(SourceEvent::EndOfTrack);
    controller.tick();

    assert_eq!(
        count(&controller, KEY_TRACK_STATE_NEWER_VERSION),
        1,
        "track b's own (different) warning, also raised exactly once"
    );
}

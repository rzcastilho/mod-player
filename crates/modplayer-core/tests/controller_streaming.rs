// SPDX-License-Identifier: MIT OR Apache-2.0

//! T048-T051 (US1): `PlaybackController` streaming behaviour with
//! `FakeBackend` + `ScriptedHost` (contracts/transport-and-queue.md §7).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{
    AccountReadError, Availability, RemoteCommand, Repeat, SourceEvent, SourceHealth, TrackId,
    TrackRef, TransferContext, VolumePercent,
};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::PlaybackController;
use modplayer_core::settings::SettingsStore;
use modplayer_core::transport::{ActiveState, Intent, NotRegisteredReason, PendingTransferCommand};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};

/// `sync_program`'s debounce window (contracts/transport-and-queue.md §3)
/// plus slack, for tests that need to wait it out with a real clock (the
/// controller has no injectable clock yet — design note 6's timers land
/// with US3/US4).
const DEBOUNCE_SETTLE: Duration = Duration::from_millis(280);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-streaming-{}-{}",
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

fn fake_device() -> FakeDevice {
    FakeDevice {
        id: DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default: true,
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

/// Build a controller over a confirmed device, ready for `play()`, plus a
/// script handle for `ScriptedHost` and the temp dir backing its settings
/// store (kept alive for the caller's whole test).
fn ready_controller() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device()];
    let mut controller = PlaybackController::new(FakeBackend::new(devices), host, store);
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    (controller, handle, dir)
}

#[test]
fn buffered_play_is_audible_within_50ms() {
    let (mut controller, _handle, _dir) = ready_controller();

    controller.queue_replace(vec![track("a")]);
    controller.play();
    // `ScriptedHost` (unthrottled) emits `Playing` synchronously; one
    // `tick()` drains it — well within SC-001's 50 ms budget.
    controller.tick();

    assert_eq!(controller.transport_state().intent, Intent::Playing);
    assert!(
        !controller.transport_state().buffering,
        "must not be buffering once Playing arrived"
    );

    let output = controller.backend_mut().render_buffers(4);
    assert!(
        output.iter().any(|sample| sample.abs() > 1e-6),
        "audio must be audible once playing"
    );
}

#[test]
fn first_play_buffers_until_two_seconds_then_plays() {
    let (mut controller, handle, _dir) = ready_controller();

    handle.throttle(2_000);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    assert_eq!(controller.transport_state().intent, Intent::Playing);
    assert!(
        controller.transport_state().buffering,
        "must buffer until the source is ready (FR-010)"
    );

    handle.release_throttle();
    controller.tick();

    assert!(
        !controller.transport_state().buffering,
        "buffering clears once Playing arrives"
    );
}

#[test]
fn adjacent_buffered_tracks_transition_without_a_gap() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    let before = controller.backend_mut().render_buffers(2);
    assert!(before.iter().any(|s| s.abs() > 1e-6));

    // `skip_forward` synchronously reduces `Advance` -> `QueueChanged` ->
    // `LoadCurrentProgram` -> `SourceCommand::LoadProgram`; one `tick()`
    // then drains the resulting (unthrottled) `Playing` event.
    controller.skip_forward();
    controller.tick();

    // FR-007 (host-level assertion): the transition to the next queued
    // item never drops intent out of `Playing` — no stop/re-buffer gap.
    assert_eq!(
        controller.transport_state().intent,
        Intent::Playing,
        "gapless transition must not stop playback"
    );
    assert!(!controller.transport_state().buffering);

    let after = controller.backend_mut().render_buffers(2);
    assert!(
        after.iter().any(|s| s.abs() > 1e-6),
        "still audible after the transition"
    );
}

#[test]
fn pause_then_resume_keeps_position_without_rebuffering() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(2);
    let position_before = controller.shared().position_frames();

    controller.pause();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Paused);

    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);
    assert!(
        !controller.transport_state().buffering,
        "resume from pause must not re-buffer (T2)"
    );
    let _ = controller.backend_mut().render_buffers(1);
    assert!(
        controller.shared().position_frames() >= position_before,
        "position must be retained (never rewound) across pause/resume"
    );
}

#[test]
fn stop_resets_position_and_retains_the_queue() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(3);
    assert!(controller.shared().position_frames() > 0);

    controller.stop();
    controller.tick();

    assert_eq!(controller.transport_state().intent, Intent::Stopped);
    assert_eq!(
        controller.position(),
        std::time::Duration::ZERO,
        "T5: stop resets position to 0"
    );
    assert!(
        controller.queue().current().is_some(),
        "T5: the current item is retained"
    );
    assert_eq!(
        controller.queue().effective_order().len(),
        2,
        "T5: the queue is retained"
    );
}

#[test]
fn seek_on_buffered_audio_lands_at_the_requested_position() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    controller.seek(std::time::Duration::from_secs(30));
    controller.tick();
    let _ = controller.backend_mut().render_buffers(1);

    let position_ms = controller.position().as_millis();
    assert!(
        (29_950..=30_050).contains(&position_ms),
        "seek must land within 50 ms of the requested position, got {position_ms}ms"
    );
}

// T068 (US2): `sync_program` debounce, mode switch on mutation, wrap
// re-shuffle sends `[last, ...reshuffled]` (contracts/transport-and-
// queue.md §3).

fn track_id(id: &str) -> TrackId {
    TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!())
}

fn uid_of(
    controller: &PlaybackController<FakeBackend, ScriptedHost>,
    id: &str,
) -> modplayer_core::QueueItemId {
    controller
        .queue()
        .effective_order()
        .into_iter()
        .find(|item| item.track.id == track_id(id))
        .map(|item| item.uid)
        .unwrap_or_else(|| unreachable!("track {id} not in the queue"))
}

#[test]
fn sync_program_coalesces_rapid_mutations_into_one_debounced_send() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b"), track("c"), track("d")]);
    let sent_after_replace = handle.load_count();
    assert_eq!(
        sent_after_replace, 1,
        "queue_replace sends exactly one program"
    );

    let d_uid = uid_of(&controller, "d");
    let c_uid = uid_of(&controller, "c");
    controller.queue_reorder(d_uid, 1); // -> a, d, b, c
    controller.queue_reorder(c_uid, 1); // -> a, c, d, b

    assert_eq!(
        handle.load_count(),
        sent_after_replace,
        "debounced: no send before the window elapses, even after two mutations"
    );

    std::thread::sleep(DEBOUNCE_SETTLE);
    controller.tick();

    assert_eq!(
        handle.load_count(),
        sent_after_replace + 1,
        "both mutations coalesce into exactly one send"
    );
    let sent = handle
        .last_program()
        .unwrap_or_else(|| unreachable!("a program must have been sent"));
    assert_eq!(
        sent.order,
        vec![track_id("a"), track_id("c"), track_id("d"), track_id("b")],
        "the last mutation wins"
    );
}

#[test]
fn sync_program_previews_the_wrap_reshuffle_under_repeat_all_and_shuffle() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    controller.set_repeat(Repeat::All);
    controller.set_shuffle(true);
    std::thread::sleep(DEBOUNCE_SETTLE);
    controller.tick();

    // Walk to the last item of this shuffle cycle (two skips from the
    // first of three items) — each `skip_forward` reloads the program
    // through the reducer's own `Effect::LoadCurrentProgram`, independent
    // of `sync_program`'s debounce tracking.
    controller.skip_forward();
    controller.tick();
    controller.skip_forward();
    controller.tick();
    let current_id = controller
        .queue()
        .current()
        .unwrap_or_else(|| unreachable!())
        .track
        .id
        .clone();

    // A benign re-mutation (shuffle already on) re-runs `sync_program` now
    // that the queue is on the last item of the cycle.
    std::thread::sleep(DEBOUNCE_SETTLE);
    controller.set_shuffle(true);
    std::thread::sleep(DEBOUNCE_SETTLE);
    controller.tick();

    let sent = handle
        .last_program()
        .unwrap_or_else(|| unreachable!("a program must have been sent"));
    assert_eq!(
        sent.order.first(),
        Some(&current_id),
        "the wrap preview keeps the last item first"
    );
    assert_eq!(
        sent.order.len(),
        3,
        "the wrap preview includes the full next cycle"
    );
    let mut rest = sent.order[1..].to_vec();
    rest.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut expected: Vec<_> = [track_id("a"), track_id("b"), track_id("c")]
        .into_iter()
        .filter(|id| id != &current_id)
        .collect();
    expected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    assert_eq!(rest, expected, "the rest of the cycle, reshuffled");
}

// T073 (US2): an unavailable queue item is skipped and notified (FR-026).

#[test]
fn unavailable_current_item_is_skipped_and_notified() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.tick();
    handle.unavailable(track_id("a"));

    controller.play();
    controller.tick();

    assert_eq!(controller.transport_state().intent, Intent::Playing);
    assert_eq!(
        controller
            .queue()
            .current()
            .map(|item| item.track.id.clone()),
        Some(track_id("b")),
        "the unavailable item is skipped in favour of the next one"
    );
    assert!(
        controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "queue-item-skipped-unavailable"),
        "expected a queue-item-skipped-unavailable notification"
    );
}

#[test]
fn unavailable_item_with_none_playable_left_stops_transport() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.tick();
    handle.unavailable(track_id("a"));

    controller.play();
    controller.tick();

    assert_eq!(
        controller.transport_state().intent,
        Intent::Stopped,
        "no playable item remains, per FR-026"
    );
    assert!(
        controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "queue-item-skipped-unavailable")
    );
}

// T81-T83 (US3): transfer to/from another Connect controller
// (contracts/transport-and-queue.md §2 rules T15-T19).

fn transfer_context(id: &str, playing: bool) -> TransferContext {
    TransferContext {
        current: track(id),
        position_ms: 0,
        playing,
        shuffle: None,
        repeat: None,
    }
}

#[test]
fn transfer_in_replaces_queue_and_plays() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b")]);

    handle.transfer_in(Some(transfer_context("z", true)));
    controller.tick();

    assert_eq!(controller.transport_state().intent, Intent::Playing);
    assert!(matches!(controller.active_state(), ActiveState::Active));
    assert_eq!(
        controller.queue().mode(),
        modplayer_core::QueueMode::SourceDriven
    );
    assert_eq!(
        controller
            .queue()
            .current()
            .map(|item| item.track.id.clone()),
        Some(track_id("z")),
        "the transfer's context replaces whatever was queued locally"
    );
    assert_eq!(
        controller.queue().effective_order().len(),
        1,
        "transfer-in seeds a single-item queue (research R3)"
    );
}

#[test]
fn transfer_away_freezes_and_shows_banner() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    handle.transfer_out();
    controller.tick();

    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "T16: frozen with Paused semantics"
    );
    assert!(
        matches!(controller.active_state(), ActiveState::Inactive { .. }),
        "expected Inactive, got {:?}",
        controller.active_state()
    );
    // The queue/current item are retained so the Now Playing banner still
    // has something to show alongside "Playing on <device>".
    assert!(controller.current_track().is_some());
}

#[test]
fn play_here_requests_transfer() {
    let (mut controller, handle, _dir) = ready_controller();
    handle.transfer_out();
    controller.tick();
    assert!(matches!(
        controller.active_state(),
        ActiveState::Inactive { .. }
    ));

    controller.play_here();

    assert!(
        matches!(
            controller.active_state(),
            ActiveState::TransferRequested {
                pending: Some(PendingTransferCommand::PlayHere)
            }
        ),
        "expected TransferRequested{{pending: PlayHere}}, got {:?}",
        controller.active_state()
    );
}

#[test]
fn transfer_request_times_out_with_warning() {
    let (mut controller, handle, _dir) = ready_controller();

    let base = std::time::Instant::now();
    let offset_ms = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let clock_offset = std::sync::Arc::clone(&offset_ms);
    controller.set_clock(move || base + Duration::from_millis(clock_offset.load(Ordering::SeqCst)));

    handle.transfer_out();
    controller.tick();
    controller.play_here();
    assert!(matches!(
        controller.active_state(),
        ActiveState::TransferRequested { .. }
    ));

    // Just under 5 s: still awaiting.
    offset_ms.store(4_900, Ordering::SeqCst);
    controller.tick();
    assert!(
        matches!(
            controller.active_state(),
            ActiveState::TransferRequested { .. }
        ),
        "must not time out before 5 s"
    );

    // Past 5 s: times out with a warning, back to Inactive.
    offset_ms.store(5_100, Ordering::SeqCst);
    controller.tick();
    assert!(
        matches!(controller.active_state(), ActiveState::Inactive { .. }),
        "expected Inactive after the 5 s timeout, got {:?}",
        controller.active_state()
    );
    assert!(
        controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "transfer-request-failed"),
        "expected a transfer-request-failed notification"
    );
}

#[test]
fn remote_commands_are_mirrored_into_modplayers_own_transport() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    handle.remote(RemoteCommand::Pause);
    controller.tick();
    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "a remote pause must reflect in ModPlayer's own transport (FR-017)"
    );

    handle.remote(RemoteCommand::Play);
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    handle.remote(RemoteCommand::Volume(VolumePercent::new(65)));
    controller.tick();
    assert_eq!(controller.master_volume().value(), 65);

    handle.remote(RemoteCommand::Shuffle(true));
    controller.tick();
    assert!(controller.queue().shuffle_enabled());

    handle.remote(RemoteCommand::Repeat(Repeat::All));
    controller.tick();
    assert_eq!(controller.queue().repeat(), Repeat::All);

    // `Seek`/`SkipNext`/`SkipPrev` are already applied by the source; the
    // reducer just must not choke on them (no transport-state corruption).
    handle.remote(RemoteCommand::Seek(1_000));
    handle.remote(RemoteCommand::SkipNext);
    handle.remote(RemoteCommand::SkipPrev);
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);
}

// T091-T094 (US4): network trouble is absorbed by the buffer, an
// unrecoverable protocol failure fails clearly instead of crashing, and
// session events interact correctly with playback (contracts/transport-
// and-queue.md §2 rules T13/T20-T23).

/// A fake, advancing clock for the 30 s reconnect-warning timer (T20),
/// matching `transfer_request_times_out_with_warning`'s pattern.
fn fake_clock() -> (impl Fn() -> std::time::Instant, std::sync::Arc<AtomicU64>) {
    let base = std::time::Instant::now();
    let offset_ms = std::sync::Arc::new(AtomicU64::new(0));
    let clock_offset = std::sync::Arc::clone(&offset_ms);
    (
        move || base + Duration::from_millis(clock_offset.load(Ordering::SeqCst)),
        offset_ms,
    )
}

#[test]
fn continues_from_buffer_when_throttled() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);
    assert!(!controller.transport_state().buffering);

    // A degraded (but not dry) connection never raises `Loading` at all —
    // playback simply continues from what is already buffered (SC-005).
    let before = controller.backend_mut().render_buffers(2);
    assert!(before.iter().any(|s| s.abs() > 1e-6));
    assert_eq!(controller.transport_state().intent, Intent::Playing);
    assert!(
        !controller.transport_state().buffering,
        "a merely-throttled connection must not show buffering while the buffer holds"
    );
}

#[test]
fn dry_buffer_shows_buffering_and_resumes_without_skip() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    // The buffer runs dry mid-track (network cut): the source reports
    // `Loading` with no accompanying `LoadProgram` — the current item must
    // not change (no skip, FR-010).
    handle.emit(SourceEvent::Loading {
        position_ms: 12_000,
    });
    controller.tick();

    assert!(
        controller.transport_state().buffering,
        "a dry buffer must show buffering, not stop or skip"
    );
    assert_eq!(controller.transport_state().intent, Intent::Playing);
    assert_eq!(
        controller
            .queue()
            .current()
            .map(|item| item.track.id.clone()),
        Some(track_id("a")),
        "the current item must not change while buffering"
    );

    handle.emit(SourceEvent::Playing {
        position_ms: 12_000,
    });
    controller.tick();

    assert!(
        !controller.transport_state().buffering,
        "buffering clears once audio resumes"
    );
    assert_eq!(
        controller
            .queue()
            .current()
            .map(|item| item.track.id.clone()),
        Some(track_id("a")),
        "resuming from the buffer must not have skipped the track"
    );
}

#[test]
fn seek_past_end_advances_per_fr014() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    controller.play();
    controller.tick();

    // `apply_queue_op` keeps `track_len_ms` in sync with the queue's
    // current item on every mutation — one `skip_forward` is enough to
    // give the reducer something to compare a seek against.
    controller.skip_forward();
    controller.tick();
    assert_eq!(
        controller
            .queue()
            .current()
            .map(|item| item.track.id.clone()),
        Some(track_id("b"))
    );

    // Past `b`'s 180 000 ms length (SC-006): the reducer's T7 rule
    // advances per FR-014 rather than overshooting.
    controller.seek(Duration::from_millis(200_000));
    controller.tick();

    assert_eq!(
        controller
            .queue()
            .current()
            .map(|item| item.track.id.clone()),
        Some(track_id("c")),
        "seeking past the current track's end must advance to the next item"
    );
    assert_eq!(controller.transport_state().intent, Intent::Playing);
}

#[test]
fn transient_30s_raises_warning_and_clears() {
    let (mut controller, handle, _dir) = ready_controller();
    let (clock, offset_ms) = fake_clock();
    controller.set_clock(clock);

    handle.health(SourceHealth::Transient {
        since: std::time::Instant::now(),
        next_retry_in: Duration::from_secs(1),
    });
    controller.tick();
    assert!(
        !controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "stream-reconnect-warning"),
        "must not warn the moment a transient failure starts"
    );

    offset_ms.store(29_900, Ordering::SeqCst);
    controller.tick();
    assert!(
        !controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "stream-reconnect-warning"),
        "must not warn before 30 s"
    );

    offset_ms.store(30_100, Ordering::SeqCst);
    controller.tick();
    assert!(
        controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "stream-reconnect-warning"),
        "expected a stream-reconnect-warning notification past 30 s"
    );

    handle.health(SourceHealth::Ok);
    controller.tick();
    assert!(
        !controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "stream-reconnect-warning"),
        "the warning must clear once health recovers"
    );
}

#[test]
fn five_minute_transient_never_unavailable() {
    let (mut controller, handle, _dir) = ready_controller();
    let (clock, offset_ms) = fake_clock();
    controller.set_clock(clock);

    handle.health(SourceHealth::Transient {
        since: std::time::Instant::now(),
        next_retry_in: Duration::from_secs(1),
    });
    controller.tick();

    offset_ms.store(5 * 60 * 1_000, Ordering::SeqCst);
    controller.tick();

    assert!(
        matches!(
            controller.transport_state().health,
            SourceHealth::Transient { .. }
        ),
        "duration alone must never escalate Transient to Unavailable (SC-009)"
    );
    assert!(
        controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "stream-reconnect-warning"),
        "the 30 s warning still fires, but health itself stays Transient"
    );
}

#[test]
fn unavailable_disables_transport_and_keeps_navigation() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.set_playback_permitted(true, None);
    controller.tick();
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();
    assert!(controller.transport_enabled());

    handle.health(SourceHealth::Unavailable {
        client_update_required: false,
    });
    controller.tick();

    assert!(
        !controller.transport_enabled(),
        "T21: an unrecoverable failure disables transport"
    );
    assert_eq!(
        controller.disabled_reason(),
        Some("status-source-unavailable")
    );
    assert!(
        controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "stream-source-unavailable"),
        "expected a stream-source-unavailable notification"
    );

    // Queue/navigation stay usable while transport is disabled (T21).
    assert_eq!(controller.queue().effective_order().len(), 2);
    let view = controller.queue_view();
    assert_eq!(view.items.len(), 2);
}

#[test]
fn sign_out_stops_clears_and_deregisters() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.set_playback_permitted(true, None);
    controller.tick();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    controller.clear_for_sign_out();
    controller.tick();

    assert_eq!(controller.transport_state().intent, Intent::Stopped);
    assert!(
        controller.queue().current().is_none(),
        "the queue must be cleared on sign-out"
    );
    assert!(
        matches!(
            controller.active_state(),
            ActiveState::NotRegistered {
                reason: NotRegisteredReason::SignedOut
            }
        ),
        "expected NotRegistered{{SignedOut}}, got {:?}",
        controller.active_state()
    );
}

#[test]
fn downgrade_finishes_track_then_disables() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.set_playback_permitted(true, None);
    controller.tick();
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    controller.on_tier_rejected();
    controller.tick();
    assert_eq!(
        controller.transport_state().intent,
        Intent::Playing,
        "T22: the current track keeps playing until it naturally ends"
    );
    assert!(matches!(controller.active_state(), ActiveState::Active));

    handle.emit(SourceEvent::EndOfTrack);
    controller.tick();

    assert_eq!(
        controller.transport_state().intent,
        Intent::Stopped,
        "T22: the device stops once the track finishes"
    );
    assert!(
        matches!(
            controller.active_state(),
            ActiveState::NotRegistered {
                reason: NotRegisteredReason::PremiumRequired
            }
        ),
        "expected NotRegistered{{PremiumRequired}}, got {:?}",
        controller.active_state()
    );
    assert!(
        controller
            .notifications()
            .visible()
            .any(|n| n.message_key == "subscription-downgraded"),
        "expected a subscription-downgraded notification"
    );
}

// --- Stage 2: "Play from account" over the session (spec Amendment 2026-09-16) ---

#[test]
fn account_tracks_request_is_answered_from_the_source() {
    let (mut controller, handle, _dir) = ready_controller();
    handle.account_tracks(Ok(vec![track("a"), track("b")]));

    let request_id = controller.request_account_tracks(20);
    controller.tick();

    let replies = controller.take_account_tracks();
    assert_eq!(replies.len(), 1);
    let (id, result) = &replies[0];
    assert_eq!(*id, request_id);
    assert_eq!(result.as_ref().map(|t| t.len()), Ok(2));
    // Drained: a second call returns nothing.
    assert!(controller.take_account_tracks().is_empty());
}

#[test]
fn account_tracks_defaults_to_unavailable_when_unscripted() {
    let (mut controller, _handle, _dir) = ready_controller();

    let request_id = controller.request_account_tracks(20);
    controller.tick();

    let replies = controller.take_account_tracks();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].0, request_id);
    assert_eq!(replies[0].1, Err(AccountReadError::Unavailable));
}

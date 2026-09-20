// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `PlayLog` / Recently Played (data-model.md §3.5, FR-012, research R11,
//! contracts/library-and-search-core.md §6): record on the
//! `TrackStarted -> Playing` transition only (never a plain pause/resume of
//! the same track), dedupe, the 100-window, `last_played` outliving the
//! window, a save -> load restart round trip, a Connect transfer-in being
//! recorded, and an unavailable track that never reaches `Playing` never
//! being recorded. `PlayLog`'s own window/dedupe unit tests live alongside
//! the type (`library/play_log.rs`); this file pins the controller-level
//! contract — the `TrackStarted -> Playing` hook itself, plus the two
//! FR-012 edge cases (transfer-in, unavailable-skip) that only a
//! `PlaybackController` + scripted source can exercise.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef, TransferContext};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::PlaybackController;
use modplayer_core::library::persist::{LibraryPaths, load_play_log, save_play_log};
use modplayer_core::library::play_log::{PlayLog, RECENT_WINDOW};
use modplayer_core::settings::SettingsStore;
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-play-log-test-{tag}-{}-{}",
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
    let dir = TempDir::new("settings");
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

fn track_id(id: &str) -> TrackId {
    TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!())
}

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
fn playing_a_track_records_it_in_recently_played() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    let recent = controller.recently_played();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, track_id("a"));
}

#[test]
fn pause_then_resume_of_the_same_track_is_not_recorded_twice() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    assert_eq!(controller.recently_played().len(), 1);

    controller.pause();
    controller.tick();
    controller.play();
    controller.tick();

    // Still just the one recorded play — a plain pause/resume of the
    // *same* track is not a new `TrackStarted -> Playing` transition
    // (research R11).
    assert_eq!(controller.recently_played().len(), 1);
}

#[test]
fn an_unavailable_track_that_never_reaches_playing_is_never_recorded() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);
    controller.tick();
    handle.unavailable(track_id("a"));

    controller.play();
    controller.tick();

    assert!(
        controller.recently_played().is_empty(),
        "an unavailable-skipped track must never be recorded (FR-012)"
    );
}

#[test]
fn a_connect_transfer_in_is_recorded_the_same_as_a_locally_started_play() {
    let (mut controller, handle, _dir) = ready_controller();
    controller.queue_replace(vec![track("a")]);

    handle.transfer_in(Some(TransferContext {
        current: track("z"),
        position_ms: 0,
        playing: true,
        shuffle: None,
        repeat: None,
    }));
    controller.tick();

    let recent = controller.recently_played();
    assert_eq!(recent.len(), 1);
    assert_eq!(
        recent[0].id,
        track_id("z"),
        "the transferred-in track lands on the same TrackStarted -> Playing path"
    );
}

#[test]
fn recent_is_capped_at_the_window_but_last_played_outlives_it() {
    let mut log = PlayLog::new();
    for i in 0..(RECENT_WINDOW + 3) {
        log.record(&track(&i.to_string()), i as u64);
    }
    assert_eq!(log.recent().len(), RECENT_WINDOW);
    assert!(
        log.last_played_at(&track_id("0")).is_some(),
        "last_played is unbounded even once the recent window is full"
    );
}

#[test]
fn a_play_log_survives_a_simulated_restart_via_save_and_load() {
    let dir = TempDir::new("restart");
    let paths = LibraryPaths::with_dir(dir.path().to_path_buf());

    let mut original = PlayLog::new();
    original.record(&track("a"), 100);
    original.record(&track("b"), 200);
    save_play_log(&paths, &original).expect("save");

    // A fresh process would load from disk exactly like this.
    let outcome = load_play_log(&paths);
    assert_eq!(outcome.warning, None);
    assert_eq!(outcome.play_log.recent(), original.recent());
    assert_eq!(outcome.play_log.recent_tracks(), original.recent_tracks());
}

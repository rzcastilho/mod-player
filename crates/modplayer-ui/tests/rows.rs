// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `search_view::apply_row_action` — the acting-list rule wired end to end
//! through a real `PlaybackController` (contracts/library-and-search-
//! core.md §3, FR-005/006/007): a search Tracks-group row's Play now uses
//! the whole loaded list cursored at the clicked track, Play next/Add to
//! queue act on just that one track, and the three placeholder actions
//! raise `coming-soon` and record no new `SourceCommand`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, SourceCommand, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{PlaybackController, Severity};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::rows::{RowAction, RowEntity};
use modplayer_ui::search_view::apply_row_action;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-rows-{label}-{}-{unique}",
            std::process::id(),
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

fn track(id: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap(),
        format!("Title {id}"),
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

fn active_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
    TempDir,
) {
    let dir = TempDir::new(label);
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![FakeDevice {
        id: DeviceId::new("dev-1").unwrap(),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default: true,
    }];
    let mut controller = PlaybackController::new(FakeBackend::new(devices), host, store);
    controller.launch();
    controller.confirm_device(DeviceId::new("dev-1").unwrap(), BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, handle, dir)
}

#[test]
fn play_now_on_a_track_row_uses_the_whole_loaded_list_cursored_at_it() {
    let (mut controller, _handle, _dir) = active_controller("play-now");
    let loaded = vec![track("a"), track("b"), track("c")];

    apply_row_action(
        &mut controller,
        &RowEntity::Track(loaded[1].clone()),
        &loaded,
        RowAction::PlayNow,
    );

    assert_eq!(
        controller.queue().current().map(|i| i.track.id.as_str()),
        Some("spotify:track:b"),
        "playback must start on the clicked track"
    );
    let order: Vec<_> = controller
        .queue()
        .effective_order()
        .iter()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(
        order,
        vec!["spotify:track:b", "spotify:track:c"],
        "the acting list is the whole loaded list, cursored at the clicked track"
    );
}

#[test]
fn play_next_on_a_track_row_queues_only_that_track() {
    let (mut controller, _handle, _dir) = active_controller("play-next");
    let loaded = vec![track("a"), track("b"), track("c")];
    controller.queue_replace(vec![track("z")]);

    apply_row_action(
        &mut controller,
        &RowEntity::Track(loaded[0].clone()),
        &loaded,
        RowAction::PlayNext,
    );

    let play_next: Vec<_> = controller
        .queue()
        .play_next_items()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(
        play_next,
        vec!["spotify:track:a"],
        "Play next on a track row queues only that one track, not the whole loaded list"
    );
}

#[test]
fn add_to_queue_on_a_track_row_appends_only_that_track() {
    let (mut controller, _handle, _dir) = active_controller("add-to-queue");
    let loaded = vec![track("a"), track("b")];
    controller.queue_replace(vec![track("z")]);

    apply_row_action(
        &mut controller,
        &RowEntity::Track(loaded[1].clone()),
        &loaded,
        RowAction::AddToQueue,
    );

    let order: Vec<_> = controller
        .queue()
        .effective_order()
        .iter()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(order, vec!["spotify:track:z", "spotify:track:b"]);
}

#[test]
fn placeholders_raise_coming_soon_and_record_no_new_command() {
    let (mut controller, handle, _dir) = active_controller("placeholders");
    let loaded = vec![track("a")];

    for action in [
        RowAction::AddToPlaylist,
        RowAction::SaveToLibrary,
        RowAction::PinForOffline,
    ] {
        let commands_before = handle.record_commands().len();

        apply_row_action(
            &mut controller,
            &RowEntity::Track(loaded[0].clone()),
            &loaded,
            action,
        );

        let notification = controller
            .notifications()
            .visible()
            .find(|n| n.message_key == "coming-soon");
        assert!(
            notification.is_some(),
            "{action:?} must raise the coming-soon notification"
        );
        assert_eq!(notification.map(|n| n.severity), Some(Severity::Info));

        let commands_after = handle.record_commands();
        assert_eq!(
            commands_after.len(),
            commands_before,
            "{action:?} must record no new SourceCommand: {commands_after:?}"
        );
        assert!(
            !commands_after
                .iter()
                .any(|c| matches!(c, SourceCommand::LoadProgram(_))),
            "{action:?} must never load a program"
        );

        controller.notifications_mut().dismiss_by_key("coming-soon");
    }
}

#[test]
fn all_six_actions_are_represented_in_the_fixed_menu_order() {
    assert_eq!(
        RowAction::ORDER,
        [
            RowAction::PlayNow,
            RowAction::PlayNext,
            RowAction::AddToQueue,
            RowAction::AddToPlaylist,
            RowAction::SaveToLibrary,
            RowAction::PinForOffline,
        ]
    );
}

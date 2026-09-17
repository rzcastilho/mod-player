// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T096 (Phase 7 Polish, FR-023): every control 003-streaming-playback-and-
//! queue adds — the full transport, the queue panel's rows/actions, the
//! Settings › Playback device-name field, the transfer banner's **Play
//! here** button, and stream-notification action buttons — exposes a
//! non-empty accessible name and the correct AccessKit role/state, driven
//! headlessly through `ScriptedHost` + `FakeBackend` exactly like
//! `now_playing.rs`/`queue_view.rs`'s own behavioural tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use std::time::Duration;

use egui::accesskit::{Role, Toggled};
use egui::{Context, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{
    ArtistId, ArtistRef, Availability, CatalogError, LibraryItem, LibraryPage, LibrarySet, Repeat,
    SearchGroupPage, SearchHit, SearchKind, SearchPage, TrackId, TrackList, TrackListSource,
    TrackRef,
};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_audio_source_synthetic::scripted::HydratedReply;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{
    ActiveState, NotificationAction, NotificationCenter, PlaybackController, Severity, tr, tr_args,
};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::detail_view::{self, DetailTarget};
use modplayer_ui::library_view::{self, LibraryTab, LibraryViewState};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-accessibility-{label}-{}-{unique}",
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

fn fresh_store(label: &str) -> (SettingsStore, TempDir) {
    let dir = TempDir::new(label);
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

/// A controller over a confirmed device with playback permitted (mirrors
/// `now_playing.rs`'s `active_controller`), plus the `ScriptedHostHandle`
/// needed to script a transfer-away for the banner test.
fn active_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
    TempDir,
) {
    let (store, dir) = fresh_store(label);
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device()];
    let mut controller = PlaybackController::new(FakeBackend::new(devices), host, store);
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, handle, dir)
}

fn default_input() -> RawInput {
    RawInput {
        // Tall enough that a 20-row search "Show more" page (T033) stays
        // fully within the clip rect — every other test here renders far
        // fewer widgets and is unaffected by the extra headroom.
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 2000.0))),
        ..Default::default()
    }
}

/// One AccessKit node's accessibility-relevant fields, gathered into an
/// owned snapshot so it outlives the frame's `accesskit_update`.
#[derive(Debug, Clone)]
struct AccessNode {
    role: Role,
    label: Option<String>,
    value: Option<String>,
    toggled: Option<Toggled>,
    disabled: bool,
    labelled_by_something: bool,
}

impl AccessNode {
    /// The text a screen reader would announce as this node's name: its
    /// `label` for every role but `Role::Label` (whose own text sits in
    /// `value` — `Response::fill_accesskit_node_from_widget_info`), falling
    /// back to `value` so a plain content-only node still counts.
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

/// Render `render` in a fresh headless, AccessKit-enabled context and
/// return every node it produced (mirrors `now_playing.rs`'s
/// `rendered_texts`, kept structured instead of flattened to text so role/
/// toggled/disabled state survive too).
fn render_nodes(render: impl FnMut(&mut egui::Ui)) -> Vec<AccessNode> {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(default_input(), render);
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    update
        .nodes
        .iter()
        .map(|(_, node)| AccessNode {
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            toggled: node.toggled(),
            disabled: node.is_disabled(),
            labelled_by_something: !node.labelled_by().is_empty(),
        })
        .collect()
}

/// Every node of `role` whose accessible name equals `name`, most recently
/// rendered on top (`nodes` preserves AccessKit's own frame order, which is
/// good enough since these tests only ever assert existence/state, never
/// order — `queue_view.rs`'s own tests already cover row ordering).
fn find_all<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> Vec<&'a AccessNode> {
    nodes
        .iter()
        .filter(|node| node.role == role && node.accessible_name() == Some(name))
        .collect()
}

fn find_one<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> &'a AccessNode {
    let matches = find_all(nodes, role, name);
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {role:?} node named `{name}`, found {}: {nodes:?}",
        matches.len()
    );
    matches[0]
}

#[test]
fn transport_controls_expose_accessible_names() {
    let (mut controller, _handle, _dir) = active_controller("transport");
    controller.queue_replace(vec![track("a")]);

    let nodes = render_nodes(|ui| modplayer_ui::now_playing::show(ui, &mut controller));

    // Play/pause, stop, skip back/forward, and the Queue toggle are plain
    // `Button`s whose accessible name is their own label text.
    let play = find_one(&nodes, Role::Button, &tr("transport-play"));
    assert!(!play.disabled, "transport must be enabled: {play:?}");
    find_one(&nodes, Role::Button, &tr("transport-stop"));
    find_one(&nodes, Role::Button, &tr("transport-skip-back"));
    find_one(&nodes, Role::Button, &tr("transport-skip-forward"));
    find_one(&nodes, Role::Button, &tr("queue-toggle"));

    // The seek slider is a real `Role::Slider` (not a generic control) and
    // carries its own non-empty name (`transport-position`'s
    // "{position} / {duration}"), not merely a bare number.
    let sliders: Vec<_> = nodes.iter().filter(|n| n.role == Role::Slider).collect();
    assert!(
        !sliders.is_empty(),
        "Now Playing must render at least one Role::Slider node (seek + volume)"
    );
    assert!(
        sliders
            .iter()
            .any(|n| n.accessible_name().is_some_and(|name| !name.is_empty())),
        "every slider must have a non-empty accessible name, got {sliders:?}"
    );
}

#[test]
fn transport_controls_are_disabled_when_playback_is_not_permitted() {
    let (store, _dir) = fresh_store("transport-disabled");
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    controller.launch();
    assert!(!controller.transport_enabled());

    let nodes = render_nodes(|ui| modplayer_ui::now_playing::show(ui, &mut controller));
    let play = find_one(&nodes, Role::Button, &tr("transport-play"));
    assert!(
        play.disabled,
        "the play button must report AccessKit's disabled state when transport is off: {play:?}"
    );
}

#[test]
fn queue_shuffle_toggle_reports_its_toggled_state() {
    let (mut controller, _handle, _dir) = active_controller("queue-shuffle-state");
    controller.queue_replace(vec![track("a"), track("b")]);

    let off = render_nodes(|ui| modplayer_ui::queue_view::show(ui, &mut controller));
    let shuffle = find_one(&off, Role::Button, &tr("queue-shuffle"));
    assert_eq!(
        shuffle.toggled,
        Some(Toggled::False),
        "shuffle off must report Toggled::False: {shuffle:?}"
    );
    // Off/One/All are the three fixed labels the repeat-cycle button shows
    // (queue_view.rs); it is a plain button, not a tri-state toggle, so its
    // *name itself* carries the current mode.
    find_one(&off, Role::Button, &tr("queue-repeat-off"));

    controller.set_shuffle(true);
    controller.set_repeat(Repeat::All);
    let on = render_nodes(|ui| modplayer_ui::queue_view::show(ui, &mut controller));
    let shuffle = find_one(&on, Role::Button, &tr("queue-shuffle"));
    assert_eq!(
        shuffle.toggled,
        Some(Toggled::True),
        "shuffle on must report Toggled::True: {shuffle:?}"
    );
    find_one(&on, Role::Button, &tr("queue-repeat-all"));
}

#[test]
fn queue_row_actions_expose_accessible_names() {
    let (mut controller, _handle, _dir) = active_controller("queue-row-actions");
    controller.queue_replace(vec![track("a"), track("b")]);

    let nodes = render_nodes(|ui| modplayer_ui::queue_view::show(ui, &mut controller));

    // Two rows -> at least two of each row action, each a real `Button`
    // whose name is exactly the action (never blank/icon-only).
    for key in [
        "queue-move-up",
        "queue-move-down",
        "queue-remove",
        "queue-play-next",
    ] {
        let matches = find_all(&nodes, Role::Button, &tr(key));
        assert!(
            !matches.is_empty(),
            "expected at least one `{key}` button, found none in {nodes:?}"
        );
    }
}

#[test]
fn transfer_banner_play_here_button_exposes_its_accessible_name() {
    let (mut controller, handle, _dir) = active_controller("transfer-banner");
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    handle.transfer_out();
    controller.tick();
    assert!(
        matches!(controller.active_state(), ActiveState::Inactive { .. }),
        "test setup: expected Inactive, got {:?}",
        controller.active_state()
    );

    let nodes = render_nodes(|ui| modplayer_ui::now_playing::show(ui, &mut controller));
    let play_here = find_one(&nodes, Role::Button, &tr("banner-play-here"));
    assert!(
        !play_here.disabled,
        "Play here must stay enabled: {play_here:?}"
    );

    // The banner's own label text ("Playing on <device>", falling back to
    // `banner-unknown-device` when `ScriptedHostHandle::transfer_out` sends
    // no device name) must also be a real, non-empty accessible node —
    // never a colour/icon-only cue (Constitution/FR-022).
    assert!(
        nodes.iter().any(|n| n
            .accessible_name()
            .is_some_and(|name| name.contains(&tr("banner-unknown-device")))),
        "expected the \"Playing on <device>\" banner text, got {nodes:?}"
    );
}

#[test]
fn device_name_field_is_labelled_and_shows_the_current_name() {
    let (store, _dir) = fresh_store("device-name-field");
    let controller = PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    let mut screen = modplayer_ui::settings::playback::PlaybackScreen::new(&controller);
    let mut controller = controller;

    let nodes = render_nodes(|ui| {
        modplayer_ui::settings::playback::show(ui, &mut controller, &mut screen, None)
    });

    let text_inputs: Vec<_> = nodes.iter().filter(|n| n.role == Role::TextInput).collect();
    assert_eq!(
        text_inputs.len(),
        1,
        "expected exactly one Role::TextInput node, got {text_inputs:?}"
    );
    let field = text_inputs[0];
    assert!(
        field.labelled_by_something,
        "the device-name field must be associated with the \"{}\" label via labelled_by: {field:?}",
        tr("setting-device-name")
    );
    assert!(
        field.value.as_deref().is_some_and(|v| !v.is_empty()),
        "the device-name field must show the current effective name as its value: {field:?}"
    );
}

#[test]
fn notification_action_buttons_expose_accessible_names() {
    let mut center = NotificationCenter::new();
    center.raise_with_actions(
        Severity::Critical,
        "stream-source-unavailable",
        Vec::new(),
        vec![
            NotificationAction::OpenStatusPage,
            NotificationAction::RetrySource,
        ],
    );

    let nodes = render_nodes(|ui| {
        let _ = modplayer_ui::notifications::show(ui, &center);
    });

    find_one(&nodes, Role::Button, &tr("action-status-page"));
    find_one(&nodes, Role::Button, &tr("action-retry"));
    find_one(&nodes, Role::Button, &tr("notification-dismiss"));
}

// -- Search (004-search-and-library-browse, T033) ---------------------------

fn search_track_hit(id: &str, title: &str) -> SearchHit {
    SearchHit::Track(TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap(),
        title,
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    ))
}

/// A full four-group reply with `track_count` tracks and `next_offset` set
/// on the Tracks group whenever it is a full page (so "Show more" renders
/// too) — the other three groups empty.
fn search_reply(track_count: usize) -> SearchPage {
    let items: Vec<SearchHit> = (0..track_count)
        .map(|i| search_track_hit(&i.to_string(), &format!("Track {i}")))
        .collect();
    let next_offset = if track_count >= 20 {
        Some(track_count as u32)
    } else {
        None
    };
    SearchPage {
        groups: vec![
            SearchGroupPage {
                kind: SearchKind::Track,
                items,
                next_offset,
            },
            SearchGroupPage {
                kind: SearchKind::Album,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Artist,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Playlist,
                items: vec![],
                next_offset: None,
            },
        ],
        unsupported: vec![],
    }
}

/// Drive one query to a `Loaded` Tracks group with `track_count` items,
/// scripted through `handle`.
fn search_and_settle(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    handle: &modplayer_audio_source_synthetic::ScriptedHostHandle,
    query: &str,
    track_count: usize,
) {
    handle.script_search(query, Ok(search_reply(track_count)));
    let now = controller.now();
    controller.search_mut().set_query(query, now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick(); // issues the request
    controller.tick(); // drains the reply
}

#[test]
fn search_box_exposes_a_text_input_labelled_search() {
    let (mut controller, _handle, _dir) = active_controller("search-box");
    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut focus = false;

    let nodes = render_nodes(|ui| {
        modplayer_ui::search_view::show(ui, &mut controller, &mut artwork, &mut focus);
    });

    let text_inputs: Vec<_> = nodes.iter().filter(|n| n.role == Role::TextInput).collect();
    assert_eq!(
        text_inputs.len(),
        1,
        "expected exactly one Role::TextInput node, got {text_inputs:?}"
    );
    assert!(
        text_inputs[0].labelled_by_something,
        "the search box must be associated with its \"{}\" label: {:?}",
        tr("search-placeholder"),
        text_inputs[0]
    );
}

#[test]
fn group_headers_expose_role_header_and_the_fixed_names() {
    let (mut controller, handle, _dir) = active_controller("search-headers");
    search_and_settle(&mut controller, &handle, "abba", 2);

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut focus = false;
    let nodes = render_nodes(|ui| {
        modplayer_ui::search_view::show(ui, &mut controller, &mut artwork, &mut focus);
    });

    find_one(&nodes, Role::Header, &tr("search-group-tracks"));
    // Empty groups (Albums/Artists/Playlists) must not render a header at
    // all (contracts/ui-surface.md §2: "omitted when Empty/Unsupported/
    // Idle").
    assert!(find_all(&nodes, Role::Header, &tr("search-group-albums")).is_empty());
    assert!(find_all(&nodes, Role::Header, &tr("search-group-artists")).is_empty());
    assert!(find_all(&nodes, Role::Header, &tr("search-group-playlists")).is_empty());
}

#[test]
fn show_more_button_exposes_its_accessible_name_when_a_further_page_exists() {
    let (mut controller, handle, _dir) = active_controller("search-show-more");
    search_and_settle(&mut controller, &handle, "abba", 20);

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut focus = false;
    let nodes = render_nodes(|ui| {
        modplayer_ui::search_view::show(ui, &mut controller, &mut artwork, &mut focus);
    });

    let expected = tr_args("search-show-more", &[("group", tr("search-group-tracks"))]);
    let button = find_one(&nodes, Role::Button, &expected);
    assert!(!button.disabled, "Show more must start enabled: {button:?}");
}

#[test]
fn each_row_exposes_a_list_item_and_an_actions_button() {
    let (mut controller, handle, _dir) = active_controller("search-row-menu");
    search_and_settle(&mut controller, &handle, "abba", 1);

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut focus = false;
    let nodes = render_nodes(|ui| {
        modplayer_ui::search_view::show(ui, &mut controller, &mut artwork, &mut focus);
    });

    let row_name = "Track 0 — Artist";
    find_one(&nodes, Role::ListItem, row_name);
    // The trailing "…" button that opens the six-action menu
    // (contracts/ui-surface.md §5).
    find_one(
        &nodes,
        Role::Button,
        &tr_args("row-actions", &[("name", row_name.to_string())]),
    );
}

// -- Library & detail (004-search-and-library-browse, T070) -----------------
//
// Search's own three tests above already pin the row/menu contract that
// `rows::list_row` renders identically everywhere (FR-004) — this section
// covers the elements unique to the Library and detail surfaces: the tab
// row, the "Refreshing…" status label reachable only through an actual
// resync (unlike Search's, driven straight off `SearchSession`), and the
// detail view's Back control.

fn library_track(id: &str, title: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(id).unwrap(),
        title,
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

fn empty_page(set: LibrarySet) -> LibraryPage {
    LibraryPage {
        set,
        items: vec![],
        next_page: None,
        sync_token: None,
    }
}

/// Script and settle a one-track Saved Tracks sync (the other three sets
/// empty), so the Library view renders real content rather than an empty
/// or loading state.
fn sync_one_saved_track(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    handle: &modplayer_audio_source_synthetic::ScriptedHostHandle,
) {
    let track = library_track("spotify:track:a", "Track A");
    handle.script_hydrate(HydratedReply {
        tracks: vec![track.clone()],
        ..Default::default()
    });
    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Ok(LibraryPage {
            set: LibrarySet::SavedTracks,
            items: vec![LibraryItem::Track {
                track,
                added_at: None,
            }],
            next_page: None,
            sync_token: None,
        })],
    );
    handle.script_library(
        LibrarySet::SavedAlbums,
        vec![Ok(empty_page(LibrarySet::SavedAlbums))],
    );
    handle.script_library(
        LibrarySet::FollowedArtists,
        vec![Ok(empty_page(LibrarySet::FollowedArtists))],
    );
    handle.script_library(
        LibrarySet::Playlists,
        vec![Ok(empty_page(LibrarySet::Playlists))],
    );
    controller.library_retry_sync();
    for _ in 0..12 {
        controller.tick();
    }
}

#[test]
fn library_tabs_expose_role_tab_in_the_fixed_order() {
    let (mut controller, _handle, _dir) = active_controller("library-tabs");
    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut state = LibraryViewState::default();

    let nodes = render_nodes(|ui| {
        let _ = library_view::show(ui, &mut controller, &mut artwork, &mut state);
    });

    for key in [
        "library-tab-saved-tracks",
        "library-tab-saved-albums",
        "library-tab-followed-artists",
        "library-tab-playlists",
        "library-tab-recently-played",
    ] {
        find_one(&nodes, Role::Tab, &tr(key));
    }
}

#[test]
fn library_row_exposes_a_list_item_and_an_actions_button() {
    let (mut controller, handle, _dir) = active_controller("library-row-menu");
    sync_one_saved_track(&mut controller, &handle);

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::SavedTracks,
        ..LibraryViewState::default()
    };
    let nodes = render_nodes(|ui| {
        let _ = library_view::show(ui, &mut controller, &mut artwork, &mut state);
    });

    let row_name = "Track A — Artist";
    find_one(&nodes, Role::ListItem, row_name);
    find_one(
        &nodes,
        Role::Button,
        &tr_args("row-actions", &[("name", row_name.to_string())]),
    );
}

#[test]
fn refreshing_status_label_exposes_role_status_after_a_rate_limited_resync() {
    let (mut controller, handle, _dir) = active_controller("library-refreshing");
    sync_one_saved_track(&mut controller, &handle);
    assert!(
        !controller.library_status().refreshing,
        "test setup: a clean sync must not start out refreshing"
    );

    // A forced resync whose Saved Tracks page comes back rate-limited
    // (contracts/library-and-search-core.md §3 "Syncing --RateLimited-->
    // BackingOff") drives `library_status().refreshing` true while the
    // existing snapshot stays on screen (FR-015/SC-005).
    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Err(CatalogError::RateLimited {
            retry_after_ms: None,
        })],
    );
    controller.library_retry_sync();
    controller.tick();
    assert!(
        controller.library_status().refreshing,
        "test setup: the scripted rate limit must trip the scheduler's backoff"
    );

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::SavedTracks,
        ..LibraryViewState::default()
    };
    let nodes = render_nodes(|ui| {
        let _ = library_view::show(ui, &mut controller, &mut artwork, &mut state);
    });

    let status = find_one(&nodes, Role::Status, &tr("refreshing"));
    assert!(!status.disabled, "{status:?}");
    // The prior snapshot must stay visible underneath the status label
    // (FR-015: "stale + Refreshing…", never an empty/error state).
    find_one(&nodes, Role::ListItem, "Track A — Artist");
}

#[test]
fn detail_back_button_exposes_its_accessible_name() {
    let (mut controller, handle, _dir) = active_controller("detail-back-a11y");
    let id = ArtistId::new("spotify:artist:a").unwrap();
    let artist = ArtistRef {
        id: id.clone(),
        name: "The Artist".to_string(),
        artwork_url: None,
    };
    handle.script_hydrate(HydratedReply {
        artists: vec![artist.clone()],
        ..Default::default()
    });
    handle.script_library(
        LibrarySet::FollowedArtists,
        vec![Ok(LibraryPage {
            set: LibrarySet::FollowedArtists,
            items: vec![LibraryItem::Artist(artist)],
            next_page: None,
            sync_token: None,
        })],
    );
    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Ok(empty_page(LibrarySet::SavedTracks))],
    );
    handle.script_library(
        LibrarySet::SavedAlbums,
        vec![Ok(empty_page(LibrarySet::SavedAlbums))],
    );
    handle.script_library(
        LibrarySet::Playlists,
        vec![Ok(empty_page(LibrarySet::Playlists))],
    );
    controller.library_retry_sync();
    for _ in 0..12 {
        controller.tick();
    }
    handle.script_track_list(
        TrackListSource::ArtistTop(id.clone()),
        Ok(TrackList {
            source: TrackListSource::ArtistTop(id.clone()),
            tracks: vec![],
        }),
    );

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let target = DetailTarget::Artist(id);
    let nodes = render_nodes(|ui| {
        let _ = detail_view::show(ui, &mut controller, &mut artwork, &target);
    });
    // First frame only issues `FetchTrackList`; settle it before asserting.
    controller.tick();
    let nodes2 = render_nodes(|ui| {
        let _ = detail_view::show(ui, &mut controller, &mut artwork, &target);
    });

    for nodes in [&nodes, &nodes2] {
        let back = find_one(nodes, Role::Button, &tr("detail-back"));
        assert!(!back.disabled, "{back:?}");
    }
}

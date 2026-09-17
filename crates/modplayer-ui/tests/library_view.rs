// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `library_view.rs`/`detail_view.rs` behaviour (contracts/ui-surface.md
//! §3/§4, FR-009-013/017/021): tab order and initial tab, each empty
//! state's own copy and action, FR-021's first-sync-failed state, the
//! playlist owner label and the absence of any edit control (FR-010), an
//! offline snapshot rendering with no error, and the three detail views.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use egui::accesskit::Role;
use egui::{Context, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, Availability, CatalogError, LibraryItem, LibraryPage,
    LibrarySet, PlaylistId, PlaylistRef, SourceHost, TrackId, TrackList, TrackListSource, TrackRef,
};
use modplayer_audio_source_synthetic::scripted::HydratedReply;
use modplayer_audio_source_synthetic::{ScriptedHost, SyntheticHost};
use modplayer_core::library::LibraryPaths;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{PlaybackController, tr, tr_args};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::detail_view::{self, DetailOutcome, DetailTarget};
use modplayer_ui::library_view::{self, LibraryOutcome, LibraryTab, LibraryViewState};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-library-view-{label}-{}-{unique}",
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
        id: DeviceId::new("dev-1").unwrap(),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default: true,
    }
}

/// A controller over a confirmed, registered device (mirrors
/// `search_view.rs`'s `active_controller`), used for every test that needs
/// the derived "online" fact true so the sync cycle actually runs.
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
    controller.confirm_device(DeviceId::new("dev-1").unwrap(), BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, handle, dir)
}

fn track(id: &str, title: &str) -> TrackRef {
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

fn album(id: &str, name: &str) -> AlbumRef {
    AlbumRef {
        id: AlbumId::new(id).unwrap(),
        name: name.to_string(),
        artists: vec!["Artist".to_string()],
        artwork_url: None,
        release_date: None,
        track_count: 1,
    }
}

fn artist(id: &str, name: &str) -> ArtistRef {
    ArtistRef {
        id: ArtistId::new(id).unwrap(),
        name: name.to_string(),
        artwork_url: None,
    }
}

fn playlist(id: &str, name: &str, owner_name: &str, editable: bool) -> PlaylistRef {
    PlaylistRef {
        id: PlaylistId::new(id).unwrap(),
        name: name.to_string(),
        owner_name: owner_name.to_string(),
        editable,
        artwork_url: None,
        track_count: 1,
        revision: None,
    }
}

fn empty_page(set: LibrarySet) -> LibraryPage {
    LibraryPage {
        set,
        items: vec![],
        next_page: None,
        sync_token: None,
    }
}

/// Script a full, successful four-set sync cycle and walk it to
/// completion (mirrors `controller_streaming.rs`'s
/// `library_page_and_hydrated_events_route_into_the_library_index...`).
/// `merge_page` itself only ever records identity + `added_at` for
/// Saved Tracks/Albums/Followed Artists and queues the id for lazy
/// hydration (`index.rs`'s own doc comment, research R4) — Playlists is
/// the sole set resolved immediately. So this helper also derives a
/// `HydratedReply` from every full ref the three lazy sets were given and
/// scripts it, letting the one sweep due immediately (`maybe_sweep_
/// hydration`, throttle window not yet elapsed on a fresh controller)
/// resolve every row this same walk.
fn sync_library(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    handle: &modplayer_audio_source_synthetic::ScriptedHostHandle,
    saved_tracks: Vec<LibraryItem>,
    saved_albums: Vec<LibraryItem>,
    followed_artists: Vec<LibraryItem>,
    playlists: Vec<LibraryItem>,
) {
    let mut hydrate = HydratedReply::default();
    for item in &saved_tracks {
        if let LibraryItem::Track { track, .. } = item {
            hydrate.tracks.push(track.clone());
        }
    }
    for item in &saved_albums {
        if let LibraryItem::Album { album, .. } = item {
            hydrate.albums.push(album.clone());
        }
    }
    for item in &followed_artists {
        if let LibraryItem::Artist(artist) = item {
            hydrate.artists.push(artist.clone());
        }
    }
    if !hydrate.tracks.is_empty() || !hydrate.albums.is_empty() || !hydrate.artists.is_empty() {
        handle.script_hydrate(hydrate);
    }

    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Ok(LibraryPage {
            set: LibrarySet::SavedTracks,
            items: saved_tracks,
            next_page: None,
            sync_token: None,
        })],
    );
    handle.script_library(
        LibrarySet::SavedAlbums,
        vec![Ok(LibraryPage {
            set: LibrarySet::SavedAlbums,
            items: saved_albums,
            next_page: None,
            sync_token: None,
        })],
    );
    handle.script_library(
        LibrarySet::FollowedArtists,
        vec![Ok(LibraryPage {
            set: LibrarySet::FollowedArtists,
            items: followed_artists,
            next_page: None,
            sync_token: None,
        })],
    );
    handle.script_library(
        LibrarySet::Playlists,
        vec![Ok(LibraryPage {
            set: LibrarySet::Playlists,
            items: playlists,
            next_page: None,
            sync_token: None,
        })],
    );
    controller.library_retry_sync();
    // Each hop's follow-up `FetchLibrary` is only visible to the *next*
    // `poll()` (contracts/catalog-source.md §4) — four sets need at most
    // four hops, plus the hydration sweep's own request/reply pair; a few
    // extra ticks are harmless.
    for _ in 0..12 {
        controller.tick();
    }
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 1200.0))),
        ..Default::default()
    }
}

fn back_key_input() -> RawInput {
    RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }],
        ..default_input()
    }
}

#[derive(Debug, Clone)]
struct Node {
    role: Role,
    label: Option<String>,
    value: Option<String>,
}

impl Node {
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

fn collect_nodes(ctx: &Context, mut output: egui::FullOutput) -> Vec<Node> {
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    let _ = ctx;
    update
        .nodes
        .iter()
        .map(|(_, node)| Node {
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
        })
        .collect()
}

fn render_library<H: SourceHost>(
    controller: &mut PlaybackController<FakeBackend, H>,
    artwork: &mut ArtworkCache,
    state: &mut LibraryViewState,
) -> (Vec<Node>, LibraryOutcome) {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut outcome = LibraryOutcome::None;
    let output = ctx.run_ui(default_input(), |ui| {
        outcome = library_view::show(ui, controller, artwork, state);
    });
    (collect_nodes(&ctx, output), outcome)
}

fn render_detail(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    target: &DetailTarget,
    input: RawInput,
) -> (Vec<Node>, DetailOutcome) {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut outcome = DetailOutcome::None;
    let output = ctx.run_ui(input, |ui| {
        outcome = detail_view::show(ui, controller, artwork, target);
    });
    (collect_nodes(&ctx, output), outcome)
}

fn has(nodes: &[Node], role: Role, label: &str) -> bool {
    nodes
        .iter()
        .any(|n| n.role == role && n.accessible_name() == Some(label))
}

/// Role-agnostic accessible-name check, for widgets (`ui.label`/
/// `ui.heading`/`ui.button`) whose exact accesskit `Role` this test suite
/// doesn't otherwise pin down — mirrors `search_view.rs`'s offline/
/// no-results assertions, which check the name alone for the same reason.
fn has_name(nodes: &[Node], name: &str) -> bool {
    nodes.iter().any(|n| n.accessible_name() == Some(name))
}

fn count(nodes: &[Node], role: Role) -> usize {
    nodes.iter().filter(|n| n.role == role).count()
}

/// Every `role` node's accessible name, in true visual/traversal order:
/// `TreeUpdate::nodes`' own order is unspecified (its doc comment: "Order
/// doesn't matter") — only a node's own `children()` list, walked from the
/// tree's `root`, reflects the order egui actually laid the nodes out in
/// (e.g. the fixed tab order, or a virtualised list's row order once it
/// renders inside a `ScrollArea`, US3 T071).
fn labels_in_tree_order(update: &egui::accesskit::TreeUpdate, role: Role) -> Vec<String> {
    let by_id: std::collections::HashMap<egui::accesskit::NodeId, &egui::accesskit::Node> =
        update.nodes.iter().map(|(id, n)| (*id, n)).collect();
    let root = update
        .tree
        .as_ref()
        .expect("a TreeUpdate with tree metadata")
        .root;
    let mut order = Vec::new();
    let mut stack = vec![root];
    // A plain stack-based DFS (rather than recursion) visits children in
    // list order as long as they're pushed in reverse.
    while let Some(id) = stack.pop() {
        let Some(node) = by_id.get(&id) else { continue };
        if node.role() == role
            && let Some(name) = node.label().or_else(|| node.value())
        {
            order.push(name.to_string());
        }
        stack.extend(node.children().iter().rev().copied());
    }
    order
}

// -- Tab order / initial tab -------------------------------------------

#[test]
fn tabs_render_in_the_fixed_order() {
    let (mut controller, _handle, _dir) = active_controller("tab-order");
    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState::default();

    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(default_input(), |ui| {
        let _ = library_view::show(ui, &mut controller, &mut artwork, &mut state);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let labels = labels_in_tree_order(&update, Role::Tab);
    output.drop_without_applying_deltas();

    assert_eq!(
        labels,
        vec![
            tr("library-tab-saved-tracks"),
            tr("library-tab-saved-albums"),
            tr("library-tab-followed-artists"),
            tr("library-tab-playlists"),
            tr("library-tab-recently-played"),
        ],
        "tabs must render Saved Tracks, Saved Albums, Followed Artists, \
         Playlists, Recently Played in that fixed order"
    );
}

#[test]
fn opens_on_saved_tracks_by_default() {
    let (mut controller, handle, _dir) = active_controller("initial-tab");
    sync_library(
        &mut controller,
        &handle,
        vec![LibraryItem::Track {
            track: track("spotify:track:a", "Track A"),
            added_at: None,
        }],
        vec![LibraryItem::Album {
            album: album("spotify:album:a", "Album A"),
            added_at: None,
        }],
        vec![],
        vec![],
    );

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState::default();
    assert_eq!(
        state.tab,
        LibraryTab::SavedTracks,
        "the default tab is Saved Tracks"
    );
    let (nodes, _outcome) = render_library(&mut controller, &mut artwork, &mut state);

    assert!(
        has(&nodes, Role::ListItem, "Track A — Artist"),
        "the saved track must render on the default tab: {nodes:?}"
    );
    assert_eq!(
        count(&nodes, Role::ListItem),
        1,
        "only the Saved Tracks tab's own row renders — not Saved Albums': {nodes:?}"
    );
}

// -- Loading / first-sync-failed / retry --------------------------------

#[test]
fn loading_state_shows_skeleton_rows() {
    let (store, dir) = fresh_store("loading");
    let paths = LibraryPaths::with_dir(dir.path().join("does-not-exist"));
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store)
            .with_library_paths(Some(paths));
    // No `tick()` yet: `library_loading` is set the moment
    // `with_library_paths` spawns the background load, and only `tick()`'s
    // `drain_library_load` ever clears it — so this render deterministically
    // observes `loading == true` regardless of how fast the background
    // thread finishes reading a nonexistent directory.
    assert!(controller.library_status().loading);

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState::default();
    let (nodes, outcome) = render_library(&mut controller, &mut artwork, &mut state);

    assert_eq!(outcome, LibraryOutcome::None);
    assert!(
        has(&nodes, Role::Status, &tr("loading")),
        "expected skeleton rows (Role::Status \"Loading\") while the index \
         hasn't been read from disk yet: {nodes:?}"
    );
    assert!(
        !has_name(&nodes, &tr("library-empty")),
        "the empty state must not race ahead of the loading state: {nodes:?}"
    );
}

#[test]
fn first_sync_failed_with_no_snapshot_shows_retry_fr021() {
    let (mut controller, handle, _dir) = active_controller("first-sync-failed");
    // A cycle only *completes* — and so only records an outcome — once
    // every set in it has answered (`SyncScheduler::apply_reply`'s error
    // arm still advances to the next set); one set failing is enough to
    // fail the whole cycle, but every set still needs a scripted reply.
    handle.script_library(LibrarySet::SavedTracks, vec![Err(CatalogError::Offline)]);
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
    for _ in 0..10 {
        controller.tick();
    }
    assert!(controller.library_status().first_sync_failed);

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState::default();
    let (nodes, _outcome) = render_library(&mut controller, &mut artwork, &mut state);

    assert!(
        has_name(&nodes, &tr("library-first-sync-failed")),
        "expected the FR-021 inline failure state: {nodes:?}"
    );
    assert!(
        has_name(&nodes, &tr("library-retry")),
        "expected a Retry button: {nodes:?}"
    );

    // The Retry button's handler is exactly `library_retry_sync()`
    // (`library_view.rs`'s own source): FR-021's guarantee is that it
    // issues a fresh `FetchLibrary` regardless of the 15-min interval or
    // current backoff — proven directly against the same method the
    // button calls (`library_sync.rs`'s scheduler-level tests already
    // cover the state machine this rests on).
    let commands_before = handle.record_commands().len();
    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Ok(empty_page(LibrarySet::SavedTracks))],
    );
    controller.library_retry_sync();
    assert!(
        handle.record_commands().len() > commands_before,
        "Retry must issue a fresh FetchLibrary command"
    );
}

// -- Empty states ---------------------------------------------------------

#[test]
fn whole_library_empty_shows_search_action() {
    let (mut controller, handle, _dir) = active_controller("whole-empty");
    sync_library(&mut controller, &handle, vec![], vec![], vec![], vec![]);

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState::default();
    let (nodes, outcome) = render_library(&mut controller, &mut artwork, &mut state);
    assert!(
        has_name(&nodes, &tr("library-empty")),
        "expected the whole-library empty state: {nodes:?}"
    );
    assert!(has_name(&nodes, &tr("action-search")), "{nodes:?}");
    // No click happened this frame, so `App` (T065) has nothing to act on
    // yet — `library_view.rs::show`'s own doc comment names `FocusSearch`
    // as exactly this button's outcome; `App::show_library` (app.rs)
    // switches `Section::Search` and sets `focus_search_requested` on it.
    assert_eq!(outcome, LibraryOutcome::None);
}

#[test]
fn saved_albums_empty_state_shows_its_own_copy() {
    let (mut controller, handle, _dir) = active_controller("albums-empty");
    // Saved Tracks non-empty so the whole-library banner doesn't shadow
    // Saved Albums' own per-tab empty state.
    sync_library(
        &mut controller,
        &handle,
        vec![LibraryItem::Track {
            track: track("spotify:track:a", "Track A"),
            added_at: None,
        }],
        vec![],
        vec![],
        vec![],
    );

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::SavedAlbums,
        ..LibraryViewState::default()
    };
    let (nodes, outcome) = render_library(&mut controller, &mut artwork, &mut state);
    assert!(has_name(&nodes, &tr("library-empty-albums")), "{nodes:?}");
    assert!(has_name(&nodes, &tr("action-search")), "{nodes:?}");
    assert_eq!(outcome, LibraryOutcome::None);
}

#[test]
fn followed_artists_empty_state_shows_its_own_copy() {
    let (mut controller, handle, _dir) = active_controller("artists-empty");
    sync_library(
        &mut controller,
        &handle,
        vec![LibraryItem::Track {
            track: track("spotify:track:a", "Track A"),
            added_at: None,
        }],
        vec![],
        vec![],
        vec![],
    );

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::FollowedArtists,
        ..LibraryViewState::default()
    };
    let (nodes, _outcome) = render_library(&mut controller, &mut artwork, &mut state);
    assert!(has_name(&nodes, &tr("library-empty-artists")), "{nodes:?}");
    assert!(has_name(&nodes, &tr("action-search")), "{nodes:?}");
}

#[test]
fn recently_played_empty_state_shows_its_own_copy() {
    let (mut controller, handle, _dir) = active_controller("recent-empty");
    sync_library(
        &mut controller,
        &handle,
        vec![LibraryItem::Track {
            track: track("spotify:track:a", "Track A"),
            added_at: None,
        }],
        vec![],
        vec![],
        vec![],
    );

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::RecentlyPlayed,
        ..LibraryViewState::default()
    };
    let (nodes, _outcome) = render_library(&mut controller, &mut artwork, &mut state);
    assert!(has_name(&nodes, &tr("library-empty-recent")), "{nodes:?}");
    assert!(has_name(&nodes, &tr("action-search")), "{nodes:?}");
}

#[test]
fn playlists_empty_state_shows_create_action_and_its_placeholder_is_inert() {
    let (mut controller, handle, _dir) = active_controller("playlists-empty");
    sync_library(
        &mut controller,
        &handle,
        vec![LibraryItem::Track {
            track: track("spotify:track:a", "Track A"),
            added_at: None,
        }],
        vec![],
        vec![],
        vec![],
    );

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::Playlists,
        ..LibraryViewState::default()
    };
    let commands_before = handle.record_commands().len();
    let (nodes, outcome) = render_library(&mut controller, &mut artwork, &mut state);
    assert!(
        has_name(&nodes, &tr("library-empty-playlists")),
        "{nodes:?}"
    );
    assert!(
        has_name(&nodes, &tr("action-create")),
        "Playlists' own empty-state action is Create, not Search: {nodes:?}"
    );
    // No click happened this frame (FR-005: the Create placeholder has no
    // real effect to report even when clicked — `show_playlists` raises
    // `coming-soon` straight into the notification center itself rather
    // than surfacing a `LibraryOutcome`, unlike Search's `FocusSearch`).
    assert_eq!(outcome, LibraryOutcome::None);
    assert_eq!(
        handle.record_commands().len(),
        commands_before,
        "merely rendering the empty state must issue no SourceCommand"
    );
}

// -- Playlist ownership / no edit controls -------------------------------

#[test]
fn followed_playlist_shows_owner_label() {
    let (mut controller, handle, _dir) = active_controller("followed-playlist");
    sync_library(
        &mut controller,
        &handle,
        vec![],
        vec![],
        vec![],
        vec![LibraryItem::Playlist(playlist(
            "spotify:playlist:a",
            "Road Trip",
            "Alex",
            false,
        ))],
    );

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::Playlists,
        ..LibraryViewState::default()
    };
    let (nodes, _outcome) = render_library(&mut controller, &mut artwork, &mut state);
    let expected = tr_args("playlist-owner", &[("name", "Alex".to_string())]);
    assert!(
        nodes
            .iter()
            .any(|n| n.accessible_name() == Some(expected.as_str())),
        "expected an \"Owner: Alex\" label since this playlist is not owned \
         (contracts/ui-surface.md §3): {nodes:?}"
    );
}

#[test]
fn owned_playlist_shows_no_owner_label() {
    let (mut controller, handle, _dir) = active_controller("owned-playlist");
    sync_library(
        &mut controller,
        &handle,
        vec![],
        vec![],
        vec![],
        vec![LibraryItem::Playlist(playlist(
            "spotify:playlist:a",
            "My Mix",
            "Me",
            true,
        ))],
    );

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::Playlists,
        ..LibraryViewState::default()
    };
    let (nodes, _outcome) = render_library(&mut controller, &mut artwork, &mut state);
    assert!(
        !nodes.iter().any(|n| n
            .accessible_name()
            .is_some_and(|name| name.contains("Owner:"))),
        "an owned (editable) playlist must show no owner label: {nodes:?}"
    );
}

#[test]
fn no_edit_control_renders_on_any_playlist_row() {
    let (mut controller, handle, _dir) = active_controller("no-edit-controls");
    sync_library(
        &mut controller,
        &handle,
        vec![],
        vec![],
        vec![],
        vec![
            LibraryItem::Playlist(playlist("spotify:playlist:a", "Road Trip", "Alex", false)),
            LibraryItem::Playlist(playlist("spotify:playlist:b", "My Mix", "Me", true)),
        ],
    );

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::Playlists,
        ..LibraryViewState::default()
    };
    let (nodes, _outcome) = render_library(&mut controller, &mut artwork, &mut state);
    for keyword in ["edit", "rename", "reorder"] {
        assert!(
            !nodes.iter().any(|n| n
                .accessible_name()
                .is_some_and(|name| name.to_ascii_lowercase().contains(keyword))),
            "FR-010: no rename/reorder/edit control may exist anywhere \
             (found something matching \"{keyword}\"): {nodes:?}"
        );
    }
}

// -- Offline with a snapshot ----------------------------------------------

#[test]
fn offline_with_a_snapshot_renders_normally_not_as_an_error() {
    let (mut controller, handle, _dir) = active_controller("offline-snapshot");
    sync_library(
        &mut controller,
        &handle,
        vec![LibraryItem::Track {
            track: track("spotify:track:a", "Track A"),
            added_at: None,
        }],
        vec![],
        vec![],
        vec![],
    );
    assert!(!controller.library_status().first_sync_failed);

    // Go offline (deregistering is the simplest available lever, mirroring
    // `search_view.rs`'s offline test) without touching the index.
    controller.set_playback_permitted(false, None);
    controller.tick();
    let status = controller.library_status();
    assert_eq!(status.connectivity, modplayer_core::Connectivity::Offline);
    assert!(!status.first_sync_failed);

    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState::default();
    let (nodes, _outcome) = render_library(&mut controller, &mut artwork, &mut state);
    assert!(
        has(&nodes, Role::ListItem, "Track A — Artist"),
        "the snapshot must keep rendering normally while offline: {nodes:?}"
    );
    assert!(
        !has_name(&nodes, &tr("library-first-sync-failed")),
        "an offline snapshot is not the FR-021 error state: {nodes:?}"
    );
}

// -- Detail views -----------------------------------------------------

/// A functional action on a Playlist/Album/Artist row whose track list is
/// not cached yet must survive the fetch: `apply_row_action` issues the
/// fetch, the view parks the action, and the next frame after the list
/// lands applies it (contracts/library-and-search-core.md §3 "fetched if
/// needed"). Regression for the 2026-09-17 manual walk (quickstart M5),
/// where wide-row menu actions were dropped outright.
#[test]
fn wide_row_action_is_deferred_until_its_track_list_lands() {
    use modplayer_ui::rows::{RowAction, RowEntity};

    let (mut controller, handle, _dir) = active_controller("wide-row-deferred-action");
    let id = PlaylistId::new("spotify:playlist:a").unwrap();
    let playlist_ref = playlist(id.as_str(), "Road Trip", "Alex", false);
    sync_library(
        &mut controller,
        &handle,
        vec![],
        vec![],
        vec![],
        vec![LibraryItem::Playlist(playlist_ref.clone())],
    );
    handle.script_track_list(
        TrackListSource::Playlist(id.clone()),
        Ok(TrackList {
            source: TrackListSource::Playlist(id.clone()),
            tracks: vec![
                track("spotify:track:1", "One"),
                track("spotify:track:2", "Two"),
            ],
        }),
    );

    // The action lands while the list is uncached: it must be parked, not
    // dropped.
    let entity = RowEntity::Playlist(playlist_ref);
    let outcome =
        library_view::apply_row_action(&mut controller, &entity, &[], RowAction::PlayNext);
    assert_eq!(outcome, library_view::RowActionOutcome::Deferred);
    assert!(
        controller.queue_view().items.is_empty(),
        "nothing can be queued before the list is known"
    );

    let mut state = LibraryViewState {
        tab: LibraryTab::Playlists,
        pending_action: Some((entity, RowAction::PlayNext)),
    };
    let mut artwork = ArtworkCache::new();
    // Still in flight: the parked action stays parked.
    let (_nodes, _) = render_library(&mut controller, &mut artwork, &mut state);
    assert!(state.pending_action.is_some(), "still loading");

    // The reply lands; the next frame applies the parked action.
    controller.tick();
    let (_nodes, _) = render_library(&mut controller, &mut artwork, &mut state);
    assert!(
        state.pending_action.is_none(),
        "applied once the list landed"
    );
    let titles: Vec<String> = controller
        .queue_view()
        .items
        .iter()
        .map(|row| row.title.clone())
        .collect();
    assert_eq!(titles, vec!["One", "Two"], "the whole playlist was queued");
}

#[test]
fn album_detail_shows_header_and_tracks_in_album_order() {
    let (mut controller, handle, _dir) = active_controller("album-detail");
    let id = AlbumId::new("spotify:album:a").unwrap();
    sync_library(
        &mut controller,
        &handle,
        vec![],
        vec![LibraryItem::Album {
            album: album(id.as_str(), "Album A"),
            added_at: None,
        }],
        vec![],
        vec![],
    );
    handle.script_track_list(
        TrackListSource::Album(id.clone()),
        Ok(TrackList {
            source: TrackListSource::Album(id.clone()),
            tracks: vec![
                track("spotify:track:1", "One"),
                track("spotify:track:2", "Two"),
            ],
        }),
    );

    let mut artwork = ArtworkCache::new();
    let target = DetailTarget::Album(id);
    // First render issues `FetchTrackList` and sees `Loading`.
    let (nodes, _) = render_detail(&mut controller, &mut artwork, &target, default_input());
    assert!(has(&nodes, Role::Status, &tr("loading")), "{nodes:?}");
    controller.tick();

    let (nodes, _) = render_detail(&mut controller, &mut artwork, &target, default_input());
    assert!(
        has_name(&nodes, "Album A"),
        "expected the album's name as the heading: {nodes:?}"
    );

    // Row order must be read from the accessibility tree's true traversal
    // order, not `TreeUpdate::nodes`' own (unspecified) order — the track
    // list renders inside a virtualised `ScrollArea` (US3 T071), same as
    // `tabs_render_in_the_fixed_order`'s own reasoning.
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut outcome = DetailOutcome::None;
    let mut output = ctx.run_ui(default_input(), |ui| {
        outcome = detail_view::show(ui, &mut controller, &mut artwork, &target);
    });
    let _ = outcome;
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    assert_eq!(
        labels_in_tree_order(&update, Role::ListItem),
        vec!["One — Artist", "Two — Artist"],
        "tracks must list in album order"
    );
}

#[test]
fn playlist_detail_shows_owner_label_and_no_tracks_message_when_empty() {
    let (mut controller, handle, _dir) = active_controller("playlist-detail-empty");
    let id = PlaylistId::new("spotify:playlist:a").unwrap();
    sync_library(
        &mut controller,
        &handle,
        vec![],
        vec![],
        vec![],
        vec![LibraryItem::Playlist(playlist(
            id.as_str(),
            "Road Trip",
            "Alex",
            false,
        ))],
    );
    handle.script_track_list(
        TrackListSource::Playlist(id.clone()),
        Ok(TrackList {
            source: TrackListSource::Playlist(id.clone()),
            tracks: vec![],
        }),
    );

    let mut artwork = ArtworkCache::new();
    let target = DetailTarget::Playlist(id);
    render_detail(&mut controller, &mut artwork, &target, default_input());
    controller.tick();
    let (nodes, _) = render_detail(&mut controller, &mut artwork, &target, default_input());

    assert!(
        nodes.iter().any(|n| n.accessible_name()
            == Some(tr_args("playlist-owner", &[("name", "Alex".to_string())]).as_str())),
        "expected the owner label since this playlist is not owned: {nodes:?}"
    );
    assert!(
        nodes
            .iter()
            .any(|n| n.accessible_name() == Some(tr("playlist-no-tracks").as_str())),
        "expected the empty-playlist message: {nodes:?}"
    );
}

#[test]
fn artist_detail_shows_up_to_ten_top_tracks() {
    let (mut controller, handle, _dir) = active_controller("artist-detail");
    let id = ArtistId::new("spotify:artist:a").unwrap();
    sync_library(
        &mut controller,
        &handle,
        vec![],
        vec![],
        vec![LibraryItem::Artist(artist(id.as_str(), "The Artist"))],
        vec![],
    );
    let top_tracks: Vec<TrackRef> = (0..10)
        .map(|i| track(&format!("spotify:track:{i}"), &format!("Top {i}")))
        .collect();
    handle.script_track_list(
        TrackListSource::ArtistTop(id.clone()),
        Ok(TrackList {
            source: TrackListSource::ArtistTop(id.clone()),
            tracks: top_tracks,
        }),
    );

    let mut artwork = ArtworkCache::new();
    let target = DetailTarget::Artist(id);
    render_detail(&mut controller, &mut artwork, &target, default_input());
    controller.tick();
    let (nodes, _) = render_detail(&mut controller, &mut artwork, &target, default_input());

    assert!(has_name(&nodes, "The Artist"), "{nodes:?}");
    assert_eq!(count(&nodes, Role::ListItem), 10, "{nodes:?}");
}

#[test]
fn back_button_returns_the_back_outcome() {
    let (mut controller, handle, _dir) = active_controller("detail-back");
    let id = ArtistId::new("spotify:artist:a").unwrap();
    sync_library(
        &mut controller,
        &handle,
        vec![],
        vec![],
        vec![LibraryItem::Artist(artist(id.as_str(), "The Artist"))],
        vec![],
    );
    handle.script_track_list(
        TrackListSource::ArtistTop(id.clone()),
        Ok(TrackList {
            source: TrackListSource::ArtistTop(id.clone()),
            tracks: vec![],
        }),
    );

    let mut artwork = ArtworkCache::new();
    let target = DetailTarget::Artist(id);
    let (_, outcome) = render_detail(&mut controller, &mut artwork, &target, back_key_input());
    assert_eq!(
        outcome,
        DetailOutcome::Back,
        "Backspace must return the Back outcome"
    );
}

// -- Scale (US3 T069, SC-002, contracts/ui-surface.md §3) -------------------

/// A 50 000 saved-track / 1 000 playlist fixture (mirrors
/// `modplayer-core tests/library_index.rs::large_fixture_merges_and_looks_up_under_budget`):
/// scrolling or typing a search must feel identical to a small library
/// (quickstart M15) because `library_view::show` only ever lays out the
/// rows the viewport can currently show (US3 T071's virtualisation pass) —
/// not the full 50 000/1 000 count. Release builds additionally assert the
/// SC-002 frame budget itself (8 ms); a debug build only asserts the row
/// count stays small, since unoptimized per-widget costs make the timing
/// assertion meaningless there (task T069's own wording).
#[test]
fn large_fixture_frame_budget() {
    let (mut controller, handle, _dir) = active_controller("large-fixture");

    let saved_tracks: Vec<LibraryItem> = (0..50_000u32)
        .map(|i| LibraryItem::Track {
            track: track(&format!("spotify:track:{i}"), &format!("Track {i}")),
            added_at: Some(u64::from(i)),
        })
        .collect();
    let playlists: Vec<LibraryItem> = (0..1_000u32)
        .map(|i| {
            LibraryItem::Playlist(playlist(
                &format!("spotify:playlist:{i}"),
                &format!("Playlist {i}"),
                "Me",
                true,
            ))
        })
        .collect();

    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Ok(LibraryPage {
            set: LibrarySet::SavedTracks,
            items: saved_tracks,
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
        vec![Ok(LibraryPage {
            set: LibrarySet::Playlists,
            items: playlists,
            next_page: None,
            sync_token: None,
        })],
    );
    controller.library_retry_sync();
    for _ in 0..12 {
        controller.tick();
    }

    let status = controller.library_status();
    assert!(
        !status.loading && !status.first_sync_failed,
        "test setup: the scripted sync must complete cleanly: {status:?}"
    );
    assert_eq!(
        controller.library().saved_tracks().len(),
        50_000,
        "test setup"
    );
    assert_eq!(controller.library().playlists().len(), 1_000, "test setup");

    let mut artwork = ArtworkCache::new();
    // A realistic, bounded window — not the whole 50 000-row content
    // height — so the viewport-visible range stays small regardless of
    // total row count (the property under test).
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    };

    // One `Context`, reused for every render below (mirrors the real app,
    // which builds its `Context`/font atlas once at startup, not per
    // frame). SC-002/FR-014 are about steady-state scroll cost, not a
    // process's very first paint, so each tab gets one untimed warm-up
    // render — building the font atlas, egui's internal layout caches, and
    // this test's own first-ever AccessKit tree — before the timed one.
    let ctx = Context::default();
    ctx.enable_accesskit();

    let mut saved_tracks_state = LibraryViewState {
        tab: LibraryTab::SavedTracks,
        ..LibraryViewState::default()
    };
    render_library_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut saved_tracks_state,
        &input,
    );
    let (saved_tracks_rows, saved_tracks_elapsed) = time_library_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut saved_tracks_state,
        &input,
    );
    assert!(
        saved_tracks_rows < 100,
        "virtualisation must only lay out the visible rows at 50 000 saved tracks, \
         got {saved_tracks_rows} row/skeleton nodes (took {saved_tracks_elapsed:?})"
    );
    #[cfg(not(debug_assertions))]
    assert!(
        saved_tracks_elapsed.as_millis() <= 8,
        "Saved Tracks frame took {saved_tracks_elapsed:?}, SC-002's budget is 8 ms"
    );

    let mut playlists_state = LibraryViewState {
        tab: LibraryTab::Playlists,
        ..LibraryViewState::default()
    };
    render_library_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut playlists_state,
        &input,
    );
    let (playlists_rows, playlists_elapsed) = time_library_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut playlists_state,
        &input,
    );
    assert!(
        playlists_rows < 100,
        "virtualisation must only lay out the visible rows at 1 000 playlists, \
         got {playlists_rows} row nodes (took {playlists_elapsed:?})"
    );
    #[cfg(not(debug_assertions))]
    assert!(
        playlists_elapsed.as_millis() <= 8,
        "Playlists frame took {playlists_elapsed:?}, SC-002's budget is 8 ms"
    );
}

/// Render one `library_view::show` frame on `ctx`, untimed — used to warm
/// up the font atlas/layout caches before [`time_library_frame`] measures.
fn render_library_frame(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    state: &mut LibraryViewState,
    input: &RawInput,
) {
    let mut output = ctx.run_ui(input.clone(), |ui| {
        let _ = library_view::show(ui, controller, artwork, state);
    });
    let _ = output.platform_output.accesskit_update.take();
    output.drop_without_applying_deltas();
}

/// Render one more `library_view::show` frame on the already-warm `ctx`,
/// returning the number of `ListItem`/`Status` (row/skeleton) nodes it
/// produced and the wall-clock time the render itself took (excludes
/// AccessKit tree extraction).
fn time_library_frame(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    state: &mut LibraryViewState,
    input: &RawInput,
) -> (usize, std::time::Duration) {
    let started = Instant::now();
    let mut output = ctx.run_ui(input.clone(), |ui| {
        let _ = library_view::show(ui, controller, artwork, state);
    });
    let elapsed = started.elapsed();
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    let row_count = update
        .nodes
        .iter()
        .filter(|(_, node)| matches!(node.role(), Role::ListItem | Role::Status))
        .count();
    (row_count, elapsed)
}

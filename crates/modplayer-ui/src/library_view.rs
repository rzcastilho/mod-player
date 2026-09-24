// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Library view (contracts/ui-surface.md §3, FR-009/010/011/012/013/
//! 017/021): five fixed-order tabs (Saved Tracks, Saved Albums, Followed
//! Artists, Playlists, Recently Played) opening on Saved Tracks, rendered
//! through `rows::list_row` exactly like Search (FR-004). `loading`/
//! `first_sync_failed`/an empty snapshot each get their own state; a
//! `refreshing` sync backoff shows inline without hiding content.
//!
//! Un-hydrated ids (`LibraryIndex::track`/`album`/`artist` returning `None`
//! for an id already in a set list) render a skeleton row and are
//! collected for `PlaybackController::library_hydrate_visible` — every list
//! here renders through `rows::virtualized_list` (US3 T071), so only the
//! rows the viewport can currently show are ever laid out, request
//! artwork, or count as "visible" for the hydration hint, in the set's own
//! display order (position-preserving, unlike the un-virtualised phase's
//! "loaded rows first, missing skeletons appended at the end").

use egui::{Ui, accesskit::Role};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{
    AlbumId, ArtistId, PlaylistId, SourceHost, TrackId, TrackListSource, TrackRef,
};
use modplayer_core::{PlaybackController, Severity, TrackListState, tr};

use crate::artwork::ArtworkCache;
use crate::rows::{
    ActingListOutcome, RowAction, RowEntity, RowEvent, RowSelection, TrackListLookup, acting_list,
    entity_key, list_row, virtualized_list,
};
use crate::theme;
use crate::widgets::controls::tab as tab_widget;
use crate::widgets::skeleton::{ROW_HEIGHT, WIDE_ROW_HEIGHT, skeleton_row};

/// The five fixed-order tabs (contracts/ui-surface.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibraryTab {
    #[default]
    SavedTracks,
    SavedAlbums,
    FollowedArtists,
    Playlists,
    RecentlyPlayed,
}

impl LibraryTab {
    pub const ORDER: [LibraryTab; 5] = [
        LibraryTab::SavedTracks,
        LibraryTab::SavedAlbums,
        LibraryTab::FollowedArtists,
        LibraryTab::Playlists,
        LibraryTab::RecentlyPlayed,
    ];

    fn label_key(self) -> &'static str {
        match self {
            LibraryTab::SavedTracks => "library-tab-saved-tracks",
            LibraryTab::SavedAlbums => "library-tab-saved-albums",
            LibraryTab::FollowedArtists => "library-tab-followed-artists",
            LibraryTab::Playlists => "library-tab-playlists",
            LibraryTab::RecentlyPlayed => "library-tab-recently-played",
        }
    }

    fn empty_key(self) -> &'static str {
        match self {
            LibraryTab::SavedTracks => "library-empty",
            LibraryTab::SavedAlbums => "library-empty-albums",
            LibraryTab::FollowedArtists => "library-empty-artists",
            LibraryTab::Playlists => "library-empty-playlists",
            LibraryTab::RecentlyPlayed => "library-empty-recent",
        }
    }
}

/// This view's own frame-persistent state (the selected tab) — not part of
/// `PlaybackController`'s shadow state (contracts/library-and-search-
/// core.md §1 names no such field), owned by `App` like `Shell::section`.
#[derive(Debug, Default)]
pub struct LibraryViewState {
    pub tab: LibraryTab,
    /// A functional action activated on an Album/Artist/Playlist row whose
    /// track list was not cached yet: `apply_row_action` issued the fetch
    /// and parked the action here; every later frame retries it until the
    /// list is `Ready` (then applied) or `Failed` (then dropped) —
    /// contracts/library-and-search-core.md §3 "fetched if needed". The
    /// 2026-09-17 manual walk (quickstart M5) found wide-row menu actions
    /// otherwise silently ignored.
    pub pending_action: Option<(RowEntity, RowAction)>,
    /// This view's single selected row, if any (US2, contracts/list-row.md
    /// §S2/S8, data-model.md §6): one field for the whole view, so a
    /// selection is exclusive across all five tabs by construction. Cleared
    /// whenever the active tab changes (FR-028) — never persisted.
    pub selection: RowSelection,
}

/// What activating something in this view asks the caller (`App`, T065) to
/// do — switching the nav section or pushing the detail-navigation stack
/// are both above this view's own scope.
#[derive(Debug, Clone, PartialEq)]
pub enum LibraryOutcome {
    None,
    /// The empty-state "Search" action (or a Playlists-tab equivalent):
    /// switch to `Section::Search` and focus its box.
    FocusSearch,
    OpenAlbum(AlbumId),
    OpenPlaylist(PlaylistId),
    OpenArtist(ArtistId),
}

/// Draw the Library view: the tab row, then the active tab's state
/// (loading/first-sync-failed/empty/refreshing/content).
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    state: &mut LibraryViewState,
) -> LibraryOutcome {
    let status = controller.library_status();

    ui.horizontal(|ui| {
        // FR-014: a count is shown for every tab once loading finishes —
        // `0` for an empty-but-loaded list, no count node at all while
        // still loading (a hidden count means "not yet known", never
        // "none"). Both `LibraryIndex` accessors below and `recently_
        // played()` take `&self`, so this shared borrow coexists with the
        // later one inside the loop.
        let index = (!status.loading).then(|| controller.library());
        for tab in LibraryTab::ORDER {
            let selected = state.tab == tab;
            let response = tab_widget(ui, selected, &tr(tab.label_key()));
            if response.clicked() && state.tab != tab {
                // FR-028, contract S8: a selection belongs to the tab it
                // was made on — switching tabs always clears it, even
                // though every tab shares one `RowSelection` field.
                state.selection.clear();
                state.tab = tab;
            }
            if let Some(index) = index {
                let count = match tab {
                    LibraryTab::SavedTracks => index.saved_tracks().len(),
                    LibraryTab::SavedAlbums => index.saved_albums().len(),
                    LibraryTab::FollowedArtists => index.followed_artists().len(),
                    LibraryTab::Playlists => index.playlists().len(),
                    LibraryTab::RecentlyPlayed => controller.recently_played().len(),
                };
                ui.label(theme::mono_text(count.to_string()));
            }
        }
    });

    if status.loading {
        for _ in 0..3 {
            skeleton_row(ui, ROW_HEIGHT);
        }
        return LibraryOutcome::None;
    }

    if status.first_sync_failed {
        ui.label(tr("library-first-sync-failed"));
        if ui.button(tr("library-retry")).clicked() {
            controller.library_retry_sync();
        }
        return LibraryOutcome::None;
    }

    // FR-017 / data-model.md §3.3 "has_any_snapshot": the whole-library
    // empty state considers the four synced sets only — Recently Played
    // starts empty for every account regardless of how full the rest of
    // the library is, so it never gates this banner.
    let index = controller.library();
    let whole_library_empty = index.saved_tracks().is_empty()
        && index.saved_albums().is_empty()
        && index.followed_artists().is_empty()
        && index.playlists().is_empty();
    if whole_library_empty {
        // FR-006, U2: empty-state copy, capped at the 72-character measure
        // (research R17) — never applied to row titles/table cells.
        ui.scope(|ui| {
            ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
            ui.label(tr("library-empty"));
        });
        if ui.button(tr("action-search")).clicked() {
            return LibraryOutcome::FocusSearch;
        }
        return LibraryOutcome::None;
    }

    if status.refreshing {
        let text = tr("refreshing");
        let response = ui.label(&text);
        ui.ctx().accesskit_node_builder(response.id, |b| {
            b.set_role(Role::Status);
            b.set_label(text.clone());
        });
    }

    retry_pending_action(controller, &mut state.pending_action);

    let mut action: Option<(RowEntity, RowAction)> = None;
    let outcome = match state.tab {
        LibraryTab::SavedTracks => show_saved_tracks(ui, controller, artwork, &mut state.selection),
        LibraryTab::SavedAlbums => {
            show_saved_albums(ui, controller, artwork, &mut action, &mut state.selection)
        }
        LibraryTab::FollowedArtists => {
            show_followed_artists(ui, controller, artwork, &mut action, &mut state.selection)
        }
        LibraryTab::Playlists => {
            show_playlists(ui, controller, artwork, &mut action, &mut state.selection)
        }
        LibraryTab::RecentlyPlayed => {
            show_recently_played(ui, controller, artwork, &mut state.selection)
        }
    };
    if let Some((entity, action)) = action
        && apply_row_action(controller, &entity, &[], action) == RowActionOutcome::Deferred
    {
        state.pending_action = Some((entity, action));
    }
    outcome
}

/// Re-apply a parked wide-row action once its track list has landed
/// (see `LibraryViewState::pending_action`).
fn retry_pending_action<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
    pending: &mut Option<(RowEntity, RowAction)>,
) {
    let Some((entity, action)) = pending.take() else {
        return;
    };
    if apply_row_action(controller, &entity, &[], action) == RowActionOutcome::Deferred {
        *pending = Some((entity, action));
    }
}

/// What `apply_row_action` did with an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowActionOutcome {
    /// Applied (or a placeholder's toast raised, or dropped for good).
    Done,
    /// The row's track list is still being fetched — retry next frame.
    Deferred,
}

fn empty_state_with_search(ui: &mut Ui, tab: LibraryTab) -> LibraryOutcome {
    // FR-006, U2: empty-state copy, capped at the 72-character measure.
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr(tab.empty_key()));
    });
    if ui.button(tr("action-search")).clicked() {
        return LibraryOutcome::FocusSearch;
    }
    LibraryOutcome::None
}

fn show_saved_tracks<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    selection: &mut RowSelection,
) -> LibraryOutcome {
    let ids: Vec<TrackId> = controller
        .library()
        .saved_tracks()
        .iter()
        .map(|entry| entry.id.clone())
        .collect();
    if ids.is_empty() {
        return empty_state_with_search(ui, LibraryTab::SavedTracks);
    }
    // Contract S3/S4, FR-012: re-check the selection against this frame's
    // own order before drawing — `key_at` never scans past the stored
    // index (O(1)).
    selection.reconcile("library-saved-tracks", |i| {
        ids.get(i).map(|id| id.as_str().to_string())
    });
    draw_virtualized_tracks(
        ui,
        controller,
        artwork,
        "library-saved-tracks",
        &ids,
        selection,
    );
    LibraryOutcome::None
}

fn show_recently_played<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    selection: &mut RowSelection,
) -> LibraryOutcome {
    let tracks = controller.recently_played();
    if tracks.is_empty() {
        return empty_state_with_search(ui, LibraryTab::RecentlyPlayed);
    }
    selection.reconcile("library-recently-played", |i| {
        tracks.get(i).map(|t| t.id.as_str().to_string())
    });
    let mut pending: Option<(RowEntity, RowAction)> = None;
    virtualized_list(
        ui,
        "library-recently-played",
        ROW_HEIGHT,
        tracks.len(),
        None,
        |ui, i| {
            let entity = RowEntity::Track(tracks[i].clone());
            let key = entity_key(&entity);
            let is_selected = selection.is_selected("library-recently-played", key, i);
            match list_row(ui, artwork, &entity, is_selected) {
                Some(RowEvent::Action(action)) => pending = Some((entity, action)),
                Some(RowEvent::Select) => {
                    selection.select("library-recently-played", key, i);
                }
                Some(RowEvent::Open) | None => {}
            }
        },
    );
    if let Some((entity, action)) = pending {
        apply_row_action(controller, &entity, &tracks, action);
    }
    LibraryOutcome::None
}

/// Draw `ids` in their own display order, position-preserving (a hydrated
/// row or a skeleton exactly where that id sits — US3 T071 drops the
/// un-virtualised phase's "loaded rows first, missing appended last"), only
/// laying out the rows the viewport can currently show. The full acting
/// list (contracts/library-and-search-core.md §3: "rows currently loaded
/// in that list") is only resolved — an O(ids) scan — the frame a row
/// action is actually activated, not on every render.
fn draw_virtualized_tracks<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    id_salt: &str,
    ids: &[TrackId],
    selection: &mut RowSelection,
) {
    let mut visible_missing = Vec::new();
    let mut pending: Option<(RowEntity, RowAction)> = None;
    virtualized_list(ui, id_salt, ROW_HEIGHT, ids.len(), None, |ui, i| {
        let id = &ids[i];
        match controller.library().track(id) {
            Some(track) => {
                let entity = RowEntity::Track(track.clone());
                let key = entity_key(&entity);
                let is_selected = selection.is_selected(id_salt, key, i);
                match list_row(ui, artwork, &entity, is_selected) {
                    Some(RowEvent::Action(action)) => pending = Some((entity, action)),
                    Some(RowEvent::Select) => selection.select(id_salt, key, i),
                    Some(RowEvent::Open) | None => {}
                }
            }
            None => {
                skeleton_row(ui, ROW_HEIGHT);
                visible_missing.push(id.clone());
            }
        }
    });
    controller.library_hydrate_visible(&visible_missing);
    if let Some((entity, action)) = pending {
        let (loaded, _missing) = resolve_tracks(controller, ids);
        apply_row_action(controller, &entity, &loaded, action);
    }
}

fn show_saved_albums<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    action: &mut Option<(RowEntity, RowAction)>,
    selection: &mut RowSelection,
) -> LibraryOutcome {
    let ids: Vec<AlbumId> = controller
        .library()
        .saved_albums()
        .iter()
        .map(|entry| entry.id.clone())
        .collect();
    if ids.is_empty() {
        return empty_state_with_search(ui, LibraryTab::SavedAlbums);
    }
    selection.reconcile("library-saved-albums", |i| {
        ids.get(i).map(|id| id.as_str().to_string())
    });
    // Only track ids are hydration targets the controller tracks visibility
    // for in this phase (contracts/library-and-search-core.md §1 names
    // `library_hydrate_visible(ids: &[TrackId])`); an un-hydrated album is
    // already in the background sweep from `merge_page` and resolves on
    // its own.
    let mut open_id: Option<AlbumId> = None;
    virtualized_list(
        ui,
        "library-saved-albums",
        WIDE_ROW_HEIGHT,
        ids.len(),
        None,
        |ui, i| {
            let id = &ids[i];
            match controller.library().album(id).cloned() {
                Some(album) => {
                    let entity = RowEntity::Album(album);
                    let key = entity_key(&entity);
                    let is_selected = selection.is_selected("library-saved-albums", key, i);
                    match list_row(ui, artwork, &entity, is_selected) {
                        Some(RowEvent::Open) => open_id = Some(id.clone()),
                        Some(RowEvent::Action(a)) => *action = Some((entity, a)),
                        Some(RowEvent::Select) => {
                            selection.select("library-saved-albums", key, i);
                        }
                        None => {}
                    }
                }
                None => skeleton_row(ui, WIDE_ROW_HEIGHT),
            }
        },
    );
    match open_id {
        Some(id) => LibraryOutcome::OpenAlbum(id),
        None => LibraryOutcome::None,
    }
}

fn show_followed_artists<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    action: &mut Option<(RowEntity, RowAction)>,
    selection: &mut RowSelection,
) -> LibraryOutcome {
    let ids: Vec<ArtistId> = controller.library().followed_artists().to_vec();
    if ids.is_empty() {
        return empty_state_with_search(ui, LibraryTab::FollowedArtists);
    }
    selection.reconcile("library-followed-artists", |i| {
        ids.get(i).map(|id| id.as_str().to_string())
    });
    let mut open_id: Option<ArtistId> = None;
    virtualized_list(
        ui,
        "library-followed-artists",
        WIDE_ROW_HEIGHT,
        ids.len(),
        None,
        |ui, i| {
            let id = &ids[i];
            match controller.library().artist(id).cloned() {
                Some(artist) => {
                    let entity = RowEntity::Artist(artist);
                    let key = entity_key(&entity);
                    let is_selected = selection.is_selected("library-followed-artists", key, i);
                    match list_row(ui, artwork, &entity, is_selected) {
                        Some(RowEvent::Open) => open_id = Some(id.clone()),
                        Some(RowEvent::Action(a)) => *action = Some((entity, a)),
                        Some(RowEvent::Select) => {
                            selection.select("library-followed-artists", key, i);
                        }
                        None => {}
                    }
                }
                None => skeleton_row(ui, WIDE_ROW_HEIGHT),
            }
        },
    );
    match open_id {
        Some(id) => LibraryOutcome::OpenArtist(id),
        None => LibraryOutcome::None,
    }
}

fn show_playlists<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    action: &mut Option<(RowEntity, RowAction)>,
    selection: &mut RowSelection,
) -> LibraryOutcome {
    let ids: Vec<PlaylistId> = controller.library().playlists().to_vec();
    if ids.is_empty() {
        // FR-006, U2: empty-state copy, capped at the 72-character measure.
        ui.scope(|ui| {
            ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
            ui.label(tr(LibraryTab::Playlists.empty_key()));
        });
        // The "create one" action is a placeholder (FR-005/FR-017): no
        // playlist creation exists in this slice.
        if ui.button(tr("action-create")).clicked() {
            controller
                .notifications_mut()
                .raise(Severity::Info, "coming-soon");
        }
        return LibraryOutcome::None;
    }
    selection.reconcile("library-playlists", |i| {
        ids.get(i).map(|id| id.as_str().to_string())
    });
    let mut open_id: Option<PlaylistId> = None;
    virtualized_list(
        ui,
        "library-playlists",
        WIDE_ROW_HEIGHT,
        ids.len(),
        None,
        |ui, i| {
            let id = &ids[i];
            // A `Playlists` page is always fully resolved on merge
            // (contracts/catalog-source.md §3) — no skeleton branch needed
            // here.
            let Some(playlist) = controller.library().playlist_ref(id).cloned() else {
                return;
            };
            let entity = RowEntity::Playlist(playlist);
            let key = entity_key(&entity);
            let is_selected = selection.is_selected("library-playlists", key, i);
            match list_row(ui, artwork, &entity, is_selected) {
                Some(RowEvent::Open) => open_id = Some(id.clone()),
                Some(RowEvent::Action(a)) => *action = Some((entity, a)),
                Some(RowEvent::Select) => selection.select("library-playlists", key, i),
                None => {}
            }
        },
    );
    match open_id {
        Some(id) => LibraryOutcome::OpenPlaylist(id),
        None => LibraryOutcome::None,
    }
}

/// Resolve `ids` against the hydrated `tracks` map, returning the resolved
/// refs (display order preserved, un-hydrated ids skipped) and the
/// still-missing ids (for the hydration-visibility hint).
fn resolve_tracks<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    ids: &[TrackId],
) -> (Vec<TrackRef>, Vec<TrackId>) {
    let mut loaded = Vec::with_capacity(ids.len());
    let mut missing = Vec::new();
    for id in ids {
        match controller.library().track(id) {
            Some(track) => loaded.push(track.clone()),
            None => missing.push(id.clone()),
        }
    }
    (loaded, missing)
}

/// Apply one `RowAction` (contracts/library-and-search-core.md §3 "Acting-
/// list rule"): identical to `search_view::apply_row_action` for a Track
/// row (acts on `loaded_rows`); for an Album/Playlist/Artist row (only
/// reachable here via its own tab's `list_row`, not
/// `draw_virtualized_tracks`) resolves the acting list through
/// `PlaybackController::library_track_list` rather than reporting "nothing
/// to queue yet" — this view is exactly
/// where that fetch is expected to happen (US2). Returns `Deferred` while
/// that fetch is in flight so the caller can park the action
/// (`LibraryViewState::pending_action`) and retry it once the list lands.
pub fn apply_row_action<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
    entity: &RowEntity,
    loaded_rows: &[TrackRef],
    action: RowAction,
) -> RowActionOutcome {
    if action.is_placeholder() {
        controller
            .notifications_mut()
            .raise(Severity::Info, "coming-soon");
        return RowActionOutcome::Done;
    }
    let lookup = track_list_lookup(controller, entity);
    let acting = match acting_list(entity, loaded_rows, lookup) {
        ActingListOutcome::Ready(acting) => acting,
        ActingListOutcome::Loading => return RowActionOutcome::Deferred,
        // The row's own in-place indicator (contracts/library-and-search-
        // core.md §3) already reflects the failure; nothing to queue.
        ActingListOutcome::Unavailable => return RowActionOutcome::Done,
    };
    match (entity, action) {
        (RowEntity::Track(track), RowAction::PlayNext) => {
            controller.queue_play_next_tracks(vec![track.clone()]);
        }
        (RowEntity::Track(track), RowAction::AddToQueue) => {
            controller.queue_add_context(vec![track.clone()]);
        }
        (_, RowAction::PlayNow) => {
            controller.queue_replace_at(acting.tracks, acting.cursor);
            controller.play();
        }
        (_, RowAction::PlayNext) => {
            controller.queue_play_next_tracks(acting.tracks);
        }
        (_, RowAction::AddToQueue) => {
            controller.queue_add_context(acting.tracks);
        }
        (_, RowAction::AddToPlaylist | RowAction::SaveToLibrary | RowAction::PinForOffline) => {
            unreachable!("placeholders are handled above")
        }
    }
    RowActionOutcome::Done
}

/// The full track list an Album/Playlist/Artist row's actions need
/// (contracts/library-and-search-core.md §3), issuing
/// `PlaybackController::library_track_list`'s underlying `FetchTrackList`
/// on first miss and caching thereafter. A no-op `Ready(vec![])` for a
/// Track row — `acting_list` never consults `track_list` for that variant.
pub fn track_list_lookup<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
    entity: &RowEntity,
) -> TrackListLookup {
    let source = match entity {
        RowEntity::Track(_) => return TrackListLookup::Ready(Vec::new()),
        RowEntity::Album(album) => TrackListSource::Album(album.id.clone()),
        RowEntity::Playlist(playlist) => TrackListSource::Playlist(playlist.id.clone()),
        RowEntity::Artist(artist) => TrackListSource::ArtistTop(artist.id.clone()),
    };
    match controller.library_track_list(source) {
        TrackListState::Cached(tracks) => TrackListLookup::Ready(tracks),
        TrackListState::Loading => TrackListLookup::Loading,
        TrackListState::Failed => TrackListLookup::Failed,
    }
}

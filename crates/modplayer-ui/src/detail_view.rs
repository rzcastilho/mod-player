// SPDX-License-Identifier: MIT OR Apache-2.0

//! Album/playlist/artist detail views (contracts/ui-surface.md §4, FR-013):
//! opened from a non-track row's **Open** (or Enter — Enter is Play now
//! only on track rows). Each header names/artworks the entity; the track
//! list below it renders through `rows::list_row` exactly like every other
//! list (FR-004), fetched via `PlaybackController::library_track_list` and
//! cached there (contracts/library-and-search-core.md §1).

use egui::{Key, RichText, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{AlbumId, ArtistId, PlaylistId, SourceHost, TrackListSource};
use modplayer_core::{PlaybackController, TrackListState, tr, tr_args};

use crate::artwork::ArtworkCache;
use crate::library_view::apply_row_action;
use crate::rows::{
    RowAction, RowEntity, RowEvent, RowSelection, entity_key, list_row, virtualized_list_in,
};
use crate::section_memory::{LibraryViewKey, SectionMemory, ViewKey};
use crate::theme;
use crate::widgets::skeleton::{ROW_HEIGHT, skeleton_row};

/// The detail screen's own heading, explicit `title` + `text_primary`
/// (014-design-tokens-and-type-scale, US2, T024, data-model.md §6 "Screen
/// headings, detail headers") — `ui.heading` already inherits this size
/// through the `Style`'s `Heading` remap, but the explicit call keeps this
/// call site paired with the `.weak()` field lines beneath it, matching
/// `rows::title_text`'s convention.
fn heading_text(ui: &Ui, text: &str) -> RichText {
    RichText::new(text)
        .text_style(theme::text::TITLE)
        .color(theme::roles(ui.visuals()).text_primary)
}

/// Which detail view is open (owned by `App`'s single-level detail-
/// navigation stack, T065 — nothing in this slice links from one detail
/// view to another).
///
/// `Hash` (020-shell-navigation-and-gates, data-model.md §4): a
/// `DetailTarget` is half of `section_memory::LibraryViewKey`'s `Detail`
/// variant, the key type of `SectionMemory`'s offset map. `AlbumId`/
/// `PlaylistId`/`ArtistId` are already `Hash` (`catalog.rs`'s `id_newtype!`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DetailTarget {
    Album(AlbumId),
    Playlist(PlaylistId),
    Artist(ArtistId),
}

/// What this frame asks the caller to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailOutcome {
    None,
    Back,
}

/// This view's own frame-persistent state (US2, data-model.md §6): the
/// selected row, and the target it was last reconciled against, so
/// switching detail targets clears the selection (FR-028, contract S8).
/// Never persisted.
#[derive(Debug, Default)]
pub struct DetailViewState {
    pub selection: RowSelection,
    pub last_target: Option<DetailTarget>,
}

/// Draw one detail view: the Back control, the header, then the track list
/// (or "This playlist has no tracks" / a loading skeleton). `state` owns
/// this view's selection (US2). `memory` retains the track list's own
/// scroll offset across section round trips (020-shell-navigation-and-
/// gates, US3, contracts/section-memory.md).
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    target: &DetailTarget,
    state: &mut DetailViewState,
    memory: &mut SectionMemory,
) -> DetailOutcome {
    // FR-028, contract S8: a different detail target invalidates the
    // previous target's row order — the selection no longer applies.
    if state.last_target.as_ref() != Some(target) {
        state.selection.clear();
        state.last_target = Some(target.clone());
    }

    let back_clicked = ui.button(tr("detail-back")).clicked();
    let back_key = ui.ctx().input(|i| {
        i.key_pressed(Key::Backspace) || (i.modifiers.alt && i.key_pressed(Key::ArrowLeft))
    });

    let source = match target {
        DetailTarget::Album(id) => TrackListSource::Album(id.clone()),
        DetailTarget::Playlist(id) => TrackListSource::Playlist(id.clone()),
        DetailTarget::Artist(id) => TrackListSource::ArtistTop(id.clone()),
    };

    match target {
        DetailTarget::Album(id) => draw_album_header(ui, controller, id),
        DetailTarget::Playlist(id) => draw_playlist_header(ui, controller, id),
        DetailTarget::Artist(id) => draw_artist_header(ui, controller, id),
    }

    match controller.library_track_list(source) {
        TrackListState::Cached(tracks) => {
            if tracks.is_empty() {
                if matches!(target, DetailTarget::Playlist(_)) {
                    ui.label(tr("playlist-no-tracks"));
                }
            } else {
                // Contract S3/S4, FR-012: re-check the selection against
                // this frame's own track order before drawing.
                state.selection.reconcile("detail-tracks", |i| {
                    tracks.get(i).map(|t| t.id.as_str().to_string())
                });
                // Virtualised (US3 T071): only the tracks the viewport can
                // currently show are laid out or fetch artwork, same as
                // every other list (contracts/ui-surface.md §4/§7).
                let mut pending: Option<(RowEntity, RowAction)> = None;
                let view_key = ViewKey::Library(LibraryViewKey::Detail(target.clone()));
                let scroll = memory.scroll_area(&view_key).auto_shrink([false, true]);
                let (_, offset) =
                    virtualized_list_in(ui, scroll, ROW_HEIGHT, tracks.len(), |ui, i| {
                        let entity = RowEntity::Track(tracks[i].clone());
                        let key = entity_key(&entity);
                        let is_selected = state.selection.is_selected("detail-tracks", key, i);
                        match list_row(ui, artwork, &entity, is_selected) {
                            Some(RowEvent::Action(action)) => pending = Some((entity, action)),
                            Some(RowEvent::Select) => {
                                state.selection.select("detail-tracks", key, i);
                            }
                            Some(RowEvent::Open) | None => {}
                        }
                    });
                memory.record(view_key, offset);
                if let Some((entity, action)) = pending {
                    apply_row_action(controller, &entity, &tracks, action);
                }
            }
        }
        TrackListState::Loading => {
            for _ in 0..3 {
                skeleton_row(ui, ROW_HEIGHT);
            }
        }
        TrackListState::Failed => {}
    }

    if back_clicked || back_key {
        DetailOutcome::Back
    } else {
        DetailOutcome::None
    }
}

fn draw_album_header<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &PlaybackController<B, H>,
    id: &AlbumId,
) {
    let Some(album) = controller.library().album(id) else {
        skeleton_row(ui, ROW_HEIGHT);
        return;
    };
    ui.label(heading_text(ui, &album.name));
    ui.label(RichText::new(album.artists.join(", ")).weak());
    if let Some(release) = &album.release_date {
        ui.label(RichText::new(release.year.to_string()).weak());
    }
}

fn draw_playlist_header<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &PlaybackController<B, H>,
    id: &PlaylistId,
) {
    let Some(playlist) = controller.library().playlist_ref(id) else {
        skeleton_row(ui, ROW_HEIGHT);
        return;
    };
    ui.label(heading_text(ui, &playlist.name));
    if !playlist.editable {
        ui.label(
            RichText::new(tr_args(
                "playlist-owner",
                &[("name", playlist.owner_name.clone())],
            ))
            .weak(),
        );
    }
    ui.label(
        RichText::new(tr_args(
            "playlist-track-count",
            &[("count", playlist.track_count.to_string())],
        ))
        .weak(),
    );
}

fn draw_artist_header<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &PlaybackController<B, H>,
    id: &ArtistId,
) {
    let Some(artist) = controller.library().artist(id) else {
        skeleton_row(ui, ROW_HEIGHT);
        return;
    };
    ui.label(heading_text(ui, &artist.name));
}

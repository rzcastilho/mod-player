// SPDX-License-Identifier: MIT OR Apache-2.0

//! Album/playlist/artist detail views (contracts/ui-surface.md §4, FR-013):
//! opened from a non-track row's **Open** (or Enter — Enter is Play now
//! only on track rows). Each header names/artworks the entity; the track
//! list below it renders through `rows::list_row` exactly like every other
//! list (FR-004), fetched via `PlaybackController::library_track_list` and
//! cached there (contracts/library-and-search-core.md §1).

use egui::{Key, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{AlbumId, ArtistId, PlaylistId, SourceHost, TrackListSource};
use modplayer_core::{PlaybackController, TrackListState, tr, tr_args};

use crate::artwork::ArtworkCache;
use crate::library_view::apply_row_action;
use crate::rows::{RowAction, RowEntity, RowEvent, list_row, virtualized_list};
use crate::widgets::skeleton::{ROW_HEIGHT, skeleton_row};

/// Which detail view is open (owned by `App`'s single-level detail-
/// navigation stack, T065 — nothing in this slice links from one detail
/// view to another).
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Draw one detail view: the Back control, the header, then the track list
/// (or "This playlist has no tracks" / a loading skeleton).
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    target: &DetailTarget,
) -> DetailOutcome {
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
                // Virtualised (US3 T071): only the tracks the viewport can
                // currently show are laid out or fetch artwork, same as
                // every other list (contracts/ui-surface.md §4/§7).
                let mut pending: Option<(RowEntity, RowAction)> = None;
                virtualized_list(
                    ui,
                    "detail-tracks",
                    ROW_HEIGHT,
                    tracks.len(),
                    None,
                    |ui, i| {
                        let entity = RowEntity::Track(tracks[i].clone());
                        if let Some(RowEvent::Action(action)) = list_row(ui, artwork, &entity) {
                            pending = Some((entity, action));
                        }
                    },
                );
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
    ui.heading(&album.name);
    ui.label(album.artists.join(", "));
    if let Some(release) = &album.release_date {
        ui.label(release.year.to_string());
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
    ui.heading(&playlist.name);
    if !playlist.editable {
        ui.label(tr_args(
            "playlist-owner",
            &[("name", playlist.owner_name.clone())],
        ));
    }
    ui.label(tr_args(
        "playlist-track-count",
        &[("count", playlist.track_count.to_string())],
    ));
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
    ui.heading(&artist.name);
}

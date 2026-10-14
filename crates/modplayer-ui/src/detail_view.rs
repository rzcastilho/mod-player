// SPDX-License-Identifier: MIT OR Apache-2.0

//! Album/playlist/artist detail views (contracts/ui-surface.md §4, FR-013):
//! opened from a non-track row's **Open** (or Enter — Enter is Play now
//! only on track rows). Each header names/artworks the entity; the track
//! list below it renders through `rows::list_row` exactly like every other
//! list (FR-004), fetched via `PlaybackController::library_track_list` and
//! cached there (contracts/library-and-search-core.md §1).

use egui::accesskit::Role;
use egui::{Align, Key, Label, Layout, RichText, Sense, Ui, UiBuilder, Vec2};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{
    AlbumId, ArtistId, PlaylistId, SourceHost, TrackListSource, TrackRef,
};
use modplayer_core::{PlaybackController, TrackListState, tr, tr_args};

use crate::artwork::ArtworkCache;
use crate::library_view::apply_row_action;
use crate::rows::{
    RowAction, RowEntity, RowEvent, RowSelection, actions_menu, draw_artwork_url, entity_key,
    list_row, virtualized_list_in,
};
use crate::section_memory::{LibraryViewKey, SectionMemory, ViewKey};
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::controls::button;
use crate::widgets::skeleton::{ROW_HEIGHT, SkeletonShape, header_skeleton, skeleton_row};

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

    let source = match target {
        DetailTarget::Album(id) => TrackListSource::Album(id.clone()),
        DetailTarget::Playlist(id) => TrackListSource::Playlist(id.clone()),
        DetailTarget::Artist(id) => TrackListSource::ArtistTop(id.clone()),
    };
    let list_state = controller.library_track_list(source);

    // The collection header (025 US1): artwork, title, one facts line,
    // Play and the six-action menu. `None` while the ref is not in the
    // library index yet.
    let model = header_model(controller, target, &list_state);
    let header_event = collection_header(ui, artwork, model.as_ref());
    let mut back_clicked = false;
    match header_event {
        HeaderEvent::None => {}
        HeaderEvent::Back => back_clicked = true,
        HeaderEvent::Action(action) => {
            if let Some(model) = &model {
                let tracks: &[TrackRef] = match &list_state {
                    TrackListState::Cached(tracks) => tracks,
                    _ => &[],
                };
                apply_row_action(controller, &model.entity, tracks, action);
            }
        }
    }
    let back_key = ui.ctx().input(|i| {
        i.key_pressed(Key::Backspace) || (i.modifiers.alt && i.key_pressed(Key::ArrowLeft))
    });

    match list_state {
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
                skeleton_row(ui, SkeletonShape::TRACK);
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

// ---------------------------------------------------------------------
// Collection header (025 US1, data-model.md §1–§3)
// ---------------------------------------------------------------------

/// Side of the header's artwork square (≥ 3 × the row artwork).
pub const DETAIL_ARTWORK_SIZE: f32 = 128.0;

/// Separator between the facts on the header's one facts line.
pub const FACTS_SEPARATOR: &str = " · ";

/// The header's fixed height (research R1): the Back row, one `space::SM`
/// gap and the artwork square. Identical for loaded, skeleton and
/// long-title states, and independent of the list width.
#[must_use]
pub fn header_height(spacing: &egui::Spacing) -> f32 {
    spacing.interact_size.y + theme::space::SM + DETAIL_ARTWORK_SIZE
}

/// What a header interaction asks `show` to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderEvent {
    None,
    Back,
    Action(RowAction),
}

/// The pure inputs of a header's facts line (data-model.md §2). `tracks` /
/// `top_tracks` are `Some` only when the track list is `Cached`.
#[derive(Debug, Clone, Copy)]
pub enum FactsInput<'a> {
    Playlist {
        owner: &'a str,
        track_count: u32,
        tracks: Option<&'a [TrackRef]>,
    },
    Album {
        artists: &'a [String],
        year: Option<u16>,
        track_count: u32,
        tracks: Option<&'a [TrackRef]>,
    },
    Artist {
        top_tracks: Option<&'a [TrackRef]>,
    },
}

/// Sum of every track's duration in ms (unavailable tracks included),
/// widened to `u64` so no list can overflow.
#[must_use]
pub fn total_runtime_ms(tracks: &[TrackRef]) -> u64 {
    tracks.iter().map(|t| u64::from(t.duration_ms)).sum()
}

/// Format a runtime (data-model.md §3): `None` for an empty list; under
/// one minute rounds up to "1 min"; an hour or more is "H hr M min".
#[must_use]
pub fn format_runtime(total_ms: u64, non_empty: bool) -> Option<String> {
    if !non_empty {
        return None;
    }
    let minutes = total_ms / 60_000;
    if minutes < 1 {
        return Some(tr_args(
            "detail-runtime-minutes",
            &[("minutes", "1".into())],
        ));
    }
    if minutes < 60 {
        return Some(tr_args(
            "detail-runtime-minutes",
            &[("minutes", minutes.to_string())],
        ));
    }
    Some(tr_args(
        "detail-runtime-hours",
        &[
            ("hours", (minutes / 60).to_string()),
            ("minutes", (minutes % 60).to_string()),
        ],
    ))
}

fn runtime_of(tracks: Option<&[TrackRef]>) -> Option<String> {
    let tracks = tracks?;
    format_runtime(total_runtime_ms(tracks), !tracks.is_empty())
}

fn track_count_fact(count: u32) -> String {
    tr_args("playlist-track-count", &[("count", count.to_string())])
}

/// The present facts, in order (data-model.md §2); never contains an
/// empty string.
#[must_use]
pub fn header_facts(input: FactsInput<'_>) -> Vec<String> {
    let mut facts: Vec<Option<String>> = Vec::new();
    match input {
        FactsInput::Playlist {
            owner,
            track_count,
            tracks,
        } => {
            facts.push(Some(tr_args(
                "detail-owner",
                &[("name", owner.to_string())],
            )));
            facts.push(Some(track_count_fact(track_count)));
            facts.push(runtime_of(tracks));
        }
        FactsInput::Album {
            artists,
            year,
            track_count,
            tracks,
        } => {
            let names: Vec<&str> = artists
                .iter()
                .map(String::as_str)
                .filter(|a| !a.is_empty())
                .collect();
            facts.push(Some(names.join(", ")));
            facts.push(year.map(|y| y.to_string()));
            facts.push(Some(track_count_fact(track_count)));
            facts.push(runtime_of(tracks));
        }
        FactsInput::Artist { top_tracks } => {
            facts.push(Some(tr("detail-kind-artist")));
            facts.push(
                top_tracks
                    .map(|t| tr_args("detail-top-track-count", &[("count", t.len().to_string())])),
            );
        }
    }
    facts
        .into_iter()
        .flatten()
        .filter(|f| !f.is_empty())
        .collect()
}

/// Join facts with [`FACTS_SEPARATOR`].
#[must_use]
pub fn facts_line(facts: &[String]) -> String {
    facts.join(FACTS_SEPARATOR)
}

/// Everything the header draws, owned so the controller borrow ends before
/// drawing.
struct HeaderModel {
    entity: RowEntity,
    title: String,
    artwork_url: Option<String>,
    facts: String,
    /// `Some(0)` disables Play.
    track_count: Option<u32>,
}

fn header_model<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    target: &DetailTarget,
    list: &TrackListState,
) -> Option<HeaderModel> {
    let cached: Option<&[TrackRef]> = match list {
        TrackListState::Cached(tracks) => Some(tracks),
        _ => None,
    };
    match target {
        DetailTarget::Album(id) => {
            let album = controller.library().album(id)?;
            let facts = facts_line(&header_facts(FactsInput::Album {
                artists: &album.artists,
                year: album.release_date.as_ref().map(|r| r.year),
                track_count: album.track_count,
                tracks: cached,
            }));
            Some(HeaderModel {
                title: album.name.clone(),
                artwork_url: album.artwork_url.clone(),
                facts,
                track_count: Some(album.track_count),
                entity: RowEntity::Album(album.clone()),
            })
        }
        DetailTarget::Playlist(id) => {
            let playlist = controller.library().playlist_ref(id)?;
            let facts = facts_line(&header_facts(FactsInput::Playlist {
                owner: &playlist.owner_name,
                track_count: playlist.track_count,
                tracks: cached,
            }));
            Some(HeaderModel {
                title: playlist.name.clone(),
                artwork_url: playlist.artwork_url.clone(),
                facts,
                track_count: Some(playlist.track_count),
                entity: RowEntity::Playlist(playlist.clone()),
            })
        }
        DetailTarget::Artist(id) => {
            let artist = controller.library().artist(id)?;
            let facts = facts_line(&header_facts(FactsInput::Artist { top_tracks: cached }));
            Some(HeaderModel {
                title: artist.name.clone(),
                artwork_url: artist.artwork_url.clone(),
                facts,
                track_count: cached.map(|t| u32::try_from(t.len()).unwrap_or(u32::MAX)),
                entity: RowEntity::Artist(artist.clone()),
            })
        }
    }
}

/// Draw the fixed-height collection header: a Back row on top, then the
/// 128 px artwork beside the title, one facts line, Play and the
/// six-action menu. `None` (ref not indexed yet) draws the Back row and a
/// row skeleton.
fn collection_header(
    ui: &mut Ui,
    artwork: &mut ArtworkCache,
    model: Option<&HeaderModel>,
) -> HeaderEvent {
    let mut event = HeaderEvent::None;
    let width = ui.available_width();
    let height = header_height(ui.spacing());
    let (rect, group) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    if let Some(model) = model {
        let title = model.title.clone();
        ui.ctx().accesskit_node_builder(group.id, |b| {
            b.set_role(Role::Group);
            b.set_label(title);
            b.set_bounds(egui::accesskit::Rect {
                x0: f64::from(rect.min.x),
                y0: f64::from(rect.min.y),
                x1: f64::from(rect.max.x),
                y1: f64::from(rect.max.y),
            });
        });
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    let ui = &mut child;
    ui.spacing_mut().item_spacing.y = 0.0;

    let Some(model) = model else {
        // Ref not indexed yet: fixed-height header skeleton, Back stays live.
        if header_skeleton(ui) {
            event = HeaderEvent::Back;
        }
        return event;
    };

    if button(ui, Variant::Quiet, tr("detail-back")).clicked() {
        event = HeaderEvent::Back;
    }
    ui.add_space(theme::space::SM);

    ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
        draw_artwork_url(
            ui,
            artwork,
            model.artwork_url.as_deref(),
            &model.title,
            DETAIL_ARTWORK_SIZE,
        );
        let column_width = ui.available_width();
        ui.vertical(|ui| {
            ui.set_width(column_width);
            let roles = *theme::roles(ui.visuals());
            let title = RichText::new(&model.title)
                .text_style(theme::text::DISPLAY.clone())
                .color(roles.text_primary);
            ui.add(Label::new(title).truncate())
                .on_hover_text(&model.title);
            if !model.facts.is_empty() {
                let facts = RichText::new(&model.facts)
                    .text_style(theme::text::SECONDARY)
                    .color(roles.text_secondary);
                ui.add(Label::new(facts).truncate());
            }
            ui.add_space(theme::space::SM);
            ui.horizontal(|ui| {
                let disabled = model.track_count == Some(0);
                let play = ui
                    .add_enabled_ui(!disabled, |ui| {
                        button(ui, Variant::Primary, tr("detail-play"))
                    })
                    .inner;
                let name = tr_args("detail-play-name", &[("name", model.title.clone())]);
                ui.ctx().accesskit_node_builder(play.id, |b| {
                    b.set_label(name);
                    if disabled {
                        b.set_description(tr("detail-no-tracks-hint"));
                    }
                });
                if disabled {
                    play.on_disabled_hover_text(tr("detail-no-tracks-hint"));
                } else if play.clicked() {
                    event = HeaderEvent::Action(RowAction::PlayNow);
                }
                if let Some(action) = actions_menu(ui, &model.title, false) {
                    event = HeaderEvent::Action(action);
                }
            });
        });
    });
    event
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use modplayer_audio_source::{Availability, TrackId};
    use proptest::prelude::*;

    fn track_ms(ms: u32) -> TrackRef {
        TrackRef::new(
            TrackId::new("spotify:track:x").unwrap(),
            "t",
            vec![],
            None,
            None,
            ms,
            Availability::Available,
        )
    }

    /// Fluent wraps placeables in bidi isolates; strip them to compare text.
    fn plain(s: impl AsRef<str>) -> String {
        s.as_ref().replace(['\u{2068}', '\u{2069}'], "")
    }

    #[test]
    fn runtime_table() {
        assert_eq!(format_runtime(0, false), None);
        assert_eq!(
            format_runtime(59_999, true).map(plain).as_deref(),
            Some("1 min")
        );
        assert_eq!(
            format_runtime(60_000, true).map(plain).as_deref(),
            Some("1 min")
        );
        assert_eq!(
            format_runtime(59 * 60_000, true).map(plain).as_deref(),
            Some("59 min")
        );
        assert_eq!(
            format_runtime(3_600_000, true).map(plain).as_deref(),
            Some("1 hr 0 min")
        );
        assert_eq!(
            format_runtime(3_600_000 + 25 * 60_000, true)
                .map(plain)
                .as_deref(),
            Some("1 hr 25 min")
        );
    }

    #[test]
    fn total_runtime_does_not_overflow_u32() {
        let tracks = vec![track_ms(u32::MAX), track_ms(u32::MAX)];
        assert_eq!(total_runtime_ms(&tracks), 2 * u64::from(u32::MAX));
        assert_eq!(total_runtime_ms(&[]), 0);
    }

    #[test]
    fn playlist_facts_owner_count_runtime() {
        let tracks = vec![track_ms(180_000), track_ms(180_000)];
        let line = facts_line(&header_facts(FactsInput::Playlist {
            owner: "Alex",
            track_count: 2,
            tracks: Some(&tracks),
        }));
        assert_eq!(
            plain(line),
            plain(format!(
                "{} · {} · 6 min",
                tr_args("detail-owner", &[("name", "Alex".into())]),
                tr_args("playlist-track-count", &[("count", "2".into())])
            ))
        );
    }

    #[test]
    fn album_facts_omit_absent_fields() {
        let facts = header_facts(FactsInput::Album {
            artists: &[],
            year: None,
            track_count: 3,
            tracks: None,
        });
        assert_eq!(facts.len(), 1);
        let facts = header_facts(FactsInput::Album {
            artists: &["A".to_string(), "B".to_string()],
            year: Some(1999),
            track_count: 3,
            tracks: None,
        });
        assert_eq!(facts[0], "A, B");
        assert_eq!(facts[1], "1999");
    }

    #[test]
    fn artist_facts() {
        let facts = header_facts(FactsInput::Artist { top_tracks: None });
        assert_eq!(facts, vec![tr("detail-kind-artist")]);
        let tracks = vec![track_ms(1)];
        let facts = header_facts(FactsInput::Artist {
            top_tracks: Some(&tracks),
        });
        assert_eq!(facts.len(), 2);
    }

    proptest! {
        #[test]
        fn facts_line_is_well_formed(
            owner in "[a-zA-Z0-9]{0,8}",
            artists in proptest::collection::vec("[a-zA-Z0-9]{0,6}", 0..3),
            year in proptest::option::of(1900u16..2100),
            count in 0u32..1000,
            durations in proptest::option::of(proptest::collection::vec(any::<u32>(), 0..5)),
            kind in 0u8..3,
        ) {
            let tracks: Option<Vec<TrackRef>> =
                durations.map(|d| d.into_iter().map(track_ms).collect());
            let input = match kind {
                0 => FactsInput::Playlist { owner: &owner, track_count: count, tracks: tracks.as_deref() },
                1 => FactsInput::Album { artists: &artists, year, track_count: count, tracks: tracks.as_deref() },
                _ => FactsInput::Artist { top_tracks: tracks.as_deref() },
            };
            let facts = header_facts(input);
            prop_assert!(facts.iter().all(|f| !f.is_empty()));
            let line = facts_line(&facts);
            prop_assert!(!line.starts_with(FACTS_SEPARATOR));
            prop_assert!(!line.ends_with(FACTS_SEPARATOR));
            let double = format!("{FACTS_SEPARATOR}{FACTS_SEPARATOR}");
            prop_assert!(!line.contains(&double));
        }
    }
}

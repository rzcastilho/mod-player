// SPDX-License-Identifier: MIT OR Apache-2.0

//! The shared row model and `ListRow` widget (data-model.md §4,
//! contracts/ui-surface.md §5, FR-004/FR-006/FR-007/FR-008/FR-024): **every**
//! catalog list — the four Search groups, the five Library tabs, and the
//! three detail views — renders its rows through [`list_row`] and computes
//! the six actions' effect through [`acting_list`]. Any per-surface
//! difference in row content, accessible name, or action set is a bug
//! (design note 6, "Rows are one widget").
//!
//! `list_row` is deliberately `PlaybackController`-free: it draws content
//! and reports what happened (a [`RowEvent`]) without applying it. The
//! call site (`search_view`/`library_view`/`detail_view`, US1/US2) owns the
//! `PlaybackController` and decides what a [`RowAction`] does — including
//! raising the `coming-soon` notification for the three placeholder actions
//! (contracts/ui-surface.md §5, FR-005).

use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Range;

use egui::accesskit::Role;
use egui::{
    Align, Key, Label, Layout, Popup, PopupKind, RichText, ScrollArea, Sense, SetOpenCommand, Ui,
    UiBuilder, Vec2,
};

use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, Availability, PlaylistId, PlaylistRef, TrackRef,
};
use modplayer_core::{tr, tr_args};

use crate::actions::{self, Claim};
use crate::artwork::{ArtworkCache, ArtworkState};
use crate::theme;
use crate::widgets::initials::initials_placeholder;
use crate::widgets::skeleton::{ROW_HEIGHT, WIDE_ROW_HEIGHT};

/// Artwork square drawn to the left of every row (research R9). Album/
/// artist/playlist rows are visually taller (`WIDE_ROW_HEIGHT`) than track
/// rows, but share the same artwork size for now — a later polish pass may
/// grow it for the wide rows.
const ARTWORK_SIZE: f32 = 40.0;
/// Width kept free at the row's trailing edge for the "…" actions button
/// (button + item spacing), so the text column truncates before it.
const ACTIONS_RESERVED_WIDTH: f32 = 40.0;

/// What one row renders (data-model.md §4). Owns the ref by value: callers
/// clone out of `SearchSession`/`LibraryIndex` state to build the rows a
/// frame draws — cheap relative to a network round trip, and keeps this
/// widget free of a borrow on host state across the whole frame.
#[derive(Debug, Clone, PartialEq)]
pub enum RowEntity {
    Track(TrackRef),
    Album(AlbumRef),
    Artist(ArtistRef),
    Playlist(PlaylistRef),
}

/// Which loaded list a *track* row belongs to (data-model.md §4) — decides
/// [`acting_list`]'s "rows currently loaded" set (contracts/library-and-
/// search-core.md §3). Only meaningful for `RowEntity::Track`; an
/// Album/Artist/Playlist row's acting list is always its own full track
/// list, regardless of where the row itself is listed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RowOrigin {
    SearchTracks,
    SavedTracks,
    RecentlyPlayed,
    AlbumTracks(AlbumId),
    PlaylistTracks(PlaylistId),
    ArtistTop(ArtistId),
}

/// The six actions every row exposes, in the fixed menu order
/// (contracts/ui-surface.md §5). `AddToPlaylist`/`SaveToLibrary`/
/// `PinForOffline` are inert placeholders (FR-005) — `is_placeholder`
/// tells the call site to raise `coming-soon` instead of executing a queue
/// operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    PlayNow,
    PlayNext,
    AddToQueue,
    AddToPlaylist,
    SaveToLibrary,
    PinForOffline,
}

impl RowAction {
    /// The fixed menu order (contracts/ui-surface.md §5): Play now, Play
    /// next, Add to queue, Add to playlist, Save to library, Pin for
    /// offline.
    pub const ORDER: [RowAction; 6] = [
        RowAction::PlayNow,
        RowAction::PlayNext,
        RowAction::AddToQueue,
        RowAction::AddToPlaylist,
        RowAction::SaveToLibrary,
        RowAction::PinForOffline,
    ];

    /// Whether activating this action should only raise `coming-soon`
    /// (data-model.md §4, FR-005) rather than run a queue operation.
    pub fn is_placeholder(self) -> bool {
        matches!(
            self,
            RowAction::AddToPlaylist | RowAction::SaveToLibrary | RowAction::PinForOffline
        )
    }

    fn fluent_key(self) -> &'static str {
        match self {
            RowAction::PlayNow => "action-play-now",
            RowAction::PlayNext => "action-play-next",
            RowAction::AddToQueue => "action-add-to-queue",
            RowAction::AddToPlaylist => "action-add-to-playlist",
            RowAction::SaveToLibrary => "action-save-to-library",
            RowAction::PinForOffline => "action-pin-offline",
        }
    }
}

/// What activating a row reports (contracts/ui-surface.md §4-5): a
/// `RowAction` from the six-item menu, or `Open` — Enter/double-click on a
/// non-track row (Enter is Play now only on track rows; Complexity
/// Tracking "Enter on album/artist/playlist rows opens the detail view").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowEvent {
    Action(RowAction),
    Open,
}

/// The list Play now/Play next/Add to queue act on, and where playback
/// should start within it (data-model.md §4, contracts/library-and-
/// search-core.md §3 "Acting-list rule").
#[derive(Debug, Clone, PartialEq)]
pub struct ActingList {
    pub tracks: Vec<TrackRef>,
    pub cursor: usize,
}

/// What the caller currently knows about an Album/Playlist/Artist row's
/// full track list (`PlaybackController::library_track_list`, US2 —
/// `library_track_list` does not exist yet in this Foundational phase, so
/// this stays a plain enum the *caller* resolves however its own phase's
/// state allows).
#[derive(Debug, Clone, PartialEq)]
pub enum TrackListLookup {
    Ready(Vec<TrackRef>),
    Loading,
    Failed,
}

/// What [`acting_list`] resolved for one row activation (contracts/
/// library-and-search-core.md §3: Album/Playlist "in-place loading
/// indicator on the action" while a fetch is outstanding).
#[derive(Debug, Clone, PartialEq)]
pub enum ActingListOutcome {
    Ready(ActingList),
    Loading,
    Unavailable,
}

/// The acting-list rule (contracts/library-and-search-core.md §3): a track
/// row acts on the rows currently loaded in its own list (`loaded_rows`),
/// cursored at itself; an Album/Playlist/Artist row acts on its full track
/// list (`track_list`, from `PlaybackController::library_track_list` once
/// US2 lands), cursored at 0.
///
/// ```
/// use modplayer_audio_source::{Availability, TrackId, TrackRef};
/// use modplayer_ui::rows::{acting_list, ActingListOutcome, RowEntity, TrackListLookup};
///
/// let t = |id: &str| {
///     TrackRef::new(
///         TrackId::new(id).expect("valid"),
///         id,
///         vec![],
///         None,
///         None,
///         1000,
///         Availability::Available,
///     )
/// };
/// let loaded = vec![t("spotify:track:a"), t("spotify:track:b")];
/// let entity = RowEntity::Track(loaded[1].clone());
/// let ActingListOutcome::Ready(acting) = acting_list(&entity, &loaded, TrackListLookup::Loading)
/// else {
///     unreachable!("a track row with a populated loaded list always resolves")
/// };
/// assert_eq!(acting.cursor, 1);
/// assert_eq!(acting.tracks, loaded);
/// ```
pub fn acting_list(
    entity: &RowEntity,
    loaded_rows: &[TrackRef],
    track_list: TrackListLookup,
) -> ActingListOutcome {
    match entity {
        RowEntity::Track(track) => {
            let cursor = loaded_rows.iter().position(|row| row.id == track.id);
            match cursor {
                Some(cursor) => ActingListOutcome::Ready(ActingList {
                    tracks: loaded_rows.to_vec(),
                    cursor,
                }),
                // The row isn't in `loaded_rows` (a stale snapshot, or the
                // caller didn't pass one) — fall back to a singleton acting
                // list of just this track, per FR-006's spirit ("Play now"
                // must always work on the row that was activated).
                None => ActingListOutcome::Ready(ActingList {
                    tracks: vec![track.clone()],
                    cursor: 0,
                }),
            }
        }
        RowEntity::Album(_) | RowEntity::Playlist(_) | RowEntity::Artist(_) => match track_list {
            TrackListLookup::Ready(tracks) => {
                ActingListOutcome::Ready(ActingList { tracks, cursor: 0 })
            }
            TrackListLookup::Loading => ActingListOutcome::Loading,
            TrackListLookup::Failed => ActingListOutcome::Unavailable,
        },
    }
}

/// The row height for `entity` (research R9: "56 px track rows, 72 px
/// album/artist/playlist rows").
fn row_height(entity: &RowEntity) -> f32 {
    match entity {
        RowEntity::Track(_) => ROW_HEIGHT,
        RowEntity::Album(_) | RowEntity::Artist(_) | RowEntity::Playlist(_) => WIDE_ROW_HEIGHT,
    }
}

/// `"Unavailable in your region"` / `"Removed from the service"`
/// (data-model.md §1.2), or `None` when playable.
fn availability_reason(availability: Availability) -> Option<String> {
    match availability {
        Availability::Available => None,
        Availability::UnavailableRegion => Some(tr("row-unavailable-region")),
        Availability::Removed => Some(tr("row-removed")),
    }
}

/// The row's accessible name (contracts/ui-surface.md §5): "<title> —
/// <artists>[ — Unavailable in your region | Removed from the service]"
/// for tracks; "<name> — <artists>" for albums; just the name for artists
/// and playlists.
pub fn accessible_name(entity: &RowEntity) -> String {
    match entity {
        RowEntity::Track(track) => {
            let mut name = format!("{} — {}", track.title, track.artists.join(", "));
            if let Some(reason) = availability_reason(track.availability) {
                name.push_str(" — ");
                name.push_str(&reason);
            }
            name
        }
        RowEntity::Album(album) => format!("{} — {}", album.name, album.artists.join(", ")),
        RowEntity::Artist(artist) => artist.name.clone(),
        RowEntity::Playlist(playlist) => playlist.name.clone(),
    }
}

/// The artwork URL to fetch, if any (contracts/ui-surface.md §6).
fn artwork_url(entity: &RowEntity) -> Option<&str> {
    match entity {
        RowEntity::Track(track) => track.artwork_url.as_deref(),
        RowEntity::Album(album) => album.artwork_url.as_deref(),
        RowEntity::Artist(artist) => artist.artwork_url.as_deref(),
        RowEntity::Playlist(playlist) => playlist.artwork_url.as_deref(),
    }
}

/// The name [`initials_placeholder`] derives from on a `Failed`/no-URL
/// artwork (contracts/ui-surface.md §6: "album title (tracks/albums),
/// artist name, playlist name"). A track with no album title falls back to
/// its own title — the table names the common case, not a track that has
/// none.
fn artwork_name(entity: &RowEntity) -> &str {
    match entity {
        RowEntity::Track(track) => track.album.as_deref().unwrap_or(&track.title),
        RowEntity::Album(album) => &album.name,
        RowEntity::Artist(artist) => &artist.name,
        RowEntity::Playlist(playlist) => &playlist.name,
    }
}

/// A stable per-row id salt, so the same track/album/artist/playlist keeps
/// its focus/menu-open state across frames as the surrounding list scrolls
/// or reorders.
fn entity_salt(entity: &RowEntity) -> &str {
    match entity {
        RowEntity::Track(track) => track.id.as_str(),
        RowEntity::Album(album) => album.id.as_str(),
        RowEntity::Artist(artist) => artist.id.as_str(),
        RowEntity::Playlist(playlist) => playlist.id.as_str(),
    }
}

fn format_duration(duration_ms: u32) -> String {
    let total_seconds = duration_ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

/// What the artwork square paints for a given [`ArtworkState`] (contracts/
/// ui-surface.md §6, FR-020): the decoded texture when `Ready`, a neutral
/// square while `Loading`, and the initials placeholder — keyed by
/// [`artwork_name`] — on `Failed` or no URL at all (`state == None`, e.g.
/// no `artwork_url`). A pure mapping kept separate from [`draw_artwork`]'s
/// painting so the "artwork failure resolves to initials" rule (US3 T067)
/// is directly unit-testable without a rendered frame.
#[derive(Debug, Clone, PartialEq)]
enum ArtworkDecision {
    Texture(egui::TextureId),
    NeutralSquare,
    Initials(String),
}

fn artwork_decision(state: Option<ArtworkState>, name: &str) -> ArtworkDecision {
    match state {
        Some(ArtworkState::Ready(texture_id)) => ArtworkDecision::Texture(texture_id),
        Some(ArtworkState::Loading) => ArtworkDecision::NeutralSquare,
        Some(ArtworkState::Failed) | None => ArtworkDecision::Initials(name.to_string()),
    }
}

/// Draw the artwork square: the decoded texture when `Ready`, a neutral
/// square while `Loading`, and the initials placeholder on `Failed` or no
/// URL at all (contracts/ui-surface.md §6, FR-020).
fn draw_artwork(ui: &mut Ui, cache: &mut ArtworkCache, entity: &RowEntity) {
    let state = artwork_url(entity).map(|url| cache.get(ui.ctx(), url));
    match artwork_decision(state, artwork_name(entity)) {
        ArtworkDecision::Texture(texture_id) => {
            ui.add(egui::Image::from_texture((
                texture_id,
                Vec2::splat(ARTWORK_SIZE),
            )));
        }
        ArtworkDecision::NeutralSquare => {
            let (rect, _response) =
                ui.allocate_exact_size(Vec2::splat(ARTWORK_SIZE), Sense::hover());
            if ui.is_rect_visible(rect) {
                ui.painter()
                    .rect_filled(rect, theme::radius::MD, ui.visuals().faint_bg_color);
            }
        }
        ArtworkDecision::Initials(name) => {
            initials_placeholder(ui, &name, ARTWORK_SIZE);
        }
    }
}

/// One truncating text line of a row (never wraps, never overflows the
/// row's content column — the 2026-09-17 manual walk, quickstart M7,
/// found the stacked one-label-per-field layout overflowing `ROW_HEIGHT`
/// into the next row).
fn line(ui: &mut Ui, text: impl Into<RichText>) -> egui::Response {
    ui.add(Label::new(text.into()).truncate())
}

/// A row's own title, explicit `body` + `text_primary` (014-design-tokens-
/// and-type-scale, US2, T021, data-model.md §6 "List row title") — visibly
/// distinct from the `.weak()`/`text_secondary` detail line beneath it.
fn title_text(ui: &Ui, text: &str) -> RichText {
    RichText::new(text)
        .text_style(theme::text::BODY)
        .color(theme::roles(ui.visuals()).text_primary)
}

/// Draw the per-kind content columns (contracts/ui-surface.md §5): a
/// track is two lines — "title [E] [reason]" over
/// "artists — album — m:ss" — so every field the contract lists fits
/// `ROW_HEIGHT`; the wide kinds keep one field per line inside
/// `WIDE_ROW_HEIGHT`.
fn draw_content(ui: &mut Ui, entity: &RowEntity) {
    ui.spacing_mut().item_spacing.y = 2.0;
    match entity {
        RowEntity::Track(track) => {
            let reason = availability_reason(track.availability);
            let unavailable = reason.is_some();
            ui.horizontal(|ui| {
                if unavailable {
                    line(ui, RichText::new(&track.title).weak());
                } else {
                    let title = title_text(ui, &track.title);
                    line(ui, title);
                }
                if track.explicit {
                    let response = ui.label("E");
                    let explicit_label = tr("row-explicit");
                    ui.ctx()
                        .accesskit_node_builder(response.id, |b| b.set_label(explicit_label));
                }
                if let Some(reason) = reason {
                    line(ui, RichText::new(reason).weak());
                }
            });
            let mut detail = track.artists.join(", ");
            if let Some(album) = &track.album {
                detail.push_str(" — ");
                detail.push_str(album);
            }
            ui.horizontal(|ui| {
                line(ui, RichText::new(detail).weak());
                line(ui, RichText::new(" — ").weak());
                // 014-design-tokens-and-type-scale (US3, T045, data-model.md
                // §6 "duration" — the field this track-detail line ends
                // with, shared by every screen that lists tracks through
                // this row): `mono` so the digits line up in a fixed-width
                // column across rows, layered on the existing `.weak()`
                // secondary weight.
                line(
                    ui,
                    theme::mono_text(format_duration(track.duration_ms)).weak(),
                );
            });
        }
        RowEntity::Album(album) => {
            let title = title_text(ui, &album.name);
            line(ui, title);
            line(ui, RichText::new(album.artists.join(", ")).weak());
            if let Some(release) = &album.release_date {
                line(ui, RichText::new(release.year.to_string()).weak());
            }
        }
        RowEntity::Artist(artist) => {
            let title = title_text(ui, &artist.name);
            line(ui, title);
        }
        RowEntity::Playlist(playlist) => {
            let title = title_text(ui, &playlist.name);
            line(ui, title);
            if !playlist.editable {
                line(
                    ui,
                    RichText::new(tr_args(
                        "playlist-owner",
                        &[("name", playlist.owner_name.clone())],
                    ))
                    .weak(),
                );
            }
        }
    }
}

/// Draw the trailing six-action `Menu` and report which item (if any) was
/// clicked this frame (contracts/ui-surface.md §5). `open_request` forces
/// the menu open this frame regardless of the button's own click — used by
/// the row's right-click/`Shift+F10` triggers, computed by the caller
/// before this menu (and thus this frame) renders.
fn actions_menu(ui: &mut Ui, accessible_name: &str, open_request: bool) -> Option<RowAction> {
    let label = tr_args("row-actions", &[("name", accessible_name.to_string())]);
    let opener = ui.button("…");
    ui.ctx()
        .accesskit_node_builder(opener.id, |b| b.set_label(label));

    let menu_id = opener.id.with("row-actions-menu");
    let command = if opener.clicked() {
        Some(SetOpenCommand::Toggle)
    } else if open_request {
        Some(SetOpenCommand::Bool(true))
    } else {
        None
    };

    let mut chosen = None;
    Popup::new(menu_id, ui.ctx().clone(), &opener, opener.layer_id)
        .kind(PopupKind::Menu)
        .layout(Layout::top_down_justified(Align::Min))
        .open_memory(command)
        .show(|ui| {
            for action in RowAction::ORDER {
                let response = ui.button(tr(action.fluent_key()));
                ui.ctx()
                    .accesskit_node_builder(response.id, |b| b.set_role(Role::MenuItem));
                if response.clicked() {
                    chosen = Some(action);
                    Popup::close_id(ui.ctx(), menu_id);
                }
            }
        });
    chosen
}

/// The one row widget every catalog list renders through (contracts/ui-
/// surface.md §5, FR-004): artwork, per-kind content, an accessible
/// `ListItem` name including the availability reason, and the trailing
/// six-action menu — reachable by the "…" button (U+2026 — the midline
/// U+22EF is not in egui's bundled fonts and drew as tofu on the
/// 2026-09-17 manual walk), right-click, or
/// `Shift+F10` while the row has focus. Enter/double-click plays a track
/// row now, or opens a non-track row's detail (`RowEvent::Open`,
/// Complexity Tracking "Enter on album/artist/playlist rows opens the
/// detail view").
///
/// Applies nothing itself: the caller matches the returned [`RowEvent`]
/// and drives `PlaybackController`/`NotificationCenter` (design note 6).
pub fn list_row(ui: &mut Ui, artwork: &mut ArtworkCache, entity: &RowEntity) -> Option<RowEvent> {
    let height = row_height(entity);
    let name = accessible_name(entity);
    let row_id = ui.id().with(entity_salt(entity));

    let (_auto_id, rect) = ui.allocate_space(Vec2::new(ui.available_width(), height));
    let row_response = ui.interact(rect, row_id, Sense::click());
    // 007, contracts/ui-actions.md §2: claim this row's own keys
    // (`Shift+F10` for the actions menu, plus the toolkit set it would
    // get anyway) so the dispatcher never intercepts them.
    actions::register_claim(
        ui.ctx(),
        row_response.id,
        Claim::Keys(actions::row_claims()),
    );

    ui.ctx().accesskit_node_builder(row_response.id, |b| {
        b.set_role(Role::ListItem);
        b.set_label(name.clone());
    });

    // Computed before drawing content, so the actions menu (drawn inside
    // the child `Ui` below) sees this frame's row-level open request.
    let open_via_row = row_response.secondary_clicked()
        || (row_response.has_focus()
            && ui
                .ctx()
                .input(|i| i.modifiers.shift && i.key_pressed(Key::F10)));

    let mut action = None;
    if ui.is_rect_visible(rect) {
        let mut content_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        draw_artwork(&mut content_ui, artwork, entity);
        // Reserve the trailing "…" button's width so the text column
        // truncates instead of running underneath it.
        let text_width = (content_ui.available_width() - ACTIONS_RESERVED_WIDTH).max(0.0);
        content_ui.vertical(|ui| {
            ui.set_max_width(text_width);
            ui.set_min_width(text_width);
            draw_content(ui, entity);
        });
        content_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            action = actions_menu(ui, &name, open_via_row);
        });
    }

    if let Some(action) = action {
        return Some(RowEvent::Action(action));
    }

    let activated = row_response.double_clicked()
        || (row_response.has_focus() && ui.ctx().input(|i| i.key_pressed(Key::Enter)));
    if activated {
        return Some(match entity {
            RowEntity::Track(_) => RowEvent::Action(RowAction::PlayNow),
            RowEntity::Album(_) | RowEntity::Artist(_) | RowEntity::Playlist(_) => RowEvent::Open,
        });
    }

    None
}

/// Draw `count` rows of `row_height` inside a vertical scroll area, calling
/// `draw_row` only for the range the current scroll position can show — the
/// virtualisation pass every catalog list needs at scale (FR-014/SC-002,
/// design note 7, contracts/ui-surface.md §2/§3 "virtualised `show_rows`"):
/// a 50 000-row list costs O(visible rows) per frame, not O(rows), because
/// [`egui::ScrollArea::show_rows`] only ever invokes `draw_row` for the rows
/// the viewport can currently show (plus one row of margin either side) —
/// everything below `draw_artwork`'s `is_rect_visible` check inherits the
/// same bound for artwork fetches, and the returned range is exactly what a
/// caller should pass to a hydration-visibility hint (design note 7:
/// "visible ids first").
///
/// `max_height` caps the scroll area's own height; `None` fills whatever
/// space the caller's `Ui` has left — the common case for a full tab or
/// detail list. `id_salt` keeps the list's own scroll position stable
/// across frames as its row count changes (e.g. a hydration reply landing).
pub fn virtualized_list(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    row_height: f32,
    count: usize,
    max_height: Option<f32>,
    mut draw_row: impl FnMut(&mut Ui, usize),
) -> Range<usize> {
    let mut scroll = ScrollArea::vertical()
        .id_salt(id_salt)
        .auto_shrink([false, true]);
    if let Some(max_height) = max_height {
        scroll = scroll.max_height(max_height);
    }
    let mut visible = 0..0;
    scroll.show_rows(ui, row_height, count, |ui, range| {
        visible = range.clone();
        for i in range {
            draw_row(ui, i);
        }
    });
    visible
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};
    use modplayer_audio_source::{AlbumId, ArtistId, PlaylistId, ReleaseDate, TrackId};

    fn track(id: &str, availability: Availability) -> TrackRef {
        TrackRef::new(
            TrackId::new(id).unwrap(),
            "Song",
            vec!["Artist".to_string()],
            Some("Album".to_string()),
            None,
            201_000,
            availability,
        )
    }

    #[test]
    fn row_action_order_is_the_contract_fixed_order() {
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

    #[test]
    fn only_the_three_named_actions_are_placeholders() {
        for action in RowAction::ORDER {
            let expected = matches!(
                action,
                RowAction::AddToPlaylist | RowAction::SaveToLibrary | RowAction::PinForOffline
            );
            assert_eq!(action.is_placeholder(), expected, "{action:?}");
        }
    }

    #[test]
    fn accessible_name_appends_availability_reason() {
        let available = RowEntity::Track(track("spotify:track:a", Availability::Available));
        assert_eq!(accessible_name(&available), "Song — Artist");

        let region_locked =
            RowEntity::Track(track("spotify:track:b", Availability::UnavailableRegion));
        assert_eq!(
            accessible_name(&region_locked),
            format!("Song — Artist — {}", tr("row-unavailable-region"))
        );

        let removed = RowEntity::Track(track("spotify:track:c", Availability::Removed));
        assert_eq!(
            accessible_name(&removed),
            format!("Song — Artist — {}", tr("row-removed"))
        );
    }

    #[test]
    fn album_accessible_name_is_name_and_artists() {
        let album = AlbumRef {
            id: AlbumId::new("spotify:album:a").unwrap(),
            name: "Abbey Road".to_string(),
            artists: vec!["The Beatles".to_string()],
            artwork_url: None,
            release_date: Some(ReleaseDate {
                year: 1969,
                month: None,
                day: None,
            }),
            track_count: 17,
        };
        assert_eq!(
            accessible_name(&RowEntity::Album(album)),
            "Abbey Road — The Beatles"
        );
    }

    #[test]
    fn playlist_and_artist_accessible_names_are_just_the_name() {
        let artist = ArtistRef {
            id: ArtistId::new("spotify:artist:a").unwrap(),
            name: "Radiohead".to_string(),
            artwork_url: None,
        };
        assert_eq!(accessible_name(&RowEntity::Artist(artist)), "Radiohead");

        let playlist = PlaylistRef {
            id: PlaylistId::new("spotify:playlist:a").unwrap(),
            name: "Road Trip".to_string(),
            owner_name: "Alex".to_string(),
            editable: true,
            artwork_url: None,
            track_count: 12,
            revision: None,
        };
        assert_eq!(accessible_name(&RowEntity::Playlist(playlist)), "Road Trip");
    }

    #[test]
    fn track_row_height_is_the_narrow_constant() {
        assert_eq!(
            row_height(&RowEntity::Track(track(
                "spotify:track:a",
                Availability::Available
            ))),
            ROW_HEIGHT
        );
    }

    #[test]
    fn non_track_rows_use_the_wide_constant() {
        let artist = RowEntity::Artist(ArtistRef {
            id: ArtistId::new("spotify:artist:a").unwrap(),
            name: "Radiohead".to_string(),
            artwork_url: None,
        });
        assert_eq!(row_height(&artist), WIDE_ROW_HEIGHT);
    }

    #[test]
    fn acting_list_for_a_track_cursors_its_position_in_the_loaded_list() {
        let a = track("spotify:track:a", Availability::Available);
        let b = track("spotify:track:b", Availability::Available);
        let loaded = vec![a.clone(), b.clone()];
        let outcome = acting_list(
            &RowEntity::Track(b.clone()),
            &loaded,
            TrackListLookup::Loading,
        );
        assert_eq!(
            outcome,
            ActingListOutcome::Ready(ActingList {
                tracks: loaded,
                cursor: 1,
            })
        );
    }

    #[test]
    fn acting_list_for_a_track_not_in_loaded_rows_falls_back_to_a_singleton() {
        let a = track("spotify:track:a", Availability::Available);
        let outcome = acting_list(&RowEntity::Track(a.clone()), &[], TrackListLookup::Loading);
        assert_eq!(
            outcome,
            ActingListOutcome::Ready(ActingList {
                tracks: vec![a],
                cursor: 0,
            })
        );
    }

    #[test]
    fn acting_list_for_an_album_uses_the_full_track_list_at_cursor_zero() {
        let tracks = vec![
            track("spotify:track:a", Availability::Available),
            track("spotify:track:b", Availability::Available),
        ];
        let album = AlbumRef {
            id: AlbumId::new("spotify:album:a").unwrap(),
            name: "Abbey Road".to_string(),
            artists: vec![],
            artwork_url: None,
            release_date: None,
            track_count: 2,
        };
        let outcome = acting_list(
            &RowEntity::Album(album),
            &[],
            TrackListLookup::Ready(tracks.clone()),
        );
        assert_eq!(
            outcome,
            ActingListOutcome::Ready(ActingList { tracks, cursor: 0 })
        );
    }

    #[test]
    fn acting_list_for_an_album_reports_loading_and_failed() {
        let album = AlbumRef {
            id: AlbumId::new("spotify:album:a").unwrap(),
            name: "Abbey Road".to_string(),
            artists: vec![],
            artwork_url: None,
            release_date: None,
            track_count: 2,
        };
        assert_eq!(
            acting_list(
                &RowEntity::Album(album.clone()),
                &[],
                TrackListLookup::Loading
            ),
            ActingListOutcome::Loading
        );
        assert_eq!(
            acting_list(&RowEntity::Album(album), &[], TrackListLookup::Failed),
            ActingListOutcome::Unavailable
        );
    }

    #[test]
    fn track_row_reports_list_item_role_and_accessible_name() {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut cache = ArtworkCache::new();
        let entity = RowEntity::Track(track("spotify:track:a", Availability::Available));

        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            list_row(ui, &mut cache, &entity);
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .expect("accesskit_update should be populated once enabled");
        output.drop_without_applying_deltas();

        let expected_name = accessible_name(&entity);
        let found = update.nodes.iter().any(|(_, node)| {
            node.role() == Role::ListItem && node.label() == Some(&*expected_name)
        });
        assert!(found, "expected a ListItem node named `{expected_name}`");
    }

    /// `actions_menu` forced open (bypassing the click/hover machinery a
    /// synthetic frame can't drive) reports all six items with `MenuItem`
    /// role (contracts/ui-surface.md §5). The "…" button is the first
    /// widget created in each fresh top-level `Ui`, so its id is
    /// deterministic across the two `run_ui` passes below: one to learn
    /// the id `actions_menu` will produce, one — after forcing that popup
    /// id open — to render it.
    #[test]
    fn actions_menu_forced_open_lists_all_six_items_as_menu_items() {
        let ctx = Context::default();
        ctx.enable_accesskit();

        let mut opener_id = None;
        ctx.run_ui(RawInput::default(), |ui| {
            opener_id = Some(ui.button("…").id);
        })
        .drop_without_applying_deltas();

        let menu_id = opener_id.expect("button rendered").with("row-actions-menu");
        Popup::open_id(&ctx, menu_id);

        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let _ = actions_menu(ui, "Test Row", false);
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .expect("accesskit_update should be populated once enabled");
        output.drop_without_applying_deltas();

        let menu_item_count = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::MenuItem)
            .count();
        assert_eq!(menu_item_count, 6, "expected all six menu items open");
    }

    // -- Initials source + artwork-failure fallback (US3 T067, contracts/
    // ui-surface.md §6, FR-020) ---------------------------------------

    fn album(name: &str) -> AlbumRef {
        AlbumRef {
            id: AlbumId::new("spotify:album:a").unwrap(),
            name: name.to_string(),
            artists: vec![],
            artwork_url: None,
            release_date: None,
            track_count: 1,
        }
    }

    #[test]
    fn initials_source_for_a_track_is_its_album_title() {
        let mut t = track("spotify:track:a", Availability::Available);
        t.album = Some("Abbey Road".to_string());
        t.title = "Come Together".to_string();
        assert_eq!(artwork_name(&RowEntity::Track(t)), "Abbey Road");
    }

    #[test]
    fn initials_source_for_a_track_without_an_album_falls_back_to_its_own_title() {
        let mut t = track("spotify:track:a", Availability::Available);
        t.album = None;
        t.title = "Standalone".to_string();
        assert_eq!(artwork_name(&RowEntity::Track(t)), "Standalone");
    }

    #[test]
    fn initials_source_for_album_artist_playlist_is_their_own_name() {
        assert_eq!(
            artwork_name(&RowEntity::Album(album("Abbey Road"))),
            "Abbey Road"
        );

        let artist = ArtistRef {
            id: ArtistId::new("spotify:artist:a").unwrap(),
            name: "Radiohead".to_string(),
            artwork_url: None,
        };
        assert_eq!(artwork_name(&RowEntity::Artist(artist)), "Radiohead");

        let playlist = PlaylistRef {
            id: PlaylistId::new("spotify:playlist:a").unwrap(),
            name: "Road Trip".to_string(),
            owner_name: "Alex".to_string(),
            editable: true,
            artwork_url: None,
            track_count: 0,
            revision: None,
        };
        assert_eq!(artwork_name(&RowEntity::Playlist(playlist)), "Road Trip");
    }

    #[test]
    fn artwork_failure_or_missing_url_resolves_to_the_initials_placeholder() {
        assert_eq!(
            artwork_decision(Some(ArtworkState::Failed), "Abbey Road"),
            ArtworkDecision::Initials("Abbey Road".to_string()),
            "a failed fetch must fall back to initials, never a broken image"
        );
        assert_eq!(
            artwork_decision(None, "Abbey Road"),
            ArtworkDecision::Initials("Abbey Road".to_string()),
            "no artwork_url at all must fall back to initials the same way"
        );
    }

    #[test]
    fn artwork_loading_resolves_to_a_neutral_square_not_initials() {
        assert_eq!(
            artwork_decision(Some(ArtworkState::Loading), "Abbey Road"),
            ArtworkDecision::NeutralSquare,
            "a still-loading fetch must not flash the initials placeholder"
        );
    }

    #[test]
    fn artwork_ready_resolves_to_the_decoded_texture_not_initials() {
        let texture_id = egui::TextureId::default();
        assert_eq!(
            artwork_decision(Some(ArtworkState::Ready(texture_id)), "Abbey Road"),
            ArtworkDecision::Texture(texture_id)
        );
    }
}

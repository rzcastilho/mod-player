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
    Align, Id, Key, Label, Layout, Modifiers, Popup, PopupKind, Response, RichText, ScrollArea,
    Sense, SetOpenCommand, Ui, UiBuilder, Vec2,
};

use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, Availability, PlaylistId, PlaylistRef, TrackRef,
};
use modplayer_core::{Origin, QueueRow, tr, tr_args};

use crate::actions::{self, Claim};
use crate::artwork::{ArtworkCache, ArtworkState};
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::controls::{button, button_over};
use crate::widgets::initials::initials_placeholder;
use crate::widgets::skeleton::{ROW_HEIGHT, WIDE_ROW_HEIGHT};

/// Artwork square drawn to the left of every row (research R9). Album/
/// artist/playlist rows are visually taller (`WIDE_ROW_HEIGHT`) than track
/// rows, but share the same artwork size for now — a later polish pass may
/// grow it for the wide rows.
pub(crate) const ARTWORK_SIZE: f32 = 40.0;
/// The glyph on every "…" actions opener (U+2026; the midline U+22EF is
/// not in egui's bundled fonts).
const OPENER_GLYPH: &str = "…";

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

/// What activating a row reports (contracts/ui-surface.md §4-5, contract
/// list-row.md §8): a `RowAction` from the six-item menu, `Open` —
/// Enter/double-click on a non-track row (Enter is Play now only on track
/// rows; Complexity Tracking "Enter on album/artist/playlist rows opens the
/// detail view") — or `Select`, a primary single click (FR-006). Precedence
/// within one frame, in the order `list_row` returns them: `Action` ->
/// `Open`/`Action(PlayNow)` -> `Select`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowEvent {
    Action(RowAction),
    Open,
    Select,
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

/// A stable per-row id/selection key, so the same track/album/artist/
/// playlist keeps its focus/menu-open state across frames as the
/// surrounding list scrolls or reorders, and so [`RowSelection`] can name
/// the entity a stored selection points at (data-model.md §4). `pub` per
/// contract S3/S6 — every view's own `RowSelection` bookkeeping needs it.
pub fn entity_key(entity: &RowEntity) -> &str {
    match entity {
        RowEntity::Track(track) => track.id.as_str(),
        RowEntity::Album(album) => album.id.as_str(),
        RowEntity::Artist(artist) => artist.id.as_str(),
        RowEntity::Playlist(playlist) => playlist.id.as_str(),
    }
}

/// The kind-correct tooltip key (contract A7, FR-010): `Track` rows get
/// `row-open-hint-track` (a second click/Enter plays it); every other kind
/// gets `row-open-hint-entity` (a second click/Enter opens it).
fn open_hint_key(entity: &RowEntity) -> &'static str {
    match entity {
        RowEntity::Track(_) => "row-open-hint-track",
        RowEntity::Album(_) | RowEntity::Artist(_) | RowEntity::Playlist(_) => {
            "row-open-hint-entity"
        }
    }
}

/// `m:ss` below an hour, `h:mm:ss` at or above it (FR-033, data-model.md
/// §3): `3_599_000 -> "59:59"`, `3_600_000 -> "1:00:00"`. Minutes/seconds
/// are zero-padded in the hour form; minutes are not in the `m:ss` form
/// (unchanged from before this rollover).
///
/// ```
/// use modplayer_ui::rows::format_duration;
///
/// assert_eq!(format_duration(0), "0:00");
/// assert_eq!(format_duration(3_599_000), "59:59");
/// assert_eq!(format_duration(3_600_000), "1:00:00");
/// assert_eq!(format_duration(3_855_000), "1:04:15");
/// ```
#[must_use]
pub fn format_duration(duration_ms: u32) -> String {
    let total_seconds = duration_ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// One view's currently selected row, if any (data-model.md §4, contracts/
/// list-row.md §S1-S12): exclusive by construction — a view holds exactly
/// one `RowSelection`, so "at most one selected row per view" cannot be
/// violated by a call site. Never persisted (FR-028, contract S9): no
/// `Serialize`, no `settings.toml` field, no `ui.memory` write.
///
/// ```
/// use modplayer_ui::rows::RowSelection;
///
/// let mut selection = RowSelection::default();
/// selection.select("library-saved-tracks", "spotify:track:a", 3);
/// assert!(selection.is_selected("library-saved-tracks", "spotify:track:a", 3));
///
/// // A different key at the same stored index (a reorder) clears it.
/// selection.reconcile("library-saved-tracks", |_index| {
///     Some("spotify:track:b".to_string())
/// });
/// assert!(!selection.is_selected("library-saved-tracks", "spotify:track:a", 3));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowSelection(Option<SelectedRow>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedRow {
    /// The `virtualized_list` id salt of the list this selection lives in
    /// (data-model.md §5) — distinguishes "different list, same view"
    /// (contract S2) from "a different view entirely" (each of which owns
    /// its own `RowSelection`).
    list: String,
    /// [`entity_key`] of the selected entity.
    key: String,
    /// The row's display index in `list`'s current order, at the time it
    /// was selected — what [`RowSelection::reconcile`] re-checks every
    /// frame (contract S3/S4).
    index: usize,
}

impl RowSelection {
    /// Whether the row at `index` in `list`, keyed `key`, is the one
    /// currently selected (contract S6: two rows sharing a key but
    /// differing in index are distinguishable).
    #[must_use]
    pub fn is_selected(&self, list: &str, key: &str, index: usize) -> bool {
        matches!(
            &self.0,
            Some(selected)
                if selected.list == list && selected.key == key && selected.index == index
        )
    }

    /// Select `key` at `index` in `list`, replacing any prior selection —
    /// in this list, in another list of the same view, or none at all
    /// (contract S2, FR-006/FR-007).
    pub fn select(&mut self, list: &str, key: &str, index: usize) {
        self.0 = Some(SelectedRow {
            list: list.to_string(),
            key: key.to_string(),
            index,
        });
    }

    /// Clear the selection unconditionally (FR-028: tab/target/query
    /// change).
    pub fn clear(&mut self) {
        self.0 = None;
    }

    /// Re-check the stored selection against `list`'s current order
    /// (contract S3/S4, FR-012). O(1): `key_at` is invoked at most once,
    /// only when `list` matches the stored salt (contract S5) — the
    /// virtualization property `virtualized_list` depends on, so this never
    /// scans the list it's guarding. Clears when `key_at(index)` returns a
    /// different key (a reorder) or `None` (the row was removed, or the
    /// list shrank past that index).
    pub fn reconcile(&mut self, list: &str, key_at: impl FnOnce(usize) -> Option<String>) {
        let Some(selected) = &self.0 else {
            return;
        };
        if selected.list != list {
            return;
        }
        if key_at(selected.index).as_deref() != Some(selected.key.as_str()) {
            self.0 = None;
        }
    }
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
    draw_artwork_url(
        ui,
        cache,
        artwork_url(entity),
        artwork_name(entity),
        ARTWORK_SIZE,
    );
}

/// The URL/name half of [`draw_artwork`], factored out so callers that
/// don't have a `RowEntity` — the Queue panel's `queue_row` (021 research
/// R8) — can draw the same 40 px artwork square from a raw URL and
/// placeholder name. `size` is the square's side: rows pass
/// [`ARTWORK_SIZE`], the 025 collection header passes its 128 px square.
pub(crate) fn draw_artwork_url(
    ui: &mut Ui,
    cache: &mut ArtworkCache,
    url: Option<&str>,
    name: &str,
    size: f32,
) {
    let state = url.map(|url| cache.get(ui.ctx(), url));
    match artwork_decision(state, name) {
        ArtworkDecision::Texture(texture_id) => {
            ui.add(egui::Image::from_texture((texture_id, Vec2::splat(size))));
        }
        ArtworkDecision::NeutralSquare => {
            let (rect, _response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
            if ui.is_rect_visible(rect) {
                ui.painter()
                    .rect_filled(rect, theme::radius::MD, ui.visuals().faint_bg_color);
            }
        }
        ArtworkDecision::Initials(name) => {
            initials_placeholder(ui, &name, size);
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

/// A selected row's title override: same run, `text_on_accent` in place of
/// whatever colour it already carries (contract A3, data-model.md §7).
/// Unselected, `text` is returned unchanged.
fn selected_or(text: RichText, selected: bool, roles: &theme::Roles) -> RichText {
    if selected {
        text.color(roles.text_on_accent)
    } else {
        text
    }
}

/// A selected row's secondary-weight run: `text_on_accent` at full strength
/// in place of the unselected `.weak()` (contract A1-A4, data-model.md §7)
/// — a selected row's fill is `accent`, so a `.weak()` run would fall below
/// the 4.5:1 floor.
fn selected_or_weak(text: impl Into<RichText>, selected: bool, roles: &theme::Roles) -> RichText {
    let text = text.into();
    if selected {
        text.color(roles.text_on_accent)
    } else {
        text.weak()
    }
}

/// A Track row's secondary-line detail string (contract L5, FR-002):
/// artists joined, plus the album when present — no duration suffix. The
/// duration now lives in the row's own trailing column, not this line.
fn track_detail(track: &TrackRef) -> String {
    let mut detail = track.artists.join(", ");
    if let Some(album) = &track.album {
        detail.push_str(" — ");
        detail.push_str(album);
    }
    detail
}

/// A Track row's trailing-column duration label (contract L5, FR-002): the
/// `mono` role, via `theme::mono_text`, never a bare `RichText::new`.
fn duration_label(track: &TrackRef) -> RichText {
    theme::mono_text(format_duration(track.duration_ms))
}

/// Draw the per-kind content columns (contracts/ui-surface.md §5): a
/// track is two lines — "title [E] [reason]" over
/// "artists — album — m:ss" — so every field the contract lists fits
/// `ROW_HEIGHT`; the wide kinds keep one field per line inside
/// `WIDE_ROW_HEIGHT`. `selected` recolours every run `text_on_accent`
/// (contract A3, data-model.md §7) — never `.weak()` — since the row's own
/// fill becomes `accent` (FR-008, FR-031).
fn draw_content(ui: &mut Ui, entity: &RowEntity, selected: bool) {
    ui.spacing_mut().item_spacing.y = 2.0;
    let roles = theme::roles(ui.visuals());
    match entity {
        RowEntity::Track(track) => {
            let reason = availability_reason(track.availability);
            let unavailable = reason.is_some();
            ui.horizontal(|ui| {
                if unavailable {
                    let title = if selected {
                        RichText::new(&track.title)
                            .text_style(theme::text::BODY)
                            .color(roles.text_on_accent)
                    } else {
                        RichText::new(&track.title).weak()
                    };
                    line(ui, title);
                } else {
                    let title = selected_or(title_text(ui, &track.title), selected, roles);
                    line(ui, title);
                }
                if track.explicit {
                    // The "E" badge is `text_primary` by default (no
                    // `.weak()`) — selected recolours it, unselected stays
                    // the plain default label (data-model.md §7).
                    let response = if selected {
                        ui.label(RichText::new("E").color(roles.text_on_accent))
                    } else {
                        ui.label("E")
                    };
                    let explicit_label = tr("row-explicit");
                    ui.ctx()
                        .accesskit_node_builder(response.id, |b| b.set_label(explicit_label));
                }
                if let Some(reason) = reason {
                    line(ui, selected_or_weak(reason, selected, roles));
                }
            });
            // Contract L5, FR-002: the secondary line is artists/album only
            // — the duration moved to the row's own trailing column (drawn
            // in `list_row`, beside the "…" menu), so it no longer ends
            // this line with a `" — m:ss"` suffix.
            line(ui, selected_or_weak(track_detail(track), selected, roles));
        }
        RowEntity::Album(album) => {
            let title = selected_or(title_text(ui, &album.name), selected, roles);
            line(ui, title);
            line(
                ui,
                selected_or_weak(album.artists.join(", "), selected, roles),
            );
            if let Some(release) = &album.release_date {
                line(
                    ui,
                    selected_or_weak(release.year.to_string(), selected, roles),
                );
            }
        }
        RowEntity::Artist(artist) => {
            let title = selected_or(title_text(ui, &artist.name), selected, roles);
            line(ui, title);
        }
        RowEntity::Playlist(playlist) => {
            let title = selected_or(title_text(ui, &playlist.name), selected, roles);
            line(ui, title);
            if !playlist.editable {
                let owner = tr_args("playlist-owner", &[("name", playlist.owner_name.clone())]);
                line(ui, selected_or_weak(owner, selected, roles));
            }
        }
    }
}

/// The width kept free at a row's trailing edge for the "…" opener
/// (contract RM3, research R4): the opener's measured width plus one
/// `item_spacing.x` either side of it. Measured, not a constant, so it
/// tracks text scale.
fn actions_reserved_width(ui: &Ui) -> f32 {
    button_width(ui, OPENER_GLYPH).max(ui.spacing().interact_size.x)
        + 2.0 * ui.spacing().item_spacing.x
}

/// A key the open actions menu reacts to (contract RM6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavKey {
    Down,
    Up,
    Home,
    End,
}

/// The item index focus moves to when `key` is pressed on item `focused`
/// of `len` (contract RM6): ↓/↑ wrap (last → first, first → last),
/// Home/End jump to the ends. Pure, so the wrap maths is unit-tested.
pub(crate) fn step(focused: usize, key: NavKey, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let len_i = len as isize;
    match key {
        NavKey::Down => (focused as isize + 1).rem_euclid(len_i) as usize,
        NavKey::Up => (focused as isize - 1).rem_euclid(len_i) as usize,
        NavKey::Home => 0,
        NavKey::End => len - 1,
    }
}

/// Per-menu keyboard state kept in egui temp memory, keyed by the menu's
/// id (research R7): where focus returns to once the menu closes. Removed
/// on close.
#[derive(Debug, Clone, Copy)]
struct MenuNav {
    return_to: Id,
}

/// Draw the trailing six-action `Menu` and report which item (if any) was
/// clicked this frame (contracts/ui-surface.md §5). `open_request` forces
/// the menu open this frame regardless of the button's own click — used by
/// the row's right-click/`Shift+F10` triggers, computed by the caller
/// before this menu (and thus this frame) renders. Focus returns to the
/// opener when the menu closes; see [`actions_menu_returning`].
pub(crate) fn actions_menu(
    ui: &mut Ui,
    accessible_name: &str,
    open_request: bool,
) -> Option<RowAction> {
    actions_menu_returning(ui, accessible_name, open_request, None, false)
}

/// [`actions_menu`] with an explicit focus-return target (contract RM7):
/// `Some(id)` is the row that opened the menu; `None` is the opener itself
/// (the header's "…"). `on_accent` draws the opener on a selected row's
/// `accent` fill, so its glyph is `text_on_accent`.
pub(crate) fn actions_menu_returning(
    ui: &mut Ui,
    accessible_name: &str,
    open_request: bool,
    return_to: Option<Id>,
    on_accent: bool,
) -> Option<RowAction> {
    let label = tr_args("row-actions", &[("name", accessible_name.to_string())]);
    let over = on_accent.then(|| {
        let roles = theme::roles(ui.visuals());
        (roles.accent, roles.text_on_accent)
    });
    let opener = button_over(ui, Variant::Quiet, OPENER_GLYPH, over);
    ui.ctx()
        .accesskit_node_builder(opener.id, |b| b.set_label(label));

    let menu_id = opener.id.with("row-actions-menu");
    let nav_id = menu_id.with("nav");
    let return_id = return_to.unwrap_or(opener.id);
    let was_open = Popup::is_id_open(ui.ctx(), menu_id);
    let opened_by_keyboard = !was_open
        && opener.has_focus()
        && ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space));
    let command = if opener.clicked() {
        Some(SetOpenCommand::Toggle)
    } else if open_request {
        Some(SetOpenCommand::Bool(true))
    } else {
        None
    };

    // Menu keys are consumed only while this menu is open, before any item
    // is drawn, so egui's spatial focus navigation never sees them.
    let nav_key = if was_open {
        ui.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                Some(NavKey::Down)
            } else if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                Some(NavKey::Up)
            } else if i.consume_key(Modifiers::NONE, Key::Home) {
                Some(NavKey::Home)
            } else if i.consume_key(Modifiers::NONE, Key::End) {
                Some(NavKey::End)
            } else {
                None
            }
        })
    } else {
        None
    };
    // Captured once, before any item is drawn or re-focused, so one key
    // press moves focus by exactly one item.
    let originally_focused = ui.memory(|m| m.focused());
    let focus_first = opened_by_keyboard || (open_request && !was_open);

    let mut chosen = None;
    Popup::new(menu_id, ui.ctx().clone(), &opener, opener.layer_id)
        .kind(PopupKind::Menu)
        .layout(Layout::top_down_justified(Align::Min))
        .open_memory(command)
        .show(|ui| {
            let mut items: Vec<Response> = Vec::with_capacity(RowAction::ORDER.len());
            for action in RowAction::ORDER {
                let response = ui.button(tr(action.fluent_key()));
                ui.ctx()
                    .accesskit_node_builder(response.id, |b| b.set_role(Role::MenuItem));
                actions::register_claim(
                    ui.ctx(),
                    response.id,
                    Claim::Keys(actions::row_menu_item_claims()),
                );
                ui.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        response.id,
                        egui::EventFilter {
                            vertical_arrows: true,
                            ..egui::EventFilter::default()
                        },
                    );
                });
                if response.clicked() {
                    chosen = Some(action);
                }
                items.push(response);
            }
            if focus_first {
                items[0].request_focus();
            } else if let Some(key) = nav_key
                && let Some(current) = items.iter().position(|r| Some(r.id) == originally_focused)
            {
                items[step(current, key, items.len())].request_focus();
            }
        });

    if chosen.is_some() {
        Popup::close_id(ui.ctx(), menu_id);
    }
    let is_open = Popup::is_id_open(ui.ctx(), menu_id);
    if is_open {
        ui.data_mut(|d| {
            d.insert_temp(
                nav_id,
                MenuNav {
                    return_to: return_id,
                },
            )
        });
    } else if was_open || chosen.is_some() {
        // Activated or dismissed: focus goes back where the menu came from.
        if let Some(nav) = ui.data_mut(|d| {
            let nav = d.get_temp::<MenuNav>(nav_id);
            d.remove::<MenuNav>(nav_id);
            nav
        }) {
            ui.memory_mut(|m| m.request_focus(nav.return_to));
        }
    }
    chosen
}

/// The middle text column's width (contract L1, FR-001, data-model.md §1):
/// `available` minus the trailing region — the "…" opener's reserved width
/// (`actions_reserved`, measured, contract RM3) and the duration column's
/// own measure — never negative. Takes no `RowEntity`/row-height parameter
/// at all, so it is, by construction, the same subtraction for every row
/// kind and both row heights (contract L2).
fn content_column_width(available_width: f32, actions_reserved: f32, ctx: &egui::Context) -> f32 {
    (available_width - actions_reserved - theme::duration_measure(ctx)).max(0.0)
}

/// The one row widget every catalog list renders through (contracts/ui-
/// surface.md §5, FR-004, contracts/list-row.md): artwork, per-kind
/// content, an accessible `ListItem` name including the availability
/// reason, and the trailing six-action menu — reachable by the "…" button
/// (U+2026 — the midline U+22EF is not in egui's bundled fonts and drew as
/// tofu on the 2026-09-17 manual walk), right-click, or `Shift+F10` while
/// the row has focus. A single primary click reports `RowEvent::Select`
/// (FR-006); double-click/Enter plays a track row now, or opens a
/// non-track row's detail (`RowEvent::Open`, Complexity Tracking "Enter on
/// album/artist/playlist rows opens the detail view"). `selected` recolours
/// the row's fill and every text run `accent`/`text_on_accent`
/// (FR-008/FR-031) and marks the accesskit node accordingly (FR-011) —
/// the caller owns the view's own [`RowSelection`] and decides `selected`
/// from it (data-model.md §4/§6).
///
/// Applies nothing itself: the caller matches the returned [`RowEvent`]
/// and drives `PlaybackController`/`NotificationCenter`/`RowSelection`
/// (design note 6).
pub fn list_row(
    ui: &mut Ui,
    artwork: &mut ArtworkCache,
    entity: &RowEntity,
    selected: bool,
) -> Option<RowEvent> {
    let height = row_height(entity);
    let name = accessible_name(entity);
    let row_id = ui.id().with(entity_key(entity));

    let (_auto_id, rect) = ui.allocate_space(Vec2::new(ui.available_width(), height));
    // Reserve the row's hover/pressed fill's paint order now, before any
    // content draws (research R5, FR-018): `list_row` already owns `rect`
    // and is about to compute `row_response` below, so both are reused
    // as-is rather than re-allocated through `widgets::controls::row_frame`
    // (design note 7).
    let where_to_put_background = ui.painter().add(egui::Shape::Noop);
    let row_response = ui.interact(rect, row_id, Sense::click());
    // Contract A7/FR-010: every row states, on hover, that a second click
    // or Enter opens (or plays) it. A rendered tooltip can't be driven
    // headlessly (research R13) — this is manual scenario M4.
    let row_response = row_response.on_hover_text(tr(open_hint_key(entity)));
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
        // Contract A5, FR-011: role/label stay exactly as above (FR-025) —
        // this is the only new thing the node gains.
        b.set_selected(selected);
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
        // The row's own hover/pressed/selected fill (FR-008, FR-009,
        // contract A1/A2/I6): painted into the shape index reserved above,
        // beneath the content this block draws next — zero layout cost,
        // `row_response`'s own fields only (I7), never `widget_state()`.
        // Selected rows fill `accent`; hover/pressed still blend *over*
        // that base exactly as they do over `surface_base` when unselected
        // (data-model.md §7) — no new colour or alpha (FR-024).
        let roles = theme::roles(ui.visuals());
        let base = if selected {
            roles.accent
        } else {
            roles.surface_base
        };
        let fill = if row_response.is_pointer_button_down_on() {
            Some(base.blend(theme::controls::pressed_fill(roles)))
        } else if row_response.hovered() {
            Some(base.blend(theme::controls::hover_fill(roles)))
        } else if selected {
            Some(base)
        } else {
            None
        };
        if let Some(fill) = fill {
            ui.painter().set(
                where_to_put_background,
                egui::Shape::rect_filled(rect, egui::CornerRadius::ZERO, fill),
            );
        }

        let mut content_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        draw_artwork(&mut content_ui, artwork, entity);
        // Reserve the trailing region — the duration column plus the "…"
        // button's width — so the text column truncates before either
        // (contract L1/L2, FR-001/FR-003).
        let reserved = actions_reserved_width(&content_ui);
        let text_width =
            content_column_width(content_ui.available_width(), reserved, content_ui.ctx());
        content_ui.vertical(|ui| {
            ui.set_max_width(text_width);
            ui.set_min_width(text_width);
            draw_content(ui, entity, selected);
        });
        content_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            action =
                actions_menu_returning(ui, &name, open_via_row, Some(row_response.id), selected);
            // The trailing duration column (contract L1/L5, FR-001/FR-002):
            // a fixed `duration_measure` width ahead of the "…" menu, right-
            // aligned. Only a Track row draws a figure into it; Album/
            // Artist/Playlist rows reserve the same width and leave it
            // empty, so the trailing edge lines up identically for every
            // row kind (contract L2).
            let duration_width = theme::duration_measure(ui.ctx());
            ui.allocate_ui_with_layout(
                Vec2::new(duration_width, height),
                Layout::right_to_left(Align::Center),
                |ui| {
                    if let RowEntity::Track(track) = entity {
                        ui.add(Label::new(selected_or_weak(
                            duration_label(track),
                            selected,
                            roles,
                        )));
                    }
                },
            );
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

    // Contract S1/S7: a plain primary click selects and nothing else — the
    // actions menu's own button (drawn after `row_response` above, so it
    // wins egui's "in tie, pick last = topmost" hit-test) never reaches
    // here, so it never reports `Select`.
    if row_response.clicked() {
        // Contract S12: a primary click also takes keyboard focus, so an
        // immediately-following Enter activates this row (`has_focus() &&
        // Enter` above) rather than whatever last held focus.
        row_response.request_focus();
        return Some(RowEvent::Select);
    }

    None
}

// ---------------------------------------------------------------------
// `queue_row` (021-transport-bar-and-panel-layout, data-model.md §8,
// contracts/queue-row.md Q1-Q11, research R8)
// ---------------------------------------------------------------------

/// Actions [`queue_row`] can report back to the caller (data-model.md §8,
/// contract Q8-Q10): the caller applies exactly one of these to the
/// controller, keyed by the row's own `uid` — `queue_row` never touches
/// `PlaybackController` itself (design note 6, mirrors [`list_row`] /
/// [`RowEvent`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueRowAction {
    MoveUp,
    MoveDown,
    PlayNext,
    Remove,
}

/// The text column's minimum width (contract Q3) below which the row's
/// four quiet actions move to a second line beneath it, rather than being
/// elided or hidden (FR-017).
const QUEUE_TEXT_MIN_WIDTH: f32 = 120.0;

/// `queue_row`'s per-row action set, in display order (contract Q8/Q9):
/// Play next is absent on the current row; the other three are always
/// present — unchanged from `queue_view.rs`'s pre-021 behaviour.
fn queue_row_actions(row: &QueueRow) -> Vec<(&'static str, QueueRowAction)> {
    let mut actions = vec![
        ("queue-move-up", QueueRowAction::MoveUp),
        ("queue-move-down", QueueRowAction::MoveDown),
    ];
    if !row.is_current {
        actions.push(("queue-play-next", QueueRowAction::PlayNext));
    }
    actions.push(("queue-remove", QueueRowAction::Remove));
    actions
}

/// The origin/availability badges a row's artist line carries (contract
/// Q1) — unchanged from `queue_view.rs`'s pre-021 behaviour.
fn queue_badges(row: &QueueRow) -> Vec<&'static str> {
    let mut badges = Vec::new();
    if row.origin == Origin::PlayNext {
        badges.push("queue-badge-play-next");
    }
    if row.unavailable {
        badges.push("queue-badge-unavailable");
    }
    badges
}

/// A label's rendered `Body`-style width (018's own `wrap_switch_before`
/// pattern) — enough to decide whether the row's actions fit on the main
/// line (contract Q3), not a pixel-exact layout.
fn label_width(ui: &Ui, text: &str) -> f32 {
    egui::WidgetText::from(text)
        .into_galley(ui, None, f32::INFINITY, egui::TextStyle::Body)
        .size()
        .x
}

/// [`label_width`] plus the padding an `egui::Button` (the [`button`]
/// widget's base) adds around its label.
fn button_width(ui: &Ui, text: &str) -> f32 {
    label_width(ui, text) + 2.0 * ui.spacing().button_padding.x
}

/// `queue_row`'s accessible name (contract Q11): `queue-row-name`, or for
/// the current row `queue-row-name-current` — both externalised, with a
/// Fluent select on `$has_artist` so an artist-less row's name doesn't
/// carry a bare trailing comma (FR-013, FR-016).
fn queue_row_name(row: &QueueRow) -> String {
    let key = if row.is_current {
        "queue-row-name-current"
    } else {
        "queue-row-name"
    };
    tr_args(
        key,
        &[
            ("title", row.title.clone()),
            ("artist", row.artist.clone()),
            (
                "has_artist",
                if row.artist.is_empty() { "no" } else { "yes" }.to_string(),
            ),
        ],
    )
}

/// The Queue panel's row widget (contracts/queue-row.md Q1-Q11, research
/// R8): a sibling of [`list_row`] sharing its primitives (`ARTWORK_SIZE`,
/// [`draw_artwork_url`], `duration_measure`, `theme::mono_text`) rather
/// than going through `RowEntity`/`list_row` itself — there is no "…"
/// menu, no selection fill, and the four move/play-next/remove actions
/// are always visible instead of hidden in a menu (research R8's
/// rationale).
///
/// Leading → trailing: 40 px artwork; the text column (for the current
/// row, a ▶ glyph then the title; the artist and its badges below,
/// `.weak()`); the four quiet actions, each its own tab stop; the
/// 1-based `position`, right-aligned in a `duration_measure`-wide column,
/// in `theme::mono_text`. When the text column would drop below
/// [`QUEUE_TEXT_MIN_WIDTH`], the actions move to a second line beneath
/// the text column instead of eliding (contract Q3, FR-017). The current
/// row also gets a `theme::controls::nav_indicator`-stroke accent bar the
/// full height of the row, painted on the row's leading edge — a shape
/// and a position, not colour alone (contract Q5/Q6, FR-013/FR-014).
///
/// Applies nothing itself: the caller matches the returned
/// [`QueueRowAction`] and drives `PlaybackController` (design note 6,
/// mirrors [`list_row`]).
///
/// `QueueRow`'s `uid` is only ever minted by `Queue` itself (no public
/// constructor, by design), so this example builds one the same way every
/// caller does: through a live `PlaybackController`.
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// use modplayer_audio_io::FakeBackend;
/// use modplayer_audio_source::{Availability, TrackId, TrackRef};
/// use modplayer_audio_source_synthetic::SyntheticHost;
/// use modplayer_core::PlaybackController;
/// use modplayer_core::settings::SettingsStore;
/// use modplayer_ui::artwork::ArtworkCache;
/// use modplayer_ui::rows::queue_row;
///
/// let path = std::env::temp_dir().join(format!(
///     "modplayer-ui-doctest-queue-row-{}.toml",
///     std::process::id()
/// ));
/// let mut controller = PlaybackController::new(
///     FakeBackend::new(vec![]),
///     SyntheticHost::new(44_100),
///     SettingsStore::with_path(path),
/// );
/// controller.queue_replace(vec![TrackRef::new(
///     TrackId::new("spotify:track:a").expect("valid"),
///     "Song",
///     vec!["Artist".to_string()],
///     None,
///     None,
///     180_000,
///     Availability::Available,
/// )]);
/// let view = controller.queue_view();
/// let mut cache = ArtworkCache::new();
/// let _ = queue_row(ui, &mut cache, &view.items[0], 1);
/// # });
/// ```
pub fn queue_row(
    ui: &mut Ui,
    artwork: &mut ArtworkCache,
    row: &QueueRow,
    position: usize,
) -> Option<QueueRowAction> {
    let row_id = ui.id().with(("queue-row", row.uid));
    let name = queue_row_name(row);
    let actions = queue_row_actions(row);
    let badges = queue_badges(row);
    let spacing = ui.spacing().item_spacing.x;
    let duration_width = theme::duration_measure(ui.ctx());

    let actions_width: f32 = actions
        .iter()
        .map(|(key, _)| button_width(ui, &tr(key)))
        .sum::<f32>()
        + spacing * actions.len().saturating_sub(1) as f32;

    let available = ui.available_width();
    let inline_text_width =
        (available - ARTWORK_SIZE - spacing - actions_width - spacing - duration_width - spacing)
            .max(0.0);
    let wrap = inline_text_width < QUEUE_TEXT_MIN_WIDTH;
    let text_width = if wrap {
        (available - ARTWORK_SIZE - spacing - duration_width - spacing).max(0.0)
    } else {
        inline_text_width
    };

    let action_row_height = ui.spacing().interact_size.y;
    let height = if wrap {
        ROW_HEIGHT + spacing + action_row_height
    } else {
        ROW_HEIGHT
    };

    let (_auto_id, rect) = ui.allocate_space(Vec2::new(available, height));
    let where_to_put_background = ui.painter().add(egui::Shape::Noop);
    let response = ui.interact(rect, row_id, Sense::hover());

    let mut action = None;
    if ui.is_rect_visible(rect) {
        let roles = theme::roles(ui.visuals());
        let fill = if response.is_pointer_button_down_on() {
            Some(
                roles
                    .surface_base
                    .blend(theme::controls::pressed_fill(roles)),
            )
        } else if response.hovered() {
            Some(roles.surface_base.blend(theme::controls::hover_fill(roles)))
        } else {
            None
        };
        if let Some(fill) = fill {
            ui.painter().set(
                where_to_put_background,
                egui::Shape::rect_filled(rect, egui::CornerRadius::ZERO, fill),
            );
        }

        let mut content_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
        );
        content_ui.allocate_ui_with_layout(
            Vec2::new(available, ROW_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                draw_artwork_url(
                    ui,
                    artwork,
                    row.artwork_url.as_deref(),
                    &row.artwork_name,
                    ARTWORK_SIZE,
                );

                ui.scope(|ui| {
                    ui.set_max_width(text_width);
                    ui.set_min_width(text_width);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            if row.is_current {
                                ui.label(
                                    RichText::new(tr("queue-playing-glyph"))
                                        .color(theme::roles(ui.visuals()).accent),
                                );
                            }
                            line(ui, title_text(ui, &row.title));
                        });
                        if !row.artist.is_empty() || !badges.is_empty() {
                            ui.horizontal(|ui| {
                                if !row.artist.is_empty() {
                                    line(ui, RichText::new(row.artist.clone()).weak());
                                }
                                for key in &badges {
                                    ui.label(RichText::new(tr(key)).weak());
                                }
                            });
                        }
                    });
                });

                if !wrap {
                    for (key, act) in &actions {
                        if button(ui, Variant::Quiet, tr(key)).clicked() {
                            action = Some(*act);
                        }
                    }
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(duration_width, ROW_HEIGHT),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.add(Label::new(theme::mono_text(position.to_string())));
                        },
                    );
                });
            },
        );

        if wrap {
            content_ui.add_space(spacing);
            content_ui.horizontal(|ui| {
                ui.add_space(ARTWORK_SIZE + spacing);
                for (key, act) in &actions {
                    if button(ui, Variant::Quiet, tr(key)).clicked() {
                        action = Some(*act);
                    }
                }
            });
        }

        if row.is_current {
            ui.painter().line_segment(
                [rect.left_top(), rect.left_bottom()],
                theme::controls::nav_indicator(roles),
            );
        }
    }

    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_role(Role::ListItem);
        b.set_label(name.clone());
    });

    action
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
    draw_row: impl FnMut(&mut Ui, usize),
) -> Range<usize> {
    let mut scroll = ScrollArea::vertical()
        .id_salt(id_salt)
        .auto_shrink([false, true]);
    if let Some(max_height) = max_height {
        scroll = scroll.max_height(max_height);
    }
    virtualized_list_in(ui, scroll, row_height, count, draw_row).0
}

/// The pre-configured-area core [`virtualized_list`] wraps (020-shell-
/// navigation-and-gates, US3, contracts/section-memory.md): identical
/// virtualisation (`ScrollArea::show_rows`, only the viewport's rows laid
/// out), but the caller supplies the already-`id_salt`ed/`auto_shrink`ed
/// `scroll` — typically `SectionMemory::scroll_area(&key)` — instead of this
/// function building one from a bare `id_salt`/`max_height` pair. Returns
/// the drawn row range (exactly as `virtualized_list` did) plus the area's
/// vertical scroll offset this frame, for the caller to hand to
/// `SectionMemory::record`.
pub fn virtualized_list_in(
    ui: &mut Ui,
    scroll: ScrollArea,
    row_height: f32,
    count: usize,
    mut draw_row: impl FnMut(&mut Ui, usize),
) -> (Range<usize>, f32) {
    let mut visible = 0..0;
    let output = scroll.show_rows(ui, row_height, count, |ui, range| {
        visible = range.clone();
        for i in range {
            draw_row(ui, i);
        }
    });
    (visible, output.state.offset.y)
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
            list_row(ui, &mut cache, &entity, false);
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

    // -- Row actions menu keyboard maths (025, contract RM3/RM6) --------------

    #[test]
    fn step_wraps_at_both_ends_and_jumps_on_home_end() {
        assert_eq!(step(5, NavKey::Down, 6), 0);
        assert_eq!(step(0, NavKey::Up, 6), 5);
        assert_eq!(step(2, NavKey::Down, 6), 3);
        assert_eq!(step(2, NavKey::Up, 6), 1);
        assert_eq!(step(3, NavKey::Home, 6), 0);
        assert_eq!(step(3, NavKey::End, 6), 5);
        assert_eq!(step(0, NavKey::Down, 0), 0);
    }

    #[test]
    fn actions_reserved_width_is_the_measured_opener_plus_two_spacings() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let expected = button_width(ui, OPENER_GLYPH).max(ui.spacing().interact_size.x)
                + 2.0 * ui.spacing().item_spacing.x;
            assert_eq!(actions_reserved_width(ui), expected);
            assert!(actions_reserved_width(ui) > button_width(ui, OPENER_GLYPH));
        })
        .drop_without_applying_deltas();
    }

    // -- Three-column grid (contract L1/L2/L5, FR-001-003) ------------------

    #[test]
    fn content_column_width_follows_the_l1_formula() {
        egui::__run_test_ctx(|ctx| {
            for available in [1000.0_f32, 300.0, 50.0, 0.0] {
                let expected = (available - 30.0 - theme::duration_measure(ctx)).max(0.0);
                assert_eq!(content_column_width(available, 30.0, ctx), expected);
            }
            // The formula takes no row-height/entity parameter at all, so it
            // holds identically whether the row draws at `ROW_HEIGHT`
            // (Track) or `WIDE_ROW_HEIGHT` (Album/Artist/Playlist).
            assert_ne!(
                row_height(&RowEntity::Track(track(
                    "spotify:track:a",
                    Availability::Available
                ))),
                row_height(&RowEntity::Album(album("Abbey Road")))
            );
        });
    }

    #[test]
    fn content_column_width_is_identical_for_every_row_kind() {
        egui::__run_test_ctx(|ctx| {
            let available = 400.0;
            let artist = RowEntity::Artist(ArtistRef {
                id: ArtistId::new("spotify:artist:a").unwrap(),
                name: "Radiohead".to_string(),
                artwork_url: None,
            });
            let playlist = RowEntity::Playlist(PlaylistRef {
                id: PlaylistId::new("spotify:playlist:a").unwrap(),
                name: "Road Trip".to_string(),
                owner_name: "Alex".to_string(),
                editable: true,
                artwork_url: None,
                track_count: 0,
                revision: None,
            });
            let kinds = [
                RowEntity::Track(track("spotify:track:a", Availability::Available)),
                RowEntity::Album(album("Abbey Road")),
                artist,
                playlist,
            ];
            let widths: Vec<f32> = kinds
                .iter()
                .map(|_kind| content_column_width(available, 30.0, ctx))
                .collect();
            for w in &widths {
                assert!((w - widths[0]).abs() < f32::EPSILON, "{widths:?}");
            }
        });
    }

    #[test]
    fn track_detail_excludes_the_duration_suffix() {
        let mut t = track("spotify:track:a", Availability::Available);
        t.duration_ms = 65_000; // "1:05"
        let detail = track_detail(&t);
        assert_eq!(
            detail,
            format!("{} — {}", t.artists.join(", "), t.album.clone().unwrap())
        );
        assert!(!detail.ends_with(&format_duration(t.duration_ms)));
    }

    #[test]
    fn duration_label_is_built_through_mono_text() {
        let mut t = track("spotify:track:a", Availability::Available);
        t.duration_ms = 65_000;
        assert_eq!(
            duration_label(&t),
            theme::mono_text(format_duration(t.duration_ms))
        );
    }

    // -- format_duration rollover (contract L4, FR-033) --------------------

    #[test]
    fn format_duration_rolls_over_at_exactly_sixty_minutes() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(3_599_000), "59:59");
        assert_eq!(format_duration(3_600_000), "1:00:00");
        assert_eq!(format_duration(3_855_000), "1:04:15");
    }

    // -- Tooltip key selector (contract A7, FR-010) -------------------------

    #[test]
    fn open_hint_key_is_track_specific_else_the_shared_entity_hint() {
        let track_entity = RowEntity::Track(track("spotify:track:a", Availability::Available));
        assert_eq!(open_hint_key(&track_entity), "row-open-hint-track");

        let album_entity = RowEntity::Album(album("Abbey Road"));
        assert_eq!(open_hint_key(&album_entity), "row-open-hint-entity");

        let artist_entity = RowEntity::Artist(ArtistRef {
            id: ArtistId::new("spotify:artist:a").unwrap(),
            name: "Radiohead".to_string(),
            artwork_url: None,
        });
        assert_eq!(open_hint_key(&artist_entity), "row-open-hint-entity");

        let playlist_entity = RowEntity::Playlist(PlaylistRef {
            id: PlaylistId::new("spotify:playlist:a").unwrap(),
            name: "Road Trip".to_string(),
            owner_name: "Alex".to_string(),
            editable: true,
            artwork_url: None,
            track_count: 0,
            revision: None,
        });
        assert_eq!(open_hint_key(&playlist_entity), "row-open-hint-entity");
    }

    // -- RowSelection (contracts/list-row.md S2-S6, S9) ---------------------

    #[test]
    fn selecting_twice_leaves_exactly_one_row_selected_even_across_lists() {
        let mut selection = RowSelection::default();
        selection.select("library-saved-tracks", "spotify:track:a", 0);
        assert!(selection.is_selected("library-saved-tracks", "spotify:track:a", 0));

        // A second `select` — even in a different list of the same view —
        // overwrites the one field (contract S2, Clarification 2).
        selection.select("library-saved-albums", "spotify:album:a", 2);
        assert!(!selection.is_selected("library-saved-tracks", "spotify:track:a", 0));
        assert!(selection.is_selected("library-saved-albums", "spotify:album:a", 2));
    }

    #[test]
    fn is_selected_distinguishes_the_same_key_at_two_indices() {
        let mut selection = RowSelection::default();
        selection.select("library-saved-tracks", "spotify:track:a", 3);
        assert!(selection.is_selected("library-saved-tracks", "spotify:track:a", 3));
        assert!(!selection.is_selected("library-saved-tracks", "spotify:track:a", 7));
    }

    #[test]
    fn reconcile_keeps_the_selection_when_the_stored_index_still_matches() {
        let mut selection = RowSelection::default();
        selection.select("library-saved-tracks", "spotify:track:a", 40);
        // Outside any plausible rendered range — reconcile is keyed on the
        // stored index, not on what a viewport currently shows (S3).
        selection.reconcile("library-saved-tracks", |i| {
            assert_eq!(i, 40);
            Some("spotify:track:a".to_string())
        });
        assert!(selection.is_selected("library-saved-tracks", "spotify:track:a", 40));
    }

    #[test]
    fn reconcile_clears_on_a_different_key_at_the_stored_index() {
        let mut selection = RowSelection::default();
        selection.select("library-saved-tracks", "spotify:track:a", 0);
        selection.reconcile("library-saved-tracks", |_i| {
            Some("spotify:track:b".to_string())
        });
        assert!(!selection.is_selected("library-saved-tracks", "spotify:track:a", 0));
    }

    #[test]
    fn reconcile_clears_on_none_removal_or_a_shrunk_list() {
        let mut selection = RowSelection::default();
        selection.select("library-saved-tracks", "spotify:track:a", 0);
        selection.reconcile("library-saved-tracks", |_i| None);
        assert!(!selection.is_selected("library-saved-tracks", "spotify:track:a", 0));
    }

    #[test]
    fn reconcile_calls_key_at_at_most_once_and_only_on_a_matching_salt() {
        use std::cell::Cell;

        let calls = Cell::new(0);
        let mut selection = RowSelection::default();
        selection.select("library-saved-tracks", "spotify:track:a", 0);

        // A non-matching salt must never invoke `key_at` (O(1), contract
        // S5) — `FnOnce` alone can't prove zero calls, so this closure
        // records its own invocation count instead.
        selection.reconcile("library-saved-albums", |i| {
            calls.set(calls.get() + 1);
            Some(format!("should-not-be-called-{i}"))
        });
        assert_eq!(
            calls.get(),
            0,
            "key_at must not run for a non-matching list"
        );
        assert!(selection.is_selected("library-saved-tracks", "spotify:track:a", 0));

        selection.reconcile("library-saved-tracks", |i| {
            calls.set(calls.get() + 1);
            (i == 0).then(|| "spotify:track:a".to_string())
        });
        assert_eq!(
            calls.get(),
            1,
            "key_at must run exactly once for a matching list"
        );
    }

    #[test]
    fn row_selection_has_no_settings_sourced_construction_path() {
        // Contract S9, FR-028: `RowSelection` is never persisted — pinned
        // structurally, not just by this runtime check: neither
        // `RowSelection` nor `SelectedRow` derives `Serialize`/
        // `Deserialize` (data-model.md §4), and `modplayer-ui` does not
        // even depend on `serde` for it. At runtime, its only two states
        // are `Default` (empty) and whatever `select`/`reconcile` produce
        // this session — there is no `From<RawSettings>` or
        // settings-store-sourced constructor to hydrate one from
        // `settings.toml`.
        let mut selection = RowSelection::default();
        assert_eq!(selection, RowSelection::default());
        selection.select("library-saved-tracks", "spotify:track:a", 0);
        selection.clear();
        assert_eq!(
            selection,
            RowSelection::default(),
            "the only way back to the empty state is `clear`, never a persisted value"
        );
    }
}

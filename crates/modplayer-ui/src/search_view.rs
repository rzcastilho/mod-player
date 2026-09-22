// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Search view (contracts/ui-surface.md §2, FR-001-003/015-018): a
//! search box editing `SearchSession.raw_query`, the offline/no-results
//! states, and the four fixed-order groups (Tracks, Albums, Artists,
//! Playlists) rendered through `rows::list_row` — skeleton rows while
//! `Pending`, "Show more" per group, stale rows kept visible while
//! `RateLimited` (with a "Refreshing…" status).
//!
//! `apply_row_action` is the pure(ish) call-site logic `rows::list_row`
//! itself deliberately stays free of (design note 6): it resolves the
//! acting-list rule and applies the result to `controller`, and is the
//! single thing both this module's row loop and `tests/rows.rs` (T032)
//! drive directly, without needing to render or click anything.

use egui::{Key, TextEdit, Ui, accesskit::Role};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{SearchHit, SearchKind, SourceHost, TrackRef};
use modplayer_core::{GroupState, PlaybackController, Severity, tr, tr_args};

use crate::actions::{self, Claim};
use crate::artwork::ArtworkCache;
use crate::rows::{
    ActingListOutcome, RowAction, RowEntity, RowEvent, TrackListLookup, acting_list, list_row,
    virtualized_list,
};
use crate::theme;
use crate::widgets::skeleton::{ROW_HEIGHT, WIDE_ROW_HEIGHT, skeleton_row};

/// A group's own scroll area is capped to this many rows tall (US3 T071,
/// contracts/ui-surface.md §2 "virtualised `show_rows`") — long enough to
/// preview a page without four groups fighting over the window's height,
/// short enough that a `next_offset`-fed group of hundreds of rows (after
/// repeated Show more) still only lays out the handful currently visible.
const GROUP_VISIBLE_ROWS: f32 = 6.0;

const GROUP_ORDER: [SearchKind; 4] = [
    SearchKind::Track,
    SearchKind::Album,
    SearchKind::Artist,
    SearchKind::Playlist,
];

/// Draw the Search view: the search box, offline/no-results states, and
/// every group with hits (contracts/ui-surface.md §2). `focus_requested`
/// is `Shell::focus_search_requested` — consumed (set back to `false`)
/// the frame the search box actually takes focus.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    focus_requested: &mut bool,
) {
    let now = controller.now();
    // A full, owned snapshot: every further read in this function comes
    // from `session`, so `controller` stays free to mutate (search_mut,
    // search_show_more, apply_row_action) without a borrow conflict —
    // the same pattern `queue_view::show`'s `controller.queue_view()`
    // snapshot uses.
    let session = controller.search().clone();

    let search_label = ui.label(tr("search-placeholder"));
    let mut query = session.raw_query().to_string();
    let response = ui
        .add(TextEdit::singleline(&mut query).hint_text(tr("search-placeholder")))
        .labelled_by(search_label.id);
    // 007, contracts/ui-actions.md §2: the dispatcher already skips a
    // frame where `ctx.text_edit_focused()` is true, but registering the
    // claim explicitly keeps this consistent with every other text field
    // (settings search, Controls filter/capture) rather than relying
    // solely on egui's own `TextEdit` detection.
    actions::register_claim(ui.ctx(), response.id, Claim::TextLike);
    if *focus_requested {
        response.request_focus();
        *focus_requested = false;
    }
    if response.has_focus() && ui.ctx().input(|i| i.key_pressed(Key::Escape)) {
        controller.search_mut().set_query(String::new(), now);
    } else if response.changed() {
        controller.search_mut().set_query(query, now);
    }

    if session.offline() {
        // FR-006, U2: empty-state copy, capped at the 72-character measure
        // (research R17).
        ui.scope(|ui| {
            ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
            ui.label(tr("search-offline"));
        });
        return;
    }

    if session.refreshing() {
        let text = tr("refreshing");
        let response = ui.label(&text);
        ui.ctx().accesskit_node_builder(response.id, |b| {
            b.set_role(Role::Status);
            b.set_label(text.clone());
        });
    }

    if session.is_no_results() {
        // FR-006, U2: empty-state copy, capped at the 72-character measure.
        ui.scope(|ui| {
            ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
            ui.label(tr_args(
                "search-no-results",
                &[("query", session.query().to_string())],
            ));
        });
        return;
    }

    for kind in GROUP_ORDER {
        show_group(ui, controller, artwork, &session, kind);
    }
}

fn show_group<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    session: &modplayer_core::SearchSession,
    kind: SearchKind,
) {
    let group = session.group(kind).clone();
    match group {
        GroupState::Idle | GroupState::Unsupported | GroupState::Empty => {}
        GroupState::Pending => {
            draw_header(ui, kind);
            for _ in 0..3 {
                skeleton_row(ui, skeleton_height(kind));
            }
        }
        GroupState::Loaded {
            items,
            next_offset,
            loading_more,
        } => {
            draw_header(ui, kind);
            draw_rows(ui, controller, artwork, session.query(), kind, &items);
            if next_offset.is_some() {
                let label = tr_args("search-show-more", &[("group", tr(group_header_key(kind)))]);
                let clicked = ui
                    .add_enabled(!loading_more, egui::Button::new(label))
                    .clicked();
                if clicked {
                    controller.search_show_more(kind);
                }
            }
        }
        // Rate-limited with nothing stale to show: the view-level
        // "Refreshing…" status already says it all — a bare group header
        // would only read as an empty result (2026-09-17 walk, M12).
        GroupState::RateLimited { stale: None } => {}
        GroupState::RateLimited { stale: Some(items) } => {
            draw_header(ui, kind);
            draw_rows(ui, controller, artwork, session.query(), kind, &items);
        }
    }
}

fn draw_rows<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    query: &str,
    kind: SearchKind,
    items: &[SearchHit],
) {
    // Only the Tracks group has any `TrackRef`s to build a loaded-list
    // acting list from (`rows::acting_list`'s "rows currently loaded in
    // that list" rule) — always empty for the other three kinds, since
    // their entities are never `RowEntity::Track`. Cheap even at this
    // group's full (post-Show-more) size, so resolved unconditionally
    // rather than only on a click (unlike the Library view's per-id lookup,
    // this is a plain clone of a page already held in memory).
    let loaded_tracks = track_hits(items);
    let mut pending: Option<(RowEntity, RowAction)> = None;
    // The query is part of the scroll area's identity so a new query's
    // rows start at the top instead of inheriting the previous result's
    // scroll offset (2026-09-17 manual walk, quickstart M2).
    virtualized_list(
        ui,
        ("search-group", kind, query),
        skeleton_height(kind),
        items.len(),
        Some(skeleton_height(kind) * GROUP_VISIBLE_ROWS),
        |ui, i| {
            let entity = hit_entity(&items[i]);
            if let Some(RowEvent::Action(action)) = list_row(ui, artwork, &entity) {
                pending = Some((entity, action));
            }
            // `RowEvent::Open` (Album/Artist/Playlist detail views land
            // with US2, `detail_view.rs`, T064) has nothing to navigate to
            // from this view — search results open no detail of their own
            // in this slice.
        },
    );
    if let Some((entity, action)) = pending {
        apply_row_action(controller, &entity, &loaded_tracks, action);
    }
}

/// Apply one `RowAction` to `controller` (contracts/library-and-search-
/// core.md §3 "Acting-list rule", FR-005/006/007): the three placeholders
/// raise `coming-soon` and do nothing else; for the rest, a Track row acts
/// on its own single track for Play next/Add to queue, but on the *whole*
/// currently-loaded list (from `loaded_rows`, cursored at itself) for Play
/// now (FR-006, matching "Queue context = loaded Tracks rows in order,
/// cursor on that track"); an Album/Artist/Playlist row acts on its full
/// track list for every functional action once `library_track_list` (US2)
/// resolves it — until then `acting_list` reports `Loading`/`Unavailable`
/// and there is nothing to queue.
pub fn apply_row_action<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
    entity: &RowEntity,
    loaded_rows: &[TrackRef],
    action: RowAction,
) {
    if action.is_placeholder() {
        controller
            .notifications_mut()
            .raise(Severity::Info, "coming-soon");
        return;
    }
    let ActingListOutcome::Ready(acting) =
        acting_list(entity, loaded_rows, TrackListLookup::Loading)
    else {
        // Album/Artist/Playlist: the full track list isn't resolvable yet
        // in this phase (`PlaybackController::library_track_list` lands
        // with US2 T060) — `acting_list` already reports `Loading`/
        // `Unavailable` for the in-place indicator; there's nothing to
        // queue.
        return;
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
}

fn track_hits(items: &[SearchHit]) -> Vec<TrackRef> {
    items
        .iter()
        .filter_map(|hit| match hit {
            SearchHit::Track(track) => Some(track.clone()),
            _ => None,
        })
        .collect()
}

fn hit_entity(hit: &SearchHit) -> RowEntity {
    match hit {
        SearchHit::Track(track) => RowEntity::Track(track.clone()),
        SearchHit::Album(album) => RowEntity::Album(album.clone()),
        SearchHit::Artist(artist) => RowEntity::Artist(artist.clone()),
        SearchHit::Playlist(playlist) => RowEntity::Playlist(playlist.clone()),
    }
}

fn skeleton_height(kind: SearchKind) -> f32 {
    match kind {
        SearchKind::Track => ROW_HEIGHT,
        SearchKind::Album | SearchKind::Artist | SearchKind::Playlist => WIDE_ROW_HEIGHT,
    }
}

fn group_header_key(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Track => "search-group-tracks",
        SearchKind::Album => "search-group-albums",
        SearchKind::Artist => "search-group-artists",
        SearchKind::Playlist => "search-group-playlists",
    }
}

/// 014-design-tokens-and-type-scale (US2, T026, data-model.md §6 "Panel/
/// group headers" -> `theme::section_label`): each result group's own
/// header renders through the `section` role like every other panel/group
/// header (`markers::panel`'s "Markers", `settings::controls`'s category
/// headings). The accessible name stays the exact, un-uppercased `text`
/// (mirrors those same call sites) — set explicitly rather than left to
/// derive from the now-uppercased painted text.
fn draw_header(ui: &mut Ui, kind: SearchKind) {
    let text = tr(group_header_key(kind));
    let response = ui.label(theme::section_label(&text));
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_role(Role::Header);
        b.set_label(text.clone());
    });
}

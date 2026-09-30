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

use egui::{
    Key, Sense, TextEdit, Ui, UiBuilder,
    accesskit::{Live, Role},
};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{SearchHit, SearchKind, SourceHost, TrackRef};
use modplayer_core::{GroupState, PlaybackController, Severity, tr, tr_args};

use crate::actions::{self, Claim};
use crate::artwork::ArtworkCache;
use crate::notifications::{severity_color, severity_glyph};
use crate::rows::{
    ActingListOutcome, RowAction, RowEntity, RowEvent, RowSelection, TrackListLookup, acting_list,
    entity_key, list_row,
};
use crate::search_layout::{GroupBlock, PlacedGroup, ResultsLayout, truncate_query};
use crate::section_memory::{SectionMemory, ViewKey};
use crate::theme::{self, controls};
use crate::widgets;
use crate::widgets::skeleton::{SkeletonShape, skeleton_row};

const GROUP_ORDER: [SearchKind; 4] = [
    SearchKind::Track,
    SearchKind::Album,
    SearchKind::Artist,
    SearchKind::Playlist,
];

/// Rows a `Pending` group reserves while its reply is outstanding.
const SKELETON_ROWS: usize = 3;

/// This view's own frame-persistent state (US2, data-model.md §6): one
/// selection shared by all four groups (contract S2 — exclusive across
/// groups, not just within one), and the last query it was reconciled
/// against, so a new query clears the selection (FR-028, contract S8) and
/// resets the results scroll offset (026 RL11). Never persisted.
#[derive(Debug, Default)]
pub struct SearchViewState {
    pub selection: RowSelection,
    pub last_query: String,
    /// The query generation whose settled result count was already
    /// announced (`Live::Polite`, 026 N3) — so Show more and later frames
    /// do not announce it again.
    pub announced_generation: Option<u64>,
    /// Set by the clear control; consumed on the next frame by
    /// `request_focus` on the field (026 C4).
    pub refocus_field: bool,
}

/// The `RowSelection` list salt for one `SearchKind` group (data-model.md
/// §5: "one per `SearchKind` group") — distinct from the tuple id
/// `virtualized_list`'s own scroll position keys off, since a selection
/// salt must be a plain `&str`.
fn selection_salt(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Track => "search-tracks",
        SearchKind::Album => "search-albums",
        SearchKind::Artist => "search-artists",
        SearchKind::Playlist => "search-playlists",
    }
}

/// What the field's feedback area shows (026 data-model §5). Precedence:
/// Offline > Idle > InFlight > NoResults > RateLimited > Settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchStatus {
    Idle,
    Offline,
    InFlight,
    NoResults,
    RateLimited,
    /// Every shown group has answered; `count` is the sum of the header
    /// counts.
    Settled {
        count: usize,
    },
}

/// The empty state (E1–E3): a real query where at least one group came back
/// `Empty` and every other group is `Unsupported`. Unsupported groups are
/// omitted like empty ones (V3), so they must not block the empty state —
/// the live source answers Tracks only and reports the other three kinds
/// unsupported (004 research V1), which otherwise left a blank page (026
/// T043 M9).
fn is_no_results(session: &modplayer_core::SearchSession) -> bool {
    if session.is_no_results() {
        return true;
    }
    let mut any_empty = false;
    for (_, group) in session.groups() {
        match group {
            GroupState::Empty => any_empty = true,
            GroupState::Unsupported => {}
            _ => return false,
        }
    }
    !session.query().is_empty() && any_empty
}

/// Derive [`SearchStatus`] from the session alone.
pub(crate) fn view_status(session: &modplayer_core::SearchSession) -> SearchStatus {
    if session.offline() {
        return SearchStatus::Offline;
    }
    if session.query().is_empty() {
        return SearchStatus::Idle;
    }
    if session.in_flight() {
        return SearchStatus::InFlight;
    }
    if is_no_results(session) {
        return SearchStatus::NoResults;
    }
    if session.refreshing() && session.retry_scheduled() {
        return SearchStatus::RateLimited;
    }
    let shown = shown_groups(session);
    if shown.iter().any(|g| g.items.is_none()) {
        // A group is still `Pending`: not settled yet.
        return SearchStatus::InFlight;
    }
    if shown.is_empty() {
        // Nothing shown and nothing to say (e.g. every group unsupported).
        return SearchStatus::Idle;
    }
    SearchStatus::Settled {
        count: shown
            .iter()
            .filter_map(|g| g.items.as_ref())
            .map(Vec::len)
            .sum(),
    }
}

/// What one shown group draws this frame.
struct ShownGroup {
    kind: SearchKind,
    /// `None` while `Pending` (skeleton rows, no count).
    items: Option<Vec<SearchHit>>,
    show_more: Option<bool>, // Some(loading_more) when a footer is drawn
}

/// The shown groups in fixed order (data-model.md V3): `Pending`,
/// `Loaded` and `RateLimited { stale: Some }`; everything else is omitted.
fn shown_groups(session: &modplayer_core::SearchSession) -> Vec<ShownGroup> {
    GROUP_ORDER
        .into_iter()
        .filter_map(|kind| match session.group(kind).clone() {
            GroupState::Idle
            | GroupState::Unsupported
            | GroupState::Empty
            | GroupState::RateLimited { stale: None, .. } => None,
            GroupState::Pending => Some(ShownGroup {
                kind,
                items: None,
                show_more: None,
            }),
            GroupState::Loaded {
                items,
                next_offset,
                loading_more,
            } => Some(ShownGroup {
                kind,
                items: Some(items),
                show_more: next_offset.map(|_| loading_more),
            }),
            GroupState::RateLimited {
                stale: Some(items), ..
            } => Some(ShownGroup {
                kind,
                items: Some(items),
                show_more: None,
            }),
        })
        .collect()
}

/// Draw the Search view: the search box above exactly one results
/// `ScrollArea` (026 contracts/results-layout.md) holding every shown group
/// in a single column, virtualised, with the current group's header pinned.
/// `focus_requested` is `Shell::focus_search_requested` — consumed (set back
/// to `false`) the frame the search box actually takes focus. `state` owns
/// this view's selection (US2); `memory` owns the results area's scroll
/// offset under `ViewKey::Search` (020).
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    focus_requested: &mut bool,
    state: &mut SearchViewState,
    memory: &mut SectionMemory,
) {
    let now = controller.now();
    // A full, owned snapshot: every further read in this function comes
    // from `session`, so `controller` stays free to mutate (search_mut,
    // search_show_more, apply_row_action) without a borrow conflict —
    // the same pattern `queue_view::show`'s `controller.queue_view()`
    // snapshot uses.
    let session = controller.search().clone();

    // FR-028, contract S8: a new query invalidates every group's rows, so
    // the selection they carried no longer applies — and the results start
    // again from the top (RL11). Show more / stale transitions keep
    // `query()` and so never reach this branch.
    let mut reset_scroll = false;
    if session.query() != state.last_query.as_str() {
        state.selection.clear();
        state.last_query = session.query().to_string();
        reset_scroll = true;
    }

    let mut query = session.raw_query().to_string();
    let spacing = ui.spacing().item_spacing.x;
    let spinner_size = ui.spacing().interact_size.y * 0.6;
    // Reserve the trailing spinner + clear slots so the field never changes
    // width as they come and go (026 F1/C2).
    let trailing = spinner_size + ui.spacing().interact_size.x + spacing * 2.0;
    let show_clear = !session.raw_query().is_empty();
    let show_spinner = !session.offline() && session.in_flight();
    let mut clear_clicked = false;
    let response = ui
        .horizontal(|ui| {
            let response = ui.add(
                TextEdit::singleline(&mut query)
                    .hint_text(tr("search-hint"))
                    .desired_width((ui.available_width() - trailing).max(0.0)),
            );
            // The field is named once, by its own accessible label (F1) —
            // no visible "Search" label widget above it.
            ui.ctx().accesskit_node_builder(response.id, |b| {
                b.set_label(tr("search-field-label"));
            });
            if show_spinner {
                let spinner = ui.add(egui::Spinner::new().size(spinner_size));
                ui.ctx().accesskit_node_builder(spinner.id, |b| {
                    b.set_role(Role::ProgressIndicator);
                    b.set_label(tr("search-in-flight"));
                });
            } else {
                ui.allocate_space(egui::vec2(spinner_size, spinner_size));
            }
            if show_clear {
                let clear = widgets::controls::button(ui, controls::Variant::Quiet, "×");
                ui.ctx().accesskit_node_builder(clear.id, |b| {
                    b.set_label(tr("search-clear"));
                });
                clear_clicked = clear.clicked();
            }
            response
        })
        .inner;
    // 007, contracts/ui-actions.md §2: the dispatcher already skips a
    // frame where `ctx.text_edit_focused()` is true, but registering the
    // claim explicitly keeps this consistent with every other text field
    // (settings search, Controls filter/capture) rather than relying
    // solely on egui's own `TextEdit` detection.
    actions::register_claim(ui.ctx(), response.id, Claim::TextLike);
    if *focus_requested || state.refocus_field {
        response.request_focus();
        *focus_requested = false;
        state.refocus_field = false;
    }
    if clear_clicked {
        // Same path as Escape (C4), then hand focus back to the field.
        controller.search_mut().set_query(String::new(), now);
        state.refocus_field = true;
    } else if (response.has_focus() || response.lost_focus())
        && ui.ctx().input(|i| i.key_pressed(Key::Escape))
    {
        // egui's `TextEdit` surrenders focus on the Escape frame itself, so
        // `lost_focus()` is what's true here (F4, 026 T043 M9).
        controller.search_mut().set_query(String::new(), now);
    } else if response.changed() {
        controller.search_mut().set_query(query, now);
    }

    if let SearchStatus::Settled { count } = view_status(&session) {
        let text = tr_args("search-result-count", &[("count", count.to_string())]);
        let announce = state.announced_generation != Some(session.generation());
        let roles = theme::roles(ui.visuals());
        let response = ui.label(egui::RichText::new(&text).color(roles.text_secondary));
        ui.ctx().accesskit_node_builder(response.id, |b| {
            b.set_role(Role::Status);
            b.set_label(text.clone());
            // Polite exactly once per generation (N3): Show more and later
            // frames update the text silently.
            b.set_live(if announce { Live::Polite } else { Live::Off });
        });
        if announce {
            state.announced_generation = Some(session.generation());
        }
    }

    if view_status(&session) == SearchStatus::RateLimited {
        let stale = shown_groups(&session).iter().any(|g| g.items.is_some());
        draw_status_strip(
            ui,
            tr(if stale {
                "search-stale"
            } else {
                "search-rate-limited"
            }),
        );
    }

    let key = ViewKey::Search;
    let mut scroll = memory.scroll_area(&key);
    if reset_scroll {
        scroll = scroll.vertical_scroll_offset(0.0);
    }
    let mut pending: Option<(RowEntity, RowAction, SearchKind)> = None;
    let mut show_more_clicked: Option<SearchKind> = None;
    let mut empty_clear_clicked = false;
    let groups = shown_groups(&session);
    let output = scroll.show_viewport(ui, |ui, viewport| {
        if session.offline() {
            // FR-006, U2: empty-state copy, capped at the 72-character
            // measure (research R17).
            ui.scope(|ui| {
                ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
                ui.label(tr("search-offline"));
            });
            return;
        }
        if is_no_results(&session) {
            ui.scope(|ui| {
                ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
                ui.label(tr_args(
                    "search-no-results",
                    &[("query", truncate_query(session.query()).into_owned())],
                ));
            });
            if widgets::controls::button(ui, controls::Variant::Default, tr("search-clear"))
                .clicked()
            {
                empty_clear_clicked = true;
            }
            return;
        }
        draw_results(
            ui,
            artwork,
            &groups,
            viewport,
            &mut state.selection,
            &mut pending,
            &mut show_more_clicked,
        );
    });
    memory.record(key, output.state.offset.y);

    if empty_clear_clicked {
        // Same path as the field's clear control (C4 / E2).
        controller.search_mut().set_query(String::new(), now);
        state.refocus_field = true;
    }

    if let Some(kind) = show_more_clicked {
        controller.search_show_more(kind);
    }
    if let Some((entity, action, kind)) = pending {
        let loaded_tracks = groups
            .iter()
            .find(|g| g.kind == kind)
            .and_then(|g| g.items.as_deref())
            .map(track_hits)
            .unwrap_or_default();
        apply_row_action(controller, &entity, &loaded_tracks, action);
    }
}

/// The rate-limit status strip (026 V1–V4): raised surface, warning glyph,
/// one `Role::Status` node whose label is the text alone.
fn draw_status_strip(ui: &mut Ui, text: String) {
    let roles = theme::roles(ui.visuals());
    let frame = egui::Frame::new()
        .fill(roles.surface_raised)
        .corner_radius(theme::tokens::radius::SM)
        .inner_margin(theme::space::SM);
    frame.show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(severity_glyph(Severity::Warning))
                    .color(severity_color(roles, Severity::Warning)),
            );
            let response = ui.label(egui::RichText::new(&text).color(roles.text_primary));
            ui.ctx().accesskit_node_builder(response.id, |b| {
                b.set_role(Role::Status);
                b.set_label(text.clone());
            });
        });
    });
}

/// Height of one group header: the `section` text row plus `space::SM`.
fn header_height(ui: &Ui) -> f32 {
    ui.text_style_height(&theme::text::SECTION) + theme::space::SM
}

/// Lay out and draw every shown group inside the results scroll area's
/// content `ui`, virtualised against `viewport` (content coordinates).
fn draw_results(
    ui: &mut Ui,
    artwork: &mut ArtworkCache,
    groups: &[ShownGroup],
    viewport: egui::Rect,
    selection: &mut RowSelection,
    pending: &mut Option<(RowEntity, RowAction, SearchKind)>,
    show_more_clicked: &mut Option<SearchKind>,
) {
    let header_h = header_height(ui);
    let spacing_y = ui.spacing().item_spacing.y;
    let footer_h = ui.spacing().interact_size.y + spacing_y;
    let blocks: Vec<GroupBlock> = groups
        .iter()
        .map(|g| GroupBlock {
            kind: g.kind,
            header_h,
            row_h: skeleton_shape(g.kind).height + spacing_y,
            rows: g.items.as_ref().map_or(SKELETON_ROWS, Vec::len),
            footer_h: g.show_more.map(|_| footer_h),
        })
        .collect();
    let layout = ResultsLayout::new(&blocks, theme::space::LG);

    // A restored/seeded offset can exceed the new content on the frame egui
    // has not yet clamped it (020 M3): clamp here so that frame still draws
    // the last rows rather than nothing. The pinned header keeps egui's real
    // viewport top while that is inside the content: egui's content height
    // runs a little past `layout.total_height`, so at max scroll the clamped
    // `top` sits above the visible top (026 T043 M1).
    let visible_top = viewport.min.y;
    let top = viewport
        .min
        .y
        .min((layout.total_height - viewport.height()).max(0.0));
    let viewport = egui::Rect::from_min_size(egui::pos2(viewport.min.x, top), viewport.size());

    let width = ui.available_width();
    ui.set_min_size(egui::vec2(width, layout.total_height));
    let origin = ui.max_rect().min;
    let pin_top = if visible_top < layout.total_height {
        visible_top
    } else {
        top
    };
    let pinned = layout.pinned_header(pin_top);

    for (gi, (group, placed)) in groups.iter().zip(&layout.groups).enumerate() {
        let count = group.items.as_ref().map(Vec::len);
        if pinned.map(|(g, _)| g) != Some(gi)
            && placed.header_top < viewport.max.y
            && placed.header_top + placed.header_h > viewport.min.y
        {
            draw_header(ui, origin, width, placed, placed.header_top, count);
        }
        let salt = selection_salt(group.kind);
        if let Some(items) = &group.items {
            selection.reconcile(salt, |i| {
                items
                    .get(i)
                    .map(|hit| entity_key(&hit_entity(hit)).to_string())
            });
        }
        for i in layout.visible_rows(gi, viewport.min.y, viewport.max.y) {
            let y = placed.rows_top + i as f32 * placed.row_h;
            let rect = egui::Rect::from_min_size(
                origin + egui::vec2(0.0, y),
                egui::vec2(width, skeleton_shape(group.kind).height),
            );
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(rect)
                    .id_salt(("search-row", group.kind, i)),
                |ui| match &group.items {
                    None => {
                        skeleton_row(ui, skeleton_shape(group.kind));
                    }
                    Some(items) => {
                        let entity = hit_entity(&items[i]);
                        let key = entity_key(&entity);
                        let is_selected = selection.is_selected(salt, key, i);
                        match list_row(ui, artwork, &entity, is_selected) {
                            Some(RowEvent::Action(action)) => {
                                *pending = Some((entity, action, group.kind));
                            }
                            Some(RowEvent::Select) => selection.select(salt, key, i),
                            // `RowEvent::Open` has nothing to navigate to from
                            // this view — search results open no detail of
                            // their own in this slice.
                            Some(RowEvent::Open) | None => {}
                        }
                    }
                },
            );
        }
        if let (Some(loading_more), Some(footer_top)) = (group.show_more, placed.footer_top) {
            let rect = egui::Rect::from_min_size(
                origin + egui::vec2(0.0, footer_top),
                egui::vec2(width, footer_h),
            );
            if footer_top + footer_h > viewport.min.y && footer_top < viewport.max.y {
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(rect)
                        .id_salt(("search-footer", group.kind)),
                    |ui| {
                        let label = tr_args(
                            "search-show-more",
                            &[("group", tr(group_header_key(group.kind)))],
                        );
                        if ui
                            .add_enabled(!loading_more, egui::Button::new(label))
                            .clicked()
                        {
                            *show_more_clicked = Some(group.kind);
                        }
                    },
                );
            }
        }
    }

    // Painted last so it covers the rows beneath and, being the most recent
    // widget, takes the pointer over them (research R3).
    if let Some((gi, y)) = pinned {
        let group = &groups[gi];
        let placed = &layout.groups[gi];
        let rect = egui::Rect::from_min_size(
            origin + egui::vec2(0.0, y),
            egui::vec2(width, placed.header_h),
        );
        let roles = theme::roles(ui.visuals());
        ui.painter().rect_filled(rect, 0.0, roles.surface_base);
        ui.interact(
            rect,
            ui.id().with(("search-pinned-header", group.kind)),
            Sense::click(),
        );
        draw_header(
            ui,
            origin,
            width,
            placed,
            y,
            group.items.as_ref().map(Vec::len),
        );
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

fn skeleton_shape(kind: SearchKind) -> SkeletonShape {
    match kind {
        SearchKind::Track => SkeletonShape::TRACK,
        SearchKind::Album => SkeletonShape::ALBUM,
        SearchKind::Artist => SkeletonShape::ARTIST,
        SearchKind::Playlist => SkeletonShape::PLAYLIST,
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
/// header, followed by its row count in `text_secondary` (026 RL8; absent
/// while `Pending`). The accessible name is the exact, un-uppercased
/// `search-group-header` text ("Tracks, 20 results"), or just the name
/// while `Pending`. Drawn at content-space `y` so the pinned copy reuses it.
fn draw_header(
    ui: &mut Ui,
    origin: egui::Pos2,
    width: f32,
    placed: &PlacedGroup,
    y: f32,
    count: Option<usize>,
) {
    let name = tr(group_header_key(placed.kind));
    let access = match count {
        Some(n) => tr_args(
            "search-group-header",
            &[("group", name.clone()), ("count", n.to_string())],
        ),
        None => name.clone(),
    };
    let rect = egui::Rect::from_min_size(
        origin + egui::vec2(0.0, y),
        egui::vec2(width, placed.header_h),
    );
    let roles = theme::roles(ui.visuals());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .id_salt(("search-header", placed.kind)),
        |ui| {
            ui.horizontal_centered(|ui| {
                let response = ui.label(theme::section_label(&name));
                if let Some(n) = count {
                    ui.label(theme::mono_text(n.to_string()).color(roles.text_secondary));
                }
                ui.ctx().accesskit_node_builder(response.id, |b| {
                    b.set_role(Role::Header);
                    b.set_label(access.clone());
                });
            });
        },
    );
}

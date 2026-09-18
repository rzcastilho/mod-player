// SPDX-License-Identifier: MIT OR Apache-2.0

//! Marker lane glyphs, overlays and the Markers panel (006,
//! contracts/ui-markers.md).
//!
//! Through US2 this drew only region A/B bracket glyphs, the current loop
//! region's `[A, B)` span and a clamped marker's warning triangle, plus the
//! panel's header/empty-state/loop-only cells. US3 adds point/cue glyphs,
//! per-glyph click-to-focus and drag (a relative-delta drag that zooms the
//! detail view toward the live position — research R14), the
//! focused-marker keyboard table (arrows/Delete/F2/C/Esc), one panel row
//! per marker (colour swatch, role, inline-editable name, `m:ss.mmm`
//! position), and the 64-marker limit's inline refusal on `M`.

use std::ops::Range;

use egui::accesskit::Role;
use egui::{
    Color32, EventFilter, Id, Key, Modifiers, Painter, Rect, RichText, Sense, Stroke, Ui, pos2,
    vec2,
};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::markers::{
    Marker, MarkerId, MarkerKind, PaletteIndex, RegionId, RepeatCount, TrackMarkers,
};
use modplayer_core::{LoopState, PlaybackController, tr, tr_args};

use crate::actions::{self, Claim};
use crate::theme;
use crate::waveform::state::DETAIL_MIN_WINDOW_MS;
use crate::waveform::{self, DetailWindow, MarkerDrag, TimeSpace, WaveformState};

/// The marker lane's fixed height (006, contracts/ui-markers.md §1).
pub const LANE_HEIGHT: f32 = 14.0;

/// How far a bracket's top/bottom tick extends sideways from its vertical
/// stem, in pixels.
const BRACKET_TICK_PX: f32 = 3.0;

/// A glyph's clickable/draggable hit target width, in pixels — wider than
/// the 1-2px painted line so it stays comfortably clickable.
const GLYPH_HIT_WIDTH: f32 = 12.0;

/// Draw the marker lane above a waveform (006, contracts/ui-markers.md
/// §1, §3): a fixed [`LANE_HEIGHT`]-tall strip sharing `window`'s
/// `TimeSpace` with the waveform painted just below it — every marker's
/// glyph (region `[`/`]` brackets, a point's downward triangle, a cue's
/// numbered square), each a small click-and-drag hit target: click (or the
/// hit target gaining egui focus) selects it (`select_marker`,
/// `waveform.focused_marker`); press-drag beyond egui's own threshold
/// starts a relative-delta drag (research R14) that commits
/// `move_marker` on release or restores the original position/detail
/// window on `Esc`. `lane_kind` distinguishes the overview's hit targets
/// from the detail's own (both lanes render every marker visible in their
/// own window, but only the lane a drag actually started on drives it).
#[allow(clippy::too_many_arguments)]
pub fn lane<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    lane_kind: &'static str,
    window: Range<u64>,
    sample_rate: u32,
    len_frames: u64,
    markers: Option<&TrackMarkers>,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
    detail: &mut DetailWindow,
) {
    let width = ui.available_width();
    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(width, LANE_HEIGHT), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let Some(markers) = markers else {
        return;
    };
    let space = TimeSpace::new(rect, window, sample_rate);

    for marker in markers.markers() {
        let color = theme::marker_color(marker.color);
        let x = space.x_of(marker.position);
        let focused = waveform.focused_marker == Some(marker.id);
        paint_glyph(ui.painter(), marker.kind, x, rect, color, focused);

        let glyph_id = Id::new(("marker-glyph", lane_kind, marker.id));
        let hit_rect =
            Rect::from_center_size(pos2(x, rect.center().y), vec2(GLYPH_HIT_WIDTH, LANE_HEIGHT));
        let response = ui.interact(hit_rect, glyph_id, Sense::click_and_drag());
        let glyph_name = glyph_accessible_name(marker, sample_rate);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, glyph_name.clone())
        });
        // 007, contracts/ui-actions.md §2: claim the widget-local keys
        // (Delete/Backspace/F2/Enter/C/Escape) — not arrows, so a focused
        // glyph's nudge keeps reaching `HostAction::NudgeEarlier`/`Later`
        // via `Scope::MarkerFocused`. The horizontal-arrow event filter
        // (design note 12) stops egui's own focus traversal from also
        // moving focus off the glyph on a nudge arrow.
        actions::register_claim(ui.ctx(), glyph_id, Claim::Keys(actions::marker_claims()));
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                glyph_id,
                EventFilter {
                    horizontal_arrows: true,
                    ..EventFilter::default()
                },
            );
        });

        if response.clicked() {
            waveform.focused_marker = Some(marker.id);
            let _ = controller.select_marker(marker.id);
            response.request_focus();
        }
        // Keyboard focus (`Tab`) counts exactly as much as a click: the
        // focused-marker key table (`handle_focused_marker_keys`) is gated
        // on `focused_marker`, so without this a keyboard-only user could
        // Tab onto a glyph and still not nudge/rename/recolour/delete it —
        // FR-022's "every action reachable without a pointer".
        else if response.has_focus() && waveform.focused_marker != Some(marker.id) {
            waveform.focused_marker = Some(marker.id);
            let _ = controller.select_marker(marker.id);
        }
        if response.drag_started() {
            waveform.focused_marker = Some(marker.id);
            let _ = controller.select_marker(marker.id);
            waveform.marker_drag = Some(MarkerDrag {
                marker: marker.id,
                origin_position: marker.position,
                live: marker.position,
                origin_detail: *detail,
            });
        }

        let Some(drag) = waveform.marker_drag else {
            continue;
        };
        if drag.marker != marker.id {
            continue;
        }
        // egui itself treats `Esc` as a global "abort any in-progress
        // drag" signal: it force-clears its own dragged-widget state
        // *before* our code runs, which makes `response.drag_stopped()`
        // true on an `Esc` press too (not just a real pointer release) —
        // so `Esc` must be checked, and handled, ahead of treating
        // `drag_stopped()` as a commit.
        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            *detail = drag.origin_detail;
            waveform.marker_drag = None;
            continue;
        }
        if response.dragged() {
            let delta_x = f64::from(ui.input(|input| input.pointer.delta().x));
            let delta_frames = (delta_x * space.frames_per_pixel()).round() as i64;
            let live = signed_add(drag.live, delta_frames).min(len_frames);
            let target_width = zoom_assist_target_width(rect.width(), sample_rate);
            *detail = detail.zoom_assist(live, target_width, len_frames, sample_rate);
            waveform.marker_drag = Some(MarkerDrag { live, ..drag });
        }
        if response.drag_stopped() {
            let _ = controller.move_marker(marker.id, drag.live);
            waveform.marker_drag = None;
        }
    }
}

/// `delta` added to `base`, saturating at `0` (never panics on the
/// unsigned<->signed boundary a relative pointer delta crosses).
fn signed_add(base: u64, delta: i64) -> u64 {
    if delta >= 0 {
        base.saturating_add(delta.unsigned_abs())
    } else {
        base.saturating_sub(delta.unsigned_abs())
    }
}

/// The marker-drag zoom-assist's target width in frames (006, research
/// R14, contracts/ui-markers.md §5): `max(rect_width_px × rate × 0.0025,
/// DETAIL_MIN_WINDOW_MS × rate / 1000)` — at most 2.5 ms/px, comfortably
/// under SC-003's 5 ms landing margin.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn zoom_assist_target_width(rect_width_px: f32, sample_rate: u32) -> u64 {
    let from_pixels = (f64::from(rect_width_px) * f64::from(sample_rate) * 0.0025).round() as u64;
    let floor = DETAIL_MIN_WINDOW_MS * u64::from(sample_rate.max(1)) / 1000;
    from_pixels.max(floor)
}

/// `marker-glyph { $role } { $name } { $time }` (contracts/ui-markers.md
/// §1, §3) — the glyph hit target's accessible name, shared with the
/// overlay/panel's own role labelling (`role_label`, below `panel`).
fn glyph_accessible_name(marker: &Marker, sample_rate: u32) -> String {
    tr_args(
        "marker-glyph",
        &[
            ("role", role_label(marker.kind)),
            ("name", marker.name.clone()),
            (
                "time",
                format_mmss_millis_frames(marker.position, sample_rate),
            ),
        ],
    )
}

/// Paint one marker's glyph at `x` (006, contracts/ui-markers.md §3):
/// region `[`/`]` brackets, a point's downward triangle, or a cue's
/// numbered square — 2px-stroked (vs. 1.5px/plain) while `focused`.
fn paint_glyph(
    painter: &Painter,
    kind: MarkerKind,
    x: f32,
    rect: Rect,
    color: Color32,
    focused: bool,
) {
    match kind {
        MarkerKind::RegionStart { .. } => paint_bracket(painter, x, rect, color, true, focused),
        MarkerKind::RegionEnd { .. } => paint_bracket(painter, x, rect, color, false, focused),
        MarkerKind::Point => paint_point_glyph(painter, x, rect, color, focused),
        MarkerKind::Cue { slot } => paint_cue_glyph(painter, x, rect, color, slot.get(), focused),
    }
}

/// One `[`/`]` bracket glyph: a vertical stem at `x` spanning `rect`'s
/// full height, with a short tick at top and bottom pointing into the
/// region (`open` = `[`, ticks point right; `open = false` = `]`, ticks
/// point left).
fn paint_bracket(painter: &Painter, x: f32, rect: Rect, color: Color32, open: bool, focused: bool) {
    let stroke = Stroke::new(if focused { 2.0 } else { 1.5 }, color);
    let dx = if open {
        BRACKET_TICK_PX
    } else {
        -BRACKET_TICK_PX
    };
    painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], stroke);
    painter.line_segment([pos2(x, rect.top()), pos2(x + dx, rect.top())], stroke);
    painter.line_segment(
        [pos2(x, rect.bottom()), pos2(x + dx, rect.bottom())],
        stroke,
    );
}

/// A `Point` marker's glyph (contracts/ui-markers.md §3): a 10px downward
/// triangle, apex pointing into the waveform below.
fn paint_point_glyph(painter: &Painter, x: f32, rect: Rect, color: Color32, focused: bool) {
    const HALF_WIDTH: f32 = 5.0;
    const HEIGHT: f32 = 10.0;
    let top = rect.top();
    let points = vec![
        pos2(x - HALF_WIDTH, top),
        pos2(x + HALF_WIDTH, top),
        pos2(x, top + HEIGHT),
    ];
    let outline = if focused {
        Stroke::new(1.5, Color32::WHITE)
    } else {
        Stroke::NONE
    };
    painter.add(egui::Shape::convex_polygon(points, color, outline));
}

/// A `Cue { slot }` marker's glyph (contracts/ui-markers.md §3): a 10px
/// square with the slot digit.
fn paint_cue_glyph(painter: &Painter, x: f32, rect: Rect, color: Color32, slot: u8, focused: bool) {
    const HALF: f32 = 5.0;
    let square = Rect::from_center_size(pos2(x, rect.center().y), vec2(HALF * 2.0, HALF * 2.0));
    painter.rect_filled(square, 1.0, color);
    if focused {
        painter.rect_stroke(
            square,
            1.0,
            Stroke::new(1.5, Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }
    painter.text(
        square.center(),
        egui::Align2::CENTER_CENTER,
        slot.to_string(),
        egui::FontId::monospace(9.0),
        Color32::WHITE,
    );
}

/// The focused-marker keyboard table (006, contracts/ui-markers.md §3,
/// minus the four nudge arrows — now `HostAction::NudgeEarlier`/`Later`/
/// `…X10` via the 007 dispatcher, `Scope::MarkerFocused`): active only
/// while a glyph/row has focus (`waveform.focused_marker`), no text field
/// of this view has focus, and no rename is already open (the row's own
/// inline `TextEdit` handles its own Enter/Esc — `show_marker_row`,
/// below). Call once per frame, after the panel (so a fresh `F2`/`Enter`
/// this same frame opens the rename that panel draws on the *next*
/// frame).
pub fn handle_focused_marker_keys<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
) {
    let Some(id) = waveform.focused_marker else {
        return;
    };
    if waveform.rename.is_some() {
        return;
    }
    if ui.ctx().text_edit_focused() {
        return;
    }

    let Some(action) = waveform::focused_marker_key(ui) else {
        return;
    };
    match action {
        waveform::MarkerKeyAction::Delete => {
            let _ = controller.delete_marker(id);
            waveform.focused_marker = None;
        }
        waveform::MarkerKeyAction::OpenRename => {
            let name = controller
                .markers()
                .and_then(|markers| markers.marker(id))
                .map(|marker| marker.name.clone())
                .unwrap_or_default();
            waveform.rename = Some((id, name));
        }
        waveform::MarkerKeyAction::CycleColor => {
            let _ = controller.cycle_marker_color(id);
        }
        waveform::MarkerKeyAction::ReturnFocus => {
            waveform.focused_marker = None;
        }
    }
}

/// Paint every marker's line plus the current loop region's `[A, B)` span
/// through the `overlays` hook (006, research R16, contracts/ui-markers.md
/// §1): a 1px (2px while `focused`) vertical line per marker in its
/// palette colour, then the current region's span — outline when
/// disarmed, hatched when armed-inactive, solid (translucent) when
/// armed-active (data-model.md §3's table). A no-op with no markers/no
/// current region.
pub fn paint_overlay(
    painter: &Painter,
    space: &TimeSpace,
    markers: Option<&TrackMarkers>,
    loop_state: u8,
    focused: Option<MarkerId>,
) {
    let Some(markers) = markers else {
        return;
    };
    let rect = space.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    for marker in markers.markers() {
        let color = theme::marker_color(marker.color);
        let x = space.x_of(marker.position);
        let width = if focused == Some(marker.id) { 2.0 } else { 1.0 };
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(width, color),
        );
        if marker.clamped {
            paint_clamped_warning(painter, x, rect.top(), color);
        }
    }

    let Some(region) = markers.current_region().and_then(|id| markers.region(id)) else {
        return;
    };
    let Some((a, b)) = region.span(markers) else {
        return;
    };
    let x0 = space.x_of(a);
    let x1 = space.x_of(b).max(x0 + 1.0);
    let span_rect = Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom()));
    let color = region
        .a
        .and_then(|id| markers.marker(id))
        .map(|marker| theme::marker_color(marker.color))
        .unwrap_or_else(|| theme::marker_color(PaletteIndex::new(0)));

    match loop_state {
        2 => {
            painter.rect_filled(span_rect, 0.0, color.gamma_multiply(0.25));
        }
        1 => paint_hatched(painter, span_rect, color),
        _ => {
            painter.rect_stroke(
                span_rect,
                0.0,
                Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// The small warning triangle drawn at the top of a `clamped` marker's
/// overlay line (006, FR-018, contracts/ui-markers.md §1): a filled
/// triangle pointing down into the line, apex at `(x, top)`.
fn paint_clamped_warning(painter: &Painter, x: f32, top: f32, color: Color32) {
    const HALF_WIDTH: f32 = 4.0;
    const HEIGHT: f32 = 6.0;
    let points = vec![
        pos2(x, top),
        pos2(x - HALF_WIDTH, top + HEIGHT),
        pos2(x + HALF_WIDTH, top + HEIGHT),
    ];
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

/// A simple diagonal-line hatch (armed-inactive, data-model.md §3) —
/// spaced 8px apart, clipped to `rect`.
fn paint_hatched(painter: &Painter, rect: Rect, color: egui::Color32) {
    let stroke = Stroke::new(1.0, color.gamma_multiply(0.6));
    let step = 8.0;
    let width = rect.width();
    let height = rect.height();
    let mut offset = -height;
    while offset < width {
        let x0 = rect.left() + offset;
        let x1 = x0 + height;
        let p0 = pos2(x0.clamp(rect.left(), rect.right()), rect.bottom());
        let p1 = pos2(x1.clamp(rect.left(), rect.right()), rect.top());
        painter.line_segment([p0, p1], stroke);
        offset += step;
    }
}

/// One marker's row data, snapshotted read-only before `panel` starts
/// mutating `controller` through the row widgets (avoids holding a
/// `&TrackMarkers` borrow of `controller` across `&mut controller` calls).
struct MarkerRowData {
    id: MarkerId,
    kind: MarkerKind,
    name: String,
    color: PaletteIndex,
    position: u64,
    clamped: bool,
    /// `Some` only for a `RegionStart` row — the region whose loop cells
    /// (arm/repeat/crossfade/wraps) render right after it (contracts/
    /// ui-markers.md §4: "A row only, spanning the region").
    region: Option<RegionId>,
}

impl MarkerRowData {
    fn from_marker(marker: &Marker) -> Self {
        let region = match marker.kind {
            MarkerKind::RegionStart { region } => Some(region),
            MarkerKind::RegionEnd { .. } | MarkerKind::Point | MarkerKind::Cue { .. } => None,
        };
        Self {
            id: marker.id,
            kind: marker.kind,
            name: marker.name.clone(),
            color: marker.color,
            position: marker.position,
            clamped: marker.clamped,
            region,
        }
    }
}

/// One region's loop cells (loop-only, spanning the region's `A` row).
struct RegionRow {
    armed: bool,
    crossfade_ms: u8,
    repeat_times: Option<u16>,
    armable: bool,
    complete: bool,
    /// Either endpoint marker's `clamped` flag (006, FR-018, SC-012):
    /// the region survived a shorter re-saved track but one of its
    /// endpoints landed beyond the new length and was pulled back.
    clamped: bool,
}

/// The Markers panel (006, contracts/ui-markers.md §4): the header
/// ("Markers", "New loop region", "Clear all markers"/its two-step
/// confirm), the empty state, or one row per marker — colour swatch,
/// role/kind label, inline-editable name, `m:ss.mmm` position, a clamped
/// warning — with a `RegionStart` row additionally showing its region's
/// loop cells (arm toggle, repeat, crossfade, wraps-remaining/infinite,
/// armed-inactive badge). Every `DragValue`/`TextEdit` registers a
/// `Claim::TextLike` (007, contracts/ui-actions.md §2) so the dispatcher
/// leaves it alone while it has focus.
pub fn panel<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
) {
    ui.horizontal(|ui| {
        ui.heading(tr("markers-panel"));
        if ui.button(tr("markers-new-loop")).clicked() {
            let _ = controller.new_loop_region();
        }
        clear_all_controls(ui, controller, waveform);
    });
    if let Some(key) = waveform.marker_status {
        ui.label(tr(key));
    }

    let count = controller.markers().map(TrackMarkers::count).unwrap_or(0);
    if count == 0 {
        ui.label(tr("markers-empty"));
        return;
    }

    let rows: Vec<MarkerRowData> = controller
        .markers()
        .map(|markers| {
            markers
                .markers()
                .iter()
                .map(MarkerRowData::from_marker)
                .collect()
        })
        .unwrap_or_default();

    for row in rows {
        show_marker_row(ui, controller, waveform, &row);
        if let Some(region) = row.region {
            show_region_cells(ui, controller, region);
        }
    }
}

/// One marker's row (contracts/ui-markers.md §4): colour swatch (click
/// cycles the palette — the same action as the focused-marker `C` key),
/// role/kind label, inline-editable name (empty region/cue names show
/// only the role), `m:ss.mmm` position (the live drag position while
/// dragging), and a clamped warning glyph.
fn show_marker_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
    row: &MarkerRowData,
) {
    let rate = controller.source_sample_rate().max(1);
    let row_name = row_accessible_name(row, rate);
    let row_id = ui.id().with(("markers-row", row.id));

    let outer = ui.horizontal(|ui| {
        let color = theme::marker_color(row.color);
        if ui
            .add(egui::Button::new("").fill(color).min_size(vec2(16.0, 16.0)))
            .on_hover_text(tr_args(
                "markers-color",
                &[("index", row.color.get().to_string())],
            ))
            .clicked()
        {
            let _ = controller.cycle_marker_color(row.id);
        }

        ui.label(role_label(row.kind));

        let renaming = waveform
            .rename
            .as_ref()
            .is_some_and(|(id, _)| *id == row.id);
        if renaming {
            let mut draft = waveform
                .rename
                .as_ref()
                .map(|(_, name)| name.clone())
                .unwrap_or_default();
            let response = ui
                .add(egui::TextEdit::singleline(&mut draft))
                .on_hover_text(tr("markers-rename"));
            ui.ctx().accesskit_node_builder(response.id, |builder| {
                builder.set_label(tr("markers-rename"));
            });
            response.request_focus();
            actions::register_claim(ui.ctx(), response.id, Claim::TextLike);
            if let Some((_, name)) = waveform.rename.as_mut() {
                *name = draft.clone();
            }
            if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
                let _ = controller.rename_marker(row.id, &draft);
                waveform.rename = None;
            } else if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                waveform.rename = None;
            }
        } else if !row.name.is_empty() {
            ui.label(row.name.clone());
        }

        let live_position = waveform
            .marker_drag
            .filter(|drag| drag.marker == row.id)
            .map_or(row.position, |drag| drag.live);
        let rate = controller.source_sample_rate().max(1);
        ui.label(format_mmss_millis_frames(live_position, rate));

        if row.clamped {
            ui.label(RichText::new("⚠").color(ui.visuals().warn_fg_color))
                .on_hover_text(tr("marker-clamped-desc"));
        }
    });

    // The row as a whole is a `markers-row` container, `Role::ListItem`
    // (contracts/ui-markers.md §4, FR-022) — a non-interactive overlay
    // response bound to the row's own rect and a stable id, registered
    // after every inner widget already handled this frame's input so it
    // never intercepts a click meant for the swatch/name/rename field
    // (mirrors `rows.rs::list_row`'s own row-level node).
    let row_response = ui.interact(outer.response.rect, row_id, Sense::hover());
    ui.ctx().accesskit_node_builder(row_response.id, |b| {
        b.set_role(Role::ListItem);
        b.set_label(row_name.clone());
    });
}

/// The Markers panel row's own accessible name (contracts/ui-markers.md
/// §4's `markers-row` container): the same `marker-glyph { role, name,
/// time }` template the lane glyph uses (`glyph_accessible_name`), built
/// from the row's own snapshot fields instead of a `&Marker` since
/// `panel` already destructured rows into [`MarkerRowData`] before this
/// point.
fn row_accessible_name(row: &MarkerRowData, sample_rate: u32) -> String {
    tr_args(
        "marker-glyph",
        &[
            ("role", role_label(row.kind)),
            ("name", row.name.clone()),
            ("time", format_mmss_millis_frames(row.position, sample_rate)),
        ],
    )
}

/// `marker-role-a` / `-b` / `-cue { $slot }` / `-point` (contracts/
/// ui-markers.md §4).
fn role_label(kind: MarkerKind) -> String {
    match kind {
        MarkerKind::RegionStart { .. } => tr("marker-role-a"),
        MarkerKind::RegionEnd { .. } => tr("marker-role-b"),
        MarkerKind::Point => tr("marker-role-point"),
        MarkerKind::Cue { slot } => tr_args("marker-role-cue", &[("slot", slot.get().to_string())]),
    }
}

/// `"m:ss.mmm"` for a frame count at `sample_rate` (contracts/ui-markers.md
/// §4's row position cell; no leading-zero minutes, matching
/// `now_playing.rs`'s `format_mmss_frames`).
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn format_mmss_millis_frames(frame: u64, sample_rate: u32) -> String {
    let rate = f64::from(sample_rate.max(1));
    let total_ms = (frame as f64 / rate * 1000.0).round() as u64;
    let millis = total_ms % 1_000;
    let total_seconds = total_ms / 1_000;
    format!(
        "{}:{:02}.{:03}",
        total_seconds / 60,
        total_seconds % 60,
        millis
    )
}

/// `region`'s loop cells (arm toggle, repeat, crossfade, wraps-remaining/
/// infinite, armed-inactive badge, clamped-endpoint warning — contracts/
/// ui-markers.md §4), rendered right after its `A` row.
fn show_region_cells<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    region: RegionId,
) {
    let rate = controller.source_sample_rate().max(1);
    let row = controller.markers().and_then(|markers| {
        let r = markers.region(region)?;
        let clamped = [r.a, r.b]
            .into_iter()
            .flatten()
            .any(|id| markers.marker(id).is_some_and(|m| m.clamped));
        Some(RegionRow {
            armed: r.armed,
            crossfade_ms: r.crossfade_ms,
            repeat_times: match r.repeat {
                RepeatCount::Infinite => None,
                RepeatCount::Times(n) => Some(n),
            },
            armable: r.is_armable(markers, rate),
            complete: r.is_complete(),
            clamped,
        })
    });
    let Some(row) = row else {
        return;
    };
    let status = controller.loop_status();

    ui.horizontal(|ui| {
        let mut armed = row.armed;
        let label = if armed {
            tr("loop-disarm")
        } else {
            tr("loop-arm")
        };
        let enabled = row.armed || row.armable;
        let response = ui.add_enabled(enabled, egui::Checkbox::new(&mut armed, label));
        if response.changed() {
            if armed {
                let _ = controller.arm_loop(region);
            } else {
                let _ = controller.disarm_loop();
            }
        }
        if !enabled {
            let reason = if row.complete {
                "loop-region-too-short"
            } else {
                "loop-region-incomplete"
            };
            response.on_disabled_hover_text(tr(reason));
        }
        if row.armed && status.state == LoopState::ArmedInactive {
            ui.label(tr("loop-armed-inactive"));
        }
        if row.clamped {
            ui.label(RichText::new("⚠").color(ui.visuals().warn_fg_color))
                .on_hover_text(tr("marker-clamped-desc"));
        }
    });

    ui.horizontal(|ui| {
        let repeat_label = ui.label(tr("loop-repeat"));
        let mut repeat_value: i64 = row.repeat_times.map(i64::from).unwrap_or(0);
        let response = ui
            .add(egui::DragValue::new(&mut repeat_value).range(0..=1_000))
            .labelled_by(repeat_label.id);
        actions::register_claim(ui.ctx(), response.id, Claim::TextLike);
        if response.changed() {
            let repeat = if repeat_value <= 0 {
                RepeatCount::Infinite
            } else {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                RepeatCount::times_clamped(repeat_value as u16)
            };
            let _ = controller.set_loop_repeat(region, repeat);
        }

        let crossfade_label = ui.label(tr("loop-crossfade"));
        let mut crossfade_value: i64 = i64::from(row.crossfade_ms);
        let response = ui
            .add(
                egui::DragValue::new(&mut crossfade_value)
                    .range(0..=50)
                    .suffix(" ms"),
            )
            .labelled_by(crossfade_label.id);
        actions::register_claim(ui.ctx(), response.id, Claim::TextLike);
        if response.changed() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let ms = crossfade_value.clamp(0, 50) as u8;
            let _ = controller.set_loop_crossfade_ms(region, ms);
        }
    });

    if row.armed && status.state == LoopState::ArmedActive {
        let text = match row.repeat_times {
            None => tr("loop-wraps-infinite"),
            Some(n) => {
                let remaining = u32::from(n).saturating_sub(status.wraps);
                tr_args("loop-wraps-remaining", &[("count", remaining.to_string())])
            }
        };
        ui.label(text);
    }
}

/// The "Clear all markers" header control (006, contracts/ui-markers.md
/// §4): a plain button until pressed, then the two-step inline
/// confirmation (`markers-clear-confirm { $count }` + yes/no buttons);
/// `Esc` cancels back to the plain button without clearing anything.
fn clear_all_controls<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
) {
    if waveform.clear_confirm {
        let count = controller.markers().map(TrackMarkers::count).unwrap_or(0);
        ui.label(tr_args(
            "markers-clear-confirm",
            &[("count", count.to_string())],
        ));
        if ui.button(tr("markers-clear-yes")).clicked() {
            controller.clear_all_markers();
            waveform.clear_confirm = false;
        }
        if ui.button(tr("markers-clear-no")).clicked() {
            waveform.clear_confirm = false;
        }
        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            waveform.clear_confirm = false;
        }
    } else if ui.button(tr("markers-clear-all")).clicked() {
        waveform.clear_confirm = true;
    }
}

/// View-level shortcut refusal reasons (contracts/ui-markers.md §2): `I`/
/// `O`/`M`/`Shift+1..8` refuse with `marker-limit-reached`; `L` refuses
/// with `loop-region-incomplete`/`loop-region-too-short`. Cleared on the
/// next successful action.
pub fn refusal_key(error: modplayer_core::markers::MarkerError) -> &'static str {
    match error {
        modplayer_core::markers::MarkerError::LimitReached => "marker-limit-reached",
        modplayer_core::markers::MarkerError::RegionIncomplete => "loop-region-incomplete",
        modplayer_core::markers::MarkerError::RegionTooShort => "loop-region-too-short",
        modplayer_core::markers::MarkerError::NotFound
        | modplayer_core::markers::MarkerError::NoTrack => "markers-status",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_core::markers::MarkerError;

    #[test]
    fn refusal_key_maps_every_variant() {
        assert_eq!(
            refusal_key(MarkerError::LimitReached),
            "marker-limit-reached"
        );
        assert_eq!(
            refusal_key(MarkerError::RegionIncomplete),
            "loop-region-incomplete"
        );
        assert_eq!(
            refusal_key(MarkerError::RegionTooShort),
            "loop-region-too-short"
        );
    }

    #[test]
    fn format_mmss_millis_frames_matches_seconds_and_millis() {
        assert_eq!(format_mmss_millis_frames(0, 44_100), "0:00.000");
        assert_eq!(format_mmss_millis_frames(44_100, 44_100), "0:01.000");
        assert_eq!(
            format_mmss_millis_frames(44_100 * 65 + 4_410, 44_100),
            "1:05.100"
        );
    }
}

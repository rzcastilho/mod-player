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

use egui::accesskit::{Role, Toggled};
use egui::{
    Color32, EventFilter, Id, Key, Modifiers, Painter, Popup, PopupCloseBehavior, Rect, Response,
    RichText, Sense, SetOpenCommand, Stroke, Ui, pos2, vec2,
};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::markers::{
    CueSlot, LoopRegion, Marker, MarkerId, MarkerKind, PaletteIndex, RegionId, RepeatCount,
    TrackMarkers,
};
use modplayer_core::{LoopState, PlaybackController, tr, tr_args};

use crate::actions::{self, Claim};
use crate::theme;
use crate::theme::controls::Variant;
use crate::waveform::state::DETAIL_MIN_WINDOW_MS;
use crate::waveform::{self, DetailWindow, MarkerDrag, TimeSpace, WaveformState};
use crate::widgets::controls::{
    CardResponse, SwitchKind, button, collapsible_panel_card, destructive_gap, switch,
};

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
    // 023-markers-panel-structure (contract P7; research R8): the
    // per-frame `RenameOnEnter` id set is cleared once, at the start of
    // the overview lane's draw — the first of this frame's three write
    // sites (`lane` x2, then `panel`'s name cells; `now_playing::show`'s
    // own draw order).
    if lane_kind == "overview" {
        reset_rename_on_enter_ids(ui.ctx());
    }
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
    let mark_color = marker_mark_color(ui.visuals());
    // FR-011/FR-012 (017-high-contrast-appearance): `Some` only while high
    // contrast is on — the single selection site (`theme::roles`) is the
    // only place this lane branches on it (S3).
    let roles = theme::roles(ui.visuals());
    let outline = theme::markers::marker_outline(roles);

    for marker in markers.markers() {
        let color = theme::marker_color(marker.color);
        let x = space.x_of(marker.position);
        let focused = waveform.focused_marker == Some(marker.id);
        paint_glyph(
            ui.painter(),
            marker.kind,
            x,
            rect,
            color,
            focused,
            mark_color,
            outline,
        );

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
        // 023-markers-panel-structure (contract P7; research R8): every
        // lane glyph is a control the key table's `Enter` alias may open
        // a rename from — the row's own name cell (`show_name_cell`)
        // writes the same set.
        register_rename_on_enter(ui.ctx(), glyph_id);

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
                waveform::format_mmss_millis(marker.position, sample_rate),
            ),
        ],
    )
}

/// The colour for a marker glyph's focus outline / cue-slot digit
/// (014-design-tokens-and-type-scale, US5, T063/T066, U5): drawn on top of
/// the marker's own (arbitrary palette) fill colour, the same "mark on a
/// colour fill" situation `theme::paint_host_glyph` is in — so this uses
/// the same role, `text_on_accent`, rather than a literal `Color32::WHITE`.
/// A pure `Visuals -> Color32` mapping so
/// `type_roles::marker_labels_use_a_role_colour` can pin it directly.
#[must_use]
pub fn marker_mark_color(visuals: &egui::Visuals) -> Color32 {
    theme::roles(visuals).text_on_accent
}

/// Paint one marker's glyph at `x` (006, contracts/ui-markers.md §3):
/// region `[`/`]` brackets, a point's downward triangle, or a cue's
/// numbered square — 2px-stroked (vs. 1.5px/plain) while `focused`.
/// `outline` is `Some` only in high contrast (O1–O3, contracts/
/// marker-outline.md §2, 017-high-contrast-appearance): each glyph gains
/// a `text_primary` casing/stroke, its own palette `color` never changed
/// (FR-011).
#[allow(clippy::too_many_arguments)]
fn paint_glyph(
    painter: &Painter,
    kind: MarkerKind,
    x: f32,
    rect: Rect,
    color: Color32,
    focused: bool,
    mark_color: Color32,
    outline: Option<Stroke>,
) {
    match kind {
        MarkerKind::RegionStart { .. } => {
            paint_bracket(painter, x, rect, color, true, focused, outline);
        }
        MarkerKind::RegionEnd { .. } => {
            paint_bracket(painter, x, rect, color, false, focused, outline);
        }
        MarkerKind::Point => {
            paint_point_glyph(painter, x, rect, color, focused, mark_color, outline)
        }
        MarkerKind::Cue { slot } => {
            paint_cue_glyph(
                painter,
                x,
                rect,
                color,
                slot.get(),
                focused,
                mark_color,
                outline,
            );
        }
    }
}

/// One `[`/`]` bracket glyph: a vertical stem at `x` spanning `rect`'s
/// full height, with a short tick at top and bottom pointing into the
/// region (`open` = `[`, ticks point right; `open = false` = `]`, ticks
/// point left). O1: in high contrast, the same polyline is cased first at
/// `casing_width(w)` in the outline colour, underneath, so the
/// focused/unfocused width delta (M9) survives identically in the casing.
#[allow(clippy::too_many_arguments)]
fn paint_bracket(
    painter: &Painter,
    x: f32,
    rect: Rect,
    color: Color32,
    open: bool,
    focused: bool,
    outline: Option<Stroke>,
) {
    let width = if focused { 2.0 } else { 1.5 };
    let dx = if open {
        BRACKET_TICK_PX
    } else {
        -BRACKET_TICK_PX
    };
    let segments = [
        [pos2(x, rect.top()), pos2(x, rect.bottom())],
        [pos2(x, rect.top()), pos2(x + dx, rect.top())],
        [pos2(x, rect.bottom()), pos2(x + dx, rect.bottom())],
    ];
    if let Some(outline) = outline {
        let casing = Stroke::new(theme::markers::casing_width(width), outline.color);
        for segment in segments {
            painter.line_segment(segment, casing);
        }
    }
    let stroke = Stroke::new(width, color);
    for segment in segments {
        painter.line_segment(segment, stroke);
    }
}

/// A `Point` marker's glyph (contracts/ui-markers.md §3): a 10px downward
/// triangle, apex pointing into the waveform below. O2: in high contrast
/// the polygon's own stroke becomes the outline colour (contracts/
/// marker-outline.md §2), taking over from the normal-mode focused/
/// unfocused stroke (M9 does not pin this glyph).
fn paint_point_glyph(
    painter: &Painter,
    x: f32,
    rect: Rect,
    color: Color32,
    focused: bool,
    mark_color: Color32,
    outline: Option<Stroke>,
) {
    const HALF_WIDTH: f32 = 5.0;
    const HEIGHT: f32 = 10.0;
    let top = rect.top();
    let points = vec![
        pos2(x - HALF_WIDTH, top),
        pos2(x + HALF_WIDTH, top),
        pos2(x, top + HEIGHT),
    ];
    let stroke = outline.unwrap_or(if focused {
        Stroke::new(1.5, mark_color)
    } else {
        Stroke::NONE
    });
    painter.add(egui::Shape::convex_polygon(points, color, stroke));
}

/// A `Cue { slot }` marker's glyph (contracts/ui-markers.md §3): a 10px
/// square with the slot digit. O3: in high contrast, an extra outside
/// `rect_stroke` in the outline colour — the inside focus stroke and the
/// digit (`text_on_accent`, FR-020) are both unchanged.
#[allow(clippy::too_many_arguments)]
fn paint_cue_glyph(
    painter: &Painter,
    x: f32,
    rect: Rect,
    color: Color32,
    slot: u8,
    focused: bool,
    mark_color: Color32,
    outline: Option<Stroke>,
) {
    const HALF: f32 = 5.0;
    let square = Rect::from_center_size(pos2(x, rect.center().y), vec2(HALF * 2.0, HALF * 2.0));
    painter.rect_filled(square, 1.0, color);
    if let Some(outline) = outline {
        painter.rect_stroke(square, 1.0, outline, egui::StrokeKind::Outside);
    }
    if focused {
        painter.rect_stroke(
            square,
            1.0,
            Stroke::new(1.5, mark_color),
            egui::StrokeKind::Inside,
        );
    }
    painter.text(
        square.center(),
        egui::Align2::CENTER_CENTER,
        slot.to_string(),
        theme::mono_font_id(),
        mark_color,
    );
}

/// The focused-marker keyboard table (006, contracts/ui-markers.md §3,
/// minus the four nudge arrows — now `HostAction::NudgeEarlier`/`Later`/
/// `…X10` via the 007 dispatcher, `Scope::MarkerFocused`): active only
/// while a glyph/row has focus (`waveform.focused_marker`), no text field
/// of this view has focus, and no rename is already open (the row's own
/// inline `TextEdit` handles its own Enter/Esc — `show_populated_row`,
/// above). Call once per frame, after the panel (so a fresh `F2`/`Enter`
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

    // 023-markers-panel-structure (contract P7; research R8): the `Enter`
    // -> rename alias fires only when the currently egui-focused widget
    // is a lane glyph or a row's name cell (this frame's `RenameOnEnter`
    // set). On the swatch or an action button, `Enter`/`Space` must only
    // activate that control (egui's own default) — never also open a
    // rename on a marker an action just removed, or stack a popover open
    // with a rename. `F2`/`Delete`/`Backspace`/`C`/`Esc` are unaffected:
    // they keep working from any row control, so only `Enter` is gated
    // here (never consumed when disallowed, since nothing else this
    // frame reads it once `handle_focused_marker_keys` returns).
    let focused_widget = ui.memory(|memory| memory.focused());
    let enter_allowed = focused_widget.is_some_and(|widget| is_rename_on_enter(ui.ctx(), widget));
    if !enter_allowed
        && ui.input(|input| input.key_pressed(Key::Enter))
        && !ui.input(|input| input.key_pressed(Key::F2))
    {
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

/// A loop region's shading treatment (022-waveform-legibility, FR-004/
/// FR-005, data-model.md §7), driven only by the region's own `armed`
/// flag and the engine's `loop_state` — never by whether the region is
/// [`TrackMarkers::current_region`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopShade {
    /// Armed and the playhead is inside `[A, B)` (`loop_state == 2`):
    /// solid translucent fill.
    ArmedActive,
    /// Armed but the playhead has not (yet) entered the region
    /// (`loop_state` 0 or 1): diagonal hatch. Also covers a fresh arm
    /// before the engine's ack lands, avoiding a one-frame flicker to
    /// [`LoopShade::Idle`] (plan.md Complexity Tracking).
    ArmedInactive,
    /// Not armed, regardless of `loop_state`: 1px outline + low-alpha
    /// fill.
    Idle,
}

/// The shading [`LoopShade`] a region with `armed` gets, given the
/// engine's current `loop_state` (022-waveform-legibility, data-model.md
/// §7's truth table):
///
/// | `armed` | `loop_state` | Result |
/// |---|---|---|
/// | `true` | `2` | [`LoopShade::ArmedActive`] |
/// | `true` | `0`, `1` | [`LoopShade::ArmedInactive`] |
/// | `false` | any | [`LoopShade::Idle`] |
///
/// ```
/// use modplayer_ui::markers::{LoopShade, loop_shade};
///
/// assert_eq!(loop_shade(true, 2), LoopShade::ArmedActive);
/// assert_eq!(loop_shade(true, 0), LoopShade::ArmedInactive);
/// assert_eq!(loop_shade(true, 1), LoopShade::ArmedInactive);
/// assert_eq!(loop_shade(false, 2), LoopShade::Idle);
/// ```
#[must_use]
pub fn loop_shade(armed: bool, loop_state: u8) -> LoopShade {
    if !armed {
        return LoopShade::Idle;
    }
    if loop_state == 2 {
        LoopShade::ArmedActive
    } else {
        LoopShade::ArmedInactive
    }
}

/// Paint every marker's line plus every complete loop region's `[A, B)`
/// span through the `overlays` hook (006, research R16,
/// 022-waveform-legibility WL4, contracts/ui-markers.md §1): a 1px (2px
/// while `focused`) vertical line per marker in its palette colour, then
/// every region with a complete span — idle (1px outline + low-alpha
/// fill), hatched (armed-inactive), or solid translucent (armed-active)
/// per its own `armed` flag via [`loop_shade`] (data-model.md §7);
/// `current_region()` is never consulted for shading. A no-op with no
/// markers. `roles` is the applied style's role table
/// (017-high-contrast-appearance, FR-011/FR-012): `theme::markers::
/// marker_outline(roles)` is `Some` only in high contrast, and every
/// palette fill/translucent-fill colour above stays byte-identical
/// either way (M8) — only an extra casing/`rect_stroke` is added.
pub fn paint_overlay(
    painter: &Painter,
    space: &TimeSpace,
    markers: Option<&TrackMarkers>,
    loop_state: u8,
    focused: Option<MarkerId>,
    roles: &theme::Roles,
) {
    let Some(markers) = markers else {
        return;
    };
    let rect = space.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let outline = theme::markers::marker_outline(roles);

    for marker in markers.markers() {
        let color = theme::marker_color(marker.color);
        let x = space.x_of(marker.position);
        let width = if focused == Some(marker.id) { 2.0 } else { 1.0 };
        // O4: cased first, underneath, at `casing_width(width)` — the
        // focused/unfocused delta survives identically (M9).
        if let Some(outline) = outline {
            painter.line_segment(
                [pos2(x, rect.top()), pos2(x, rect.bottom())],
                Stroke::new(theme::markers::casing_width(width), outline.color),
            );
        }
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(width, color),
        );
        if marker.clamped {
            paint_clamped_warning(painter, x, rect.top(), color, outline);
        }
    }

    // Every complete-span region, shaded by its own `armed` flag via
    // `loop_shade` — `current_region()` is never read for shading
    // (FR-004/FR-005/FR-018, data-model.md §7). Idle regions paint first,
    // in `regions()` order; the armed region (at most one, 006 I6), if
    // any, paints last so it always sits on top of an idle region it
    // overlaps.
    let mut armed_entry = None;
    for region in markers.regions() {
        let Some((a, b)) = region.span(markers) else {
            continue;
        };
        let x0 = space.x_of(a);
        let x1 = space.x_of(b).max(x0 + 1.0);
        let span_rect = Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom()));
        let color = region
            .a
            .and_then(|id| markers.marker(id))
            .map(|marker| theme::marker_color(marker.color))
            .unwrap_or_else(|| theme::marker_color(PaletteIndex::new(0)));
        match loop_shade(region.armed, loop_state) {
            LoopShade::Idle => {
                paint_region_shade(painter, span_rect, color, LoopShade::Idle, outline);
            }
            shade => armed_entry = Some((span_rect, color, shade)),
        }
    }
    if let Some((span_rect, color, shade)) = armed_entry {
        paint_region_shade(painter, span_rect, color, shade, outline);
    }
}

/// One loop region's span rect, painted per its [`LoopShade`] (WL4):
/// `ArmedActive` — solid translucent fill; `ArmedInactive` — diagonal
/// hatch (006, unchanged); `Idle` — low-alpha fill + 1px outline (FR-005).
/// `outline` (`Some` only in high contrast) adds an extra `marker_outline`
/// stroke to `ArmedActive` and `Idle` (O5) — the palette fill/hatch stroke
/// itself never changes (M8).
fn paint_region_shade(
    painter: &Painter,
    span_rect: Rect,
    color: Color32,
    shade: LoopShade,
    outline: Option<Stroke>,
) {
    match shade {
        LoopShade::ArmedActive => {
            painter.rect_filled(
                span_rect,
                0.0,
                color.gamma_multiply(theme::markers::LOOP_ARMED_FILL_ALPHA),
            );
            if let Some(outline) = outline {
                painter.rect_stroke(span_rect, 0.0, outline, egui::StrokeKind::Inside);
            }
        }
        LoopShade::ArmedInactive => paint_hatched(painter, span_rect, color),
        LoopShade::Idle => {
            painter.rect_filled(
                span_rect,
                0.0,
                color.gamma_multiply(theme::markers::LOOP_IDLE_FILL_ALPHA),
            );
            painter.rect_stroke(
                span_rect,
                0.0,
                Stroke::new(theme::markers::LOOP_OUTLINE_WIDTH, color),
                egui::StrokeKind::Inside,
            );
            if let Some(outline) = outline {
                painter.rect_stroke(span_rect, 0.0, outline, egui::StrokeKind::Inside);
            }
        }
    }
}

/// The small warning triangle drawn at the top of a `clamped` marker's
/// overlay line (006, FR-018, contracts/ui-markers.md §1): a filled
/// triangle pointing down into the line, apex at `(x, top)`. O6: in high
/// contrast, `outline` becomes the polygon's own stroke — the fill
/// colour is unchanged (M8).
fn paint_clamped_warning(
    painter: &Painter,
    x: f32,
    top: f32,
    color: Color32,
    outline: Option<Stroke>,
) {
    const HALF_WIDTH: f32 = 4.0;
    const HEIGHT: f32 = 6.0;
    let points = vec![
        pos2(x, top),
        pos2(x - HALF_WIDTH, top + HEIGHT),
        pos2(x + HALF_WIDTH, top + HEIGHT),
    ];
    painter.add(egui::Shape::convex_polygon(
        points,
        color,
        outline.unwrap_or(Stroke::NONE),
    ));
}

/// A simple diagonal-line hatch (armed-inactive, data-model.md §3) —
/// spaced [`theme::markers::LOOP_HATCH_SPACING`] apart, clipped to `rect`.
fn paint_hatched(painter: &Painter, rect: Rect, color: egui::Color32) {
    let stroke = Stroke::new(1.0, color.gamma_multiply(theme::markers::LOOP_HATCH_ALPHA));
    let step = theme::markers::LOOP_HATCH_SPACING;
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

/// The Markers panel (006, contracts/ui-markers.md §4; 021-transport-bar-
/// and-panel-layout, contracts/ui-now-playing-layout.md C1-C2, C4;
/// 023-markers-panel-structure, contracts/ui-markers-panel.md P1): the
/// card header ("Markers"), the status line, then either the empty layout
/// (`markers-empty` + "New loop region") or the three groups — Loop
/// Region, Points, Cues, each with its own heading and count — followed
/// by the "Clear all markers" footer. Built once per frame from a pure
/// [`PanelModel::snapshot`] (research R1) so the grouping/ordering/count
/// rules are unit-tested without an egui context. Every `DragValue`/
/// `TextEdit` registers a `Claim::TextLike` (007, contracts/ui-actions.md
/// §2) so the dispatcher leaves it alone while it has focus. Rendered
/// only while a track is loaded (021 contract C4); the card is
/// collapsible through `*open`, but — unlike Effect Chain/Transport/Queue
/// — Markers has no bar toggle (spec Clarification 5), so the caller only
/// ever reads `response.toggled` off the header disclosure.
pub fn panel<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
    open: &mut bool,
) -> CardResponse {
    // 016-list-row-and-panel-components (FR-017/FR-018/FR-021, research
    // R7), superseded by 021 contract C1/C2 (research R7): the shared
    // collapsible card now draws the `markers-panel` header plus a header
    // disclosure — the in-row `section_label` this panel used to draw
    // itself stays deleted, so it is never rendered twice.
    collapsible_panel_card(ui, &tr("markers-panel"), open, |ui| {
        // FR-021 (023-markers-panel-structure, contract P1): the status
        // line sits directly under the card header, above every group.
        if let Some(key) = waveform.marker_status {
            ui.label(tr(key));
        }

        let rate = controller.source_sample_rate().max(1);
        let model = PanelModel::snapshot(controller.markers(), rate);
        // T033 (research R11): resolves any pending post-`Remove` focus
        // target against *this* frame's fresh model before anything
        // renders, so the target's own control can `request_focus()`
        // while it draws below.
        let pending_focus = resolve_pending_panel_focus(waveform, &model);

        if model.is_empty() {
            show_empty(ui, controller, pending_focus);
            return;
        }

        let mut intents: Vec<PanelIntent> = Vec::new();
        show_loop_group(
            ui,
            controller,
            waveform,
            &model,
            rate,
            pending_focus,
            &mut intents,
        );
        show_points_group(ui, waveform, &model, rate, pending_focus, &mut intents);
        show_cues_group(ui, waveform, &model, rate, pending_focus, &mut intents);
        show_footer(ui, controller, waveform);

        // research R2: every row/popover/rename interaction above only
        // pushed a `PanelIntent` — this is the one place `&mut
        // controller`/`&mut waveform` are mutated from user input, after
        // every group (and the footer) has already rendered against the
        // same, self-consistent snapshot.
        apply_intents(&mut intents, ui.ctx(), controller, waveform);
    })
}

/// The empty layout (contract P1 "Empty"; FR-020, SC-007): `markers-empty`
/// followed by "New loop region" only — no headings, cue rows or Clear
/// all markers.
fn show_empty<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    pending_focus: Option<waveform::PanelFocus>,
) {
    ui.label(tr("markers-empty"));
    let response = button(ui, Variant::Default, tr("markers-new-loop"));
    if pending_focus == Some(waveform::PanelFocus::NewLoopRegion) {
        response.request_focus();
    }
    if response.clicked() {
        let _ = controller.new_loop_region();
    }
}

/// T033 (research R11): resolves `waveform.panel_focus` — a one-shot
/// target set by a `Remove` intent applied on a *previous* frame — into
/// the concrete target this frame's controls should `request_focus()`
/// against: the recorded row if it still exists in `model`, else the
/// model's own first populated row, else always `NewLoopRegion`.
/// Consumes (clears) `waveform.panel_focus`, mirroring `settings/
/// category_row.rs`'s own one-shot `focus_after_close`.
fn resolve_pending_panel_focus(
    waveform: &mut WaveformState,
    model: &PanelModel,
) -> Option<waveform::PanelFocus> {
    let focus = waveform.panel_focus.take()?;
    let target = match focus {
        waveform::PanelFocus::Row(id) if model.visual_order().contains(&id) => {
            waveform::PanelFocus::Row(id)
        }
        waveform::PanelFocus::Row(_) => model
            .visual_order()
            .first()
            .copied()
            .map(waveform::PanelFocus::Row)
            .unwrap_or(waveform::PanelFocus::NewLoopRegion),
        waveform::PanelFocus::NewLoopRegion => waveform::PanelFocus::NewLoopRegion,
    };
    Some(target)
}

/// A group heading (contract P2; research R4): the group's uppercase
/// `section` label plus its `mono`, `text_secondary` count, and a
/// non-interactive `Role::Heading` node labelled
/// `markers-group-heading { $label } { $count }`.
fn show_group_heading(ui: &mut Ui, key: &'static str, count: usize) {
    let roles = theme::roles(ui.visuals());
    let label = tr_args(key, &[("count", count.to_string())]);
    let outer = ui.horizontal(|ui| {
        ui.label(theme::section_label(&label));
        ui.label(theme::mono_text(count.to_string()).color(roles.text_secondary));
    });
    let heading_id = ui.id().with(("markers-group-heading", key));
    let rect = outer.response.rect;
    let response = ui.interact(rect, heading_id, Sense::hover());
    let access_label = tr_args(
        "markers-group-heading",
        &[("label", label), ("count", count.to_string())],
    );
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_role(Role::Heading);
        b.set_label(access_label);
        // `Sense::hover()` is never focusable, so egui's own automatic
        // bounds-fill (gated on focusability, `Context::create_widget`)
        // never runs for this node — set it explicitly from the same rect
        // `interact` was given, as `Response::fill_accesskit_node_common`
        // does for a real widget.
        b.set_bounds(egui::accesskit::Rect {
            x0: rect.min.x.into(),
            y0: rect.min.y.into(),
            x1: rect.max.x.into(),
            y1: rect.max.y.into(),
        });
    });
}

/// The Loop Region group (contract P1/P2/P3-cells; FR-001-FR-004;
/// research R3, R4, R13): heading, "New loop region"
/// (`Variant::Default`, the group's first line), then one block per
/// [`LoopBlock`] — its `A` row (if present), `B` row (if present), then
/// its loop cells after the last present boundary — `space::SM` between
/// blocks. Always rendered once the panel is populated (FR-004), even at
/// count 0.
#[allow(clippy::too_many_arguments)]
fn show_loop_group<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
    model: &PanelModel,
    rate: u32,
    pending_focus: Option<waveform::PanelFocus>,
    intents: &mut Vec<PanelIntent>,
) {
    show_group_heading(ui, "markers-group-loop", model.loop_count());
    ui.add_space(theme::space::XS);
    let new_loop = button(ui, Variant::Default, tr("markers-new-loop"));
    if pending_focus == Some(waveform::PanelFocus::NewLoopRegion) {
        new_loop.request_focus();
    }
    if new_loop.clicked() {
        let _ = controller.new_loop_region();
    }

    for (index, block) in model.loop_blocks.iter().enumerate() {
        if index > 0 {
            ui.add_space(theme::space::SM);
        }
        if let Some(a) = &block.a {
            show_populated_row(ui, waveform, a, rate, pending_focus, intents);
        }
        if let Some(b) = &block.b {
            show_populated_row(ui, waveform, b, rate, pending_focus, intents);
        }
        show_region_cells(ui, controller, block.region, &block.cells);
    }
}

/// The Points group (contract P1/P2; FR-001, FR-005): heading then every
/// point row by position. Rendered only when non-empty (FR-001).
fn show_points_group(
    ui: &mut Ui,
    waveform: &mut WaveformState,
    model: &PanelModel,
    rate: u32,
    pending_focus: Option<waveform::PanelFocus>,
    intents: &mut Vec<PanelIntent>,
) {
    if !model.show_points_group() {
        return;
    }
    ui.add_space(theme::space::MD);
    show_group_heading(ui, "markers-group-points", model.point_count());
    ui.add_space(theme::space::XS);
    for row in &model.points {
        show_populated_row(ui, waveform, row, rate, pending_focus, intents);
    }
}

/// The Cues group (contract P1/P2/P8; FR-002, FR-016; research R12):
/// heading (count = occupied slots) then all 8 slots in order — an
/// occupied slot as a full row, an empty slot as the muted,
/// non-interactive `markers-cue-empty { $slot }` row. Always rendered
/// once the panel is populated (FR-016).
fn show_cues_group(
    ui: &mut Ui,
    waveform: &mut WaveformState,
    model: &PanelModel,
    rate: u32,
    pending_focus: Option<waveform::PanelFocus>,
    intents: &mut Vec<PanelIntent>,
) {
    ui.add_space(theme::space::MD);
    show_group_heading(ui, "markers-group-cues", model.cue_count());
    ui.add_space(theme::space::XS);
    for slot in &model.cues {
        match slot {
            CueSlotRow::Occupied(row) => {
                show_populated_row(ui, waveform, row, rate, pending_focus, intents);
            }
            CueSlotRow::Empty(slot) => show_empty_cue_row(ui, *slot),
        }
    }
}

/// An empty cue slot's muted, non-interactive placeholder row (contract
/// P8; FR-015/FR-016; research R12): `markers-cue-empty { $slot }` in
/// `text_secondary`, no swatch, name field, actions, hover fill or tab
/// stop — a static-text AccessKit node, no `markers-row` `ListItem`
/// container.
fn show_empty_cue_row(ui: &mut Ui, slot: CueSlot) {
    let roles = theme::roles(ui.visuals());
    ui.label(
        RichText::new(tr_args(
            "markers-cue-empty",
            &[("slot", slot.get().to_string())],
        ))
        .color(roles.text_secondary),
    );
}

/// One populated row (contract P3, FR-007-FR-014; research R5-R9): swatch
/// → palette popover (P4), role label, name cell → inline rename (P5),
/// then the trailing column — position, clamped ⚠, jump, nudge earlier,
/// nudge later, remove (P6) — built left to right in a pre-allocated
/// rect so creation order is both the visual order and the Tab order
/// (Clarification 6, research R5). Every interaction below pushes a
/// [`PanelIntent`] into `intents`; nothing here calls `&mut controller`
/// directly (research R2) — `panel`'s own [`apply_intents`] applies every
/// intent once, after every group has rendered against this same,
/// self-consistent [`PanelModel`] snapshot. Shared by the Loop Region,
/// Points and Cues groups.
fn show_populated_row(
    ui: &mut Ui,
    waveform: &mut WaveformState,
    row: &RowData,
    rate: u32,
    pending_focus: Option<waveform::PanelFocus>,
    intents: &mut Vec<PanelIntent>,
) {
    let row_name = row_accessible_name(row, rate);
    let row_id = ui.id().with(("markers-row", row.id));
    // Contract P3: the taller of the swatch's own fixed size and the
    // style's interactive height, floored at the swatch (research R6
    // sizes every Quiet action to the row's own height, as `settings/
    // category_row.rs` does for its Quiet "…" opener).
    let row_height = ui.spacing().interact_size.y.max(16.0);

    // FR-009, contract I6: reserve the row's hover fill's paint order
    // before content draws (research R5, FR-018) — this row already
    // computes its own rect+response below (`row_response`, pre-dating
    // this feature), reused as-is rather than re-allocated through
    // `widgets::controls::row_frame` (design note 7).
    let where_to_put_background = ui.painter().add(egui::Shape::Noop);

    let outer = ui.horizontal(|ui| {
        show_swatch(ui, waveform, row, pending_focus, intents);
        ui.label(role_label(row.kind));
        show_name_cell(ui, waveform, row, intents);

        // research R5: a pre-allocated rect plus a `left_to_right` child
        // `Ui`, so the trailing cells' creation order stays left to
        // right (matching Tab order) while `main_align: Max` packs them
        // against the row's own right edge whatever space `available`
        // leaves after the name cell.
        let available = vec2(ui.available_width(), row_height);
        ui.allocate_ui_with_layout(
            available,
            egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Max),
            |ui| {
                show_trailing_cells(ui, waveform, row, rate, intents);
            },
        );
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
        // `Sense::hover()` is never focusable, so egui's own automatic
        // bounds-fill never runs for this node (023-markers-panel-
        // structure, T011: block ordering is asserted from this bounds
        // value) — set it explicitly from the row's own rect.
        b.set_bounds(egui::accesskit::Rect {
            x0: outer.response.rect.min.x.into(),
            y0: outer.response.rect.min.y.into(),
            x1: outer.response.rect.max.x.into(),
            y1: outer.response.rect.max.y.into(),
        });
    });

    if ui.is_rect_visible(outer.response.rect) {
        let roles = theme::roles(ui.visuals());
        let fill = if row_response.is_pointer_button_down_on() {
            Some(
                roles
                    .surface_base
                    .blend(theme::controls::pressed_fill(roles)),
            )
        } else if row_response.hovered() {
            Some(roles.surface_base.blend(theme::controls::hover_fill(roles)))
        } else {
            None
        };
        if let Some(fill) = fill {
            ui.painter().set(
                where_to_put_background,
                egui::Shape::rect_filled(outer.response.rect, egui::CornerRadius::ZERO, fill),
            );
        }
    }
}

/// Cell 1 (contract P3): the 16×16 colour swatch — `Role::Button`,
/// label `markers-color { $index }` (1-based; the raw, 0-based
/// `PaletteIndex` fed the fix's own regression, research R7) — that
/// opens the [`show_palette_popup`] recolour popover on click/Enter/
/// Space. Registers this row's focus claim/lock and pushes
/// [`PanelIntent::Focus`] on gaining focus (research R8); applies
/// `pending_focus` (research R11).
fn show_swatch(
    ui: &mut Ui,
    waveform: &WaveformState,
    row: &RowData,
    pending_focus: Option<waveform::PanelFocus>,
    intents: &mut Vec<PanelIntent>,
) {
    let color = theme::marker_color(row.color);
    let a11y = tr_args(
        "markers-color",
        &[("index", (row.color.get() + 1).to_string())],
    );
    let response = ui
        .add(egui::Button::new("").fill(color).min_size(vec2(16.0, 16.0)))
        .on_hover_text(a11y.clone());
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_role(Role::Button);
        b.set_label(a11y.clone());
    });
    register_row_focus_claim(ui, &response);
    // T033 (research R11): `request_focus()` before the `has_focus()`
    // check below, so a pending post-`Remove` target lands on this very
    // frame — both egui's own focus memory and `PanelIntent::Focus` are
    // driven from the *same* live `response`, and `has_focus()` reads
    // `ctx` memory live rather than a value cached on `response`.
    if pending_focus == Some(waveform::PanelFocus::Row(row.id)) {
        response.request_focus();
    }
    if response.has_focus() && waveform.focused_marker != Some(row.id) {
        intents.push(PanelIntent::Focus(row.id));
    }
    show_palette_popup(ui, &response, row, intents);
}

/// Cell 3 (contract P3/P5; FR-012): a Quiet button showing the marker's
/// name, or `markers-name-placeholder` when empty — click/activation
/// pushes [`PanelIntent::OpenRename`] — or, while `waveform.rename`
/// targets this row, the inline `TextEdit` itself: `request_focus()`
/// only while `rename_focus_pending` (research R9, so click-away can
/// commit at all), `Esc` checked before focus loss so it cancels rather
/// than commits, and every other path (Enter or focus loss) commits.
/// Both the placeholder button and the `TextEdit` write this row's id
/// into the per-frame `RenameOnEnter` set (research R8) — the lane
/// glyph's own analogous write is in [`lane`].
fn show_name_cell(
    ui: &mut Ui,
    waveform: &mut WaveformState,
    row: &RowData,
    intents: &mut Vec<PanelIntent>,
) {
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
        actions::register_claim(ui.ctx(), response.id, Claim::TextLike);
        register_rename_on_enter(ui.ctx(), response.id);
        if let Some((_, name)) = waveform.rename.as_mut() {
            *name = draft.clone();
        }
        if waveform.rename_focus_pending {
            response.request_focus();
            waveform.rename_focus_pending = false;
            // research R9: the draft starts as the current name (never
            // empty for, e.g., a point marker's own auto-assigned
            // "Marker N"), so the whole draft is selected here — the
            // conventional click-to-rename affordance — rather than
            // leaving egui's own default end-of-text cursor, which would
            // make the first keystroke append instead of replace.
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                let end = egui::text::CCursor::new(draft.chars().count());
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        end,
                    )));
                egui::TextEdit::store_state(ui.ctx(), response.id, state);
            }
        }
        // research R9: `Esc` is checked first, because egui's `TextEdit`
        // also surrenders focus on `Esc` — checking `lost_focus()` first
        // would otherwise treat that same `Esc` as a commit.
        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            intents.push(PanelIntent::CancelRename);
        } else if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
            || response.lost_focus()
        {
            intents.push(PanelIntent::CommitRename(row.id, draft));
        }
    } else {
        let label = if row.name.is_empty() {
            tr("markers-name-placeholder")
        } else {
            row.name.clone()
        };
        let response = button(ui, Variant::Quiet, label.clone());
        ui.ctx().accesskit_node_builder(response.id, |b| {
            b.set_role(Role::Button);
            b.set_label(label.clone());
        });
        register_row_focus_claim(ui, &response);
        register_rename_on_enter(ui.ctx(), response.id);
        if response.has_focus() && waveform.focused_marker != Some(row.id) {
            intents.push(PanelIntent::Focus(row.id));
        }
        if response.clicked() {
            intents.push(PanelIntent::OpenRename(row.id));
        }
    }
}

/// Cells 4–9 (contract P3): the live-drag-aware `mono` position, the
/// clamped ⚠ (only when `row.clamped`), then the four always-visible
/// Quiet row actions — jump, nudge earlier, nudge later, remove — in
/// that fixed creation order (research R5).
fn show_trailing_cells(
    ui: &mut Ui,
    waveform: &WaveformState,
    row: &RowData,
    rate: u32,
    intents: &mut Vec<PanelIntent>,
) {
    let live_position = waveform
        .marker_drag
        .filter(|drag| drag.marker == row.id)
        .map_or(row.position, |drag| drag.live);
    ui.label(theme::mono_text(waveform::format_mmss_millis(
        live_position,
        rate,
    )));

    if row.clamped {
        ui.label(RichText::new("⚠").color(ui.visuals().warn_fg_color))
            .on_hover_text(tr("marker-clamped-desc"));
    }

    show_row_action(
        ui,
        waveform,
        row,
        intents,
        theme::markers::ROW_ACTION_JUMP_GLYPH,
        "markers-jump",
        PanelIntent::Jump(row.id),
    );
    show_row_action(
        ui,
        waveform,
        row,
        intents,
        theme::markers::ROW_ACTION_NUDGE_EARLIER_GLYPH,
        "markers-nudge-earlier",
        PanelIntent::Nudge(row.id, -1),
    );
    show_row_action(
        ui,
        waveform,
        row,
        intents,
        theme::markers::ROW_ACTION_NUDGE_LATER_GLYPH,
        "markers-nudge-later",
        PanelIntent::Nudge(row.id, 1),
    );
    show_row_action(
        ui,
        waveform,
        row,
        intents,
        theme::markers::ROW_ACTION_REMOVE_GLYPH,
        "markers-remove",
        PanelIntent::Remove(row.id),
    );
}

/// One Quiet row-action button (contract P3 cells 6–9; FR-008–FR-011):
/// `.on_hover_text` and the AccessKit label are both `tr(key)` — the
/// accessible name is the action's words, not its glyph (research R6).
/// Registers this row's focus claim/lock and pushes
/// [`PanelIntent::Focus`] on gaining focus (research R8), exactly like
/// the swatch and name cell, so the focused-marker key table works from
/// any row control (Clarification 6). Never registered as a
/// `RenameOnEnter` id: `Enter` here only activates the button itself
/// (contract P7).
fn show_row_action(
    ui: &mut Ui,
    waveform: &WaveformState,
    row: &RowData,
    intents: &mut Vec<PanelIntent>,
    glyph: &str,
    key: &'static str,
    intent: PanelIntent,
) {
    let label = tr(key);
    let response = button(ui, Variant::Quiet, glyph).on_hover_text(label.clone());
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_role(Role::Button);
        b.set_label(label.clone());
    });
    register_row_focus_claim(ui, &response);
    if response.has_focus() && waveform.focused_marker != Some(row.id) {
        intents.push(PanelIntent::Focus(row.id));
    }
    if response.clicked() {
        intents.push(intent);
    }
}

/// Registers `response`'s widget-local key claim (`marker_claims`) and
/// horizontal-arrow focus lock (research R8) — the same registration
/// every lane glyph already makes in [`lane`], now shared by every
/// focusable row control (swatch, name cell, jump, nudge−, nudge+,
/// remove) so the 007 dispatcher leaves Delete/Backspace/F2/Enter/C/Esc
/// to the focused-marker key table, and a nudge arrow's own
/// `HostAction::NudgeEarlier`/`Later` (`Scope::MarkerFocused`) never also
/// moves egui's own focus off the control.
fn register_row_focus_claim(ui: &mut Ui, response: &Response) {
    actions::register_claim(ui.ctx(), response.id, Claim::Keys(actions::marker_claims()));
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            response.id,
            EventFilter {
                horizontal_arrows: true,
                ..EventFilter::default()
            },
        );
    });
}

/// The [`show_palette_popup`] recolour popover's own fixed id, keyed by
/// `marker` — never derived from the swatch button's own auto id
/// (research R7).
fn palette_popup_id(marker: MarkerId) -> Id {
    Id::new(("markers-palette", marker))
}

/// The swatch's recolour popover (contract P4; FR-013; research R7): an
/// `egui::Popup` below `swatch`, `CloseOnClickOutside`, `Role::
/// RadioGroup` labelled `markers-palette`, holding 8 `Role::RadioButton`
/// swatches (`markers-color { $index }`, 1-based) — the current one
/// marked by a `text_primary` outline plus
/// [`theme::markers::PALETTE_CURRENT_GLYPH`] in [`marker_mark_color`]
/// (a non-colour indicator, NFR-6.4). Picking one pushes
/// [`PanelIntent::Recolor`] and closes the popup; opening by keyboard
/// (Enter/Space on `swatch`) focuses the *current* swatch, and ←/→ move
/// among them (Tab already does, by egui's own default focus order).
/// `Esc`/click-outside close with no change; either way, once the popup
/// was open and is no longer, this consumes any pending `Esc` itself, so
/// the focused-marker key table never also treats it as `ReturnFocus`
/// (research R7).
fn show_palette_popup(
    ui: &mut Ui,
    swatch: &Response,
    row: &RowData,
    intents: &mut Vec<PanelIntent>,
) {
    let popup_id = palette_popup_id(row.id);
    let was_open = Popup::is_id_open(ui.ctx(), popup_id);
    let opened_by_keyboard = !was_open
        && swatch.has_focus()
        && ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space));
    let command = swatch.clicked().then_some(SetOpenCommand::Toggle);
    let roles = theme::roles(ui.visuals());

    let popup_response = Popup::new(popup_id, ui.ctx().clone(), swatch, swatch.layer_id)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .open_memory(command)
        .show(|ui| {
            let originally_focused = ui.memory(|m| m.focused());
            let arrow_next = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight));
            let arrow_prev = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft));
            let mut pending_forward_focus = false;
            let mut previous: Option<Response> = None;

            ui.horizontal(|ui| {
                for raw in 0u8..8 {
                    let index = PaletteIndex::new(raw);
                    let is_current = index == row.color;
                    let stroke = if is_current {
                        Stroke::new(1.0, roles.text_primary)
                    } else {
                        Stroke::NONE
                    };
                    let item = ui.add(
                        egui::Button::new("")
                            .fill(theme::marker_color(index))
                            .stroke(stroke)
                            .min_size(vec2(16.0, 16.0)),
                    );
                    if is_current {
                        ui.painter().text(
                            item.rect.center(),
                            egui::Align2::CENTER_CENTER,
                            theme::markers::PALETTE_CURRENT_GLYPH,
                            theme::mono_font_id(),
                            marker_mark_color(ui.visuals()),
                        );
                    }
                    let label = tr_args("markers-color", &[("index", (raw + 1).to_string())]);
                    ui.ctx().accesskit_node_builder(item.id, |b| {
                        b.set_role(Role::RadioButton);
                        b.set_toggled(if is_current {
                            Toggled::True
                        } else {
                            Toggled::False
                        });
                        b.set_label(label.clone());
                    });
                    // research R7 (mirrors `settings/category_row.rs`'s
                    // own menu items): ←/→ move focus *between swatches*,
                    // not egui's own built-in spatial navigation, which
                    // would otherwise jump to whatever widget happens to
                    // sit beside this popup on screen.
                    ui.memory_mut(|memory| {
                        memory.set_focus_lock_filter(
                            item.id,
                            EventFilter {
                                horizontal_arrows: true,
                                ..EventFilter::default()
                            },
                        );
                    });

                    if opened_by_keyboard && is_current {
                        item.request_focus();
                    }
                    if pending_forward_focus {
                        item.request_focus();
                        pending_forward_focus = false;
                    }
                    if Some(item.id) == originally_focused {
                        if arrow_next {
                            pending_forward_focus = true;
                        } else if arrow_prev && let Some(prev) = &previous {
                            prev.request_focus();
                        }
                    }

                    if item.clicked() {
                        intents.push(PanelIntent::Recolor(row.id, index));
                        Popup::close_id(ui.ctx(), popup_id);
                    }
                    previous = Some(item);
                }
            });
        });

    if let Some(inner) = popup_response {
        ui.ctx().accesskit_node_builder(inner.response.id, |b| {
            b.set_role(Role::RadioGroup);
            b.set_label(tr("markers-palette"));
        });
    }

    let is_open_now = Popup::is_id_open(ui.ctx(), popup_id);
    if was_open && !is_open_now {
        // research R7: the popup may have just closed itself on `Esc`
        // (its own built-in handling only *peeks* at the key, never
        // consumes it) — consumed here so the focused-marker key table
        // never also treats this same `Esc` as `ReturnFocus`. A
        // click-outside or our own pick-triggered close leaves no `Esc`
        // event to consume, so this is a harmless no-op either way.
        ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Escape);
        });
    }
}

/// The Markers panel row's own accessible name (contracts/ui-markers.md
/// §4's `markers-row` container): the same `marker-glyph { role, name,
/// time }` template the lane glyph uses (`glyph_accessible_name`), built
/// from the row's own snapshot fields instead of a `&Marker` since
/// `panel` already destructured rows into [`RowData`] before this point.
fn row_accessible_name(row: &RowData, sample_rate: u32) -> String {
    tr_args(
        "marker-glyph",
        &[
            ("role", role_label(row.kind)),
            ("name", row.name.clone()),
            (
                "time",
                waveform::format_mmss_millis(row.position, sample_rate),
            ),
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

// ---------------------------------------------------------------------
// 023-markers-panel-structure — the `RenameOnEnter` id set (data-model.md
// §6; research R8): which egui-focused widget the key table's `Enter`
// alias may open a rename from. Stored in `egui` temp memory, cleared
// once per frame (`reset_rename_on_enter_ids`, called from `lane`'s
// overview pass) and written by every lane glyph (`lane`) and every
// row's name-cell control (`show_name_cell`) as they draw; read by
// `handle_focused_marker_keys`.
// ---------------------------------------------------------------------

/// The fixed id the per-frame `RenameOnEnter` set lives under.
fn rename_on_enter_memory_id() -> Id {
    Id::new("modplayer-ui::markers::rename-on-enter")
}

/// Clears this frame's `RenameOnEnter` set.
fn reset_rename_on_enter_ids(ctx: &egui::Context) {
    ctx.memory_mut(|memory| {
        memory.data.insert_temp(
            rename_on_enter_memory_id(),
            std::collections::HashSet::<Id>::new(),
        );
    });
}

/// Registers `id` as a control the key table's `Enter` alias may open a
/// rename from this frame.
fn register_rename_on_enter(ctx: &egui::Context, id: Id) {
    ctx.memory_mut(|memory| {
        memory
            .data
            .get_temp_mut_or_default::<std::collections::HashSet<Id>>(rename_on_enter_memory_id())
            .insert(id);
    });
}

/// Whether `id` was registered into this frame's `RenameOnEnter` set.
fn is_rename_on_enter(ctx: &egui::Context, id: Id) -> bool {
    ctx.memory(|memory| {
        memory
            .data
            .get_temp::<std::collections::HashSet<Id>>(rename_on_enter_memory_id())
    })
    .is_some_and(|set| set.contains(&id))
}

/// `region`'s loop cells (arm toggle, repeat, crossfade, wraps-remaining/
/// infinite, armed-inactive badge, clamped-endpoint warning — contracts/
/// ui-markers.md §4; 023-markers-panel-structure, Clarification 12),
/// rendered right after the owning [`LoopBlock`]'s last present boundary
/// row — so a `B`-only region shows its cells too. `cells` is the
/// [`PanelModel`] snapshot's own precomputed [`RegionCells`]; only the
/// arm/repeat/crossfade mutations below still read/write `controller`
/// directly — data-model.md §4 defines no [`PanelIntent`] for them.
fn show_region_cells<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    region: RegionId,
    cells: &RegionCells,
) {
    let status = controller.loop_status();

    ui.horizontal(|ui| {
        let mut armed = cells.armed;
        let label = if armed {
            tr("loop-disarm")
        } else {
            tr("loop-arm")
        };
        let enabled = cells.armed || cells.armable;
        let response = ui
            .add_enabled_ui(enabled, |ui| {
                switch(ui, SwitchKind::Checkbox, &mut armed, &label)
            })
            .inner;
        if response.changed() {
            if armed {
                let _ = controller.arm_loop(region);
            } else {
                let _ = controller.disarm_loop();
            }
        }
        if !enabled {
            let reason = if cells.complete {
                "loop-region-too-short"
            } else {
                "loop-region-incomplete"
            };
            response.on_disabled_hover_text(tr(reason));
        }
        if cells.armed && status.state == LoopState::ArmedInactive {
            ui.label(tr("loop-armed-inactive"));
        }
        if cells.clamped {
            ui.label(RichText::new("⚠").color(ui.visuals().warn_fg_color))
                .on_hover_text(tr("marker-clamped-desc"));
        }
    });

    ui.horizontal(|ui| {
        let repeat_label = ui.label(tr("loop-repeat"));
        let mut repeat_value: i64 = cells.repeat_times.map(i64::from).unwrap_or(0);
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
        let mut crossfade_value: i64 = i64::from(cells.crossfade_ms);
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

    if cells.armed && status.state == LoopState::ArmedActive {
        let text = match cells.repeat_times {
            None => tr("loop-wraps-infinite"),
            Some(n) => {
                let remaining = u32::from(n).saturating_sub(status.wraps);
                tr_args("loop-wraps-remaining", &[("count", remaining.to_string())])
            }
        };
        ui.label(text);
    }
}

/// The destructive footer (006 §4, moved by 023-markers-panel-structure
/// T035, contract P9; FR-018/FR-019; research R13): a right-aligned row
/// below the Cues group, spatially and visibly separate from "New loop
/// region" (Clarification/SC-005, SC-006) — the Loop, Points and Cues
/// groups always lie between them. A plain Destructive button until
/// pressed, then the unchanged two-step inline confirmation
/// (`markers-clear-confirm { $count }` + yes/no buttons); `Esc` cancels
/// back to the plain button without clearing anything. Only called from
/// `panel`'s populated branch, so it never renders on the Empty layout
/// (P1).
fn show_footer<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
) {
    ui.add_space(theme::space::MD);
    // A true `Direction::RightToLeft` child (mirrors `rows.rs`'s own
    // trailing-column right alignment): its main axis packs against the
    // *right* edge of the full available width regardless of `main_align`
    // (egui only honours `main_align` on the cross axis for a horizontal
    // layout), unlike `Layout::left_to_right(..).with_main_align(Max)`,
    // which does not move the main-axis position at all. `destructive_gap`
    // is the first widget call in each arm below, so it lands as the
    // right-most (i.e. outer) element — a small inset from the panel's own
    // right edge — keeping the "Yes"/"No"/"Clear all markers" line-order
    // immediately below it exactly as required by
    // `destructive_gap_at_named_instances` (control-variants.md B10).
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if waveform.clear_confirm {
            destructive_gap(ui);
            if ui.button(tr("markers-clear-no")).clicked() {
                waveform.clear_confirm = false;
            }
            if button(ui, Variant::Destructive, tr("markers-clear-yes")).clicked() {
                controller.clear_all_markers();
                waveform.clear_confirm = false;
            }
            let count = controller.markers().map(TrackMarkers::count).unwrap_or(0);
            ui.label(tr_args(
                "markers-clear-confirm",
                &[("count", count.to_string())],
            ));
            if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                waveform.clear_confirm = false;
            }
        } else {
            destructive_gap(ui);
            if button(ui, Variant::Destructive, tr("markers-clear-all")).clicked() {
                waveform.clear_confirm = true;
            }
        }
    });
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
        | modplayer_core::markers::MarkerError::NoTrack
        // 009: only a plugin `Request` can hit this (ownership check on a
        // marker/region it doesn't own) — no host keyboard shortcut can
        // produce it, but the match must stay exhaustive.
        | modplayer_core::markers::MarkerError::NotOwner => "markers-status",
    }
}

// ---------------------------------------------------------------------
// 023-markers-panel-structure — `PanelModel`, its row/block/cue-slot
// projections, and `PanelIntent`/`apply_intents` (data-model.md §1-4;
// research R1-R3). `panel()` builds and renders a `PanelModel` (Phase 3,
// US1, T014); as of Phase 4 (US2), every row widget (`show_swatch`,
// `show_name_cell`, `show_row_action`, `show_palette_popup`) pushes a
// `PanelIntent`, and `panel()`'s own `apply_intents` call applies them —
// `PanelModel::visual_order` now also drives `resolve_pending_panel_
// focus` (T033). A handful of `PanelIntent` variants
// (`NewLoopRegion`/`ClearRequest`/`ClearConfirm`/`ClearCancel`) stay
// unconstructed: "New loop region" and the footer's Clear-all still
// mutate `controller`/`waveform` directly (Phase 3/5), which is why the
// enum itself keeps `#[allow(dead_code)]` — deliberate, not accidental.
// ---------------------------------------------------------------------

/// One populated Markers-panel row (023-markers-panel-structure, data-
/// model.md §2): the panel's own read-only snapshot of a [`Marker`],
/// replacing today's [`MarkerRowData`] — its region membership is no
/// longer a field, since it is implied by which [`PanelModel`] collection
/// holds it ([`LoopBlock`]'s `a`/`b`, or [`CueSlotRow::Occupied`]).
#[derive(Debug, Clone, PartialEq)]
struct RowData {
    id: MarkerId,
    kind: MarkerKind,
    name: String,
    color: PaletteIndex,
    /// Frames; the panel renders this via `waveform::format_mmss_millis`,
    /// or the live drag position while dragging (data-model.md §2).
    position: u64,
    clamped: bool,
}

impl RowData {
    fn from_marker(marker: &Marker) -> Self {
        Self {
            id: marker.id,
            kind: marker.kind,
            name: marker.name.clone(),
            color: marker.color,
            position: marker.position,
            clamped: marker.clamped,
        }
    }
}

/// A loop region's cells (023-markers-panel-structure, data-model.md §3):
/// today's [`RegionRow`] renamed. Rendered after the owning [`LoopBlock`]'s
/// *last present* boundary row (Clarification 12) rather than only after
/// `A`, so a `B`-only region finally shows its cells too.
#[derive(Debug, Clone, PartialEq)]
struct RegionCells {
    armed: bool,
    crossfade_ms: u8,
    repeat_times: Option<u16>,
    armable: bool,
    complete: bool,
    /// Either endpoint marker's `clamped` flag (006, FR-018).
    clamped: bool,
}

/// One loop region's block (023-markers-panel-structure, data-model.md
/// §3): its `A`/`B` rows — either may be absent while the region is
/// incomplete, and both may be absent for a just-created, still-empty
/// region (`controller::new_loop_region`) — plus its [`RegionCells`].
#[derive(Debug, Clone, PartialEq)]
struct LoopBlock {
    region: RegionId,
    a: Option<RowData>,
    b: Option<RowData>,
    cells: RegionCells,
}

impl LoopBlock {
    fn from_region(region: &LoopRegion, markers: &TrackMarkers, sample_rate: u32) -> Self {
        let a = region
            .a
            .and_then(|id| markers.marker(id))
            .map(RowData::from_marker);
        let b = region
            .b
            .and_then(|id| markers.marker(id))
            .map(RowData::from_marker);
        let clamped =
            a.as_ref().is_some_and(|row| row.clamped) || b.as_ref().is_some_and(|row| row.clamped);
        let cells = RegionCells {
            armed: region.armed,
            crossfade_ms: region.crossfade_ms,
            repeat_times: match region.repeat {
                RepeatCount::Infinite => None,
                RepeatCount::Times(n) => Some(n),
            },
            armable: region.is_armable(markers, sample_rate),
            complete: region.is_complete(),
            clamped,
        };
        Self {
            region: region.id,
            a,
            b,
            cells,
        }
    }

    /// `(earliest present boundary position, region id)` (FR-003,
    /// research R3): `A`'s position if present, else `B`'s, else `0` for
    /// a still-empty region — tie-broken by [`RegionId`] either way.
    fn sort_key(&self) -> (u64, RegionId) {
        let position = self
            .a
            .as_ref()
            .or(self.b.as_ref())
            .map_or(0, |row| row.position);
        (position, self.region)
    }
}

/// One of the panel's 8 fixed cue-slot rows (023-markers-panel-structure,
/// data-model.md §3, contract P8): `Occupied` renders as a full row;
/// `Empty` renders as a muted, non-interactive placeholder naming its own
/// slot (FR-015/FR-016).
#[derive(Debug, Clone, PartialEq)]
enum CueSlotRow {
    Occupied(RowData),
    Empty(CueSlot),
}

/// A pure, per-frame, read-only projection of [`TrackMarkers`] into the
/// panel's three groups (023-markers-panel-structure, data-model.md §1;
/// research R1-R3): built once by [`PanelModel::snapshot`] before any
/// widget renders, so the partition/order/count rules (FR-001-FR-006,
/// FR-016) are unit-testable without an egui context, and rendering never
/// holds a `&TrackMarkers` borrow across a `&mut controller` call
/// (research R2 — the reason today's [`MarkerRowData`] exists at all).
#[derive(Debug, Clone, PartialEq)]
struct PanelModel {
    /// One per [`TrackMarkers::regions`] entry, sorted by
    /// [`LoopBlock::sort_key`] (FR-003).
    loop_blocks: Vec<LoopBlock>,
    /// Every `MarkerKind::Point`, sorted by `(position, id)` (FR-005).
    points: Vec<RowData>,
    /// Index `i` <-> `CueSlot::new(i + 1)` (FR-016).
    cues: [CueSlotRow; 8],
    /// `TrackMarkers::count()`; `0` when no `TrackMarkers` (FR-020).
    total: usize,
}

impl PanelModel {
    /// Builds the snapshot from `markers` (`None` selects the empty
    /// state, same as `count() == 0`, FR-020), for `sample_rate`
    /// ([`RegionCells::armable`]).
    fn snapshot(markers: Option<&TrackMarkers>, sample_rate: u32) -> Self {
        let Some(markers) = markers else {
            return Self::empty_cues();
        };

        let mut loop_blocks: Vec<LoopBlock> = markers
            .regions()
            .iter()
            .map(|region| LoopBlock::from_region(region, markers, sample_rate))
            .collect();
        loop_blocks.sort_by_key(LoopBlock::sort_key);

        let mut points: Vec<RowData> = markers
            .markers()
            .iter()
            .filter(|marker| matches!(marker.kind, MarkerKind::Point))
            .map(RowData::from_marker)
            .collect();
        points.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));

        let cues = std::array::from_fn(|i| {
            let slot = cue_slot_at(i);
            match markers.cue(slot) {
                Some(marker) => CueSlotRow::Occupied(RowData::from_marker(marker)),
                None => CueSlotRow::Empty(slot),
            }
        });

        Self {
            loop_blocks,
            points,
            cues,
            total: markers.count(),
        }
    }

    /// The empty-state snapshot (no `TrackMarkers` at all): no blocks, no
    /// points, all 8 cue slots empty, `total == 0`.
    fn empty_cues() -> Self {
        Self {
            loop_blocks: Vec::new(),
            points: Vec::new(),
            cues: std::array::from_fn(|i| CueSlotRow::Empty(cue_slot_at(i))),
            total: 0,
        }
    }

    /// `total == 0`, which selects the empty layout (FR-020).
    fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// `loop_blocks.len()` (FR-002; an incomplete or still-empty region
    /// counts 1).
    fn loop_count(&self) -> usize {
        self.loop_blocks.len()
    }

    /// `points.len()`.
    fn point_count(&self) -> usize {
        self.points.len()
    }

    /// The number of occupied cue slots (FR-002; never more than 8).
    fn cue_count(&self) -> usize {
        self.cues
            .iter()
            .filter(|slot| matches!(slot, CueSlotRow::Occupied(_)))
            .count()
    }

    /// The Points group renders only when non-empty (FR-001).
    fn show_points_group(&self) -> bool {
        !self.is_empty() && self.point_count() > 0
    }

    /// The Loop Region group always renders once populated (FR-004).
    #[allow(dead_code)]
    fn show_loop_group(&self) -> bool {
        !self.is_empty()
    }

    /// The Cues group always renders once populated, showing all 8 slots
    /// (FR-016).
    #[allow(dead_code)]
    fn show_cues_group(&self) -> bool {
        !self.is_empty()
    }

    /// Populated rows in render order (research R11): each block's `A`
    /// then `B`, then points by position, then occupied cues by slot —
    /// the order [`PanelIntent::Remove`]'s post-removal focus target
    /// walks, and [`resolve_pending_panel_focus`] re-validates against.
    fn visual_order(&self) -> Vec<MarkerId> {
        let mut order = Vec::new();
        for block in &self.loop_blocks {
            if let Some(a) = &block.a {
                order.push(a.id);
            }
            if let Some(b) = &block.b {
                order.push(b.id);
            }
        }
        order.extend(self.points.iter().map(|row| row.id));
        for slot in &self.cues {
            if let CueSlotRow::Occupied(row) = slot {
                order.push(row.id);
            }
        }
        order
    }
}

/// `CueSlot::new(i + 1)` for `i` in `0..8` — always `Some` since `i + 1`
/// is always in `1..=8`.
fn cue_slot_at(i: usize) -> CueSlot {
    #[allow(clippy::cast_possible_truncation)]
    let raw = (i + 1) as u8;
    CueSlot::new(raw).unwrap_or_else(|| unreachable!("cue_slot_at({i}) must be 1..=8"))
}

/// A row/footer/palette widget's requested mutation, pushed during render
/// and applied afterward by [`apply_intents`] (023-markers-panel-
/// structure, data-model.md §4; research R2): rendering itself never
/// calls `&mut controller`, which removes today's borrow-juggling
/// ([`MarkerRowData`]'s whole reason to exist) and makes "a rename commits
/// before another row's action" (Edge Case) hold by construction —
/// [`apply_intents`] applies any `CommitRename`/`CancelRename` first, then
/// every other intent in emission order.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
enum PanelIntent {
    /// "New loop region" (FR-017).
    NewLoopRegion,
    /// Any row control gaining focus (research R8).
    Focus(MarkerId),
    /// The name cell opening an inline rename (FR-012).
    OpenRename(MarkerId),
    /// The rename `TextEdit`'s Enter / focus-loss-without-Esc (FR-012).
    CommitRename(MarkerId, String),
    /// The rename `TextEdit`'s Esc (FR-012).
    CancelRename,
    /// A palette swatch pick (FR-013).
    Recolor(MarkerId, PaletteIndex),
    /// The row's jump action (FR-009).
    Jump(MarkerId),
    /// The row's nudge-earlier (`-1`) / nudge-later (`1`) action
    /// (FR-010).
    Nudge(MarkerId, i8),
    /// The row's remove action (FR-011).
    Remove(MarkerId),
    /// The footer's "Clear all markers" button (FR-019).
    ClearRequest,
    /// The footer confirmation's "Yes" (FR-019).
    ClearConfirm,
    /// The footer confirmation's "No" / `Esc` (FR-019).
    ClearCancel,
}

/// Drains `intents` and applies each to `controller`/`waveform`
/// (023-markers-panel-structure, data-model.md §4; research R2): pass one
/// applies any [`PanelIntent::CommitRename`]/[`PanelIntent::CancelRename`]
/// first; pass two applies every other intent in emission order. Calls
/// `ctx.request_repaint()` once if anything was applied, so a rename's or
/// recolour's effect on the lanes (drawn *before* the panel, 021) shows on
/// the very next rendered frame rather than waiting for the next input
/// event (research R9).
fn apply_intents<B: OutputBackend, H: SourceHost>(
    intents: &mut Vec<PanelIntent>,
    ctx: &egui::Context,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
) {
    if intents.is_empty() {
        return;
    }
    let drained: Vec<PanelIntent> = std::mem::take(intents);

    for intent in &drained {
        match intent {
            PanelIntent::CommitRename(id, draft) => {
                let _ = controller.rename_marker(*id, draft);
                waveform.rename = None;
            }
            PanelIntent::CancelRename => {
                waveform.rename = None;
            }
            _ => {}
        }
    }

    for intent in drained {
        match intent {
            PanelIntent::CommitRename(..) | PanelIntent::CancelRename => {}
            PanelIntent::NewLoopRegion => {
                let _ = controller.new_loop_region();
            }
            PanelIntent::Focus(id) => {
                waveform.focused_marker = Some(id);
                let _ = controller.select_marker(id);
            }
            PanelIntent::OpenRename(id) => {
                let name = controller
                    .markers()
                    .and_then(|markers| markers.marker(id))
                    .map(|marker| marker.name.clone())
                    .unwrap_or_default();
                waveform.rename = Some((id, name));
                waveform.rename_focus_pending = true;
            }
            PanelIntent::Recolor(id, color) => {
                let _ = controller.recolor_marker(id, color);
            }
            PanelIntent::Jump(id) => {
                let dragging = waveform.marker_drag.is_some_and(|drag| drag.marker == id);
                if !dragging
                    && let Some(position) = controller.markers().and_then(|m| m.position_of(id))
                {
                    controller.seek_frames(position);
                }
            }
            PanelIntent::Nudge(id, direction) => {
                let _ = controller.nudge_marker(id, direction, 1);
            }
            PanelIntent::Remove(id) => {
                let rate = controller.source_sample_rate().max(1);
                let order = PanelModel::snapshot(controller.markers(), rate).visual_order();
                let target = remove_focus_target(&order, id);
                let _ = controller.delete_marker(id);
                if waveform.focused_marker == Some(id) {
                    waveform.focused_marker = None;
                }
                if waveform
                    .rename
                    .as_ref()
                    .is_some_and(|(rename_id, _)| *rename_id == id)
                {
                    waveform.rename = None;
                }
                waveform.panel_focus = Some(target);
            }
            PanelIntent::ClearRequest => {
                waveform.clear_confirm = true;
            }
            PanelIntent::ClearConfirm => {
                controller.clear_all_markers();
                waveform.clear_confirm = false;
            }
            PanelIntent::ClearCancel => {
                waveform.clear_confirm = false;
            }
        }
    }

    ctx.request_repaint();
}

/// The post-[`PanelIntent::Remove`] focus target (research R11,
/// Clarification 8): the next id after `removed` in `order` (render
/// order), else the previous one, else [`waveform::PanelFocus::
/// NewLoopRegion`] — including when `removed` is not (or is no longer) in
/// `order` at all, which makes a same-frame double-remove a silent no-op
/// that still resolves to a sensible fallback target.
fn remove_focus_target(order: &[MarkerId], removed: MarkerId) -> waveform::PanelFocus {
    let Some(index) = order.iter().position(|&id| id == removed) else {
        return waveform::PanelFocus::NewLoopRegion;
    };
    if let Some(&next) = order.get(index + 1) {
        return waveform::PanelFocus::Row(next);
    }
    if index > 0
        && let Some(&prev) = order.get(index - 1)
    {
        return waveform::PanelFocus::Row(prev);
    }
    waveform::PanelFocus::NewLoopRegion
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_core::markers::{MAX_MARKERS, MarkerError, Owner};

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

    // -------------------------------------------------------------
    // 023-markers-panel-structure — T002 (Foundational): `PanelModel`
    // unit tests (contract §9 `panel_model_partitions_by_kind`) and the
    // `panel_model_invariants` proptest (P1-P4).
    // -------------------------------------------------------------

    fn track_markers(label: &str) -> TrackMarkers {
        let id = modplayer_audio_source::TrackId::new(format!("spotify:track:{label}"))
            .unwrap_or_else(|_| unreachable!());
        TrackMarkers::new(id, 44_100, 44_100 * 200)
    }

    /// contract §9 `panel_model_partitions_by_kind`: one region (`A`+`B`),
    /// one point and one cue partition into their own groups, each with
    /// the id the model was built from — pins P1/P2 (data-model.md §1)
    /// concretely, alongside the proptest's general form below.
    #[test]
    fn panel_model_partitions_by_kind() {
        let mut markers = track_markers("panel-model-partition");
        let (region, a_id) = markers
            .set_loop_endpoint_owned(None, true, 1_000, Owner::Host)
            .unwrap_or_else(|e| unreachable!("set_loop_endpoint_owned a: {e}"));
        let (_, b_id) = markers
            .set_loop_endpoint_owned(Some(region), false, 5_000, Owner::Host)
            .unwrap_or_else(|e| unreachable!("set_loop_endpoint_owned b: {e}"));
        let point_id = markers
            .add_point(2_000)
            .unwrap_or_else(|e| unreachable!("add_point: {e}"));
        let cue_slot = CueSlot::new(3).unwrap_or_else(|| unreachable!());
        let cue_id = markers
            .set_cue_owned(cue_slot, 3_000, Owner::Host)
            .unwrap_or_else(|e| unreachable!("set_cue_owned: {e}"));

        let model = PanelModel::snapshot(Some(&markers), 44_100);

        assert_eq!(model.loop_blocks.len(), 1);
        assert_eq!(model.loop_blocks[0].region, region);
        assert_eq!(
            model.loop_blocks[0].a.as_ref().map(|row| row.id),
            Some(a_id)
        );
        assert_eq!(
            model.loop_blocks[0].b.as_ref().map(|row| row.id),
            Some(b_id)
        );
        assert_eq!(model.points.len(), 1);
        assert_eq!(model.points[0].id, point_id);
        assert!(
            matches!(&model.cues[2], CueSlotRow::Occupied(row) if row.id == cue_id),
            "slot 3 (index 2) must hold the cue just created"
        );
        for (i, slot) in model.cues.iter().enumerate() {
            if i != 2 {
                assert!(
                    matches!(slot, CueSlotRow::Empty(_)),
                    "slot {i} must be empty"
                );
            }
        }
        assert_eq!(model.total, 4);
        assert_eq!(model.loop_count(), 1);
        assert_eq!(model.point_count(), 1);
        assert_eq!(model.cue_count(), 1);
    }

    /// An empty `TrackMarkers` (no markers at all): `is_empty`, every
    /// count `0`, every group visibility flag `false`, but all 8 cue
    /// slots still exist as `Empty` (FR-016, FR-020).
    #[test]
    fn panel_model_empty_track_markers_is_empty() {
        let markers = track_markers("panel-model-empty");
        let model = PanelModel::snapshot(Some(&markers), 44_100);

        assert!(model.is_empty());
        assert_eq!(model.loop_count(), 0);
        assert_eq!(model.point_count(), 0);
        assert_eq!(model.cue_count(), 0);
        assert!(!model.show_points_group());
        assert!(!model.show_loop_group());
        assert!(!model.show_cues_group());
        assert_eq!(model.cues.len(), 8);
        assert!(
            model
                .cues
                .iter()
                .all(|slot| matches!(slot, CueSlotRow::Empty(_)))
        );
        assert!(model.visual_order().is_empty());
    }

    /// No `TrackMarkers` at all (no track loaded): the same empty
    /// snapshot as `count() == 0` (FR-020).
    #[test]
    fn panel_model_no_track_markers_is_empty() {
        let model = PanelModel::snapshot(None, 44_100);
        assert!(model.is_empty());
        assert_eq!(model.total, 0);
        assert_eq!(model.cues.len(), 8);
    }

    /// Points-only track: the Loop and Cues groups still show (always
    /// once populated, FR-004/FR-016) even though `loop_count()`/
    /// `cue_count()` are both `0`.
    #[test]
    fn panel_model_counts_match_kinds() {
        let mut markers = track_markers("panel-model-counts");
        markers
            .add_point(1_000)
            .unwrap_or_else(|e| unreachable!("add_point: {e}"));
        markers
            .add_point(2_000)
            .unwrap_or_else(|e| unreachable!("add_point: {e}"));

        let model = PanelModel::snapshot(Some(&markers), 44_100);

        assert_eq!(model.point_count(), 2);
        assert!(model.show_points_group());
        assert!(
            model.show_loop_group(),
            "loop group must always show once anything is populated"
        );
        assert!(
            model.show_cues_group(),
            "cues group must always show once anything is populated"
        );
        assert_eq!(model.loop_count(), 0);
        assert_eq!(model.cue_count(), 0);
    }

    /// A `B`-only region (Clarification 12): still one `LoopBlock`,
    /// sorted by `B`'s own position since `A` is absent (research R3).
    #[test]
    fn panel_model_b_only_region_is_one_block_sorted_by_b() {
        let mut markers = track_markers("panel-model-b-only");
        let (region, b_id) = markers
            .set_loop_endpoint_owned(None, false, 7_000, Owner::Host)
            .unwrap_or_else(|e| unreachable!("set_loop_endpoint_owned b: {e}"));

        let model = PanelModel::snapshot(Some(&markers), 44_100);

        assert_eq!(model.loop_blocks.len(), 1);
        assert_eq!(model.loop_blocks[0].region, region);
        assert!(model.loop_blocks[0].a.is_none());
        assert_eq!(
            model.loop_blocks[0].b.as_ref().map(|row| row.id),
            Some(b_id)
        );
        assert_eq!(model.loop_blocks[0].sort_key(), (7_000, region));
    }

    proptest::proptest! {
        /// contract §9 `proptest panel_model_invariants` (data-model.md
        /// §1 P1-P4): any mix of regions (including incomplete/empty
        /// ones), points and cues built through the real `TrackMarkers`
        /// mutation API.
        #[test]
        fn panel_model_invariants(
            point_positions in proptest::collection::vec(0u64..(44_100 * 200), 0..8),
            region_specs in proptest::collection::vec(
                (proptest::bool::ANY, proptest::bool::ANY, 0u64..(44_100 * 200), 0u64..(44_100 * 200)),
                0..4,
            ),
            cue_specs in proptest::collection::vec((1u8..=8, 0u64..(44_100 * 200)), 0..8),
        ) {
            let mut markers = track_markers("panel-model-proptest");

            for pos in point_positions {
                if markers.count() >= MAX_MARKERS {
                    break;
                }
                let _ = markers.add_point(pos);
            }

            for (has_a, has_b, a_pos, b_pos) in region_specs {
                if !has_a && !has_b {
                    continue;
                }
                let region = markers.new_loop_region();
                if has_a {
                    if markers.count() >= MAX_MARKERS {
                        break;
                    }
                    let _ = markers.set_loop_endpoint_owned(Some(region), true, a_pos, Owner::Host);
                }
                if has_b {
                    if markers.count() >= MAX_MARKERS {
                        break;
                    }
                    let _ = markers.set_loop_endpoint_owned(Some(region), false, b_pos, Owner::Host);
                }
            }

            let mut seen_slots = std::collections::HashSet::new();
            for (raw_slot, pos) in cue_specs {
                if !seen_slots.insert(raw_slot) {
                    continue;
                }
                if markers.count() >= MAX_MARKERS {
                    break;
                }
                let slot = CueSlot::new(raw_slot).unwrap_or_else(|| unreachable!());
                let _ = markers.set_cue_owned(slot, pos, Owner::Host);
            }

            let model = PanelModel::snapshot(Some(&markers), 44_100);

            // P1: the multiset of populated-row ids equals the set of
            // `TrackMarkers::markers()` ids — no marker missing or
            // duplicated.
            let mut model_ids: Vec<MarkerId> = model
                .loop_blocks
                .iter()
                .flat_map(|block| [block.a.as_ref().map(|row| row.id), block.b.as_ref().map(|row| row.id)])
                .flatten()
                .chain(model.points.iter().map(|row| row.id))
                .chain(model.cues.iter().filter_map(|slot| match slot {
                    CueSlotRow::Occupied(row) => Some(row.id),
                    CueSlotRow::Empty(_) => None,
                }))
                .collect();
            let mut real_ids: Vec<MarkerId> = markers.markers().iter().map(|m| m.id).collect();
            model_ids.sort();
            real_ids.sort();
            proptest::prop_assert_eq!(&model_ids, &real_ids);

            // P2: every row's kind matches its own group. The `matches!`
            // boolean is bound to a local first — `prop_assert!` stringifies
            // its condition expression into its own failure-message format
            // string, and a literal `{ field }` struct-pattern brace inside
            // that expression breaks that formatting (a `matches!(.., Kind
            // { field } if ..)` condition, not this proptest itself).
            for block in &model.loop_blocks {
                if let Some(a) = &block.a {
                    let a_is_region_start =
                        matches!(a.kind, MarkerKind::RegionStart { region } if region == block.region);
                    proptest::prop_assert!(a_is_region_start);
                }
                if let Some(b) = &block.b {
                    let b_is_region_end =
                        matches!(b.kind, MarkerKind::RegionEnd { region } if region == block.region);
                    proptest::prop_assert!(b_is_region_end);
                }
            }
            for row in &model.points {
                proptest::prop_assert!(matches!(row.kind, MarkerKind::Point));
            }
            for (i, slot) in model.cues.iter().enumerate() {
                if let CueSlotRow::Occupied(row) = slot {
                    let expected_slot = cue_slot_at(i);
                    let row_is_expected_cue =
                        matches!(row.kind, MarkerKind::Cue { slot } if slot == expected_slot);
                    proptest::prop_assert!(row_is_expected_cue);
                }
            }

            // P3: `visual_order()` is a permutation of P1's set.
            let mut order = model.visual_order();
            order.sort();
            let mut expected = real_ids.clone();
            expected.sort();
            proptest::prop_assert_eq!(order, expected);

            // P4: inside a block, `A` (if any) precedes `B` (if any) in
            // `visual_order()`.
            let order = model.visual_order();
            for block in &model.loop_blocks {
                if let (Some(a), Some(b)) = (&block.a, &block.b) {
                    let a_index = order.iter().position(|&id| id == a.id);
                    let b_index = order.iter().position(|&id| id == b.id);
                    proptest::prop_assert!(a_index < b_index);
                }
            }
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The waveform widgets: the whole-track overview and the zoomed detail
//! view, both click/drag/keyboard-seekable, the detail additionally zoom/
//! pan-able (005-now-playing-waveform, contracts/ui-waveform.md). `coords`,
//! `state`, `paint` and `input` are its sub-modules.

pub mod coords;
pub mod hover;
pub mod input;
pub mod paint;
pub mod state;

use std::ops::Range;

use egui::{Painter, Pos2, Sense, Ui, Vec2, Visuals, WidgetInfo, vec2};
use modplayer_core::{tr, tr_args};

use crate::theme;

pub use coords::{TimeSpace, WaveformResponse};
pub use hover::{HoverIndicator, hover_indicator};
pub use input::{MarkerKeyAction, WaveformEvent, focused_marker_key};
pub use paint::{ColumnPaint, WaveformPaint, waveform_columns};
pub use state::{DetailWindow, DragOrigin, DragPreview, MarkerDrag, PanelFocus, WaveformState};

/// `"m:ss"` for a frame count at `sample_rate` (no leading-zero minutes,
/// matching `now_playing.rs`'s existing `format_mmss`).
fn format_mmss_frames(frame: u64, sample_rate: u32) -> String {
    let total_seconds = frame / u64::from(sample_rate.max(1));
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

/// `"m:ss.mmm"` for a frame count at `sample_rate` (022-waveform-
/// legibility, US3, contracts/ui-waveform-legibility.md WL5/WL6, R12): no
/// leading-zero minutes, matching [`format_mmss_frames`] above. Shared by
/// the hover indicator's label ([`hover::hover_indicator`]) and the
/// Markers panel/lane's own `m:ss.mmm` readouts (`markers.rs`) — promoted
/// from `markers.rs`'s former, identically-behaved `format_mmss_millis_
/// frames` (WL6 regression: output stays byte-identical, e.g. `0:35.204`).
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub(crate) fn format_mmss_millis(frame: u64, sample_rate: u32) -> String {
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

/// The hover indicator's label text size for `text`, measured without
/// painting it (022-waveform-legibility, WL5): [`hover::hover_indicator`]
/// pads this by `theme::space::XS` on every side itself, so this helper
/// measures only, never repeats that padding math. Sums each character's
/// own advance in the `mono` role rather than calling `Painter::
/// layout_no_wrap` (which needs a throwaway fill colour — a `Color32`
/// literal this crate's own colour-literal scan does not allow outside
/// `theme/**`, FR-018a).
fn hover_label_galley_size(painter: &Painter, text: &str) -> Vec2 {
    let font_id = theme::mono_font_id();
    painter.fonts_mut(|fonts| {
        let width: f32 = text.chars().map(|c| fonts.glyph_width(&font_id, c)).sum();
        Vec2::new(width, fonts.row_height(&font_id))
    })
}

/// Paint this frame's hover scrub indicator, if any (022-waveform-
/// legibility, contracts/ui-waveform-legibility.md WL1/WL5): called after
/// the `overlays` hook and before the playhead, on both views. `pointer`
/// is `response.hover_pos()`, already gated to `None` by the caller
/// whenever the view is disabled (WL5) — this function itself only gates
/// on `paint_data.hover_suppressed`. Probes [`hover::hover_indicator`]
/// twice: once to learn this pointer position's frame/label text (an
/// empty galley size is enough for that), then again with the text's own
/// measured size, so the pill is sized to what it actually paints without
/// this module duplicating the pure x-to-frame mapping [`hover::
/// hover_indicator`] already owns.
fn paint_hover_indicator(
    painter: &Painter,
    space: &TimeSpace,
    pointer: Option<Pos2>,
    paint_data: &WaveformPaint<'_>,
    visuals: &Visuals,
) {
    let Some(probe) =
        hover::hover_indicator(space, pointer, paint_data.hover_suppressed, Vec2::ZERO)
    else {
        return;
    };
    let label_size = hover_label_galley_size(painter, &probe.text);
    if let Some(indicator) =
        hover::hover_indicator(space, pointer, paint_data.hover_suppressed, label_size)
    {
        hover::paint_hover(painter, space, &indicator, visuals);
    }
}

/// Draw the whole-track waveform overview and resolve this frame's input
/// on it (contracts/ui-waveform.md §1-4): a full-width, `height`-tall
/// `Sense::click_and_drag()` rect, painted via [`paint::paint`], exposed
/// to AccessKit as `Role::Slider` named `transport-seek` with value text
/// `m:ss / m:ss` and description `waveform-overview-desc`. `height` is
/// the caller's `layout::waveform_heights` result (018-window-sizing-and-
/// responsive-dock, contract D8, FR-012) — this module no longer fixes a
/// height itself. `overlays` is called after the peaks/highlight and
/// before the playhead (006, research R16, contracts/ui-markers.md §5) —
/// 005's promised attachment point for marker lines and the loop-region
/// span. Returns the [`WaveformResponse`] and at most one
/// [`WaveformEvent`] for the caller (`now_playing.rs`) to apply — this
/// module never calls `PlaybackController` itself, keeping it free of
/// that type.
#[allow(clippy::too_many_arguments)]
pub fn overview(
    ui: &mut Ui,
    len_frames: u64,
    sample_rate: u32,
    playhead: u64,
    previewing: bool,
    enabled: bool,
    height: f32,
    paint_data: &WaveformPaint<'_>,
    overlays: &mut dyn FnMut(&Painter, &TimeSpace),
) -> (WaveformResponse, Option<WaveformEvent>) {
    let width = ui.available_width();
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
    let window: Range<u64> = 0..len_frames.max(1);
    let space = TimeSpace::new(rect, window.clone(), sample_rate);
    // WL5: only an `enabled` view ever asks for its own hover position —
    // a disabled view shows no scrub indicator regardless of where the
    // pointer physically sits.
    let pointer = if enabled { response.hover_pos() } else { None };

    if ui.is_rect_visible(rect) {
        paint::paint(ui.painter(), &space, ui.visuals(), paint_data);
        overlays(ui.painter(), &space);
        paint_hover_indicator(ui.painter(), &space, pointer, paint_data, ui.visuals());
        paint::playhead(ui.painter(), &space, paint_data.playhead, ui.visuals());
    }

    let value_text = format!(
        "{} / {}",
        format_mmss_frames(playhead.min(window.end), sample_rate),
        format_mmss_frames(window.end, sample_rate)
    );
    let position_seconds = (playhead.min(window.end) as f64) / f64::from(sample_rate.max(1));
    response.widget_info(|| WidgetInfo {
        current_text_value: Some(value_text.clone()),
        ..WidgetInfo::slider(enabled, position_seconds, tr("transport-seek"))
    });
    let description = tr("waveform-overview-desc");
    ui.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_description(description.clone());
    });

    let event = if enabled {
        input::handle(ui, &response, &space, playhead, previewing, false)
    } else {
        None
    };

    (WaveformResponse { response, space }, event)
}

/// Draw the zoomed waveform detail view and resolve this frame's input on
/// it (contracts/ui-waveform.md §1-4): same `Sense::click_and_drag()` /
/// paint / `WidgetInfo::slider` pattern as [`overview`], but scoped to
/// `window` (the current [`DetailWindow`]'s bounds) rather than the whole
/// track, named `waveform-detail` with description `waveform-detail-window
/// { $start } { $end }` (contracts/ui-waveform.md §1). `height` is the
/// caller's `layout::waveform_heights` result, same as [`overview`]
/// (018-window-sizing-and-responsive-dock, contract D8, FR-012). Returns
/// the [`WaveformResponse`] and at most one [`WaveformEvent`] — seeks,
/// zoom, and pan alike — for the caller to apply; this module never
/// touches `PlaybackController` or `DetailWindow` itself. `overlays` is
/// called after the peaks and before the playhead, same as [`overview`]
/// (006, research R16, contracts/ui-markers.md §5).
#[allow(clippy::too_many_arguments)]
pub fn detail(
    ui: &mut Ui,
    window: Range<u64>,
    sample_rate: u32,
    playhead: u64,
    previewing: bool,
    enabled: bool,
    height: f32,
    paint_data: &WaveformPaint<'_>,
    overlays: &mut dyn FnMut(&Painter, &TimeSpace),
) -> (WaveformResponse, Option<WaveformEvent>) {
    let width = ui.available_width();
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
    let space = TimeSpace::new(rect, window.clone(), sample_rate);
    let pointer = if enabled { response.hover_pos() } else { None };

    if ui.is_rect_visible(rect) {
        paint::paint(ui.painter(), &space, ui.visuals(), paint_data);
        overlays(ui.painter(), &space);
        paint_hover_indicator(ui.painter(), &space, pointer, paint_data, ui.visuals());
        paint::playhead(ui.painter(), &space, paint_data.playhead, ui.visuals());
    }

    let value_text = format!(
        "{} / {}",
        format_mmss_frames(playhead.clamp(window.start, window.end), sample_rate),
        format_mmss_frames(window.end, sample_rate)
    );
    let position_seconds =
        (playhead.clamp(window.start, window.end) as f64) / f64::from(sample_rate.max(1));
    response.widget_info(|| WidgetInfo {
        current_text_value: Some(value_text.clone()),
        ..WidgetInfo::slider(enabled, position_seconds, tr("waveform-detail"))
    });
    let description = tr_args(
        "waveform-detail-window",
        &[
            ("start", format_mmss_frames(window.start, sample_rate)),
            ("end", format_mmss_frames(window.end, sample_rate)),
        ],
    );
    ui.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_description(description.clone());
    });

    let event = if enabled {
        input::handle(ui, &response, &space, playhead, previewing, true)
    } else {
        None
    };

    (WaveformResponse { response, space }, event)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WL6 regression: byte-identical to the former `markers.rs`
    /// `format_mmss_millis_frames` this function replaces.
    #[test]
    fn format_mmss_millis_matches_seconds_and_millis() {
        assert_eq!(format_mmss_millis(0, 44_100), "0:00.000");
        assert_eq!(format_mmss_millis(44_100, 44_100), "0:01.000");
        assert_eq!(format_mmss_millis(44_100 * 65 + 4_410, 44_100), "1:05.100");
    }
}

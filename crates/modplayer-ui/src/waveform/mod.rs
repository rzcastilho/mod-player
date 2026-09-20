// SPDX-License-Identifier: MIT OR Apache-2.0

//! The waveform widgets: the whole-track overview and the zoomed detail
//! view, both click/drag/keyboard-seekable, the detail additionally zoom/
//! pan-able (005-now-playing-waveform, contracts/ui-waveform.md). `coords`,
//! `state`, `paint` and `input` are its sub-modules.

pub mod coords;
pub mod input;
pub mod paint;
pub mod state;

use std::ops::Range;

use egui::{Painter, Sense, Ui, WidgetInfo, vec2};
use modplayer_core::{tr, tr_args};

pub use coords::{TimeSpace, WaveformResponse};
pub use input::{MarkerKeyAction, WaveformEvent, focused_marker_key};
pub use paint::{ColumnPaint, WaveformPaint, waveform_columns};
pub use state::{DetailWindow, DragOrigin, DragPreview, MarkerDrag, WaveformState};

/// The overview's fixed height (contracts/ui-waveform.md §1).
pub const OVERVIEW_HEIGHT: f32 = 72.0;

/// `"m:ss"` for a frame count at `sample_rate` (no leading-zero minutes,
/// matching `now_playing.rs`'s existing `format_mmss`).
fn format_mmss_frames(frame: u64, sample_rate: u32) -> String {
    let total_seconds = frame / u64::from(sample_rate.max(1));
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

/// Draw the whole-track waveform overview and resolve this frame's input
/// on it (contracts/ui-waveform.md §1-4): a full-width, `OVERVIEW_HEIGHT`-
/// tall `Sense::click_and_drag()` rect, painted via [`paint::paint`],
/// exposed to AccessKit as `Role::Slider` named `transport-seek` with value
/// text `m:ss / m:ss` and description `waveform-overview-desc`. `overlays`
/// is called after the peaks/highlight and before the playhead (006,
/// research R16, contracts/ui-markers.md §5) — 005's promised attachment
/// point for marker lines and the loop-region span. Returns the
/// [`WaveformResponse`] and at most one [`WaveformEvent`] for the caller
/// (`now_playing.rs`) to apply — this module never calls
/// `PlaybackController` itself, keeping it free of that type.
#[allow(clippy::too_many_arguments)]
pub fn overview(
    ui: &mut Ui,
    len_frames: u64,
    sample_rate: u32,
    playhead: u64,
    previewing: bool,
    enabled: bool,
    paint_data: &WaveformPaint<'_>,
    overlays: &mut dyn FnMut(&Painter, &TimeSpace),
) -> (WaveformResponse, Option<WaveformEvent>) {
    let width = ui.available_width();
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, OVERVIEW_HEIGHT), sense);
    let window: Range<u64> = 0..len_frames.max(1);
    let space = TimeSpace::new(rect, window.clone(), sample_rate);

    if ui.is_rect_visible(rect) {
        paint::paint(ui.painter(), &space, ui.visuals(), paint_data);
        overlays(ui.painter(), &space);
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

/// The zoomed-in detail height (contracts/ui-waveform.md §1) — taller than
/// the overview since it is the primary navigation surface once zoomed.
pub const DETAIL_HEIGHT: f32 = 120.0;

/// Draw the zoomed waveform detail view and resolve this frame's input on
/// it (contracts/ui-waveform.md §1-4): same `Sense::click_and_drag()` /
/// paint / `WidgetInfo::slider` pattern as [`overview`], but scoped to
/// `window` (the current [`DetailWindow`]'s bounds) rather than the whole
/// track, named `waveform-detail` with description `waveform-detail-window
/// { $start } { $end }` (contracts/ui-waveform.md §1). Returns the
/// [`WaveformResponse`] and at most one [`WaveformEvent`] — seeks, zoom,
/// and pan alike — for the caller to apply; this module never touches
/// `PlaybackController` or `DetailWindow` itself. `overlays` is called
/// after the peaks and before the playhead, same as [`overview`] (006,
/// research R16, contracts/ui-markers.md §5).
#[allow(clippy::too_many_arguments)]
pub fn detail(
    ui: &mut Ui,
    window: Range<u64>,
    sample_rate: u32,
    playhead: u64,
    previewing: bool,
    enabled: bool,
    paint_data: &WaveformPaint<'_>,
    overlays: &mut dyn FnMut(&Painter, &TimeSpace),
) -> (WaveformResponse, Option<WaveformEvent>) {
    let width = ui.available_width();
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, DETAIL_HEIGHT), sense);
    let space = TimeSpace::new(rect, window.clone(), sample_rate);

    if ui.is_rect_visible(rect) {
        paint::paint(ui.painter(), &space, ui.visuals(), paint_data);
        overlays(ui.painter(), &space);
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

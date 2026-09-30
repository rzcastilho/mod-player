// SPDX-License-Identifier: MIT OR Apache-2.0

//! Waveform painting: column folding from a `PeakLevel`, filled bars,
//! placeholder bands for absent/pending buckets, the playhead line, and
//! the "analysis unavailable" label — theme tokens only
//! (005-now-playing-waveform, research R10, contracts/ui-waveform.md §4).

use std::ops::Range;

use egui::{Align2, Color32, Painter, Pos2, Rect, Stroke, Visuals, pos2};
use modplayer_core::{AnalysisStatus, WaveformPeaks};

use crate::theme;

use super::coords::TimeSpace;

/// One pixel column's paint decision (contracts/ui-waveform.md §4): the
/// buckets overlapping that pixel's frame range are either all present
/// (`Present`, a `min..max` bar plus an `rms` average) or not
/// (`Placeholder`) — "no special case" for a multi-gap store, research
/// R10. `rms` is the max of the column's buckets' `rms` (022-waveform-
/// legibility, R6; data-model.md §4), the same fold `min`/`max` already
/// use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnPaint {
    Present { min: i8, max: i8, rms: u8 },
    Placeholder,
}

/// Decide every column's paint value across `width` pixel columns of
/// `space`, from the ladder level `space.frames_per_pixel()` selects
/// (contracts/ui-waveform.md §4, data-model.md §3.2). Pure — no egui
/// painting — so it is directly unit-testable and reusable by both the
/// paint routine and tests that assert on placeholder placement (T036).
pub fn waveform_columns(
    space: &TimeSpace,
    peaks: &WaveformPeaks,
    width: usize,
) -> Vec<ColumnPaint> {
    if width == 0 {
        return Vec::new();
    }
    let level = peaks.level_for(space.frames_per_pixel());
    let frames_per_bucket = u64::from(level.frames_per_bucket.max(1));
    let left = space.rect.left();

    (0..width)
        .map(|col| {
            let x0 = left + col as f32;
            let x1 = x0 + 1.0;
            let frame_start = space.frame_at(x0);
            let frame_end = space.frame_at(x1).max(frame_start + 1);

            let bucket_from = (frame_start / frames_per_bucket) as usize;
            if bucket_from >= level.len() {
                return ColumnPaint::Placeholder;
            }
            let bucket_to_exclusive = ((frame_end - 1) / frames_per_bucket + 1) as usize;
            let bucket_to = bucket_to_exclusive.max(bucket_from + 1).min(level.len());

            if (bucket_from..bucket_to).all(|index| level.is_present(index)) {
                let mut min_v = i8::MAX;
                let mut max_v = i8::MIN;
                let mut rms_v = 0u8;
                for index in bucket_from..bucket_to {
                    let bucket = level.buckets[index];
                    min_v = min_v.min(bucket.min);
                    max_v = max_v.max(bucket.max);
                    rms_v = rms_v.max(bucket.rms);
                }
                ColumnPaint::Present {
                    min: min_v,
                    max: max_v,
                    rms: rms_v,
                }
            } else {
                ColumnPaint::Placeholder
            }
        })
        .collect()
}

/// Everything `paint` needs about the current track's analysis, gathered
/// once by the caller (`waveform::mod`) from `PlaybackController::
/// analysis()` — kept separate from the controller type so this module
/// stays free of it.
pub struct WaveformPaint<'a> {
    pub status: AnalysisStatus,
    pub peaks: Option<&'a WaveformPeaks>,
    /// `None` while dragging is not in effect and no track plays; the
    /// playhead line is skipped when `None`.
    pub playhead: Option<u64>,
    pub unavailable_text: &'a str,
    /// The detail window's current bounds, drawn as a translucent overlay
    /// on the overview (contracts/ui-waveform.md §4); `None` for the
    /// detail view itself, which has no highlight of its own.
    pub highlight: Option<Range<u64>>,
    /// `true` while a 005 seek-drag or 006 marker-drag is in progress
    /// (022-waveform-legibility, contracts/ui-waveform-legibility.md WL5,
    /// FR-009): the hover scrub indicator is skipped this frame, on both
    /// views, regardless of the pointer's own position.
    pub hover_suppressed: bool,
}

/// Paint one waveform widget's whole rect (contracts/ui-waveform.md §4):
/// background, columns (or the "analysis unavailable" label) and the
/// detail-window highlight, using only `visuals`' own tokens (research
/// R10 — no new colour literals). The playhead line is [`playhead`],
/// called separately (006, research R16) so a marker overlay can paint
/// between the two.
pub fn paint(painter: &Painter, space: &TimeSpace, visuals: &Visuals, input: &WaveformPaint<'_>) {
    let rect = space.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    painter.rect_filled(
        rect,
        visuals.noninteractive().corner_radius,
        visuals.extreme_bg_color,
    );

    let failed_without_peaks = input.status == AnalysisStatus::Failed && input.peaks.is_none();
    if failed_without_peaks {
        paint_placeholder_band(painter, rect, visuals);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            input.unavailable_text,
            // 014-design-tokens-and-type-scale (US3, T043): the `mono`
            // role's fixed size replaces the height-proportional literal
            // (FR-018a's font-size-literal removal).
            theme::mono_font_id(),
            visuals.weak_text_color(),
        );
    } else if let Some(peaks) = input.peaks {
        let width = rect.width().round().max(0.0) as usize;
        let columns = waveform_columns(space, peaks, width);
        paint_columns(painter, rect, space, visuals, input.playhead, &columns);
    } else {
        // `Pending` (no snapshot yet): the whole area is placeholder.
        paint_placeholder_band(painter, rect, visuals);
    }

    if let Some(window) = &input.highlight {
        let x0 = space.x_of(window.start);
        let x1 = space.x_of(window.end);
        let highlight_rect = Rect::from_min_max(
            Pos2::new(x0.min(x1), rect.top()),
            Pos2::new(x0.max(x1).max(x0 + 1.0), rect.bottom()),
        );
        painter.rect_filled(
            highlight_rect,
            0.0,
            visuals.selection.bg_fill.gamma_multiply(0.25),
        );
    }
}

/// Draw the playhead line, if any (022-waveform-legibility, contracts/
/// ui-waveform-legibility.md WL3, FR-003, FR-019): split out of [`paint`]
/// so the caller can run the `overlays` hook (marker lines, the
/// loop-region span) *between* the peaks/highlight and the playhead — the
/// playhead always paints on top. Two strokes, casing then core, rather
/// than 005/006's single `strong_text_color` line: because one of
/// `playhead_core`/`playhead_casing` is near-black and the other
/// near-white in every appearance, at least one always clears ≥ 3:1
/// against any backdrop (data-model.md §6, SC-001) — a same-colour single
/// stroke could not guarantee that over every fill/backdrop combination.
pub fn playhead(painter: &Painter, space: &TimeSpace, playhead: Option<u64>, visuals: &Visuals) {
    let rect = space.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    if let Some(frame) = playhead {
        let x = space.x_of(frame);
        let points = [pos2(x, rect.top()), pos2(x, rect.bottom())];
        let tokens = theme::waveform_roles(theme::roles(visuals));
        painter.line_segment(
            points,
            Stroke::new(
                theme::waveform::PLAYHEAD_CASING_WIDTH,
                tokens.playhead_casing,
            ),
        );
        painter.line_segment(
            points,
            Stroke::new(theme::waveform::PLAYHEAD_CORE_WIDTH, tokens.playhead_core),
        );
    }
}

/// Paint every column (022-waveform-legibility, contracts/
/// ui-waveform-legibility.md WL2, FR-001, FR-002, R6/R7): each `Present`
/// column draws an outer peak bar and, when tall enough, an inner ±RMS
/// average band clipped to it — in `played_*` tones left of the displayed
/// `playhead` (drag-preview-aware: this is `WaveformPaint::playhead`,
/// whatever value the caller already resolved for the frame) or
/// `unplayed_*` tones at/right of it. `Placeholder` columns are unchanged
/// (005) and carry no played/unplayed treatment.
fn paint_columns(
    painter: &Painter,
    rect: Rect,
    space: &TimeSpace,
    visuals: &Visuals,
    playhead: Option<u64>,
    columns: &[ColumnPaint],
) {
    let mid_y = rect.center().y;
    let half_height = rect.height() / 2.0;
    let tokens = theme::waveform_roles(theme::roles(visuals));
    // WL2: "Played iff `playhead` is `Some(p)` and `col_left + 0.5 <
    // space.x_of(p)`; else unplayed" — precompute the pixel boundary once
    // rather than re-mapping frame-to-x per column.
    let playhead_x = playhead.map(|frame| space.x_of(frame));

    for (col, decision) in columns.iter().enumerate() {
        let x0 = rect.left() + col as f32;
        let x1 = x0 + 1.0;
        match decision {
            ColumnPaint::Present { min, max, rms } => {
                let played = playhead_x.is_some_and(|px| x0 + 0.5 < px);
                let (peak_color, average_color) = if played {
                    (tokens.played_peak, tokens.played_average)
                } else {
                    (tokens.unplayed_peak, tokens.unplayed_average)
                };
                let min_f = f32::from(*min) / 127.0;
                let max_f = f32::from(*max) / 127.0;
                let (low, high) = if min_f <= max_f {
                    (min_f, max_f)
                } else {
                    (max_f, min_f)
                };
                let y_top = mid_y - high * half_height;
                let y_bottom = mid_y - low * half_height;
                // A silent/flat bucket (`min == max`) still draws a visible
                // hairline rather than nothing.
                let (y_top, y_bottom) = if (y_bottom - y_top).abs() < 1.0 {
                    (mid_y - 0.5, mid_y + 0.5)
                } else {
                    (y_top, y_bottom)
                };
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(x0, y_top), Pos2::new(x1, y_bottom)),
                    0.0,
                    peak_color,
                );

                // Average band: `[mid - rms*h, mid + rms*h] ∩ peak rect`,
                // drawn only if the clipped result is at least 1px tall
                // (WL2).
                let rms_f = f32::from(*rms) / 127.0;
                if rms_f > 0.0 {
                    let band_top = (mid_y - rms_f * half_height).max(y_top);
                    let band_bottom = (mid_y + rms_f * half_height).min(y_bottom);
                    if band_bottom - band_top >= 1.0 {
                        painter.rect_filled(
                            Rect::from_min_max(Pos2::new(x0, band_top), Pos2::new(x1, band_bottom)),
                            0.0,
                            average_color,
                        );
                    }
                }
            }
            ColumnPaint::Placeholder => {
                paint_placeholder_column(painter, x0, x1, rect, visuals);
            }
        }
    }
}

/// A dimmed band at 40% height, centred vertically — the "undecoded
/// region" placeholder (research R10, contracts/ui-waveform.md §4).
fn paint_placeholder_band(painter: &Painter, rect: Rect, visuals: &Visuals) {
    let half_height = rect.height() * 0.2;
    let band = Rect::from_min_max(
        Pos2::new(rect.left(), rect.center().y - half_height),
        Pos2::new(rect.right(), rect.center().y + half_height),
    );
    painter.rect_filled(band, 0.0, visuals.weak_text_color().gamma_multiply(0.4));
}

fn paint_placeholder_column(painter: &Painter, x0: f32, x1: f32, rect: Rect, visuals: &Visuals) {
    let half_height = rect.height() * 0.2;
    let band = Rect::from_min_max(
        Pos2::new(x0, rect.center().y - half_height),
        Pos2::new(x1, rect.center().y + half_height),
    );
    painter.rect_filled(band, 0.0, muted_placeholder_color(visuals));
}

fn muted_placeholder_color(visuals: &Visuals) -> Color32 {
    visuals.weak_text_color().gamma_multiply(0.4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;
    use modplayer_audio_source::PeakBucket;
    use modplayer_core::PeakLevel;

    fn space(width: f32) -> TimeSpace {
        TimeSpace::new(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(width, 40.0)),
            0..1_000,
            44_100,
        )
    }

    fn peaks_with_gap() -> WaveformPeaks {
        // frames_per_bucket small enough that 1_000 frames over a 10px
        // width picks level 0 (100 frames/pixel < 128).
        let buckets: Vec<PeakBucket> = (0..100)
            .map(|i| PeakBucket {
                min: -i as i8,
                max: i as i8,
                rms: i as u8,
            })
            .collect();
        // Leave a gap: buckets 40..60 not present.
        let present = (0..100).filter(|i| !(40..60).contains(i));
        let level = PeakLevel::partial(10, buckets, present);
        WaveformPeaks {
            sample_rate: 44_100,
            len_frames: 1_000,
            levels: vec![level],
        }
    }

    #[test]
    fn present_buckets_yield_present_columns_absent_yield_placeholder() {
        let peaks = peaks_with_gap();
        let space = space(10.0);
        let columns = waveform_columns(&space, &peaks, 10);
        assert_eq!(columns.len(), 10);
        // Each column covers 100 frames = 10 level-0 buckets; column 4
        // (frames 400..500, buckets 40..50) falls entirely inside the gap.
        assert_eq!(columns[4], ColumnPaint::Placeholder);
        assert!(matches!(columns[0], ColumnPaint::Present { .. }));
        assert!(matches!(columns[9], ColumnPaint::Present { .. }));
    }

    #[test]
    fn empty_width_yields_no_columns() {
        let peaks = peaks_with_gap();
        let space = space(10.0);
        assert!(waveform_columns(&space, &peaks, 0).is_empty());
    }
}

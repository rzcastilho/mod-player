// SPDX-License-Identifier: MIT OR Apache-2.0

//! `TimeSpace`: pure pixel-to-frame coordinate mapping for the waveform
//! widgets, no egui state beyond a `Rect` (005-now-playing-waveform,
//! data-model.md §5.3, research R12).

use std::ops::Range;

use egui::Rect;

/// The mapping between a widget's screen `rect` and the track-frame
/// `window` it currently shows (data-model.md §5.3). Constructed fresh on
/// every draw from the widget's current window; returned to the caller in
/// `WaveformResponse` so later overlays (markers, loops, plugins — FR-014)
/// paint through the same space without a widget redesign.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeSpace {
    pub rect: Rect,
    pub window: Range<u64>,
    pub sample_rate: u32,
}

impl TimeSpace {
    pub fn new(rect: Rect, window: Range<u64>, sample_rate: u32) -> Self {
        Self {
            rect,
            window,
            sample_rate,
        }
    }

    /// The window's frame span (`0` for a degenerate/empty window).
    fn span(&self) -> u64 {
        self.window.end.saturating_sub(self.window.start)
    }

    /// How many track frames one pixel of `rect`'s width covers. `1.0` for
    /// a zero-width rect or an empty window (never zero/NaN, so callers can
    /// divide by it — e.g. `WaveformPeaks::level_for`).
    pub fn frames_per_pixel(&self) -> f64 {
        let width = f64::from(self.rect.width());
        if width <= 0.0 {
            return 1.0;
        }
        (self.span() as f64 / width).max(f64::MIN_POSITIVE)
    }

    /// The x coordinate `frame` maps to, clamped to `window` first (a frame
    /// outside the window still returns a finite, on-rect x — at whichever
    /// edge it overshot).
    pub fn x_of(&self, frame: u64) -> f32 {
        let span = self.span();
        if span == 0 {
            return self.rect.left();
        }
        let clamped = frame.clamp(self.window.start, self.window.end);
        let fraction = (clamped - self.window.start) as f64 / span as f64;
        self.rect.left() + (fraction as f32) * self.rect.width()
    }

    /// The frame `x` maps to, clamped to `window` (data-model.md §5.3).
    pub fn frame_at(&self, x: f32) -> u64 {
        let span = self.span();
        if span == 0 {
            return self.window.start;
        }
        let width = self.rect.width();
        if width <= 0.0 {
            return self.window.start;
        }
        let fraction = f64::from((x - self.rect.left()) / width).clamp(0.0, 1.0);
        let offset = (fraction * span as f64).round() as u64;
        self.window.start + offset.min(span)
    }

    /// The window this space currently shows (data-model.md §5.3) — for the
    /// overview this is always `0..track_len_frames`; the detail widget
    /// (US2) narrows it.
    pub fn visible_window(&self) -> Range<u64> {
        self.window.clone()
    }
}

/// What `waveform::overview`/`waveform::detail` return alongside the
/// `egui::Response` (data-model.md §5.3): the exact space the widget just
/// painted with, for a caller/overlay to map frames to screen positions.
#[derive(Debug, Clone)]
pub struct WaveformResponse {
    pub response: egui::Response,
    pub space: TimeSpace,
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn space(window: Range<u64>) -> TimeSpace {
        TimeSpace::new(
            Rect::from_min_max(pos2(100.0, 0.0), pos2(300.0, 40.0)),
            window,
            44_100,
        )
    }

    #[test]
    fn x_of_start_and_end_land_on_the_rect_edges() {
        let space = space(0..200);
        assert_eq!(space.x_of(0), 100.0);
        assert_eq!(space.x_of(200), 300.0);
        assert_eq!(space.x_of(100), 200.0);
    }

    #[test]
    fn x_of_clamps_frames_outside_the_window() {
        let space = space(0..200);
        assert_eq!(space.x_of(10_000), 300.0);
    }

    #[test]
    fn frame_at_start_and_end_land_on_the_window_edges() {
        let space = space(0..200);
        assert_eq!(space.frame_at(100.0), 0);
        assert_eq!(space.frame_at(300.0), 200);
        assert_eq!(space.frame_at(200.0), 100);
    }

    #[test]
    fn frame_at_clamps_x_outside_the_rect() {
        let space = space(0..200);
        assert_eq!(space.frame_at(0.0), 0);
        assert_eq!(space.frame_at(10_000.0), 200);
    }

    #[test]
    fn round_trip_is_within_one_pixel() {
        let space = space(0..44_100 * 180);
        for frame in [0, 1, 44_100, 44_100 * 90, 44_100 * 179, 44_100 * 180] {
            let x = space.x_of(frame);
            let back = space.frame_at(x);
            let fpp = space.frames_per_pixel();
            let delta = frame.abs_diff(back) as f64;
            assert!(
                delta <= fpp.max(1.0),
                "frame {frame} round-tripped to {back} (fpp {fpp})"
            );
        }
    }

    #[test]
    fn frames_per_pixel_never_zero_or_nan_for_a_degenerate_rect() {
        let degenerate = TimeSpace::new(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(0.0, 0.0)),
            0..0,
            44_100,
        );
        let fpp = degenerate.frames_per_pixel();
        assert!(fpp.is_finite() && fpp > 0.0);
    }
}

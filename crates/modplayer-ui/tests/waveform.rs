// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T040 (US2, FR-014, Constitution VIII): `TimeSpace`'s pixel<->frame
//! mapping round-trips within one pixel for arbitrary windows/rects — the
//! coordinate space every later overlay (markers, loops, plugins) attaches
//! through (data-model.md §5.3). `DetailWindow`'s own pure-method tests
//! live alongside its implementation in `src/waveform/state.rs` (T039).

use egui::{Rect, pos2};
use modplayer_ui::waveform::TimeSpace;
use proptest::prelude::*;

proptest! {
    #[test]
    fn time_space_round_trips_within_one_pixel(
        left in 0.0f32..10_000.0,
        width in 1.0f32..4_000.0,
        window_start in 0u64..(3_600 * 192_000),
        span in 1u64..(3_600 * 192_000),
        sample_rate in 8_000u32..192_000,
        frame_offset in 0.0f64..1.0,
    ) {
        let rect = Rect::from_min_size(pos2(left, 0.0), egui::vec2(width, 40.0));
        let window = window_start..window_start.saturating_add(span);
        let space = TimeSpace::new(rect, window.clone(), sample_rate);

        let frame = window.start + ((frame_offset * span as f64).round() as u64).min(span);
        let x = space.x_of(frame);
        let back = space.frame_at(x);
        let fpp = space.frames_per_pixel();

        prop_assert!(x.is_finite());
        prop_assert!(
            (frame.abs_diff(back) as f64) <= fpp.max(1.0),
            "frame {frame} round-tripped to {back} (fpp {fpp})"
        );
    }
}

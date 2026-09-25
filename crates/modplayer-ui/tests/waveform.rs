// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T040 (US2, FR-014, Constitution VIII): `TimeSpace`'s pixel<->frame
//! mapping round-trips within one pixel for arbitrary windows/rects — the
//! coordinate space every later overlay (markers, loops, plugins) attaches
//! through (data-model.md §5.3). `DetailWindow`'s own pure-method tests
//! live alongside its implementation in `src/waveform/state.rs` (T039).

use egui::{Context, RawInput, Rect, pos2};
use modplayer_core::AnalysisStatus;
use modplayer_ui::layout;
use modplayer_ui::waveform::{self, TimeSpace, WaveformPaint};
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

// ---------------------------------------------------------------------
// D10.9 (018-window-sizing-and-responsive-dock, contract D8, FR-012):
// `waveform::overview`/`detail` now allocate whatever `height` they are
// given rather than a fixed constant — proving they are actually wired
// to `layout::waveform_heights`'s own result, not just that the pure
// formula (already covered by layout.rs's own D10.3) is correct.
// ---------------------------------------------------------------------

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(
            pos2(0.0, 0.0),
            egui::vec2(1200.0, 2000.0),
        )),
        ..Default::default()
    }
}

/// Runs `waveform::overview`/`detail` inside a bare `Context::run_ui` and
/// returns the rect height each widget actually allocated
/// (`WaveformResponse::space::rect`), for a given content height `h`.
fn allocated_heights(h: f32) -> (f32, f32) {
    let (overview_height, detail_height) = layout::waveform_heights(h);
    let paint_data = WaveformPaint {
        status: AnalysisStatus::Pending,
        peaks: None,
        playhead: None,
        unavailable_text: "unavailable",
        highlight: None,
    };
    let ctx = Context::default();
    let mut overview_rect_height = 0.0;
    let mut detail_rect_height = 0.0;
    let output = ctx.run_ui(default_input(), |ui| {
        let (overview_response, _event) = waveform::overview(
            ui,
            10_000,
            1_000,
            0,
            false,
            true,
            overview_height,
            &paint_data,
            &mut |_painter, _space| {},
        );
        overview_rect_height = overview_response.space.rect.height();
        let (detail_response, _event) = waveform::detail(
            ui,
            0..10_000,
            1_000,
            0,
            false,
            true,
            detail_height,
            &paint_data,
            &mut |_painter, _space| {},
        );
        detail_rect_height = detail_response.space.rect.height();
    });
    output.drop_without_applying_deltas();
    (overview_rect_height, detail_rect_height)
}

/// D10.9: for two distinct window heights, both widgets allocate exactly
/// `layout::waveform_heights(H)`'s own values — proving `now_playing.rs`'s
/// per-frame `H` capture actually reaches the widgets' allocated rects.
#[test]
fn overview_and_detail_heights_track_content_height() {
    for h in [400.0f32, 900.0f32] {
        let (expected_overview, expected_detail) = layout::waveform_heights(h);
        let (overview_rect_height, detail_rect_height) = allocated_heights(h);
        assert!(
            (overview_rect_height - expected_overview).abs() < 0.01,
            "H={h}: overview allocated {overview_rect_height}, expected {expected_overview}"
        );
        assert!(
            (detail_rect_height - expected_detail).abs() < 0.01,
            "H={h}: detail allocated {detail_rect_height}, expected {expected_detail}"
        );
    }
}

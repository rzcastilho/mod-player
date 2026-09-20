// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T083 (US3, 011-plugin-ui-contributions, contracts/overlays-settings-
//! notify.md §1.2 "O" rules): `plugin_overlays::paint` re-projects a
//! primitive's track-time position through whatever `TimeSpace` it is
//! given (O6/O10, SC-003 — no plugin call happens here at all, proving
//! the re-projection is a pure function of the space), a `label`
//! primitive draws only in the detail view (O7), a paint call sits
//! between markers/loop and the playhead in the shape stream when driven
//! in that order (O5, mirrors `now_playing.rs`'s own closure sequence,
//! T089), and a primitive beyond the currently-known track length is
//! skipped outright rather than clamped to the edge (O6). Mirrors
//! `markers.rs`'s own `painted_shapes` direct-paint-function technique.

use std::ops::Range;

use egui::{Context, Painter, Pos2, RawInput, Rect, Sense, Shape, Vec2, vec2};
use modplayer_audio_source::TrackId;
use modplayer_capability_gateway::ui::{GlyphRef, HostGlyph, OverlayColor, OverlayPrimitive, UiId};
use modplayer_core::markers::TrackMarkers;
use modplayer_core::plugins::ui::assets::PluginAssets;
use modplayer_core::plugins::{OverlayLayer, PluginId};
use modplayer_ui::plugin_overlays::{self, ViewKind};
use modplayer_ui::waveform::TimeSpace;

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 2000.0))),
        ..Default::default()
    }
}

fn id(s: &str) -> UiId {
    UiId::parse(s).unwrap_or_else(|| unreachable!("{s:?} must be valid grammar"))
}

/// Runs `paint` against a freshly allocated rect/`TimeSpace` (with room
/// reserved above it, mirroring `now_playing.rs`'s own lane-height
/// `ui.add_space` before `waveform::overview`/`detail`, T089) and returns
/// the raw (pre-tessellation) shapes it produced.
fn painted_shapes(
    window: Range<u64>,
    sample_rate: u32,
    paint: impl FnOnce(&Painter, &TimeSpace),
) -> Vec<Shape> {
    let ctx = Context::default();
    let mut paint = Some(paint);
    let output = ctx.run_ui(default_input(), |ui| {
        ui.add_space(plugin_overlays::LANE_HEIGHT + 8.0);
        let (rect, _response) = ui.allocate_exact_size(Vec2::new(400.0, 72.0), Sense::hover());
        let space = TimeSpace::new(rect, window.clone(), sample_rate);
        if let Some(paint) = paint.take() {
            paint(ui.painter(), &space);
        }
    });
    let shapes = output
        .shapes
        .iter()
        .map(|clipped| clipped.shape.clone())
        .collect();
    output.drop_without_applying_deltas();
    shapes
}

fn layer(primitives: Vec<OverlayPrimitive>) -> OverlayLayer {
    OverlayLayer {
        plugin_seq: 0,
        plugin: PluginId(0),
        primitives,
        glyph_assets: PluginAssets::default(),
    }
}

fn line(s: &str, at_ms: u64) -> OverlayPrimitive {
    OverlayPrimitive::Line {
        id: id(s),
        at_ms,
        color: OverlayColor::Accent,
    }
}

/// O6/O10: the same primitive, painted through two different `TimeSpace`s
/// (one "zoomed", one "scrolled") re-projects to exactly the x each
/// space's own `x_of` maps its track-time frame to — proving the paint
/// reads the space fresh, never a cached screen position, with zero
/// plugin involvement (this test never touches a controller or plugin at
/// all).
#[test]
fn primitives_reproject_under_zoom_and_scroll() {
    const SAMPLE_RATE: u32 = 1_000; // 1 frame per ms, for simple arithmetic
    let layers = vec![layer(vec![line("l1", 5_000)])];

    for window in [0..20_000u64, 4_000..6_000u64, 4_500..5_500u64] {
        let expected_x = {
            let rect = Rect::from_min_size(Pos2::new(0.0, 100.0), Vec2::new(400.0, 72.0));
            TimeSpace::new(rect, window.clone(), SAMPLE_RATE).x_of(5_000)
        };
        let shapes = painted_shapes(window, SAMPLE_RATE, |painter, space| {
            plugin_overlays::paint(painter, space, 20_000, ViewKind::Overview, &layers);
        });
        let line_shapes: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::LineSegment { points, .. } => Some(*points),
                _ => None,
            })
            .collect();
        assert_eq!(
            line_shapes.len(),
            1,
            "exactly one Line primitive must paint one segment"
        );
        assert!(
            (line_shapes[0][0].x - expected_x).abs() < 0.01,
            "expected x={expected_x}, got {:?}",
            line_shapes[0]
        );
    }
}

/// O7: a `label` primitive draws in the detail view only, never the
/// overview.
#[test]
fn label_only_on_detail() {
    let layers = vec![layer(vec![OverlayPrimitive::Label {
        id: id("l1"),
        at_ms: 1_000,
        text: "Verse 1".to_string(),
        color: OverlayColor::Accent,
    }])];

    for (view, expect_text) in [(ViewKind::Overview, false), (ViewKind::Detail, true)] {
        let shapes = painted_shapes(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, view, &layers);
        });
        let has_text = shapes.iter().any(|shape| matches!(shape, Shape::Text(_)));
        assert_eq!(has_text, expect_text, "view={view:?}");
    }
}

/// O5: driven in `now_playing.rs`'s own closure order — markers/loop
/// first, this module's `paint` second, the playhead last — the
/// resulting shape stream preserves exactly that order (egui appends to
/// one layer's shape list in call order), so a plugin's line always
/// paints over a host marker and under the playhead.
#[test]
fn above_markers_below_playhead_order() {
    const SAMPLE_RATE: u32 = 1_000;
    let track_id = TrackId::new("spotify:track:overlay-order").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(track_id, SAMPLE_RATE, 20_000);
    markers
        .add_point(2_000)
        .unwrap_or_else(|e| unreachable!("add_point: {e:?}"));
    let layers = vec![layer(vec![line("l1", 5_000)])];

    let shapes = painted_shapes(0..20_000, SAMPLE_RATE, |painter, space| {
        modplayer_ui::markers::paint_overlay(painter, space, Some(&markers), 0, None);
        plugin_overlays::paint(painter, space, 20_000, ViewKind::Overview, &layers);
        modplayer_ui::waveform::paint::playhead(
            painter,
            space,
            Some(8_000),
            &egui::Visuals::dark(),
        );
    });

    // `add_point`'s default colour is palette index 1 ("point default",
    // `theme.rs`'s own doc note on `MARKER_PALETTE`).
    let marker_color =
        modplayer_ui::theme::marker_color(modplayer_core::markers::PaletteIndex::new(1));
    let plugin_color =
        modplayer_ui::theme::overlay_color(OverlayColor::Accent, &egui::Visuals::dark());
    let playhead_color = egui::Visuals::dark().strong_text_color();

    let index_of = |color: egui::Color32| {
        shapes
            .iter()
            .position(
                |shape| matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == color),
            )
            .unwrap_or_else(|| unreachable!("no LineSegment with color {color:?} in {shapes:?}"))
    };
    let marker_idx = index_of(marker_color);
    let plugin_idx = index_of(plugin_color);
    let playhead_idx = index_of(playhead_color);

    assert!(
        marker_idx < plugin_idx,
        "the plugin's line must paint after (over) the marker's"
    );
    assert!(
        plugin_idx < playhead_idx,
        "the playhead must paint after (over) the plugin's line"
    );
}

/// O6: a primitive whose position is beyond the currently-known track
/// length is skipped outright (never clamped to the edge, unlike
/// `TimeSpace::x_of`'s own clamping for a frame outside the *window*).
#[test]
fn beyond_duration_not_drawn() {
    let layers = vec![layer(vec![line("beyond", 6_000)])];
    let shapes = painted_shapes(0..10_000, 1_000, |painter, space| {
        // The window covers 0..10_000ms, but the track itself is only
        // known to be 5_000ms long so far (streaming) — 6_000ms is past
        // that, even though it's still inside the window.
        plugin_overlays::paint(painter, space, 5_000, ViewKind::Overview, &layers);
    });
    let line_count = shapes
        .iter()
        .filter(|shape| matches!(shape, Shape::LineSegment { .. }))
        .count();
    assert_eq!(
        line_count, 0,
        "a primitive beyond len_frames must not paint at all"
    );
}

/// O12: a host glyph paints without a manifest asset, at the lane's
/// vertical centre — proves `theme::paint_host_glyph` is actually wired
/// through `paint`'s own `Glyph` arm (not just unit-tested in isolation).
#[test]
fn host_glyph_paints_without_an_asset() {
    let layers = vec![layer(vec![OverlayPrimitive::Glyph {
        id: id("g1"),
        at_ms: 1_000,
        icon: GlyphRef::Host(HostGlyph::Star),
        color: OverlayColor::Accent,
    }])];
    let shapes = painted_shapes(0..10_000, 1_000, |painter, space| {
        plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &layers);
    });
    assert!(!shapes.is_empty(), "a host glyph must paint something");
}

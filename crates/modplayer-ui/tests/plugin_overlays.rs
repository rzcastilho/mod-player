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

use egui::{Color32, Context, Painter, Pos2, RawInput, Rect, Sense, Shape, Stroke, Vec2, vec2};
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
    painted_shapes_on(&Context::default(), window, sample_rate, paint)
}

/// As [`painted_shapes`], but against a caller-supplied `Context` — used
/// by [`painted_shapes_high_contrast`] (017-high-contrast-appearance) so
/// `paint`'s own `ctx.style_of(ctx.theme())` read resolves to the
/// high-contrast tables.
fn painted_shapes_on(
    ctx: &Context,
    window: Range<u64>,
    sample_rate: u32,
    paint: impl FnOnce(&Painter, &TimeSpace),
) -> Vec<Shape> {
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

/// As [`painted_shapes`], with the high-contrast token style installed
/// on the `Context` first (017-high-contrast-appearance) — mirrors what
/// `App::new`/`App::ui` do once `controller.high_contrast()` is `true`.
fn painted_shapes_high_contrast(
    window: Range<u64>,
    sample_rate: u32,
    paint: impl FnOnce(&Painter, &TimeSpace),
) -> Vec<Shape> {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens_for(&ctx, true);
    painted_shapes_on(&ctx, window, sample_rate, paint)
}

/// The active high-contrast style's marker outline (`Some` — high
/// contrast always produces one), for comparing a plugin overlay's
/// outline against the host's own value (017-high-contrast-appearance,
/// `M12`/`P4`).
fn high_contrast_outline() -> Stroke {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens_for(&ctx, true);
    let visuals = ctx.style_of(ctx.theme()).visuals.clone();
    modplayer_ui::theme::markers::marker_outline(modplayer_ui::theme::roles(&visuals))
        .unwrap_or_else(|| unreachable!("high contrast must produce a marker outline"))
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
        modplayer_ui::markers::paint_overlay(
            painter,
            space,
            Some(&markers),
            0,
            None,
            modplayer_ui::theme::roles(&egui::Visuals::dark()),
        );
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

// ---------------------------------------------------------------------
// 017-high-contrast-appearance (T009, contracts/marker-outline.md M11):
// `overlay_outline`'s value only — no paint call site is asserted here
// (that is M12, Phase 6).
// ---------------------------------------------------------------------

/// M11: `Some(1px text_primary)` for `Positive`/`Warning` only when
/// `visuals` is a high-contrast style; `None` for `Accent`/`Secondary`/
/// `Neutral` in every style.
#[test]
fn overlay_outline_covers_exactly_the_palette_tokens() {
    use modplayer_ui::theme::markers::overlay_outline;
    use modplayer_ui::theme::style::build_style;

    let normal_visuals = [egui::Visuals::light(), egui::Visuals::dark()];
    for visuals in &normal_visuals {
        for token in [
            OverlayColor::Accent,
            OverlayColor::Secondary,
            OverlayColor::Neutral,
            OverlayColor::Positive,
            OverlayColor::Warning,
        ] {
            assert_eq!(
                overlay_outline(token, visuals),
                None,
                "{token:?} must be None outside a built high-contrast style"
            );
        }
    }

    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let visuals = build_style(theme, true).visuals;
        for token in [OverlayColor::Positive, OverlayColor::Warning] {
            assert!(
                overlay_outline(token, &visuals).is_some(),
                "{token:?} must outline in high contrast"
            );
        }
        for token in [
            OverlayColor::Accent,
            OverlayColor::Secondary,
            OverlayColor::Neutral,
        ] {
            assert_eq!(
                overlay_outline(token, &visuals),
                None,
                "{token:?} must never outline (FR-011 last sentence)"
            );
        }
    }
}

// ---------------------------------------------------------------------
// 017-high-contrast-appearance (T035, Phase 6, US4, contracts/marker-
// outline.md §3 M12, P4, P5): the paint-**site** coverage — `overlay_
// outline`'s *value* is M11 above; this is whether `paint`'s four
// primitive arms actually call it.
// ---------------------------------------------------------------------

fn line_token(s: &str, at_ms: u64, color: OverlayColor) -> OverlayPrimitive {
    OverlayPrimitive::Line {
        id: id(s),
        at_ms,
        color,
    }
}

fn region_token(s: &str, from_ms: u64, to_ms: u64, color: OverlayColor) -> OverlayPrimitive {
    OverlayPrimitive::Region {
        id: id(s),
        from_ms,
        to_ms,
        color,
    }
}

fn label_token(s: &str, at_ms: u64, text: &str, color: OverlayColor) -> OverlayPrimitive {
    OverlayPrimitive::Label {
        id: id(s),
        at_ms,
        text: text.to_string(),
        color,
    }
}

fn glyph_host_token(s: &str, at_ms: u64, icon: HostGlyph, color: OverlayColor) -> OverlayPrimitive {
    OverlayPrimitive::Glyph {
        id: id(s),
        at_ms,
        icon: GlyphRef::Host(icon),
        color,
    }
}

/// `true` if any painted shape carries `outline` — a `LineSegment`/`Rect`
/// stroke, a `Circle`'s casing fill, or a `Text` section's colour (the
/// four outline forms O7-O10 use, mirroring `markers.rs`'s own
/// `has_outline_shapes`).
fn has_overlay_outline(shapes: &[Shape], outline: Color32) -> bool {
    shapes.iter().any(|shape| match shape {
        Shape::LineSegment { stroke, .. } => stroke.color == outline && stroke.width > 1.0,
        Shape::Rect(r) => r.stroke.width > 0.0 && r.stroke.color == outline,
        Shape::Circle(c) => c.fill == outline,
        Shape::Text(t) => t
            .galley
            .job
            .sections
            .iter()
            .any(|s| s.format.color == outline),
        _ => false,
    })
}

/// How many shapes of each primitive's own kind painted — used to prove
/// "no *extra* shape appears" (M12's `Accent`/`Secondary`/`Neutral`/O11
/// half) without relying on colour comparison, which a role token's own
/// resolved colour (`weak_text_color()` promotes onto `text_primary` in
/// high contrast, `H13`) can coincidentally equal the outline colour and
/// falsely look like a "gain".
fn shape_kind_count(shapes: &[Shape]) -> (usize, usize, usize, usize) {
    let mut lines = 0;
    let mut rects = 0;
    let mut circles = 0;
    let mut texts = 0;
    for shape in shapes {
        match shape {
            Shape::LineSegment { .. } => lines += 1,
            Shape::Rect(_) => rects += 1,
            Shape::Circle(_) => circles += 1,
            Shape::Text(_) => texts += 1,
            _ => {}
        }
    }
    (lines, rects, circles, texts)
}

/// M12: with high contrast on, a `Line` (O7), `Region` (O8), `Label`
/// (O9) and host `Glyph` (O10) primitive coloured `Positive`/`Warning`
/// each gain the outline; with high contrast off, none does.
#[test]
fn plugin_palette_primitives_gain_an_outline() {
    let outline = high_contrast_outline();
    for token in [OverlayColor::Positive, OverlayColor::Warning] {
        let line_layers = vec![layer(vec![line_token("l", 5_000, token)])];
        let region_layers = vec![layer(vec![region_token("r", 1_000, 3_000, token)])];
        let label_layers = vec![layer(vec![label_token("t", 1_000, "x", token)])];
        let glyph_layers = vec![layer(vec![glyph_host_token(
            "g",
            1_000,
            HostGlyph::Dot,
            token,
        )])];

        let hc_line = painted_shapes_high_contrast(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &line_layers);
        });
        assert!(
            has_overlay_outline(&hc_line, outline.color),
            "{token:?} Line must gain the outline in high contrast"
        );
        let normal_line = painted_shapes(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &line_layers);
        });
        assert!(
            !has_overlay_outline(&normal_line, outline.color),
            "{token:?} Line must not carry an outline outside high contrast"
        );

        let hc_region = painted_shapes_high_contrast(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &region_layers);
        });
        assert!(
            has_overlay_outline(&hc_region, outline.color),
            "{token:?} Region must gain the outline in high contrast"
        );
        let normal_region = painted_shapes(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &region_layers);
        });
        assert!(
            !has_overlay_outline(&normal_region, outline.color),
            "{token:?} Region must not carry an outline outside high contrast"
        );

        let hc_label = painted_shapes_high_contrast(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Detail, &label_layers);
        });
        assert!(
            has_overlay_outline(&hc_label, outline.color),
            "{token:?} Label must gain a halo outline in high contrast"
        );
        let normal_label = painted_shapes(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Detail, &label_layers);
        });
        assert!(
            !has_overlay_outline(&normal_label, outline.color),
            "{token:?} Label must not carry an outline outside high contrast"
        );

        let hc_glyph = painted_shapes_high_contrast(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &glyph_layers);
        });
        assert!(
            has_overlay_outline(&hc_glyph, outline.color),
            "{token:?} host Glyph must gain the outline in high contrast"
        );
        let normal_glyph = painted_shapes(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &glyph_layers);
        });
        assert!(
            !has_overlay_outline(&normal_glyph, outline.color),
            "{token:?} host Glyph must not carry an outline outside high contrast"
        );
    }
}

/// M12 (O11): `Accent`/`Secondary`/`Neutral` primitives never gain the
/// palette outline, in any style; neither does the `Package`-glyph
/// fallback (unresolved key), even coloured `Positive`. Checked by shape
/// **count** (identical normal vs. high contrast), not colour — a role
/// token's own resolved colour promotes onto `text_primary` in high
/// contrast (`H13`) and could otherwise coincidentally equal the outline
/// colour and look like a false "gain".
#[test]
fn plugin_role_primitives_do_not() {
    for token in [
        OverlayColor::Accent,
        OverlayColor::Secondary,
        OverlayColor::Neutral,
    ] {
        let layers = vec![layer(vec![
            line_token("l", 5_000, token),
            region_token("r", 1_000, 3_000, token),
            label_token("t", 1_000, "x", token),
            glyph_host_token("g", 2_000, HostGlyph::Dot, token),
        ])];
        let normal = painted_shapes(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Detail, &layers);
        });
        let hc = painted_shapes_high_contrast(0..10_000, 1_000, |painter, space| {
            plugin_overlays::paint(painter, space, 10_000, ViewKind::Detail, &layers);
        });
        assert_eq!(
            shape_kind_count(&hc),
            shape_kind_count(&normal),
            "{token:?} must never gain an extra shape (no outline)"
        );
    }

    let fallback_layers = vec![layer(vec![OverlayPrimitive::Glyph {
        id: id("g"),
        at_ms: 1_000,
        icon: GlyphRef::Package("does-not-resolve".to_string()),
        color: OverlayColor::Positive,
    }])];
    let normal = painted_shapes(0..10_000, 1_000, |painter, space| {
        plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &fallback_layers);
    });
    let hc = painted_shapes_high_contrast(0..10_000, 1_000, |painter, space| {
        plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &fallback_layers);
    });
    assert_eq!(
        shape_kind_count(&hc),
        shape_kind_count(&normal),
        "the Package-glyph fallback must never gain an extra shape (no outline)"
    );
}

/// P4: a docked plugin panel is painted from the exact same applied
/// style as host chrome. Proven here by pairing a plugin `Positive`
/// overlay's outline against the host's own `theme::markers::
/// marker_outline` value and casing rule (`casing_width`): both must be
/// byte-identical, because both resolve through the one
/// `ctx.style_of(ctx.theme())` object `apply_tokens_for` installs once
/// (FR-017) — a docked plugin panel cannot diverge from host chrome.
#[test]
fn docked_plugin_panel_matches_host_chrome() {
    let outline = high_contrast_outline();
    let casing = modplayer_ui::theme::markers::casing_width(1.0);
    let layers = vec![layer(vec![line_token("l", 5_000, OverlayColor::Positive)])];
    let shapes = painted_shapes_high_contrast(0..10_000, 1_000, |painter, space| {
        plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &layers);
    });
    let matches_host = shapes.iter().any(|shape| {
        matches!(
            shape,
            Shape::LineSegment { stroke, .. }
                if stroke.color == outline.color && (stroke.width - casing).abs() < 0.01
        )
    });
    assert!(
        matches_host,
        "a docked plugin panel's overlay outline must match the host's exactly, {shapes:?}"
    );
}

/// P5 (US4 AS-2): toggling high contrast produces no plugin-side call —
/// `paint` re-projects the identical, already-fetched `layers` slice
/// under either style; the host never re-asks the plugin just because
/// the applied style changed (the extra shape below is the outline this
/// same frame's *style* now supplies, not a new primitive).
#[test]
fn toggling_high_contrast_calls_no_plugin() {
    let layers = vec![layer(vec![line_token("l", 5_000, OverlayColor::Positive)])];
    let normal = painted_shapes(0..10_000, 1_000, |painter, space| {
        plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &layers);
    });
    let hc = painted_shapes_high_contrast(0..10_000, 1_000, |painter, space| {
        plugin_overlays::paint(painter, space, 10_000, ViewKind::Overview, &layers);
    });
    assert!(!normal.is_empty(), "the normal-mode frame must still paint");
    assert!(
        hc.len() > normal.len(),
        "the identical `layers` value paints an extra outline shape under \
         high contrast with no re-fetch from the plugin"
    );
}

// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! T049 (015-control-variants, Phase 6 User Story 4): the integration
//! suite for meter colour bands and scale marks
//! (contracts/meter-bands.md), written **before** the peak-meter/
//! level-pair segmentation it pins (T050-T051) so the red -> green
//! transition is real (Constitution VIII). Covers M6-M9 (bands) and
//! K1/K2/K5-K7 (scale marks); M1-M5 are pinned by
//! `theme::controls::tests` (Phase 2) and R1-R4 by the unmodified
//! `design_token_roles.rs`/`accessibility.rs`/`plugins_view.rs` suites.

use std::fs;
use std::path::PathBuf;

use egui::epaint::ClippedShape;
use egui::{Color32, Context, RawInput, Rect, Shape, Stroke};
use modplayer_core::LevelPair;
use modplayer_ui::theme;
use modplayer_ui::theme::Roles;
use modplayer_ui::theme::controls::{self, BAND_WARNING_DB, CEILING_MARK_WIDTH, SCALE_MARK_WIDTH};
use modplayer_ui::widgets::chain_meters::level_pair;
use modplayer_ui::widgets::peak_meter::{SCALE_MAX_DB, SCALE_MIN_DB, peak_meter};

/// A position tolerance for float comparisons on painted geometry.
const EPS: f32 = 0.05;

/// Same formula as the widgets' own private `fraction_of` (data-model.md
/// §6, K7) — duplicated here as every integration suite in this crate
/// duplicates the widget's private helpers rather than reaching into it.
fn fraction_of(db: f32) -> f32 {
    ((db - SCALE_MIN_DB) / (SCALE_MAX_DB - SCALE_MIN_DB)).clamp(0.0, 1.0)
}

/// The linear amplitude whose `20 * log10` is `db` — the inverse of the
/// widgets' own `to_db`, so a test can drive a meter to an exact dB
/// position.
fn amplitude_for_db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// The meter's own rect: the one `Shape::Rect` with a non-zero stroke and
/// `TRANSPARENT` fill, which is exactly what `Painter::rect_stroke` (the
/// meter's border, painted last) produces (`RectShape::stroke`). Reading
/// it back out of the paint output — rather than assuming a layout width
/// — keeps this suite independent of the headless test ui's available
/// width (research R16: values only, no rendering harness).
fn border_rect(shapes: &[ClippedShape]) -> Rect {
    shapes
        .iter()
        .find_map(|c| match &c.shape {
            Shape::Rect(r) if r.fill == Color32::TRANSPARENT && r.stroke.width > 0.0 => {
                Some(r.rect)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!("expected one bordered Shape::Rect (the meter's own rect_stroke)")
        })
}

/// Every `Shape::Rect` painted with exactly `color`, in paint order.
fn fill_rects(shapes: &[ClippedShape], color: Color32) -> Vec<Rect> {
    shapes
        .iter()
        .filter_map(|c| match &c.shape {
            Shape::Rect(r) if r.fill == color => Some(r.rect),
            _ => None,
        })
        .collect()
}

/// The paint-order index of the first `Shape::Rect` filled with `color`
/// (index into the full shapes list, so relative order between colours is
/// comparable — later index paints on top).
fn first_fill_index(shapes: &[ClippedShape], color: Color32) -> Option<usize> {
    shapes.iter().position(|c| match &c.shape {
        Shape::Rect(r) => r.fill == color,
        _ => false,
    })
}

/// Every `Shape::LineSegment` painted with stroke width `width`, as
/// `(x, stroke)` (every mark in this feature is a vertical tick, so
/// `points[0].x == points[1].x`).
fn lines_of_width(shapes: &[ClippedShape], width: f32) -> Vec<(f32, Stroke)> {
    shapes
        .iter()
        .filter_map(|c| match &c.shape {
            Shape::LineSegment { points, stroke } if (stroke.width - width).abs() < 1e-6 => {
                assert!(
                    (points[0].x - points[1].x).abs() < 1e-6,
                    "every mark in this feature is a vertical tick"
                );
                Some((points[0].x, *stroke))
            }
            _ => None,
        })
        .collect()
}

/// Strips `//`-comment text from every line (the same shape as
/// `interaction_states.rs::host_controls_do_not_use_widget_state`'s scan):
/// a doc comment may still *name* a removed call while explaining its
/// removal (K5, M9); only actual code is forbidden from naming it.
fn strip_line_comments(contents: &str) -> String {
    contents
        .lines()
        .map(|line| line.split("//").next().unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn nearest_x(lines: &[(f32, Stroke)], x: f32) -> (f32, Stroke) {
    *lines
        .iter()
        .min_by(|a, b| (a.0 - x).abs().total_cmp(&(b.0 - x).abs()))
        .unwrap_or_else(|| panic!("expected at least one line near x={x}, found none in {lines:?}"))
}

/// Runs `peak_meter(peak_db, ceiling_db)` headlessly and returns its
/// painted shapes, its own rect (`border_rect`) and the `Roles` it
/// resolved (light or dark, whichever `Context::default()` starts in).
fn run_peak_meter(peak_db: f32, ceiling_db: f32) -> (Vec<ClippedShape>, Rect, Roles) {
    let ctx = Context::default();
    theme::apply_tokens(&ctx);
    let mut roles_used = None;
    let output = ctx.run_ui(RawInput::default(), |ui| {
        roles_used = Some(*theme::roles(ui.visuals()));
        peak_meter(ui, amplitude_for_db(peak_db), ceiling_db);
    });
    let shapes = output.shapes.clone();
    let rect = border_rect(&shapes);
    output.drop_without_applying_deltas();
    (
        shapes,
        rect,
        roles_used.unwrap_or_else(|| panic!("peak_meter must run its closure")),
    )
}

/// Runs `level_pair(peak_db, rms_db)` headlessly, same contract as
/// [`run_peak_meter`].
fn run_level_pair(peak_db: f32, rms_db: f32) -> (Vec<ClippedShape>, Rect, Roles) {
    let ctx = Context::default();
    theme::apply_tokens(&ctx);
    let level = LevelPair {
        peak_l: amplitude_for_db(peak_db),
        peak_r: amplitude_for_db(peak_db),
        rms_l: amplitude_for_db(rms_db),
        rms_r: amplitude_for_db(rms_db),
    };
    let mut roles_used = None;
    let output = ctx.run_ui(RawInput::default(), |ui| {
        roles_used = Some(*theme::roles(ui.visuals()));
        level_pair(ui, "effects-pre", level);
    });
    let shapes = output.shapes.clone();
    let rect = border_rect(&shapes);
    output.drop_without_applying_deltas();
    (
        shapes,
        rect,
        roles_used.unwrap_or_else(|| panic!("level_pair must run its closure")),
    )
}

// -----------------------------------------------------------------
// T049 — M6: fill_is_segmented_by_db_position
// -----------------------------------------------------------------

/// **M6**: a bar driven past its boundary shows `positive`, then
/// `warning`, then `danger` across its length — three pairwise-distinct
/// band-coloured rects, left to right in dB order.
#[test]
fn fill_is_segmented_by_db_position() {
    let (shapes, rect, roles) = run_peak_meter(0.0, -3.0);

    let positive = fill_rects(
        &shapes,
        controls::band_color(&roles, controls::Band::Positive),
    );
    let warning = fill_rects(
        &shapes,
        controls::band_color(&roles, controls::Band::Warning),
    );
    let danger = fill_rects(
        &shapes,
        controls::band_color(&roles, controls::Band::Danger),
    );

    assert_eq!(positive.len(), 1, "expected exactly one positive segment");
    assert_eq!(warning.len(), 1, "expected exactly one warning segment");
    assert_eq!(danger.len(), 1, "expected exactly one danger segment");

    let minus6_x = rect.left() + rect.width() * fraction_of(BAND_WARNING_DB);
    let boundary_x = rect.left() + rect.width() * fraction_of(-3.0);

    assert!((positive[0].min.x - rect.left()).abs() < EPS);
    assert!((warning[0].min.x - minus6_x).abs() < EPS);
    assert!((warning[0].max.x - boundary_x).abs() < EPS);
    assert!((danger[0].min.x - boundary_x).abs() < EPS);
    assert!((danger[0].max.x - rect.right()).abs() < EPS);

    // Left to right, in dB order.
    assert!(warning[0].min.x < warning[0].max.x);
    assert!(warning[0].max.x <= danger[0].min.x + EPS);
}

// -----------------------------------------------------------------
// T049 — M7: rightmost_column_is_danger_over_the_boundary
// -----------------------------------------------------------------

/// **M7** (SC-005): at `level >= boundary` the rightmost filled column is
/// `danger` — its rect reaches the same right edge as the fill, and it is
/// painted after (on top of) the positive/warning segments beneath it.
#[test]
fn rightmost_column_is_danger_over_the_boundary() {
    let (shapes, rect, roles) = run_peak_meter(-1.0, -3.0);

    let positive_color = controls::band_color(&roles, controls::Band::Positive);
    let warning_color = controls::band_color(&roles, controls::Band::Warning);
    let danger_color = controls::band_color(&roles, controls::Band::Danger);

    let danger = fill_rects(&shapes, danger_color);
    assert_eq!(danger.len(), 1, "expected exactly one danger segment");

    let fill_right = rect.left() + rect.width() * fraction_of(-1.0);
    assert!((danger[0].max.x - fill_right).abs() < EPS);

    let positive_idx = first_fill_index(&shapes, positive_color)
        .unwrap_or_else(|| panic!("expected a positive segment"));
    let warning_idx = first_fill_index(&shapes, warning_color)
        .unwrap_or_else(|| panic!("expected a warning segment"));
    let danger_idx = first_fill_index(&shapes, danger_color)
        .unwrap_or_else(|| panic!("expected a danger segment"));

    assert!(
        danger_idx > positive_idx && danger_idx > warning_idx,
        "danger must paint on top of (after) positive and warning: positive={positive_idx} warning={warning_idx} danger={danger_idx}"
    );
}

// -----------------------------------------------------------------
// T049 — M8: each_meter_uses_its_own_boundary
// -----------------------------------------------------------------

/// **M8**: the peak meter's danger boundary is the ceiling it is handed;
/// the level pair's is fixed at 0 dBFS regardless. The same −2 dBFS level
/// is `danger` on a peak meter ceilinged at −5 dBFS, but not on the level
/// pair, which has no ceiling input.
#[test]
fn each_meter_uses_its_own_boundary() {
    let (peak_shapes, _, peak_roles) = run_peak_meter(-2.0, -5.0);
    let peak_danger = fill_rects(
        &peak_shapes,
        controls::band_color(&peak_roles, controls::Band::Danger),
    );
    assert_eq!(
        peak_danger.len(),
        1,
        "peak meter at -2 dBFS with a -5 dBFS ceiling must be in the danger band"
    );

    let (pair_shapes, _, pair_roles) = run_level_pair(-2.0, SCALE_MIN_DB);
    let pair_danger = fill_rects(
        &pair_shapes,
        controls::band_color(&pair_roles, controls::Band::Danger),
    );
    assert!(
        pair_danger.is_empty(),
        "level_pair's boundary is fixed at 0 dBFS; -2 dBFS must not be danger"
    );
}

// -----------------------------------------------------------------
// T049 — M9: rms_bands_and_is_not_dimmed
// -----------------------------------------------------------------

/// **M9** (FR-012a): the RMS sub-bar bands identically to the peak
/// sub-bar and is no longer dimmed — its danger-band colour is the exact
/// role colour, not `gamma_multiply(0.7)` of anything. Neither sub-bar's
/// source reads `selection.bg_fill` any more.
#[test]
fn rms_bands_and_is_not_dimmed() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/widgets/chain_meters.rs");
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    // Scoped to level_pair (and the helpers it alone uses, defined right
    // after it) — chain_meters::spectrum is explicitly out of scope
    // (data-model.md §6) and still reads selection.bg_fill for its own,
    // unrelated bars.
    let start = contents
        .find("pub fn level_pair")
        .unwrap_or_else(|| panic!("expected a level_pair function"));
    let end = contents.find("pub fn spectrum").unwrap_or(contents.len());
    let level_pair_code = strip_line_comments(&contents[start..end]);
    assert!(
        !level_pair_code.contains("gamma_multiply(0.7)"),
        "level_pair must no longer dim the RMS sub-bar with gamma_multiply(0.7) (M9)"
    );
    assert!(
        !level_pair_code.contains("selection.bg_fill"),
        "level_pair must not read selection.bg_fill on either sub-bar (M9)"
    );

    // rms_db at full scale (0 dBFS) is level_pair's own fixed boundary,
    // so its sub-bar's rightmost column is danger; peak stays silent.
    let (shapes, rect, roles) = run_level_pair(SCALE_MIN_DB, 0.0);
    let danger_color = controls::band_color(&roles, controls::Band::Danger);
    let danger = fill_rects(&shapes, danger_color);
    assert_eq!(
        danger.len(),
        1,
        "expected exactly one danger segment (the RMS sub-bar's)"
    );

    let half = rect.width() / 2.0;
    assert!(
        danger[0].min.x >= rect.left() + half - EPS,
        "the danger segment must be in the RMS (right) half: {:?}, half starts at {}",
        danger[0],
        rect.left() + half
    );
}

// -----------------------------------------------------------------
// T049 — K1: both_meters_draw_both_marks
// -----------------------------------------------------------------

/// **K1**: both meters draw ticks at −6 dBFS and 0 dBFS,
/// `SCALE_MARK_WIDTH == 1.0` — one pair per meter, and one pair per
/// sub-bar in the level pair (data-model.md §6: "(peak **and** RMS
/// sub-bars)").
#[test]
fn both_meters_draw_both_marks() {
    let (peak_shapes, ..) = run_peak_meter(-20.0, -10.0);
    let peak_marks = lines_of_width(&peak_shapes, SCALE_MARK_WIDTH);
    assert_eq!(
        peak_marks.len(),
        2,
        "peak meter must draw exactly its two scale marks (-6 dB, 0 dB): {peak_marks:?}"
    );

    let (pair_shapes, ..) = run_level_pair(-20.0, -20.0);
    let pair_marks = lines_of_width(&pair_shapes, SCALE_MARK_WIDTH);
    assert_eq!(
        pair_marks.len(),
        4,
        "level_pair must draw two scale marks per sub-bar (peak + RMS): {pair_marks:?}"
    );
}

// -----------------------------------------------------------------
// T049 — K2: ceiling_tick_is_two_px
// -----------------------------------------------------------------

/// **K2**: the peak meter additionally draws its ceiling tick at
/// `CEILING_MARK_WIDTH == 2.0`, distinguishable from a 1 px scale mark by
/// width, at the ceiling's own dB position.
#[test]
fn ceiling_tick_is_two_px() {
    let (shapes, rect, _) = run_peak_meter(-1.0, -20.0);
    let ceiling_marks = lines_of_width(&shapes, CEILING_MARK_WIDTH);
    assert_eq!(
        ceiling_marks.len(),
        1,
        "expected exactly one 2 px ceiling tick: {ceiling_marks:?}"
    );

    let expected_x = rect.left() + rect.width() * fraction_of(-20.0);
    assert!(
        (ceiling_marks[0].0 - expected_x).abs() < EPS,
        "ceiling tick at {} must sit at the ceiling's own dB position {expected_x}",
        ceiling_marks[0].0
    );
}

// -----------------------------------------------------------------
// T049 — K5: ceiling_tick_is_not_warn_fg
// -----------------------------------------------------------------

/// **K5**: `peak_meter.rs`'s `warn_fg_color` ceiling tick is removed —
/// after M6 it would be invisible over a `warning` band.
#[test]
fn ceiling_tick_is_not_warn_fg() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/widgets/peak_meter.rs");
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let code = strip_line_comments(&contents);
    assert!(
        !code.contains("warn_fg_color"),
        "peak_meter.rs must no longer paint the ceiling tick with warn_fg_color (K5)"
    );
}

// -----------------------------------------------------------------
// T049 — K6: zero_db_mark_is_inset
// -----------------------------------------------------------------

/// **K6**: the 0 dBFS mark sits at the scale maximum and is inset by its
/// own width, so it stays visible against the meter's border stroke —
/// checked on the peak meter and on each of the level pair's two
/// sub-bars.
#[test]
fn zero_db_mark_is_inset() {
    let (peak_shapes, peak_rect, _) = run_peak_meter(-1.0, -1.0);
    let peak_marks = lines_of_width(&peak_shapes, SCALE_MARK_WIDTH);
    let (zero_x, _) = nearest_x(&peak_marks, peak_rect.right());
    assert!(
        (zero_x - (peak_rect.right() - SCALE_MARK_WIDTH)).abs() < EPS,
        "peak meter's 0 dB mark at {zero_x} must be inset by SCALE_MARK_WIDTH from {}",
        peak_rect.right()
    );
    assert!(
        zero_x < peak_rect.right(),
        "the 0 dB mark must not sit flush on the border"
    );

    let (pair_shapes, pair_rect, _) = run_level_pair(-1.0, -1.0);
    let pair_marks = lines_of_width(&pair_shapes, SCALE_MARK_WIDTH);
    let half = pair_rect.width() / 2.0;

    let (peak_half_zero_x, _) = nearest_x(&pair_marks, pair_rect.left() + half);
    assert!(
        (peak_half_zero_x - (pair_rect.left() + half - SCALE_MARK_WIDTH)).abs() < EPS,
        "level_pair's peak sub-bar 0 dB mark at {peak_half_zero_x} must be inset from its half's right edge"
    );

    let (rms_half_zero_x, _) = nearest_x(&pair_marks, pair_rect.right());
    assert!(
        (rms_half_zero_x - (pair_rect.right() - SCALE_MARK_WIDTH)).abs() < EPS,
        "level_pair's RMS sub-bar 0 dB mark at {rms_half_zero_x} must be inset from the track's right edge"
    );
}

// -----------------------------------------------------------------
// T049 — K7: mark_positions
// -----------------------------------------------------------------

/// **K7**: mark positions are `fraction_of(-6.0)` and `fraction_of(0.0)`
/// of the track width, on the meters' existing
/// `SCALE_MIN_DB..=SCALE_MAX_DB` scale — the 0 dB mark inset per K6.
#[test]
fn mark_positions() {
    let (peak_shapes, peak_rect, _) = run_peak_meter(-1.0, -1.0);
    let peak_marks = lines_of_width(&peak_shapes, SCALE_MARK_WIDTH);
    let expected_minus6 = peak_rect.left() + peak_rect.width() * fraction_of(BAND_WARNING_DB);
    let expected_zero = peak_rect.left() + peak_rect.width() * fraction_of(0.0) - SCALE_MARK_WIDTH;
    let (minus6_x, _) = nearest_x(&peak_marks, expected_minus6);
    let (zero_x, _) = nearest_x(&peak_marks, expected_zero);
    assert!((minus6_x - expected_minus6).abs() < EPS);
    assert!((zero_x - expected_zero).abs() < EPS);

    let (pair_shapes, pair_rect, _) = run_level_pair(-1.0, -1.0);
    let pair_marks = lines_of_width(&pair_shapes, SCALE_MARK_WIDTH);
    let half = pair_rect.width() / 2.0;

    for origin_x in [pair_rect.left(), pair_rect.left() + half] {
        let expected_minus6 = origin_x + half * fraction_of(BAND_WARNING_DB);
        let expected_zero = origin_x + half * fraction_of(0.0) - SCALE_MARK_WIDTH;
        let (minus6_x, _) = nearest_x(&pair_marks, expected_minus6);
        let (zero_x, _) = nearest_x(&pair_marks, expected_zero);
        assert!(
            (minus6_x - expected_minus6).abs() < EPS,
            "sub-bar at origin {origin_x}: -6 dB mark {minus6_x} != expected {expected_minus6}"
        );
        assert!(
            (zero_x - expected_zero).abs() < EPS,
            "sub-bar at origin {origin_x}: 0 dB mark {zero_x} != expected {expected_zero}"
        );
    }
}

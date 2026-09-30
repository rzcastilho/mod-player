// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Effect Chain panel's pre-/post-chain level pair and spectrum
//! (008, contracts/ui-effect-chain.md §2): `level_pair` reuses
//! `peak_meter`'s dBFS scale for a peak+RMS bar pair; `spectrum` draws
//! the 64-band log-frequency display. Both are pure display widgets —
//! the caller reads `PlaybackController::chain_meters()` fresh each
//! frame and hands the values in (data-model.md §2.5).

use egui::{
    Align2, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
    pos2,
};
use modplayer_core::{LevelPair, tr};

use crate::theme::controls::{self, Band};
use crate::theme::{self, Roles};
use crate::widgets::peak_meter::{SCALE_MAX_DB, SCALE_MIN_DB};

fn to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * amplitude.log10()
    }
}

/// Always 8 characters — `" -inf dB"` … `"  0.0 dB"` (data-model.md §4.2,
/// contract L1): a right-aligned 5-character magnitude (`{:>5.1}`, or
/// `-inf` padded to the same width) plus the constant `" dB"` suffix, so a
/// level-pair readout's width never shifts as the value crosses zero or
/// drops to silence.
fn format_db(db: f32) -> String {
    let magnitude = if db.is_finite() {
        format!("{db:.1}")
    } else {
        "-inf".to_string()
    };
    format!("{magnitude:>5} dB")
}

fn fraction_of(db: f32) -> f32 {
    ((db - SCALE_MIN_DB) / (SCALE_MAX_DB - SCALE_MIN_DB)).clamp(0.0, 1.0)
}

/// One side's (pre- or post-chain) peak+RMS level pair (contracts/ui-
/// effect-chain.md §2). `side_label_key` is `effects-pre`/`effects-post`;
/// `level` is the linear-amplitude peak/RMS, L/R (data-model.md §2.5) —
/// the louder channel of each is shown, on `peak_meter`'s own dBFS scale.
pub fn level_pair(ui: &mut Ui, side_label_key: &str, level: LevelPair) {
    let peak_db = to_db(level.peak_l.max(level.peak_r)).clamp(SCALE_MIN_DB, SCALE_MAX_DB);
    let rms_db = to_db(level.rms_l.max(level.rms_r)).clamp(SCALE_MIN_DB, SCALE_MAX_DB);
    let side = tr(side_label_key);
    let peak_text = format!("{}: {}", tr("effects-peak"), format_db(peak_db));
    let rms_text = format!("{}: {}", tr("effects-rms"), format_db(rms_db));
    let accessible = format!("{side} {peak_text}, {rms_text}");

    ui.horizontal(|ui| {
        ui.label(&side);

        let size = Vec2::new(ui.available_width().clamp(60.0, 140.0), 14.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let corner = theme::radius::SM;
            let roles = theme::roles(ui.visuals());
            painter.rect_filled(rect, corner, ui.visuals().extreme_bg_color);

            // Both sub-bars band identically (M9, FR-012a): fixed 0 dBFS
            // boundary (no ceiling input), the RMS bar's `gamma_multiply(0.7)`
            // dim removed, neither reads `selection.bg_fill` any more.
            let half = rect.width() / 2.0;
            let peak_origin = rect.min;
            let rms_origin = pos2(rect.left() + half, rect.top());

            draw_meter_half(
                painter,
                peak_origin,
                half,
                rect.height(),
                corner,
                roles,
                peak_db,
            );
            draw_meter_half(
                painter,
                rms_origin,
                half,
                rect.height(),
                corner,
                roles,
                rms_db,
            );

            painter.rect_stroke(
                rect,
                corner,
                ui.visuals().window_stroke,
                StrokeKind::Outside,
            );
        }
        response.widget_info(|| {
            WidgetInfo::labeled(WidgetType::ProgressIndicator, true, accessible.clone())
        });
        response.on_hover_text(accessible);

        // 014-design-tokens-and-type-scale (US3, T042): peak/RMS meter
        // readouts are numeric fields compared side by side — `mono` so
        // their digits line up in a fixed-width column.
        ui.label(theme::mono_text(peak_text));
        ui.label(theme::mono_text(rms_text));
    });
}

/// One sub-bar's segmented fill plus its own −6/0 dB scale marks
/// (data-model.md §6, contract M9): the same band selection and mark
/// rules `peak_meter` uses, applied to a half-width track whose danger
/// boundary is fixed at `SCALE_MAX_DB` (0 dBFS) — `level_pair` has no
/// ceiling input (M8).
fn draw_meter_half(
    painter: &egui::Painter,
    origin: Pos2,
    width: f32,
    height: f32,
    corner: CornerRadius,
    roles: &Roles,
    level_db: f32,
) {
    let fill_right = origin.x + width * fraction_of(level_db);

    let fill_rect =
        Rect::from_min_size(origin, Vec2::new((fill_right - origin.x).max(0.0), height));
    painter.rect_filled(
        fill_rect,
        corner,
        controls::band_color(roles, Band::Positive),
    );

    let warning_right_db = level_db.min(SCALE_MAX_DB);
    if warning_right_db > controls::BAND_WARNING_DB {
        let warn_left = origin.x + width * fraction_of(controls::BAND_WARNING_DB);
        let warn_right = origin.x + width * fraction_of(warning_right_db);
        painter.rect_filled(
            Rect::from_min_max(
                pos2(warn_left, origin.y),
                pos2(warn_right, origin.y + height),
            ),
            0.0,
            controls::band_color(roles, Band::Warning),
        );
    }

    if level_db >= SCALE_MAX_DB {
        let danger_left = origin.x + width * fraction_of(SCALE_MAX_DB);
        painter.rect_filled(
            Rect::from_min_max(
                pos2(danger_left, origin.y),
                pos2(fill_right, origin.y + height),
            ),
            0.0,
            controls::band_color(roles, Band::Danger),
        );
    }

    let minus6_x = origin.x + width * fraction_of(controls::BAND_WARNING_DB);
    paint_mark(
        painter,
        origin.y,
        height,
        minus6_x,
        fill_right,
        controls::SCALE_MARK_WIDTH,
        roles,
    );

    let zero_x = origin.x + width - controls::SCALE_MARK_WIDTH;
    paint_mark(
        painter,
        origin.y,
        height,
        zero_x,
        fill_right,
        controls::SCALE_MARK_WIDTH,
        roles,
    );
}

/// One scale mark (contract K3, shared with `peak_meter`): `surface.base`
/// where the fill has reached `x`, `text.secondary` where it has not.
fn paint_mark(
    painter: &egui::Painter,
    top: f32,
    height: f32,
    x: f32,
    fill_right: f32,
    width: f32,
    roles: &Roles,
) {
    let filled = fill_right >= x;
    painter.line_segment(
        [pos2(x, top), pos2(x, top + height)],
        Stroke::new(width, controls::mark_color(roles, filled)),
    );
}

/// The nominal centre frequency 64 log-spaced bands (20 Hz–20 kHz)
/// assign to band `i` — a display-only approximation (the real FFT/bin
/// folding is `SpectrumRing`'s, at the actual source rate); good enough
/// for the axis labels and the hover value.
fn band_center_hz(i: usize, bands: usize) -> f32 {
    let t = (i as f32 + 0.5) / bands.max(1) as f32;
    20.0 * (1_000f32).powf(t)
}

fn format_hz(hz: f32) -> String {
    if hz >= 1_000.0 {
        format!("{:.1} kHz", hz / 1_000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

/// The dynamic range the spectrum bars span, in dB below full scale: a
/// band at `SPECTRUM_FLOOR_DB` or quieter draws nothing, a full-scale
/// band fills the height.
const SPECTRUM_FLOOR_DB: f32 = -60.0;

/// A linear band magnitude (0..1, data-model.md §2.5) as a 0..1 bar
/// height on a dB scale over [`SPECTRUM_FLOOR_DB`]..0 dBFS. Drawn linear,
/// real music's per-band energy (−20..−40 dBFS) filled ≈ 1–10 % of the
/// box even at a clipping post level (2026-09-19 manual walk, M12), so
/// the display is log-magnitude like every other spectrum analyser.
fn spectrum_bar_height(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * value.log10();
    ((db - SPECTRUM_FLOOR_DB) / -SPECTRUM_FLOOR_DB).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------
// Spectrum axis model (024-effect-chain-rows-and-meters, data-model.md
// §4.3/§4.4, contracts/ui-chain-meters.md §S): the dB gutter's reference
// lines, the frequency tick strip, and the bars' band segmentation — all
// pure, unit-tested helpers `spectrum` alone calls.
// ---------------------------------------------------------------------

/// The bars' existing log-frequency mapping bounds (unchanged) — shared
/// with [`freq_to_frac`] so the tick strip lines up with the bars.
const SPECTRUM_MIN_HZ: f32 = 20.0;
const SPECTRUM_MAX_HZ: f32 = 20_000.0;

/// Labeled ticks (contract S5): `(hz, fluent key)` — every tick's text
/// comes from Fluent (FR-015), never a literal at the paint site.
const MAJOR_TICKS_HZ: [(f32, &str); 3] = [
    (100.0, "effects-spectrum-tick-100"),
    (1_000.0, "effects-spectrum-tick-1k"),
    (10_000.0, "effects-spectrum-tick-10k"),
];

/// Unlabeled ticks (contract S5): a shorter mark, no text.
const MINOR_TICKS_HZ: [f32; 5] = [50.0, 200.0, 500.0, 2_000.0, 5_000.0];

/// The three dBFS reference lines (contract S3), each paired with its
/// gutter Fluent key (contract S4).
const REFERENCE_DB: [(f32, &str); 3] = [
    (0.0, "effects-spectrum-ref-0db"),
    (-30.0, "effects-spectrum-ref-minus30"),
    (-60.0, "effects-spectrum-ref-minus60"),
];

/// A frequency in Hz as a 0..1 x-fraction of the plot (data-model.md
/// §4.3): `log10(hz / 20) / log10(1000)`, clamped — `freq_to_frac(100) ≈
/// 0.233`, `(1_000) ≈ 0.566`, `(10_000) ≈ 0.900`. Shared by the bars'
/// existing mapping and the tick strip, so both agree exactly.
fn freq_to_frac(hz: f32) -> f32 {
    let span = (SPECTRUM_MAX_HZ / SPECTRUM_MIN_HZ).log10();
    ((hz / SPECTRUM_MIN_HZ).max(f32::MIN_POSITIVE).log10() / span).clamp(0.0, 1.0)
}

/// A dBFS value as a 0..1 y-fraction of the plot, `1.0` at the top
/// (data-model.md §4.3): the inverse of the bars' own height rule, over
/// the same [`SPECTRUM_FLOOR_DB`]..0 range — `db_to_frac(0) == 1.0`,
/// `(-30) == 0.5`, `(-60) == 0.0`.
fn db_to_frac(db: f32) -> f32 {
    ((db - SPECTRUM_FLOOR_DB) / -SPECTRUM_FLOOR_DB).clamp(0.0, 1.0)
}

/// Each major tick's label span (contract S6), centred on
/// `freq_to_frac(hz) * plot_width` and clamped to stay inside the plot
/// and clear of the previous label — assigned left to right (frequency
/// only ever increases across [`MAJOR_TICKS_HZ`]), so no two spans can
/// overlap by construction (data-model.md §4.3).
fn tick_label_spans(plot_width: f32, label_widths: [f32; 3]) -> [(f32, f32); 3] {
    let mut spans = [(0.0_f32, 0.0_f32); 3];
    let mut min_left = 0.0_f32;
    for (i, &(hz, _)) in MAJOR_TICKS_HZ.iter().enumerate() {
        let width = label_widths[i].max(0.0);
        let max_left = (plot_width - width).max(min_left);
        let center = freq_to_frac(hz) * plot_width;
        let left = (center - width / 2.0).clamp(min_left, max_left);
        let right = left + width;
        spans[i] = (left, right);
        min_left = right;
    }
    spans
}

/// A bar's colour segmentation (data-model.md §4.4): fractions of the
/// plot height (`0.0` at the floor, `1.0` at 0 dBFS), boundaries reused
/// from `controls::BAND_WARNING_DB`/`SCALE_MAX_DB`. Only the first `usize`
/// entries of the returned array are meaningful — `0` (silent, `value <=`
/// the floor), `1` (positive only), `2` (positive + warning), or `3`
/// (positive + warning + a danger cap, drawn by the caller as a fixed
/// pixel strip rather than the returned fraction — contract S2).
fn spectrum_segments(value: f32) -> ([(Band, f32, f32); 3], usize) {
    let height = spectrum_bar_height(value);
    if height <= 0.0 {
        return ([(Band::Positive, 0.0, 0.0); 3], 0);
    }

    let warn_start = spectrum_bar_height(10f32.powf(controls::BAND_WARNING_DB / 20.0));
    let danger_start = spectrum_bar_height(10f32.powf(SCALE_MAX_DB / 20.0));

    if height <= warn_start {
        return (
            [
                (Band::Positive, 0.0, height),
                (Band::Positive, 0.0, 0.0),
                (Band::Positive, 0.0, 0.0),
            ],
            1,
        );
    }
    if height < danger_start {
        return (
            [
                (Band::Positive, 0.0, warn_start),
                (Band::Warning, warn_start, height),
                (Band::Positive, 0.0, 0.0),
            ],
            2,
        );
    }
    (
        [
            (Band::Positive, 0.0, warn_start),
            (Band::Warning, warn_start, danger_start),
            (Band::Danger, danger_start, danger_start),
        ],
        3,
    )
}

/// A text's rendered width in the `mono` role, measured without painting
/// it (mirrors `waveform/mod.rs`'s own `hover_label_galley_size`): summing
/// glyph advances avoids `Painter::layout_no_wrap`'s throwaway fill colour,
/// which this crate's literal scan does not allow outside `theme/**`
/// (FR-018a).
fn text_width(ui: &Ui, font_id: &egui::FontId, text: &str) -> f32 {
    ui.ctx()
        .fonts_mut(|fonts| text.chars().map(|c| fonts.glyph_width(font_id, c)).sum())
}

/// The post-chain 64-band spectrum (contracts/ui-effect-chain.md §2,
/// 024-effect-chain-rows-and-meters contracts/ui-chain-meters.md §S): a
/// log-frequency bar display, 20 Hz–20 kHz, `bands` linear magnitude 0..1
/// (data-model.md §2.5), drawn on a 60 dB log scale — now with a left dB
/// gutter (0/−30/−60 reference lines), a bottom frequency tick strip
/// (100/1k/10k major, five unlabeled minor ticks), and positive/warning/
/// danger bar segmentation in place of the flat `selection.bg_fill`.
pub fn spectrum(ui: &mut Ui, bands: &[f32]) {
    let n = bands.len().max(1);
    let outer_width = ui.available_width().clamp(160.0, 420.0);
    let plot_height = 40.0;
    let tick_strip_height = 12.0;
    let gutter_pad = theme::space::XS;

    let roles = theme::roles(ui.visuals());
    let font_id = theme::mono_font_id();
    let ref_labels: [String; 3] = [
        tr(REFERENCE_DB[0].1),
        tr(REFERENCE_DB[1].1),
        tr(REFERENCE_DB[2].1),
    ];
    let gutter_width = ref_labels
        .iter()
        .map(|text| text_width(ui, &font_id, text))
        .fold(0.0_f32, f32::max)
        + gutter_pad * 2.0;

    let size = Vec2::new(outer_width, plot_height + tick_strip_height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let mark_color = controls::mark_color(roles, false);
        let plot_rect = Rect::from_min_max(
            pos2((rect.left() + gutter_width).min(rect.right()), rect.top()),
            pos2(rect.right(), rect.top() + plot_height),
        );
        let tick_top = plot_rect.bottom();

        // Bars (S1/S2/S9): same x/height mapping as before, coloured by
        // `spectrum_segments` instead of a single flat fill.
        let bar_w = plot_rect.width() / n as f32;
        for (i, &value) in bands.iter().enumerate() {
            let x0 = plot_rect.left() + i as f32 * bar_w;
            let bar_rect = Rect::from_min_max(
                pos2(x0, plot_rect.top()),
                pos2(x0 + (bar_w * 0.9).max(1.0), plot_rect.bottom()),
            );
            let (segments, count) = spectrum_segments(value);
            for &(band, start, end) in segments.iter().take(count) {
                if band == Band::Danger {
                    let cap = 3.0_f32.min(plot_rect.height());
                    let danger_rect = Rect::from_min_max(
                        pos2(bar_rect.left(), plot_rect.top()),
                        pos2(bar_rect.right(), plot_rect.top() + cap),
                    );
                    painter.rect_filled(danger_rect, 0.0, controls::band_color(roles, band));
                } else {
                    let seg_rect = Rect::from_min_max(
                        pos2(
                            bar_rect.left(),
                            plot_rect.bottom() - end * plot_rect.height(),
                        ),
                        pos2(
                            bar_rect.right(),
                            plot_rect.bottom() - start * plot_rect.height(),
                        ),
                    );
                    painter.rect_filled(seg_rect, 0.0, controls::band_color(roles, band));
                }
            }
        }

        // Reference lines + gutter labels (S3/S4), painted after the bars.
        for (&(db, _), label) in REFERENCE_DB.iter().zip(ref_labels.iter()) {
            let y = plot_rect.bottom() - db_to_frac(db) * plot_rect.height();
            painter.line_segment(
                [pos2(plot_rect.left(), y), pos2(plot_rect.right(), y)],
                Stroke::new(controls::SCALE_MARK_WIDTH, mark_color),
            );
            painter.text(
                pos2(plot_rect.left() - gutter_pad, y),
                Align2::RIGHT_CENTER,
                label,
                theme::mono_font_id(),
                mark_color,
            );
        }

        // Frequency tick strip (S5): major ticks labeled, minor unlabeled.
        let tick_labels: [String; 3] = [
            tr(MAJOR_TICKS_HZ[0].1),
            tr(MAJOR_TICKS_HZ[1].1),
            tr(MAJOR_TICKS_HZ[2].1),
        ];
        let label_widths = [
            text_width(ui, &font_id, &tick_labels[0]),
            text_width(ui, &font_id, &tick_labels[1]),
            text_width(ui, &font_id, &tick_labels[2]),
        ];
        let spans = tick_label_spans(plot_rect.width(), label_widths);
        for (i, &(hz, _)) in MAJOR_TICKS_HZ.iter().enumerate() {
            let x = plot_rect.left() + freq_to_frac(hz) * plot_rect.width();
            painter.line_segment(
                [pos2(x, tick_top), pos2(x, tick_top + 4.0)],
                Stroke::new(controls::SCALE_MARK_WIDTH, mark_color),
            );
            let (left, _right) = spans[i];
            painter.text(
                pos2(plot_rect.left() + left, tick_top + 4.0),
                Align2::LEFT_TOP,
                &tick_labels[i],
                theme::mono_font_id(),
                mark_color,
            );
        }
        for &hz in &MINOR_TICKS_HZ {
            let x = plot_rect.left() + freq_to_frac(hz) * plot_rect.width();
            painter.line_segment(
                [pos2(x, tick_top), pos2(x, tick_top + 2.0)],
                Stroke::new(controls::SCALE_MARK_WIDTH, mark_color),
            );
        }

        painter.rect_stroke(
            plot_rect,
            0.0,
            ui.visuals().window_stroke,
            StrokeKind::Outside,
        );
    }

    let (peak_band, &peak_value) = bands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((0, &0.0));
    let name = format!("{}, {n}", tr("effects-spectrum"));
    let value = if peak_value > 0.0 {
        format_hz(band_center_hz(peak_band, n))
    } else {
        String::new()
    };
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(WidgetType::ProgressIndicator, true, name.clone());
        info.current_text_value = if value.is_empty() {
            None
        } else {
            Some(value.clone())
        };
        info
    });
    if !value.is_empty() {
        response.on_hover_text(format!("{name}: {value}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_of_maps_scale_bounds_to_unit_interval() {
        assert_eq!(fraction_of(SCALE_MIN_DB), 0.0);
        assert_eq!(fraction_of(SCALE_MAX_DB), 1.0);
    }

    #[test]
    fn band_center_hz_spans_20_to_20000() {
        let first = band_center_hz(0, 64);
        let last = band_center_hz(63, 64);
        assert!(
            first > 20.0 && first < 100.0,
            "first band near 20 Hz: {first}"
        );
        assert!(
            last > 10_000.0 && last < 20_000.0,
            "last band near 20 kHz: {last}"
        );
    }

    #[test]
    fn format_hz_switches_to_khz_at_one_thousand() {
        assert_eq!(format_hz(440.0), "440 Hz");
        assert_eq!(format_hz(1_200.0), "1.2 kHz");
    }

    /// The bars read in dB: full scale fills the box, −30 dBFS (a typical
    /// per-band music level) draws half of it, the floor and silence draw
    /// nothing.
    #[test]
    fn spectrum_bar_height_is_a_60_db_log_scale() {
        assert!((spectrum_bar_height(1.0) - 1.0).abs() < 1e-6);
        let minus_30_db = 10f32.powf(-30.0 / 20.0);
        assert!((spectrum_bar_height(minus_30_db) - 0.5).abs() < 1e-3);
        assert_eq!(spectrum_bar_height(0.001), 0.0);
        assert_eq!(spectrum_bar_height(0.0), 0.0);
        assert_eq!(spectrum_bar_height(4.0), 1.0);
    }

    // -------------------------------------------------------------
    // T021 (US2, data-model.md §4.2, contract L1): `format_db` is always
    // 8 characters, from silence to full scale.
    // -------------------------------------------------------------
    #[test]
    fn format_db_is_fixed_width() {
        for db in [f32::NEG_INFINITY, -60.0, -12.3, -6.0, 0.0] {
            let text = format_db(db);
            assert_eq!(
                text.chars().count(),
                8,
                "format_db({db}) = `{text}`, expected exactly 8 characters"
            );
        }
        // Pinning the exact contract examples, not just the width.
        assert_eq!(format_db(f32::NEG_INFINITY), " -inf dB");
        assert_eq!(format_db(-60.0), "-60.0 dB");
        assert_eq!(format_db(-12.3), "-12.3 dB");
        assert_eq!(format_db(-6.0), " -6.0 dB");
        assert_eq!(format_db(0.0), "  0.0 dB");
    }

    // -------------------------------------------------------------
    // T022 (US2, data-model.md §4.3, contract S5/S6): the spectrum's
    // frequency/dB axis helpers.
    // -------------------------------------------------------------
    #[test]
    fn freq_to_frac_matches_bar_mapping() {
        assert!((freq_to_frac(100.0) - 0.233).abs() < 0.01);
        assert!((freq_to_frac(1_000.0) - 0.566).abs() < 0.01);
        assert!((freq_to_frac(10_000.0) - 0.900).abs() < 0.01);
        // Clamped at the ends.
        assert_eq!(freq_to_frac(SPECTRUM_MIN_HZ), 0.0);
        assert_eq!(freq_to_frac(SPECTRUM_MAX_HZ), 1.0);
    }

    #[test]
    fn db_to_frac_inverts_bar_height() {
        assert_eq!(db_to_frac(0.0), 1.0);
        assert!((db_to_frac(-30.0) - 0.5).abs() < 1e-6);
        assert_eq!(db_to_frac(-60.0), 0.0);
    }

    #[test]
    fn tick_label_spans_never_overlap_160_to_420() {
        // Representative widths for "100"/"1k"/"10k" at the mono role —
        // the invariant holds for any label widths that fit the plot, so
        // this exercises the shape the real strip actually draws.
        let label_widths = [24.0, 16.0, 24.0];
        let mut width = 160.0_f32;
        while width <= 420.0 {
            let spans = tick_label_spans(width, label_widths);
            for span in &spans {
                assert!(
                    span.0 >= 0.0 && span.1 <= width && span.0 <= span.1,
                    "span {span:?} escapes [0, {width}]"
                );
            }
            for pair in spans[..].windows(2) {
                assert!(
                    pair[0].1 <= pair[1].0 + 1e-3,
                    "spans overlap at plot_width={width}: {spans:?}"
                );
            }
            width += 10.0;
        }
    }

    // -------------------------------------------------------------
    // T023 (US2, data-model.md §4.4, contract S2): the bars' band
    // segmentation.
    // -------------------------------------------------------------
    #[test]
    fn spectrum_segments_band_boundaries() {
        let (segments, count) = spectrum_segments(0.001);
        assert_eq!(
            count, 0,
            "at/below the floor: no segments, got {segments:?}"
        );

        let (segments, count) = spectrum_segments(0.1);
        assert_eq!(count, 1, "below -6 dBFS: positive only");
        assert_eq!(segments[0].0, Band::Positive);
        assert_eq!(segments[0].1, 0.0);
        assert!((segments[0].2 - 0.667).abs() < 0.01);

        let (segments, count) = spectrum_segments(0.7);
        assert_eq!(count, 2, "between -6 and 0 dBFS: positive + warning");
        assert_eq!(segments[0].0, Band::Positive);
        assert!((segments[0].2 - 0.9).abs() < 1e-3);
        assert_eq!(segments[1].0, Band::Warning);
        assert!((segments[1].1 - 0.9).abs() < 1e-3);
        assert!((segments[1].2 - 0.948).abs() < 0.01);

        let (segments, count) = spectrum_segments(1.5);
        assert_eq!(count, 3, "at/above 0 dBFS: positive + warning + danger cap");
        assert_eq!(segments[0].0, Band::Positive);
        assert_eq!(segments[1].0, Band::Warning);
        assert_eq!(segments[2].0, Band::Danger);
    }
}

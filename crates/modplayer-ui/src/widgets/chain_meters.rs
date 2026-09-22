// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Effect Chain panel's pre-/post-chain level pair and spectrum
//! (008, contracts/ui-effect-chain.md §2): `level_pair` reuses
//! `peak_meter`'s dBFS scale for a peak+RMS bar pair; `spectrum` draws
//! the 64-band log-frequency display. Both are pure display widgets —
//! the caller reads `PlaybackController::chain_meters()` fresh each
//! frame and hands the values in (data-model.md §2.5).

use egui::{Rect, Sense, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType, pos2};
use modplayer_core::{LevelPair, tr};

use crate::theme;
use crate::widgets::peak_meter::{SCALE_MAX_DB, SCALE_MIN_DB};

fn to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * amplitude.log10()
    }
}

fn format_db(db: f32) -> String {
    if db.is_finite() {
        format!("{db:.1} dB")
    } else {
        "-inf dB".to_string()
    }
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
            painter.rect_filled(rect, corner, ui.visuals().extreme_bg_color);

            let half = rect.width() / 2.0;
            let peak_rect = Rect::from_min_size(
                rect.min,
                Vec2::new(half * fraction_of(peak_db), rect.height()),
            );
            painter.rect_filled(peak_rect, corner, ui.visuals().selection.bg_fill);

            let rms_min = pos2(rect.left() + half, rect.top());
            let rms_rect = Rect::from_min_size(
                rms_min,
                Vec2::new(half * fraction_of(rms_db), rect.height()),
            );
            painter.rect_filled(
                rms_rect,
                corner,
                ui.visuals().selection.bg_fill.gamma_multiply(0.7),
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

/// The post-chain 64-band spectrum (contracts/ui-effect-chain.md §2): a
/// log-frequency bar display, 20 Hz–20 kHz. `bands` is linear magnitude,
/// 0..1 (data-model.md §2.5), drawn on a 60 dB log scale.
pub fn spectrum(ui: &mut Ui, bands: &[f32]) {
    let n = bands.len().max(1);
    let size = Vec2::new(ui.available_width().clamp(160.0, 420.0), 40.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let bar_w = rect.width() / n as f32;
        for (i, &value) in bands.iter().enumerate() {
            let h = rect.height() * spectrum_bar_height(value);
            let x0 = rect.left() + i as f32 * bar_w;
            let bar_rect = Rect::from_min_max(
                pos2(x0, rect.bottom() - h),
                pos2(x0 + (bar_w * 0.9).max(1.0), rect.bottom()),
            );
            painter.rect_filled(bar_rect, 0.0, ui.visuals().selection.bg_fill);
        }
        painter.rect_stroke(rect, 0.0, ui.visuals().window_stroke, StrokeKind::Outside);
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
}

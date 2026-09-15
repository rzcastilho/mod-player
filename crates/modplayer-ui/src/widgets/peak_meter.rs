// SPDX-License-Identifier: MIT OR Apache-2.0

//! The peak meter (contracts/ui-surface.md `peak-meter`): a custom bar on
//! a dBFS scale (`SCALE_MIN_DB..=SCALE_MAX_DB`) with the active limiter
//! ceiling marked, reading `RtShared::peak_bits` every frame (via the
//! caller — this widget takes the already-read `peak`/`ceiling_db`, so it
//! stays independent of the controller type). Accessible value is the last
//! peak in dBFS.

use egui::{Color32, CornerRadius, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType};
use modplayer_core::tr;

/// Lower bound of the meter's dBFS scale (contracts/ui-surface.md).
pub const SCALE_MIN_DB: f32 = -60.0;
/// Upper bound of the meter's dBFS scale.
pub const SCALE_MAX_DB: f32 = 0.0;

/// Draw the peak meter for `peak` (linear amplitude, `RtShared::peak()`)
/// with `ceiling_db` (the active limiter ceiling, `CeilingDb::db()`) marked
/// as a tick on the scale.
pub fn peak_meter(ui: &mut Ui, peak: f32, ceiling_db: f32) {
    ui.label(tr("peak-meter"));

    let peak_db = to_db(peak).clamp(SCALE_MIN_DB, SCALE_MAX_DB);
    let size = Vec2::new(ui.available_width().clamp(80.0, 240.0), 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let corner = CornerRadius::from(2u8);
        painter.rect_filled(rect, corner, ui.visuals().extreme_bg_color);

        let fraction = fraction_of(peak_db);
        let mut fill = rect;
        fill.set_width(rect.width() * fraction);
        let over_ceiling = peak_db >= ceiling_db.clamp(SCALE_MIN_DB, SCALE_MAX_DB);
        let fill_color = if over_ceiling {
            Color32::from_rgb(220, 60, 60)
        } else {
            ui.visuals().selection.bg_fill
        };
        painter.rect_filled(fill, corner, fill_color);

        // Ceiling tick.
        let ceiling_x =
            rect.left() + rect.width() * fraction_of(ceiling_db.clamp(SCALE_MIN_DB, SCALE_MAX_DB));
        painter.line_segment(
            [
                egui::pos2(ceiling_x, rect.top()),
                egui::pos2(ceiling_x, rect.bottom()),
            ],
            Stroke::new(2.0, ui.visuals().warn_fg_color),
        );

        painter.rect_stroke(
            rect,
            corner,
            ui.visuals().window_stroke,
            StrokeKind::Outside,
        );
    }

    let accessible_value = format!("{}: {}", tr("peak-meter"), format_db(peak_db));
    response.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::ProgressIndicator,
            true,
            accessible_value.clone(),
        )
    });
    response.on_hover_text(accessible_value);
}

fn fraction_of(db: f32) -> f32 {
    ((db - SCALE_MIN_DB) / (SCALE_MAX_DB - SCALE_MIN_DB)).clamp(0.0, 1.0)
}

fn to_db(peak: f32) -> f32 {
    if peak <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * peak.log10()
    }
}

fn format_db(db: f32) -> String {
    if db.is_finite() {
        format!("{db:.1} dBFS")
    } else {
        "-inf dBFS".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_of_maps_scale_bounds_to_unit_interval() {
        assert_eq!(fraction_of(SCALE_MIN_DB), 0.0);
        assert_eq!(fraction_of(SCALE_MAX_DB), 1.0);
        assert!((fraction_of(-30.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn to_db_of_silence_is_negative_infinity() {
        assert_eq!(to_db(0.0), f32::NEG_INFINITY);
        assert!((to_db(1.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn format_db_handles_infinity() {
        assert_eq!(format_db(f32::NEG_INFINITY), "-inf dBFS");
        assert_eq!(format_db(-6.0), "-6.0 dBFS");
    }
}

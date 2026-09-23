// SPDX-License-Identifier: MIT OR Apache-2.0

//! The peak meter (contracts/ui-surface.md `peak-meter`): a custom bar on
//! a dBFS scale (`SCALE_MIN_DB..=SCALE_MAX_DB`) with the active limiter
//! ceiling marked, reading `RtShared::peak_bits` every frame (via the
//! caller — this widget takes the already-read `peak`/`ceiling_db`, so it
//! stays independent of the controller type). Accessible value is the last
//! peak in dBFS.

use egui::{Rect, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType, pos2};
use modplayer_core::tr;

use crate::theme::controls::{self, Band};
use crate::theme::{self, Roles};

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
        let corner = theme::radius::SM;
        let roles = theme::roles(ui.visuals());
        painter.rect_filled(rect, corner, ui.visuals().extreme_bg_color);

        let boundary_db = ceiling_db.clamp(SCALE_MIN_DB, SCALE_MAX_DB);
        let fill_right = rect.left() + rect.width() * fraction_of(peak_db);

        // Segmented fill (data-model.md §6, design note D5): the filled
        // span is drawn once in `positive`, rounded at the left end, then
        // the warning and danger spans are overdrawn as square rects
        // clipped to the filled span.
        let mut fill = rect;
        fill.set_width(fill_right - rect.left());
        painter.rect_filled(fill, corner, controls::band_color(roles, Band::Positive));

        let warning_right_db = peak_db.min(boundary_db);
        if warning_right_db > controls::BAND_WARNING_DB {
            let warn_left = rect.left() + rect.width() * fraction_of(controls::BAND_WARNING_DB);
            let warn_right = rect.left() + rect.width() * fraction_of(warning_right_db);
            painter.rect_filled(
                Rect::from_min_max(pos2(warn_left, rect.top()), pos2(warn_right, rect.bottom())),
                0.0,
                controls::band_color(roles, Band::Warning),
            );
        }

        if peak_db >= boundary_db {
            let danger_left = rect.left() + rect.width() * fraction_of(boundary_db);
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(danger_left, rect.top()),
                    pos2(fill_right, rect.bottom()),
                ),
                0.0,
                controls::band_color(roles, Band::Danger),
            );
        }

        // Scale marks: -6 dB, 0 dB (inset, K6) and the ceiling tick — one
        // colour rule for all three (K3), the ceiling tick only thicker
        // (K2). `warn_fg_color` is no longer used (K5): over a `warning`
        // band it would be invisible.
        let minus6_x = rect.left() + rect.width() * fraction_of(controls::BAND_WARNING_DB);
        paint_mark(
            painter,
            rect,
            minus6_x,
            fill_right,
            controls::SCALE_MARK_WIDTH,
            roles,
        );

        let zero_x = rect.right() - controls::SCALE_MARK_WIDTH;
        paint_mark(
            painter,
            rect,
            zero_x,
            fill_right,
            controls::SCALE_MARK_WIDTH,
            roles,
        );

        let ceiling_x = rect.left() + rect.width() * fraction_of(boundary_db);
        paint_mark(
            painter,
            rect,
            ceiling_x,
            fill_right,
            controls::CEILING_MARK_WIDTH,
            roles,
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
    // 014-design-tokens-and-type-scale (US3, T041): the dB reading is this
    // widget's one numeric readout — `mono` so its digits share one
    // advance width, matching every other numeric readout's column.
    response.on_hover_text(theme::mono_text(accessible_value));
}

/// One scale mark, shared by the −6 dB / 0 dB marks and the ceiling tick
/// (contract K3): `surface.base` where the fill has reached `x` (a gap cut
/// through the band), `text.secondary` where it has not. `width`
/// distinguishes a scale mark from the ceiling tick (K2).
fn paint_mark(
    painter: &egui::Painter,
    rect: Rect,
    x: f32,
    fill_right: f32,
    width: f32,
    roles: &Roles,
) {
    let filled = fill_right >= x;
    painter.line_segment(
        [pos2(x, rect.top()), pos2(x, rect.bottom())],
        Stroke::new(width, controls::mark_color(roles, filled)),
    );
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

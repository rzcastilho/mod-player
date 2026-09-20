// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `knob` panel widget kind (011-plugin-ui-contributions,
//! contracts/ui-panels.md §3: "`knob` | `widgets::knob` | `Slider` ·
//! label · number | same as slider | same as slider"): a rotary painter
//! drawn *over* a plain, horizontal `egui::Slider`'s own
//! [`egui::Response`] — the slider itself supplies drag, its native
//! `←/→` stepping, focus and AccessKit role/name/value for free; this
//! module only replaces its default groove-and-handle visual with a
//! circular dial. `plugin_panels.rs`'s shared numeric-widget helper layers
//! the `↑/↓`/`PageUp`/`PageDown` steps and the drag-commit-on-release rule
//! on top of the returned `Response` identically for `slider` and `knob`
//! (contracts/ui-panels.md §3's "same as slider"), so a knob is exactly as
//! accessible and keyboard-operable as a slider without re-implementing
//! any of that (Constitution X: reuse the toolkit). No colour literal
//! appears here (contracts/ui-panels.md A4): every stroke/fill comes from
//! `ui.visuals()`.

use std::ops::RangeInclusive;

use egui::{Slider, Stroke, Ui};

/// Roughly a 270° sweep, centred at the bottom (like most hardware
/// knobs): from -135° to +135° measured from straight up.
const SWEEP_RADIANS: f32 = 3.0 * std::f32::consts::FRAC_PI_2;
const START_RADIANS: f32 = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI - SWEEP_RADIANS / 2.0;

/// Draw a knob bound to `*value` over `range` stepped by `step`, sized to
/// `diameter`, with `label` as its AccessKit name (contracts/ui-panels.md
/// A1: "Tempo, slider, 120") — `Slider::text` is what
/// `WidgetInfo::slider` actually reads, regardless of `show_value`, so a
/// knob's accessible name/value are real even with no visible number.
/// Returns the underlying `Slider`'s own `Response` — callers read
/// `changed()`/`dragged()`/`drag_stopped()`/`has_focus()` on it exactly as
/// they would for `widgets::volume`'s slider.
pub fn knob(
    ui: &mut Ui,
    value: &mut f64,
    range: RangeInclusive<f64>,
    step: f64,
    diameter: f32,
    label: &str,
) -> egui::Response {
    // Horizontal (egui's default orientation), not vertical: its native
    // key handling then steps on `←/→`, matching a plain `slider`'s own
    // native axis — `plugin_panels.rs`'s shared numeric-widget helper adds
    // `↑/↓` on top for both kinds identically (contracts/ui-panels.md §3).
    let response = ui.add_sized(
        egui::vec2(diameter, diameter),
        Slider::new(value, range.clone())
            .step_by(step.max(f64::EPSILON))
            .show_value(false)
            .text(label.to_string()),
    );

    if ui.is_rect_visible(response.rect) {
        let painter = ui.painter();
        let visuals = ui.visuals();
        let center = response.rect.center();
        let radius = response.rect.width().min(response.rect.height()) / 2.0 - 2.0;

        // Erase the slider's own groove/handle, then draw the dial face
        // and pointer purely from `ui.visuals()` (A4: no colour literal).
        painter.circle_filled(center, radius, visuals.extreme_bg_color);
        painter.circle_stroke(center, radius, visuals.window_stroke);

        let span = range.end() - range.start();
        let fraction = if span > 0.0 {
            ((*value - range.start()) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let angle = START_RADIANS + fraction as f32 * SWEEP_RADIANS;
        let pointer_color = if response.has_focus() {
            visuals.selection.bg_fill
        } else {
            visuals.text_color()
        };
        let tip = center + radius * 0.85 * egui::vec2(angle.cos(), angle.sin());
        painter.line_segment([center, tip], Stroke::new(2.0, pointer_color));
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_covers_three_quarter_turn() {
        assert!((SWEEP_RADIANS - 3.0 * std::f32::consts::FRAC_PI_2).abs() < f32::EPSILON);
    }
}

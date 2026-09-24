// SPDX-License-Identifier: MIT OR Apache-2.0

//! WCAG 2.x contrast and the exact alpha-compositing egui paints with
//! (research R16, contracts/design-tokens.md C1-C7): shared by the token
//! module (disabled-alpha selection, research R11) and
//! `tests/design_token_contrast.rs`.

use egui::Color32;

/// WCAG 2.x relative luminance: sRGB -> linear with the 0.04045 knee,
/// weighted 0.2126/0.7152/0.0722.
pub fn relative_luminance(c: Color32) -> f32 {
    fn linear(channel: u8) -> f32 {
        let c = f32::from(channel) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
}

/// WCAG 2.x contrast ratio between two colours: symmetric, `1.0` for equal
/// colours, `21.0` for black vs white.
///
/// ```
/// # use modplayer_ui::theme::contrast::ratio;
/// # use egui::Color32;
/// assert!((ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
/// assert_eq!(ratio(Color32::BLACK, Color32::WHITE), ratio(Color32::WHITE, Color32::BLACK));
/// ```
pub fn ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// The colour egui actually paints for `fg` at `alpha` over `bg`
/// (`Ui::disable()`'s path, research R11): the same premultiplied blend
/// `epaint::Color32::blend` uses, not an idealised alpha model.
pub fn composite(fg: Color32, alpha: f32, bg: Color32) -> Color32 {
    bg.blend(fg.gamma_multiply(alpha))
}

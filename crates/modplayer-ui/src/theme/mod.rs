// SPDX-License-Identifier: MIT OR Apache-2.0

//! Theme application (contracts/ui-surface.md "Theme", FR-017):
//! `Theme::System` tracks the OS theme live (egui/winit do this natively
//! once `ThemePreference::System` is set — research R9, no custom polling
//! needed); `Light`/`Dark` are fixed regardless of OS changes. `apply` is
//! called once at startup, before the first frame (`app.rs`'s `App::new`),
//! and would be called again immediately on any future change (the
//! Appearance settings screen that lets the user change it lands in US5).
//!
//! `apply_tokens` (014-design-tokens-and-type-scale, FR-002, research R7)
//! installs the complete token `Style` for both `egui::Theme`s: built once
//! into a `OnceLock<(Arc<Style>, Arc<Style>)>`, then two `set_style_of`
//! calls — idempotent and allocation-free after the first call. It is
//! called from `App::new` before the first paint and as the first
//! statement of every frame (`App::ui`), so a token change or a theme
//! switch is visible everywhere on the next frame with zero per-view code
//! (design note 4).

use std::sync::{Arc, OnceLock};

use egui::{Context, Style, Theme as EguiTheme, ThemePreference};
use modplayer_engine::Theme;

pub mod contrast;
mod markers;
pub mod style;
pub mod tokens;

pub use markers::{MARKER_PALETTE, marker_color, overlay_color, paint_host_glyph};
pub use tokens::{
    Roles, body_measure, divider_color, initials_font_id, mono_font_id, mono_text, radius, roles,
    secondary_font_id, section_label, space, text,
};

/// Map the domain `Theme` to egui's `ThemePreference` and apply it to `ctx`.
pub fn apply(ctx: &Context, theme: Theme) {
    ctx.set_theme(to_preference(theme));
}

fn to_preference(theme: Theme) -> ThemePreference {
    match theme {
        Theme::System => ThemePreference::System,
        Theme::Light => ThemePreference::Light,
        Theme::Dark => ThemePreference::Dark,
    }
}

static STYLES: OnceLock<(Arc<Style>, Arc<Style>)> = OnceLock::new();

/// Install both themes' complete token `Style` (research R7, contract A1):
/// idempotent and allocation-free after the first call. Call from
/// `App::new` before the first paint and as the first statement of every
/// frame (contract A2).
pub fn apply_tokens(ctx: &Context) {
    let (light, dark) = STYLES.get_or_init(|| {
        (
            Arc::new(style::build_style(EguiTheme::Light)),
            Arc::new(style::build_style(EguiTheme::Dark)),
        )
    });
    ctx.set_style_of(EguiTheme::Light, Arc::clone(light));
    ctx.set_style_of(EguiTheme::Dark, Arc::clone(dark));
}

/// The 1 px in-panel rule (FR-008): drawn across the available width in
/// [`divider_color`]. Panel-to-panel separation uses
/// `ui.add_space(space::XL)` instead (research R20) — call sites choose
/// which; this only draws the line.
pub fn divider(ui: &mut egui::Ui) {
    let color = divider_color(ui.visuals());
    let available = ui.available_size_before_wrap();
    let (rect, _response) =
        ui.allocate_at_least(egui::vec2(available.x, 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.left()..=rect.right(),
        rect.center().y,
        egui::Stroke::new(1.0, color),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_theme_to_its_preference() {
        assert_eq!(to_preference(Theme::System), ThemePreference::System);
        assert_eq!(to_preference(Theme::Light), ThemePreference::Light);
        assert_eq!(to_preference(Theme::Dark), ThemePreference::Dark);
    }
}

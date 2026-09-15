// SPDX-License-Identifier: MIT OR Apache-2.0

//! Theme application (contracts/ui-surface.md "Theme", FR-017):
//! `Theme::System` tracks the OS theme live (egui/winit do this natively
//! once `ThemePreference::System` is set — research R9, no custom polling
//! needed); `Light`/`Dark` are fixed regardless of OS changes. `apply` is
//! called once at startup, before the first frame (`app.rs`'s `App::new`),
//! and would be called again immediately on any future change (the
//! Appearance settings screen that lets the user change it lands in US5).

use egui::{Context, ThemePreference};
use modplayer_engine::Theme;

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

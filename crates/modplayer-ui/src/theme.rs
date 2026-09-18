// SPDX-License-Identifier: MIT OR Apache-2.0

//! Theme application (contracts/ui-surface.md "Theme", FR-017):
//! `Theme::System` tracks the OS theme live (egui/winit do this natively
//! once `ThemePreference::System` is set — research R9, no custom polling
//! needed); `Light`/`Dark` are fixed regardless of OS changes. `apply` is
//! called once at startup, before the first frame (`app.rs`'s `App::new`),
//! and would be called again immediately on any future change (the
//! Appearance settings screen that lets the user change it lands in US5).

use egui::{Color32, Context, ThemePreference};
use modplayer_core::markers::PaletteIndex;
use modplayer_engine::Theme;

/// Map the domain `Theme` to egui's `ThemePreference` and apply it to `ctx`.
pub fn apply(ctx: &Context, theme: Theme) {
    ctx.set_theme(to_preference(theme));
}

/// The fixed 8-colour marker/loop-region accent palette (006, research
/// R13, contracts/ui-markers.md §6): the only place these literals
/// appear (005's "no new colour literals outside `theme.rs`" rule). The
/// persisted value is a [`PaletteIndex`] into this array, never a colour
/// itself, so a marker's colour stays stable across a theme switch.
/// Chosen for >= 3:1 contrast against both the light and dark panel
/// backgrounds. Defaults by kind (data-model.md §1.3): loop region `0`,
/// point `1`, cue `2`.
pub const MARKER_PALETTE: [Color32; 8] = [
    Color32::from_rgb(0xD9, 0x4F, 0x4F), // 0 red    — loop region default
    Color32::from_rgb(0x3E, 0x8E, 0xDE), // 1 blue   — point default
    Color32::from_rgb(0x4C, 0xAF, 0x50), // 2 green  — cue default
    Color32::from_rgb(0xE0, 0x9B, 0x1A), // 3 amber
    Color32::from_rgb(0x9C, 0x5C, 0xE0), // 4 violet
    Color32::from_rgb(0x00, 0xAC, 0xB0), // 5 teal
    Color32::from_rgb(0xDE, 0x6F, 0xB4), // 6 pink
    Color32::from_rgb(0x8F, 0x9A, 0x4B), // 7 olive
];

/// The theme colour for a marker's `PaletteIndex` (006, contracts/
/// ui-markers.md §6). `PaletteIndex` is already clamped to `0..=7`, so
/// this never falls back to a default entry.
pub fn marker_color(index: PaletteIndex) -> Color32 {
    MARKER_PALETTE[usize::from(index.get())]
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

    #[test]
    fn marker_color_indexes_the_fixed_palette() {
        for raw in 0..8u8 {
            let index = PaletteIndex::new(raw);
            assert_eq!(marker_color(index), MARKER_PALETTE[usize::from(raw)]);
        }
    }

    #[test]
    fn marker_palette_has_eight_distinct_colours() {
        let mut seen = std::collections::HashSet::new();
        for color in MARKER_PALETTE {
            assert!(
                seen.insert(color.to_array()),
                "duplicate palette entry {color:?}"
            );
        }
        assert_eq!(seen.len(), MARKER_PALETTE.len());
    }
}

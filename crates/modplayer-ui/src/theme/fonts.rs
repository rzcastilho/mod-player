// SPDX-License-Identifier: MIT OR Apache-2.0

//! App font installation (029-keyboard-shortcut-reference, research R3).
//!
//! egui's default fonts lack the macOS modifier glyphs `⇧` and `⌥`, so
//! key labels would render as tofu. [`ensure_installed`] appends a
//! 3-glyph OFL subset (`ModPlayerKeys`: `⌘ ⇧ ⌥`) and egui's own `Hack`
//! (which covers the arrows) at the **end** of the Proportional and
//! Monospace fallback lists. Appending means no glyph that resolves
//! today changes its font; only glyphs that were tofu gain a source.

use std::sync::Arc;

use egui::{Context, FontData, FontDefinitions, FontFamily};

/// Registered name of the key-symbol subset.
const KEYS_FONT: &str = "ModPlayerKeys";
/// egui's built-in monospace font, reused for the arrow glyphs.
const HACK_FONT: &str = "Hack";

static KEYS_TTF: &[u8] = include_bytes!("../../assets/fonts/ModPlayerKeys-Regular.ttf");

/// egui's default font set plus the app additions, appended after every
/// existing fallback entry (a name already in a family is not repeated).
///
/// ```
/// # use egui::{FontDefinitions, FontFamily};
/// let defs = modplayer_ui::theme::fonts::definitions();
/// let base = FontDefinitions::default();
/// let family = &defs.families[&FontFamily::Proportional];
/// assert!(family.starts_with(&base.families[&FontFamily::Proportional]));
/// assert!(family.iter().any(|name| name == "ModPlayerKeys"));
/// ```
pub fn definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    defs.font_data.insert(
        KEYS_FONT.to_owned(),
        Arc::new(FontData::from_static(KEYS_TTF)),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let names = defs.families.entry(family).or_default();
        for name in [KEYS_FONT, HACK_FONT] {
            if !names.iter().any(|existing| existing == name) {
                names.push(name.to_owned());
            }
        }
    }
    defs
}

/// Install [`definitions`] on `ctx` once per `Context`; later calls are a
/// cheap flag check (called from `theme::apply_tokens_for` every frame).
pub fn ensure_installed(ctx: &Context) {
    let flag = egui::Id::new("modplayer-fonts-installed");
    let installed = ctx.data(|d| d.get_temp::<bool>(flag)).unwrap_or(false);
    if installed {
        return;
    }
    ctx.set_fonts(definitions());
    ctx.data_mut(|d| d.insert_temp(flag, true));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// K11: existing entries and their order are untouched; the new
    /// fonts are appended.
    #[test]
    fn new_fonts_are_appended_without_reordering() {
        let base = FontDefinitions::default();
        let defs = definitions();
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            let before = &base.families[&family];
            let after = &defs.families[&family];
            assert_eq!(&after[..before.len()], &before[..], "{family:?} reordered");
            let tail: Vec<&str> = after[before.len()..].iter().map(String::as_str).collect();
            let keys_at = after.iter().position(|n| n == KEYS_FONT);
            assert_eq!(
                keys_at,
                Some(before.len()),
                "{family:?}: keys font not first appended"
            );
            assert!(after.iter().any(|n| n == HACK_FONT));
            assert!(!tail.is_empty());
            let mut seen = std::collections::HashSet::new();
            assert!(
                after.iter().all(|n| seen.insert(n)),
                "{family:?}: duplicates"
            );
        }
        assert!(defs.font_data.contains_key(KEYS_FONT));
    }

    #[test]
    fn ensure_installed_is_idempotent() {
        let ctx = Context::default();
        ensure_installed(&ctx);
        ensure_installed(&ctx);
        let flag = egui::Id::new("modplayer-fonts-installed");
        assert_eq!(ctx.data(|d| d.get_temp::<bool>(flag)), Some(true));
    }
}

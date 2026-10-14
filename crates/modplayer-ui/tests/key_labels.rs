// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Key label coverage (029-keyboard-shortcut-reference, contracts K7/K8):
//! every chord the app can display renders with glyphs the bundled fonts
//! have, and the en-US label equals core's `Chord::display`.

use egui::{Context, Pos2, RawInput, Rect, TextStyle};
use modplayer_core::{Chord, KEY_NAMES, KeyName, Mods, Platform};
use modplayer_ui::key_label::KeyLabel;

fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx
}

fn input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

/// All `KEY_NAMES` × 8 `Mods` combinations.
fn all_chords() -> Vec<Chord> {
    let mut out = Vec::new();
    for name in KEY_NAMES {
        let key = KeyName::parse(name).unwrap();
        for bits in 0u8..8 {
            let mods = Mods {
                primary: bits & 1 != 0,
                shift: bits & 2 != 0,
                alt: bits & 4 != 0,
            };
            out.push(Chord::new(mods, key));
        }
    }
    out
}

/// K7: the label of every chord on both platforms is non-empty, free of
/// replacement/tofu characters, and covered by the proportional body font
/// after the app fonts are installed.
#[test]
fn every_key_label_is_covered_by_app_fonts() {
    let ctx = fresh_ctx();
    // Fonts set by `apply_tokens` take effect on the next frame.
    ctx.run_ui(input(), |_ui| {}).drop_without_applying_deltas();
    let body = TextStyle::Body.resolve(&ctx.global_style());
    let chords = all_chords();
    assert_eq!(chords.len(), KEY_NAMES.len() * 8);
    for platform in [Platform::Mac, Platform::Other] {
        for chord in &chords {
            let label = KeyLabel::new(*chord, platform);
            assert!(!label.visible.is_empty(), "{} empty", chord.encode());
            assert!(
                !label.visible.contains(['\u{FFFD}', '\u{25A1}']),
                "{} contains a tofu character",
                chord.encode()
            );
            let covered = ctx.fonts_mut(|fonts| fonts.has_glyphs(&body, &label.visible));
            assert!(
                covered,
                "{:?} ({} on {platform:?}) has glyphs missing from the bundled fonts",
                label.visible,
                chord.encode()
            );
        }
    }
}

/// K8: under en-US the visible label is exactly core's `display`.
#[test]
fn visible_label_matches_core_display() {
    for platform in [Platform::Mac, Platform::Other] {
        for chord in all_chords() {
            assert_eq!(
                KeyLabel::new(chord, platform).visible,
                chord.display(platform),
                "{} on {platform:?}",
                chord.encode()
            );
        }
    }
}

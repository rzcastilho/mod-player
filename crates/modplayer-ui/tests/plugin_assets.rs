// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `plugin_assets::show_icon`/`show_glyph` (011-plugin-ui-contributions,
//! research R11/R18) driven headlessly with a decoded asset present, so
//! the texture *upload* path — not just the generic-glyph fallback — is
//! exercised on every run rather than only when an off-thread decode
//! happens to land before the first paint.

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{Context, Pos2, RawInput, Rect, vec2};
use modplayer_core::plugins::PluginId;
use modplayer_core::plugins::ui::assets::{DecodedPng, PluginAssets};
use modplayer_ui::plugin_assets;

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0))),
        ..Default::default()
    }
}

/// A 2×2 opaque red square, already decoded (what `PluginAssets::load`
/// hands the UI).
fn red_png() -> DecodedPng {
    DecodedPng {
        width: 2,
        height: 2,
        rgba: Arc::from([255u8, 0, 0, 255].repeat(4)),
    }
}

/// Regression: the first paint of an asset used to upload its texture
/// from *inside* `ctx.memory_mut(..)`, and `load_texture` takes the same
/// `Context` lock again — a deadlock on the UI thread (egui's debug build
/// bails out after 10 s; release hangs). The whole test is "this returns".
#[test]
fn first_paint_of_an_icon_and_glyph_uploads_without_deadlocking() {
    let mut glyphs = BTreeMap::new();
    glyphs.insert("loop".to_string(), red_png());
    let assets = PluginAssets {
        icon: Some(red_png()),
        glyphs,
    };
    let ctx = Context::default();

    // Two frames: the first uploads (cache miss), the second must hit the
    // cache and upload nothing new.
    for _ in 0..2 {
        ctx.run_ui(default_input(), |ui| {
            plugin_assets::show_icon(ui, PluginId(0), &assets, 16.0);
            plugin_assets::show_glyph(ui, PluginId(0), &assets, "loop", 16.0);
        })
        .drop_without_applying_deltas();
    }
    let textures = ctx.tex_manager().read().allocated().len();
    // The font atlas plus exactly the icon and the glyph.
    assert_eq!(
        textures, 3,
        "one texture per distinct asset, uploaded once and then cached"
    );
}

/// A glyph key the plugin never shipped falls back to the generic glyph
/// and allocates no texture at all.
#[test]
fn missing_glyph_falls_back_without_uploading() {
    let assets = PluginAssets {
        icon: None,
        glyphs: BTreeMap::new(),
    };
    let ctx = Context::default();
    ctx.run_ui(default_input(), |ui| {
        plugin_assets::show_glyph(ui, PluginId(0), &assets, "nope", 16.0);
    })
    .drop_without_applying_deltas();
    assert_eq!(
        ctx.tex_manager().read().allocated().len(),
        1,
        "only the font atlas"
    );
}

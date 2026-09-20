// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plugin icon/glyph textures (011-plugin-ui-contributions, FR-014a,
//! data-model.md §4.6): a per-viewer `TextureHandle` cache keyed by
//! `(PluginId, asset key)`, plus the generic-glyph fallback every header
//! and overlay icon draws when no real asset resolved (which, until a
//! bundled package embeds resources, research R18, is every plugin today
//! — `PluginAssets::icon`/`glyphs` stay empty; `modplayer-core::plugins::
//! ui::assets::load` already warns and omits, this module just never has
//! anything to upload yet). Decoding itself happened once, at plugin
//! discovery (`PluginAssets`, T032); this cache only avoids re-uploading
//! the same already-decoded pixels to the GPU on every frame.
//!
//! The cache lives in `egui::Context` memory (mirrors `actions::
//! FocusClaims`'s own pattern) rather than as `App`-owned state passed
//! through every draw call: `now_playing.rs`/`plugin_panels.rs` only ever
//! have a `&mut Ui`, and threading a new cache field through their public
//! signatures would ripple out to `app.rs`, which no Phase 3 task touches.

use std::collections::HashMap;

use egui::{
    ColorImage, Context, Id, Sense, TextureHandle, TextureOptions, Ui, Vec2, WidgetInfo, WidgetType,
};
use modplayer_core::plugins::ui::assets::DecodedPng;
use modplayer_core::{PluginAssets, PluginId, tr};

/// One cached asset: a plugin id plus which of its assets ("icon" or a
/// named glyph).
type CacheKey = (PluginId, String);

/// The per-viewer texture cache (see module doc for why it lives in `ctx`
/// memory rather than `App` state).
#[derive(Clone, Default)]
struct PluginAssetCache {
    textures: HashMap<CacheKey, TextureHandle>,
}

fn cache_memory_id() -> Id {
    Id::new("modplayer-ui::plugin-asset-cache")
}

/// `png`'s pixels uploaded as a fresh `TextureHandle` (never re-decoded —
/// `png.rgba` is already straight RGBA8, `PluginAssets::load`'s one-time
/// job).
fn upload(ctx: &Context, name: &str, png: &DecodedPng) -> TextureHandle {
    let size = [png.width as usize, png.height as usize];
    let image = ColorImage::from_rgba_unmultiplied(size, &png.rgba);
    ctx.load_texture(name.to_string(), image, TextureOptions::LINEAR)
}

/// The cached (or freshly uploaded) texture for one of `assets`' entries,
/// `None` if `assets` has nothing under `key` (the common case today).
fn cached_texture(
    ctx: &Context,
    plugin: PluginId,
    key: &str,
    png: Option<&DecodedPng>,
) -> Option<TextureHandle> {
    let png = png?;
    ctx.memory_mut(|memory| {
        let cache = memory
            .data
            .get_temp_mut_or_default::<PluginAssetCache>(cache_memory_id());
        let handle = cache
            .textures
            .entry((plugin, key.to_string()))
            .or_insert_with(|| upload(ctx, key, png));
        Some(handle.clone())
    })
}

/// L5/FR-014a: draw a plugin's header icon at `size` — its own decoded
/// icon if one resolved, [`generic_glyph`] otherwise. Always exposes a
/// non-empty accessible name (A5/NFR-6.2): a decorative image is never
/// silently nameless to a screen reader.
pub fn show_icon(ui: &mut Ui, plugin: PluginId, assets: &PluginAssets, size: f32) {
    let ctx = ui.ctx().clone();
    if let Some(handle) = cached_texture(&ctx, plugin, "icon", assets.icon.as_ref()) {
        let response = ui.add(egui::Image::new(&handle).fit_to_exact_size(Vec2::splat(size)));
        let desc = tr("plugin-generic-glyph-desc");
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Image, true, desc.clone()));
        return;
    }
    generic_glyph(ui, size);
}

/// One of `assets.glyphs`' entries at `size`, by its manifest key —
/// `generic_glyph` if `key` never resolved (over-limit, missing, or not
/// yet embedded, research R18).
pub fn show_glyph(ui: &mut Ui, plugin: PluginId, assets: &PluginAssets, key: &str, size: f32) {
    let ctx = ui.ctx().clone();
    if let Some(handle) = cached_texture(&ctx, plugin, key, assets.glyphs.get(key)) {
        ui.add(egui::Image::new(&handle).fit_to_exact_size(Vec2::splat(size)));
        return;
    }
    generic_glyph(ui, size);
}

/// As [`show_glyph`], for a raw `Painter`-based draw (011-plugin-ui-
/// contributions US3, `plugin_overlays.rs`): a plugin overlay's `glyph`
/// primitive has only a `Painter`/`TimeSpace`, never a `Ui` to draw an
/// `egui::Image` widget through. `None` if `key` never resolved — the
/// caller's own job to fall back to the generic glyph (`theme::
/// paint_host_glyph(.., HostGlyph::Dot, ..)`, exactly like
/// [`generic_glyph`]'s own weak-text-colour circle).
#[must_use]
pub fn glyph_texture_id(
    ctx: &Context,
    plugin: PluginId,
    key: &str,
    assets: &PluginAssets,
) -> Option<egui::TextureId> {
    cached_texture(ctx, plugin, key, assets.glyphs.get(key)).map(|handle| handle.id())
}

/// A plugin's icon at `size`, read-only (011-plugin-ui-contributions US5,
/// `notifications.rs`'s own attribution icon, contracts/overlays-
/// settings-notify.md §3 N3): the notification area has no `PluginAssets`
/// of its own to upload from (only a `PluginId`), so this only ever
/// serves what some earlier draw this frame or session (a panel header,
/// `plugins_view`'s own row) already cached under `(plugin, "icon")` —
/// [`generic_glyph`] otherwise, which today (research R18, no bundled
/// package embeds resources yet) is every plugin, exactly like
/// [`show_icon`]'s own common case.
pub fn show_cached_icon_or_generic(ui: &mut Ui, plugin: PluginId, size: f32) {
    let ctx = ui.ctx().clone();
    let cached = ctx.memory_mut(|memory| {
        memory
            .data
            .get_temp_mut_or_default::<PluginAssetCache>(cache_memory_id())
            .textures
            .get(&(plugin, "icon".to_string()))
            .cloned()
    });
    if let Some(handle) = cached {
        let response = ui.add(egui::Image::new(&handle).fit_to_exact_size(Vec2::splat(size)));
        let desc = tr("plugin-generic-glyph-desc");
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Image, true, desc.clone()));
        return;
    }
    generic_glyph(ui, size);
}

/// The neutral fallback glyph (FR-014a): a plain filled circle in the
/// current theme's own weak-text colour — no colour literal
/// (contracts/ui-panels.md A4) — with `plugin-generic-glyph-desc` as its
/// sole accessible name.
pub fn generic_glyph(ui: &mut Ui, size: f32) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().circle_filled(
            rect.center(),
            size / 2.0 - 1.0,
            ui.visuals().weak_text_color(),
        );
    }
    let desc = tr("plugin-generic-glyph-desc");
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Image, true, desc.clone()));
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The fixed marker/loop-region accent palette and the plugin overlay
//! colour/glyph primitives that key off it (006, 011-plugin-ui-
//! contributions). Kept separate from the rest of `theme/` because these
//! values are `PaletteIndex`-keyed host primitives (Constitution III), not
//! part of the semantic colour-role scale the other `theme/` modules build.

use egui::{Color32, Painter, Pos2, Rect, Stroke, Visuals, pos2, vec2};
use modplayer_capability_gateway::ui::{HostGlyph, OverlayColor};
use modplayer_core::markers::PaletteIndex;

/// The fixed 8-colour marker/loop-region accent palette (006, research
/// R14, contracts/ui-markers.md §6): the only place these literals
/// appear (005's "no new colour literals outside `theme.rs`" rule). The
/// persisted value is a [`PaletteIndex`] into this array, never a colour
/// itself, so a marker's colour stays stable across a theme switch.
/// Verified >= 3:1 contrast against both `surface.base` values
/// (`#ffffff` light, `#141417` dark — data-model.md §5.4); indices 2, 3,
/// 5, 6 were re-toned to clear that floor (research R14), 0, 1, 4, 7 are
/// unchanged. Defaults by kind (data-model.md §1.3): loop region `0`,
/// point `1`, cue `2`.
pub const MARKER_PALETTE: [Color32; 8] = [
    Color32::from_rgb(0xD9, 0x4F, 0x4F), // 0 red    — loop region default
    Color32::from_rgb(0x3E, 0x8E, 0xDE), // 1 blue   — point default
    Color32::from_rgb(0x3D, 0x8B, 0x43), // 2 green  — cue default (re-toned)
    Color32::from_rgb(0xB0, 0x71, 0x1A), // 3 amber  (re-toned)
    Color32::from_rgb(0x9C, 0x5C, 0xE0), // 4 violet
    Color32::from_rgb(0x00, 0x86, 0x8A), // 5 teal   (re-toned)
    Color32::from_rgb(0xC2, 0x51, 0x9A), // 6 pink   (re-toned)
    Color32::from_rgb(0x8F, 0x9A, 0x4B), // 7 olive
];

/// The theme colour for a marker's `PaletteIndex` (006, contracts/
/// ui-markers.md §6). `PaletteIndex` is already clamped to `0..=7`, so
/// this never falls back to a default entry.
pub fn marker_color(index: PaletteIndex) -> Color32 {
    MARKER_PALETTE[usize::from(index.get())]
}

/// 011-plugin-ui-contributions (O8, contracts/overlays-settings-notify.md
/// §1.2): the fixed mapping from a plugin overlay primitive's colour
/// *token* to an actual `Color32` — the only place besides
/// `MARKER_PALETTE` a colour literal appears for a plugin-drawn primitive
/// (contracts/ui-panels.md A4: no colour literal outside `theme.rs`).
/// `accent`/`secondary`/`neutral` follow the current `Visuals` (so a
/// plugin's overlay re-themes exactly like everything else); `positive`/
/// `warning` reuse two fixed `MARKER_PALETTE` entries so a plugin's own
/// "good"/"caution" reads consistently with a host marker's.
#[must_use]
pub fn overlay_color(token: OverlayColor, visuals: &Visuals) -> Color32 {
    match token {
        OverlayColor::Accent => visuals.selection.bg_fill,
        OverlayColor::Secondary => visuals.hyperlink_color,
        OverlayColor::Positive => MARKER_PALETTE[2],
        OverlayColor::Warning => MARKER_PALETTE[3],
        OverlayColor::Neutral => visuals.weak_text_color(),
    }
}

/// FR-012 (017-high-contrast-appearance): the palette-outline stroke
/// width. Chosen over a thicker stroke so a glyph's own focused/unfocused
/// difference (2.0 vs 1.5 px, research R8) is not swamped.
pub const MARKER_OUTLINE_WIDTH: f32 = 1.0;

/// FR-011/FR-012 (017-high-contrast-appearance): `Some` only in high
/// contrast. The colour is the active theme's `text_primary` — its
/// extreme luminance end, so it holds >= 7:1 against both waveform
/// surfaces whichever of the eight palette entries it encircles
/// (research R6).
pub fn marker_outline(roles: &super::tokens::Roles) -> Option<Stroke> {
    roles
        .high_contrast
        .then(|| Stroke::new(MARKER_OUTLINE_WIDTH, roles.text_primary))
}

/// FR-011 (017-high-contrast-appearance): `Some` only where the resolved
/// overlay colour is palette *data*. `Accent`/`Secondary`/`Neutral`
/// resolve through the recoloured roles and are excluded unconditionally
/// (research R9) — they are already at their high-contrast value, so an
/// outline would only obscure it.
pub fn overlay_outline(token: OverlayColor, visuals: &Visuals) -> Option<Stroke> {
    match token {
        OverlayColor::Positive | OverlayColor::Warning => {
            marker_outline(super::tokens::roles(visuals))
        }
        OverlayColor::Accent | OverlayColor::Secondary | OverlayColor::Neutral => None,
    }
}

/// The casing width for a stroked shape (research R8): the palette
/// stroke drawn `2 * MARKER_OUTLINE_WIDTH` wider, underneath, so the
/// focused/unfocused delta survives the casing uniformly.
pub fn casing_width(base: f32) -> f32 {
    base + 2.0 * MARKER_OUTLINE_WIDTH
}

/// 011-plugin-ui-contributions (O12), outlined per O10
/// (017-high-contrast-appearance): the six host-drawn glyphs a plugin's
/// `glyph` overlay primitive can name without shipping its own asset —
/// plain vector shapes (no bitmap), so they render identically regardless
/// of theme and never depend on the `image` crate. `center` is the
/// glyph's on-screen centre; `size` its square bounding box. `visuals`
/// selects the active theme's `text_on_accent` for the `Warning` glyph's
/// exclamation mark (014-design-tokens-and-type-scale, U5, research R15):
/// a fixed white mark is wrong on a light-theme warning triangle.
/// `outline` is `Some` only when the caller resolved a palette-data
/// colour in high contrast (`overlay_outline`, O10); every filled form
/// gains a casing underneath at `MARKER_OUTLINE_WIDTH` extra radius/
/// thickness, every stroked form takes the outline as its own stroke
/// (mirrors O1-O2's `casing_width`/"stroke becomes the outline colour"
/// techniques) — the exclamation mark (a role, not palette data) is
/// never touched, same as the cue glyph's digit (O3, FR-020).
pub fn paint_host_glyph(
    painter: &Painter,
    glyph: HostGlyph,
    center: Pos2,
    size: f32,
    color: Color32,
    visuals: &Visuals,
    outline: Option<Stroke>,
) {
    let half = size / 2.0;
    match glyph {
        HostGlyph::Dot => {
            let radius = half * 0.7;
            if let Some(outline) = outline {
                painter.circle_filled(center, radius + MARKER_OUTLINE_WIDTH, outline.color);
            }
            painter.circle_filled(center, radius, color);
        }
        HostGlyph::Flag => {
            let pole_x = center.x - half * 0.5;
            let pole = [pos2(pole_x, center.y + half), pos2(pole_x, center.y - half)];
            if let Some(outline) = outline {
                painter.line_segment(pole, Stroke::new(casing_width(1.5), outline.color));
            }
            painter.line_segment(pole, Stroke::new(1.5, color));
            let points = vec![
                pos2(pole_x, center.y - half),
                pos2(pole_x, center.y),
                pos2(pole_x + size * 0.6, center.y - half * 0.5),
            ];
            painter.add(egui::Shape::convex_polygon(
                points,
                color,
                outline.unwrap_or(Stroke::NONE),
            ));
        }
        HostGlyph::Note => {
            let head = pos2(center.x - half * 0.3, center.y + half * 0.4);
            let radius = half * 0.5;
            if let Some(outline) = outline {
                painter.circle_filled(head, radius + MARKER_OUTLINE_WIDTH, outline.color);
            }
            painter.circle_filled(head, radius, color);
            let stem = [
                pos2(head.x + half * 0.45, head.y),
                pos2(head.x + half * 0.45, center.y - half),
            ];
            if let Some(outline) = outline {
                painter.line_segment(stem, Stroke::new(casing_width(1.5), outline.color));
            }
            painter.line_segment(stem, Stroke::new(1.5, color));
        }
        HostGlyph::Chord => {
            let r = half * 0.4;
            for c in [
                pos2(center.x - half * 0.4, center.y),
                pos2(center.x + half * 0.4, center.y),
            ] {
                if let Some(outline) = outline {
                    painter.circle_filled(c, r + MARKER_OUTLINE_WIDTH, outline.color);
                }
                painter.circle_filled(c, r, color);
            }
        }
        HostGlyph::Star => {
            let arm = half * 0.9;
            let thickness = (size * 0.22).max(1.0);
            for bar in [
                Rect::from_center_size(center, vec2(arm * 2.0, thickness)),
                Rect::from_center_size(center, vec2(thickness, arm * 2.0)),
            ] {
                if let Some(outline) = outline {
                    painter.rect_filled(bar.expand(MARKER_OUTLINE_WIDTH), 0.0, outline.color);
                }
                painter.rect_filled(bar, 0.0, color);
            }
        }
        HostGlyph::Warning => {
            let mark_color = super::tokens::roles(visuals).text_on_accent;
            let points = vec![
                pos2(center.x, center.y - half),
                pos2(center.x - half, center.y + half),
                pos2(center.x + half, center.y + half),
            ];
            painter.add(egui::Shape::convex_polygon(
                points,
                color,
                outline.unwrap_or(Stroke::NONE),
            ));
            // The exclamation mark is `text_on_accent`, a role — never
            // outlined (O10 note, mirrors O3's cue-digit exclusion).
            painter.line_segment(
                [
                    pos2(center.x, center.y - half * 0.1),
                    pos2(center.x, center.y + half * 0.35),
                ],
                Stroke::new(1.5, mark_color),
            );
            painter.circle_filled(pos2(center.x, center.y + half * 0.6), 1.0, mark_color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_color_indexes_the_fixed_palette() {
        for raw in 0..8u8 {
            let index = PaletteIndex::new(raw);
            assert_eq!(marker_color(index), MARKER_PALETTE[usize::from(raw)]);
        }
    }

    /// O8: every token maps to a real colour, and `positive`/`warning`
    /// really do reuse `MARKER_PALETTE`'s own entries 2/3 (the mapping's
    /// documented rationale, kept honest by a test).
    #[test]
    fn overlay_color_maps_every_token() {
        let visuals = Visuals::dark();
        assert_eq!(
            overlay_color(OverlayColor::Accent, &visuals),
            visuals.selection.bg_fill
        );
        assert_eq!(
            overlay_color(OverlayColor::Secondary, &visuals),
            visuals.hyperlink_color
        );
        assert_eq!(
            overlay_color(OverlayColor::Positive, &visuals),
            MARKER_PALETTE[2]
        );
        assert_eq!(
            overlay_color(OverlayColor::Warning, &visuals),
            MARKER_PALETTE[3]
        );
        assert_eq!(
            overlay_color(OverlayColor::Neutral, &visuals),
            visuals.weak_text_color()
        );
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

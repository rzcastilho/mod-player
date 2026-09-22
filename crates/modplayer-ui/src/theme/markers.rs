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

/// 011-plugin-ui-contributions (O12): the six host-drawn glyphs a
/// plugin's `glyph` overlay primitive can name without shipping its own
/// asset — plain vector shapes (no bitmap), so they render identically
/// regardless of theme and never depend on the `image` crate. `center` is
/// the glyph's on-screen centre; `size` its square bounding box. `visuals`
/// selects the active theme's `text_on_accent` for the `Warning` glyph's
/// exclamation mark (014-design-tokens-and-type-scale, U5, research R15):
/// a fixed white mark is wrong on a light-theme warning triangle.
pub fn paint_host_glyph(
    painter: &Painter,
    glyph: HostGlyph,
    center: Pos2,
    size: f32,
    color: Color32,
    visuals: &Visuals,
) {
    let half = size / 2.0;
    match glyph {
        HostGlyph::Dot => {
            painter.circle_filled(center, half * 0.7, color);
        }
        HostGlyph::Flag => {
            let pole_x = center.x - half * 0.5;
            painter.line_segment(
                [pos2(pole_x, center.y + half), pos2(pole_x, center.y - half)],
                Stroke::new(1.5, color),
            );
            let points = vec![
                pos2(pole_x, center.y - half),
                pos2(pole_x, center.y),
                pos2(pole_x + size * 0.6, center.y - half * 0.5),
            ];
            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
        }
        HostGlyph::Note => {
            let head = pos2(center.x - half * 0.3, center.y + half * 0.4);
            painter.circle_filled(head, half * 0.5, color);
            painter.line_segment(
                [
                    pos2(head.x + half * 0.45, head.y),
                    pos2(head.x + half * 0.45, center.y - half),
                ],
                Stroke::new(1.5, color),
            );
        }
        HostGlyph::Chord => {
            let r = half * 0.4;
            painter.circle_filled(pos2(center.x - half * 0.4, center.y), r, color);
            painter.circle_filled(pos2(center.x + half * 0.4, center.y), r, color);
        }
        HostGlyph::Star => {
            let arm = half * 0.9;
            let thickness = (size * 0.22).max(1.0);
            painter.rect_filled(
                Rect::from_center_size(center, vec2(arm * 2.0, thickness)),
                0.0,
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(center, vec2(thickness, arm * 2.0)),
                0.0,
                color,
            );
        }
        HostGlyph::Warning => {
            let mark_color = super::tokens::roles(visuals).text_on_accent;
            let points = vec![
                pos2(center.x, center.y - half),
                pos2(center.x - half, center.y + half),
                pos2(center.x + half, center.y + half),
            ];
            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
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

// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! FR-018b, contracts/design-tokens.md C1-C7: every contrast floor this
//! feature promises, pinned so a future value change fails the build
//! instead of shipping unreadable text. Ratios are computed by
//! `theme::contrast::ratio` (WCAG 2.x relative luminance) and the disabled
//! composite by `theme::contrast::composite` — the same
//! `bg.blend(fg.gamma_multiply(alpha))` egui itself paints with
//! (research R16).

use egui::epaint::ClippedShape;
use egui::{Color32, Context, RawInput, Shape};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::rows::{RowEntity, list_row};
use modplayer_ui::theme::MARKER_PALETTE;
use modplayer_ui::theme::contrast::{composite, ratio};
use modplayer_ui::theme::tokens::{
    self, DARK, DARK_HIGH_CONTRAST, LIGHT, LIGHT_HIGH_CONTRAST, divider_color_for,
};

const TEXT_FLOOR: f32 = 4.5;
const NON_TEXT_FLOOR: f32 = 3.0;
const RAISED_FLOOR: f32 = 1.2;
const MARKER_FLOOR: f32 = 3.0;
/// FR-018: the enhanced floor high contrast promises (contract H17).
const ENHANCED_TEXT_FLOOR: f32 = 7.0;

/// C1 (FR-011)/C2 (FR-012): `text_primary`/`text_secondary` >= 4.5:1,
/// `text_disabled` >= 3:1, against both surfaces, both themes.
#[test]
fn every_text_role_clears_its_floor() {
    for roles in [&LIGHT, &DARK] {
        for surface in [roles.surface_base, roles.surface_raised] {
            let primary = ratio(roles.text_primary, surface);
            assert!(
                primary >= TEXT_FLOOR,
                "text_primary vs {surface:?} = {primary:.2}, floor {TEXT_FLOOR}"
            );
            let secondary = ratio(roles.text_secondary, surface);
            assert!(
                secondary >= TEXT_FLOOR,
                "text_secondary vs {surface:?} = {secondary:.2}, floor {TEXT_FLOOR}"
            );
            let disabled = ratio(roles.text_disabled, surface);
            assert!(
                disabled >= NON_TEXT_FLOOR,
                "text_disabled vs {surface:?} = {disabled:.2}, floor {NON_TEXT_FLOOR}"
            );
        }
    }
}

/// C3 (FR-012, research R11): the disabled *composite* — `text_primary`
/// multiplied by `disabled_alpha` and blended over each surface, exactly
/// as `Ui::disable()` paints it — clears the 3:1 non-text floor.
#[test]
fn disabled_composite_clears_the_non_text_floor() {
    for roles in [&LIGHT, &DARK] {
        for surface in [roles.surface_base, roles.surface_raised] {
            let painted = composite(roles.text_primary, roles.disabled_alpha, surface);
            let measured = ratio(painted, surface);
            assert!(
                measured >= NON_TEXT_FLOOR,
                "disabled composite vs {surface:?} = {measured:.2}, floor {NON_TEXT_FLOOR}"
            );
        }
    }
}

/// C4 (FR-013): `surface_raised` >= 1.2:1 vs `surface_base`, both themes.
#[test]
fn raised_surface_is_separated() {
    for roles in [&LIGHT, &DARK] {
        let measured = ratio(roles.surface_raised, roles.surface_base);
        assert!(
            measured >= RAISED_FLOOR,
            "surface_raised vs surface_base = {measured:.2}, floor {RAISED_FLOOR}"
        );
    }
}

/// C5 (FR-010a, FR-014): `text_on_accent` >= 4.5:1 vs `accent`;
/// `accent`/`positive`/`warning`/`danger` >= 4.5:1 vs `surface_base`.
#[test]
fn on_accent_is_readable() {
    for roles in [&LIGHT, &DARK] {
        let measured = ratio(roles.text_on_accent, roles.accent);
        assert!(
            measured >= TEXT_FLOOR,
            "text_on_accent vs accent = {measured:.2}, floor {TEXT_FLOOR}"
        );
    }
}

#[test]
fn status_roles_clear_the_text_floor() {
    for roles in [&LIGHT, &DARK] {
        for (name, colour) in [
            ("accent", roles.accent),
            ("positive", roles.positive),
            ("warning", roles.warning),
            ("danger", roles.danger),
        ] {
            let measured = ratio(colour, roles.surface_base);
            assert!(
                measured >= TEXT_FLOOR,
                "{name} vs surface_base = {measured:.2}, floor {TEXT_FLOOR}"
            );
        }
    }
}

/// C6 (FR-015): all 8 `MARKER_PALETTE` entries >= 3:1 against both
/// `surface_base` values.
#[test]
fn marker_palette_clears_three_to_one() {
    for (index, colour) in MARKER_PALETTE.iter().enumerate() {
        for base in [LIGHT.surface_base, DARK.surface_base] {
            let measured = ratio(*colour, base);
            assert!(
                measured >= MARKER_FLOOR,
                "MARKER_PALETTE[{index}] vs {base:?} = {measured:.2}, floor {MARKER_FLOOR}"
            );
        }
    }
}

/// C7: `ratio` is symmetric, `1.0` for equal colours, `21.0` for black vs
/// white.
/// The widest selected-row case: a Track with an availability reason
/// (region-locked) and an explicit "E" badge — every text run this row can
/// draw (title, secondary line, duration, badge, reason) in one frame. An
/// `artwork_url` keeps `draw_artwork` in its `Loading` state (a plain
/// square, no text) rather than `Failed`'s initials placeholder, which
/// paints outside this contract's named row text runs in the ambient
/// `visuals().text_color()`, not `text_on_accent`.
fn widest_track_entity(id: &str) -> RowEntity {
    let mut track = TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap(),
        format!("Title {id}"),
        vec!["Artist".to_string()],
        None,
        Some(format!("https://example.invalid/{id}.png")),
        180_000,
        Availability::UnavailableRegion,
    );
    track.explicit = true;
    RowEntity::Track(track)
}

/// Every colour any *recoloured* text run in `shapes` resolved to (mirrors
/// `tests/rows.rs`'s own `text_colors` helper): `RichText::color`/`.weak()`
/// both bake into the `Galley`'s `LayoutJob` at layout time. Excludes
/// `Color32::PLACEHOLDER` — egui's sentinel for a run with no explicit
/// colour (e.g. the trailing "…" menu button's own label), resolved to the
/// ambient widget colour only at paint time and not one of contract A4's
/// named row text runs (title, secondary line, duration, badge, reason).
fn text_colors(shapes: &[ClippedShape]) -> Vec<Color32> {
    let mut colors: Vec<Color32> = shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(t) => Some(t.galley.job.sections.iter().map(|s| s.format.color)),
            _ => None,
        })
        .flatten()
        .filter(|c| *c != Color32::PLACEHOLDER)
        .collect();
    colors.sort_by_key(|c| c.to_array());
    colors.dedup();
    colors
}

/// A4 (FR-031, NFR-6.5, SC-011): every text run on a selected row — sampled
/// on the widest case, a Track with an availability reason and an explicit
/// badge — clears the 4.5:1 text floor against the row's `accent` fill, in
/// both themes.
#[test]
fn selected_row_text_clears_the_floor_against_accent() {
    for dark in [false, true] {
        let ctx = Context::default();
        let theme = if dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_theme(egui::ThemePreference::from(theme));
        let roles = *tokens::for_dark_mode(dark);
        let mut cache = ArtworkCache::new();
        let entity = widest_track_entity(if dark {
            "contrast-a4-dark"
        } else {
            "contrast-a4-light"
        });

        let output = ctx.run_ui(RawInput::default(), |ui| {
            let _ = list_row(ui, &mut cache, &entity, true);
        });
        let colors = text_colors(&output.shapes);
        output.drop_without_applying_deltas();
        assert!(
            !colors.is_empty(),
            "dark={dark}: a selected row with a reason and badge must draw text runs"
        );
        for colour in colors {
            let measured = ratio(colour, roles.accent);
            assert!(
                measured >= TEXT_FLOOR,
                "dark={dark}: selected-row text {colour:?} vs accent = {measured:.2}, floor {TEXT_FLOOR}"
            );
        }
    }
}

// ---------------------------------------------------------------------
// 017-high-contrast-appearance (contracts/high-contrast-tokens.md
// H17/H18, research R4/R6): a *new* block over
// `[&LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST]` only — the five loops
// above iterate `[&LIGHT, &DARK]` and stay verbatim (research R14), so
// they remain the regression net for "no change outside high contrast".
// Forbidden assertion: "every high-contrast role differs from its
// normal-mode value" — dark `warning` is deliberately identical (R4).
// ---------------------------------------------------------------------

/// H17 (roles): `accent`/`positive`/`warning`/`danger` each measure
/// >= 7.0 against both surfaces, both high-contrast tables.
#[test]
fn high_contrast_roles_clear_the_enhanced_floor() {
    for roles in [&LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
        for surface in [roles.surface_base, roles.surface_raised] {
            for (name, colour) in [
                ("accent", roles.accent),
                ("positive", roles.positive),
                ("warning", roles.warning),
                ("danger", roles.danger),
            ] {
                let measured = ratio(colour, surface);
                assert!(
                    measured >= ENHANCED_TEXT_FLOOR,
                    "{name} vs {surface:?} = {measured:.2}, floor {ENHANCED_TEXT_FLOOR}"
                );
            }
        }
    }
}

/// H17 (text): `text_primary`/`text_secondary` each measure >= 7.0
/// against both surfaces, both high-contrast tables.
#[test]
fn high_contrast_text_clears_the_enhanced_floor() {
    for roles in [&LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
        for surface in [roles.surface_base, roles.surface_raised] {
            for (name, colour) in [
                ("text_primary", roles.text_primary),
                ("text_secondary", roles.text_secondary),
            ] {
                let measured = ratio(colour, surface);
                assert!(
                    measured >= ENHANCED_TEXT_FLOOR,
                    "{name} vs {surface:?} = {measured:.2}, floor {ENHANCED_TEXT_FLOOR}"
                );
            }
        }
    }
}

/// H17 (divider): the raised divider measures >= 3.0 against both
/// surfaces, both high-contrast tables.
#[test]
fn high_contrast_divider_clears_the_non_text_floor() {
    for roles in [&LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
        let divider = divider_color_for(roles);
        for surface in [roles.surface_base, roles.surface_raised] {
            let measured = ratio(divider, surface);
            assert!(
                measured >= NON_TEXT_FLOOR,
                "divider vs {surface:?} = {measured:.2}, floor {NON_TEXT_FLOOR}"
            );
        }
    }
}

/// H18: `text_on_accent` still pairs with the high-contrast `accent` at
/// a ratio of 4.5:1 or higher (research R5: FR-010's conditional
/// counterpart does not fire).
#[test]
fn text_on_accent_still_pairs_with_the_high_contrast_accent() {
    for roles in [&LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
        let measured = ratio(roles.text_on_accent, roles.accent);
        assert!(
            measured >= TEXT_FLOOR,
            "text_on_accent vs high-contrast accent = {measured:.2}, floor {TEXT_FLOOR}"
        );
    }
}

// ---------------------------------------------------------------------
// 020-shell-navigation-and-gates (US3, contracts/shell-chrome.md C7):
// nav_indicator ≥ 3:1 against the rail's own surface, in every role set.
// ---------------------------------------------------------------------

/// C7: `nav_indicator(roles).color` vs `roles.surface_base` measures at
/// least 3.0:1 for `LIGHT`, `DARK`, and both high-contrast role sets (the
/// rail's `Panel::left` fill is `panel_fill` == `surface_base`).
#[test]
fn nav_indicator_clears_the_non_text_floor_against_the_rail_surface() {
    use modplayer_ui::theme::controls::nav_indicator;

    for roles in [&LIGHT, &DARK, &LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
        let measured = ratio(nav_indicator(roles).color, roles.surface_base);
        assert!(
            measured >= NON_TEXT_FLOOR,
            "nav_indicator vs surface_base = {measured:.2}, floor {NON_TEXT_FLOOR}"
        );
    }
}

#[test]
fn ratio_matches_the_wcag_reference() {
    assert!((ratio(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-3);
    assert!((ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.05);
    assert_eq!(
        ratio(LIGHT.text_primary, LIGHT.surface_base),
        ratio(LIGHT.surface_base, LIGHT.text_primary)
    );
}

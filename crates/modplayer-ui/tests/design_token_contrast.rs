// SPDX-License-Identifier: MIT OR Apache-2.0

//! FR-018b, contracts/design-tokens.md C1-C7: every contrast floor this
//! feature promises, pinned so a future value change fails the build
//! instead of shipping unreadable text. Ratios are computed by
//! `theme::contrast::ratio` (WCAG 2.x relative luminance) and the disabled
//! composite by `theme::contrast::composite` — the same
//! `bg.blend(fg.gamma_multiply(alpha))` egui itself paints with
//! (research R16).

use egui::Color32;
use modplayer_ui::theme::MARKER_PALETTE;
use modplayer_ui::theme::contrast::{composite, ratio};
use modplayer_ui::theme::tokens::{DARK, LIGHT};

const TEXT_FLOOR: f32 = 4.5;
const NON_TEXT_FLOOR: f32 = 3.0;
const RAISED_FLOOR: f32 = 1.2;
const MARKER_FLOOR: f32 = 3.0;

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
#[test]
fn ratio_matches_the_wcag_reference() {
    assert!((ratio(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-3);
    assert!((ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.05);
    assert_eq!(
        ratio(LIGHT.text_primary, LIGHT.surface_base),
        ratio(LIGHT.surface_base, LIGHT.text_primary)
    );
}

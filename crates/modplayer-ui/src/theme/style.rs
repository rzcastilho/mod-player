// SPDX-License-Identifier: MIT OR Apache-2.0

//! `build_style(theme) -> Style`: the single site that constructs an
//! `egui::Style` (design note 3, "one construction site") — the built-in
//! `text_styles` remap (FR-003a), the full `Visuals` colour map
//! (FR-010b, data-model.md §5.3) including the US1 fix (`weak_text_color`,
//! research R10) and the disabled-alpha agreement (research R11), and the
//! `Spacing` assignments of research R18. No geometry or interaction field
//! is touched (A8, FR-019): `expansion`, `interaction`, `animation_time`,
//! `striped`, `handle_shape`, `interact_cursor` and the component sizes in
//! `Spacing` all keep whatever `Visuals::light()`/`dark()`/`Style::default()`
//! already had.

use std::collections::BTreeMap;

use egui::style::WidgetVisuals;
use egui::{Color32, FontFamily, FontId, Margin, Stroke, Style, TextStyle, Theme, Visuals};

use super::tokens::{self, space, text};

/// Build a complete `Style` for `theme`. The only function in the crate
/// that writes a `Style`, `Visuals` or `Spacing` field (design note 3).
pub fn build_style(theme: Theme) -> Style {
    let mut style = Style {
        text_styles: text_styles(),
        drag_value_text_style: TextStyle::Monospace,
        visuals: visuals(theme),
        ..Style::default()
    };
    apply_spacing(&mut style.spacing);
    style
}

/// The complete `Style::text_styles` remap (FR-003a, contract T3): exactly
/// seven entries, so a widget naming no style still renders from the
/// scale.
fn text_styles() -> BTreeMap<TextStyle, FontId> {
    [
        (
            TextStyle::Heading,
            FontId::new(tokens::SIZE_TITLE, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(tokens::SIZE_BODY, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(tokens::SIZE_BODY, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(tokens::SIZE_SECONDARY, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(tokens::SIZE_MONO, FontFamily::Monospace),
        ),
        (
            text::DISPLAY.clone(),
            FontId::new(tokens::SIZE_DISPLAY, FontFamily::Proportional),
        ),
        (
            text::SECTION.clone(),
            FontId::new(tokens::SIZE_SECTION, FontFamily::Proportional),
        ),
    ]
    .into_iter()
    .collect()
}

/// `Spacing` per research R18: `item_spacing`/`button_padding`/
/// `window_margin`/`menu_margin`/`indent`/`icon_spacing`/`menu_spacing`
/// from the scale (A6). Component *sizes* (`interact_size`, `slider_width`,
/// `combo_width`, `text_edit_width`, `tooltip_width`, `menu_width`,
/// `default_area_size`, `scroll.*`) are left untouched.
fn apply_spacing(spacing: &mut egui::Spacing) {
    spacing.item_spacing = egui::vec2(space::SM, space::XS);
    spacing.button_padding = egui::vec2(space::SM, space::XS);
    spacing.window_margin = Margin::same(space::LG as i8);
    spacing.menu_margin = Margin::same(space::SM as i8);
    spacing.indent = space::LG;
    spacing.icon_spacing = space::XS;
    spacing.menu_spacing = space::XS;
}

/// Every colour field of `Visuals` (FR-010b, data-model.md §5.3), starting
/// from the matching egui default for every non-colour field (shadows,
/// `handle_shape`, `text_cursor`, `interaction`, …).
fn visuals(theme: Theme) -> Visuals {
    let dark_mode = matches!(theme, Theme::Dark);
    let mut visuals = if dark_mode {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let roles = tokens::for_dark_mode(dark_mode);
    let divider = tokens::divider_color_for(roles);

    visuals.dark_mode = dark_mode;

    // The US1 fix (research R10, contract A4): every `ui.weak()`/
    // `weak_text_color()` call site — including
    // `overlay_color(Neutral, ..)` — inherits `text_secondary` with no
    // call-site edit. `weak_text_alpha` is inert once `weak_text_color` is
    // `Some`, but is still set to `1.0` so no path can fall back to an
    // alpha multiply.
    visuals.override_text_color = None;
    visuals.weak_text_color = Some(roles.text_secondary);
    visuals.weak_text_alpha = 1.0;

    visuals.panel_fill = roles.surface_base;
    visuals.window_fill = roles.surface_base;
    visuals.extreme_bg_color = roles.surface_base;
    visuals.text_edit_bg_color = Some(roles.surface_base);

    visuals.faint_bg_color = roles.surface_raised;
    visuals.code_bg_color = roles.surface_raised;

    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        recolor_widget(widget, roles.surface_raised, roles.text_primary, divider);
    }

    visuals.selection.bg_fill = roles.accent;
    visuals.selection.stroke = Stroke::new(1.0, roles.text_on_accent);
    visuals.hyperlink_color = roles.accent;
    visuals.warn_fg_color = roles.warning;
    visuals.error_fg_color = roles.danger;

    visuals.window_stroke = Stroke::new(1.0, divider);
    visuals.window_corner_radius = tokens::radius::MD;
    visuals.menu_corner_radius = tokens::radius::MD;

    visuals.disabled_alpha = roles.disabled_alpha;

    visuals
}

/// Recolour one `WidgetVisuals` slot (all five states get the same
/// treatment — data-model.md §5.3, design note 12: token *colours*, not a
/// new interaction state). `expansion` is left untouched (A8).
fn recolor_widget(
    widget: &mut WidgetVisuals,
    surface_raised: Color32,
    text_primary: Color32,
    divider: Color32,
) {
    widget.bg_fill = surface_raised;
    widget.weak_bg_fill = surface_raised;
    widget.bg_stroke = Stroke::new(1.0, divider);
    widget.corner_radius = tokens::radius::SM;
    widget.fg_stroke = Stroke::new(1.0, text_primary);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::apply_tokens;
    use std::sync::Arc;

    #[test]
    fn apply_tokens_installs_both_themes() {
        egui::__run_test_ctx(|ctx| {
            apply_tokens(ctx);
            let light = ctx.style_of(Theme::Light);
            let dark = ctx.style_of(Theme::Dark);
            assert!(!light.visuals.dark_mode);
            assert!(dark.visuals.dark_mode);
            assert_eq!(light.text_styles.len(), 7);
            assert_eq!(dark.text_styles.len(), 7);
        });
    }

    #[test]
    fn apply_tokens_is_idempotent() {
        egui::__run_test_ctx(|ctx| {
            apply_tokens(ctx);
            let first = ctx.style_of(Theme::Light);
            apply_tokens(ctx);
            let second = ctx.style_of(Theme::Light);
            assert!(Arc::ptr_eq(&first, &second));
        });
    }

    #[test]
    fn every_visuals_colour_comes_from_a_role() {
        let style = build_style(Theme::Light);
        let roles = &tokens::LIGHT;

        assert_eq!(style.visuals.panel_fill, roles.surface_base);
        assert_eq!(style.visuals.window_fill, roles.surface_base);
        assert_eq!(style.visuals.extreme_bg_color, roles.surface_base);
        assert_eq!(style.visuals.text_edit_bg_color, Some(roles.surface_base));
        assert_eq!(style.visuals.faint_bg_color, roles.surface_raised);
        assert_eq!(style.visuals.code_bg_color, roles.surface_raised);

        for widget in [
            &style.visuals.widgets.noninteractive,
            &style.visuals.widgets.inactive,
            &style.visuals.widgets.hovered,
            &style.visuals.widgets.active,
            &style.visuals.widgets.open,
        ] {
            assert_eq!(widget.bg_fill, roles.surface_raised);
            assert_eq!(widget.weak_bg_fill, roles.surface_raised);
            assert_eq!(widget.fg_stroke.color, roles.text_primary);
            assert_eq!(widget.corner_radius, tokens::radius::SM);
        }

        assert_eq!(style.visuals.selection.bg_fill, roles.accent);
        assert_eq!(style.visuals.selection.stroke.color, roles.text_on_accent);
        assert_eq!(style.visuals.hyperlink_color, roles.accent);
        assert_eq!(style.visuals.warn_fg_color, roles.warning);
        assert_eq!(style.visuals.error_fg_color, roles.danger);
        assert_eq!(style.visuals.window_corner_radius, tokens::radius::MD);
        assert_eq!(style.visuals.menu_corner_radius, tokens::radius::MD);
    }

    #[test]
    fn weak_text_is_the_secondary_role_not_an_alpha() {
        let style = build_style(Theme::Light);
        assert_eq!(
            style.visuals.weak_text_color,
            Some(tokens::LIGHT.text_secondary)
        );
        assert_eq!(style.visuals.weak_text_alpha, 1.0);
    }

    #[test]
    fn disabled_alpha_matches_the_disabled_token() {
        assert_eq!(build_style(Theme::Light).visuals.disabled_alpha, 0.55);
        assert_eq!(build_style(Theme::Dark).visuals.disabled_alpha, 0.44);
    }

    #[test]
    fn spacing_uses_only_scale_steps() {
        let style = build_style(Theme::Light);
        assert_eq!(style.spacing.item_spacing, egui::vec2(space::SM, space::XS));
        assert_eq!(
            style.spacing.button_padding,
            egui::vec2(space::SM, space::XS)
        );
        assert_eq!(style.spacing.window_margin, Margin::same(space::LG as i8));
        assert_eq!(style.spacing.menu_margin, Margin::same(space::SM as i8));
        assert_eq!(style.spacing.indent, space::LG);
        assert_eq!(style.spacing.icon_spacing, space::XS);
        assert_eq!(style.spacing.menu_spacing, space::XS);
    }

    #[test]
    fn radii_come_from_the_scale() {
        let style = build_style(Theme::Light);
        assert_eq!(
            style.visuals.widgets.inactive.corner_radius,
            tokens::radius::SM
        );
        assert_eq!(style.visuals.window_corner_radius, tokens::radius::MD);
        assert_eq!(style.visuals.menu_corner_radius, tokens::radius::MD);
    }

    #[test]
    fn no_geometry_or_interaction_field_changes() {
        let built = build_style(Theme::Light);
        let default_visuals = Visuals::light();
        let default_spacing = egui::Spacing::default();

        assert_eq!(
            built.visuals.widgets.inactive.expansion,
            default_visuals.widgets.inactive.expansion
        );
        assert_eq!(
            built.visuals.widgets.hovered.expansion,
            default_visuals.widgets.hovered.expansion
        );
        assert_eq!(
            built.visuals.widgets.active.expansion,
            default_visuals.widgets.active.expansion
        );
        assert_eq!(built.visuals.striped, default_visuals.striped);
        assert_eq!(built.visuals.handle_shape, default_visuals.handle_shape);
        assert_eq!(
            built.visuals.interact_cursor,
            default_visuals.interact_cursor
        );
        assert_eq!(built.animation_time, Style::default().animation_time);

        assert_eq!(built.spacing.interact_size, default_spacing.interact_size);
        assert_eq!(built.spacing.slider_width, default_spacing.slider_width);
        assert_eq!(built.spacing.combo_width, default_spacing.combo_width);
        assert_eq!(
            built.spacing.text_edit_width,
            default_spacing.text_edit_width
        );
        assert_eq!(built.spacing.tooltip_width, default_spacing.tooltip_width);
        assert_eq!(built.spacing.menu_width, default_spacing.menu_width);
        assert_eq!(
            built.spacing.default_area_size,
            default_spacing.default_area_size
        );
    }
}

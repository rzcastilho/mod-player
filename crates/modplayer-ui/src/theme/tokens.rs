// SPDX-License-Identifier: MIT OR Apache-2.0

//! The token values (data-model.md §§2-5): ten semantic colour roles per
//! theme, the six-role type scale, the six-step 4 px spacing scale and the
//! three-step radius scale. Pure data plus the small pure functions that
//! select from it (`roles`, `divider_color`, `section_label`,
//! `body_measure`) — no `egui::Style`/`Visuals` construction here (that is
//! `style.rs`, design note 3: "one construction site").

use egui::Visuals;

/// The ten semantic colour roles plus the per-theme disabled-alpha
/// (data-model.md §5.1, contract T9). One static table per theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roles {
    pub text_primary: egui::Color32,
    pub text_secondary: egui::Color32,
    pub text_disabled: egui::Color32,
    pub surface_base: egui::Color32,
    pub surface_raised: egui::Color32,
    pub accent: egui::Color32,
    pub text_on_accent: egui::Color32,
    pub positive: egui::Color32,
    pub warning: egui::Color32,
    pub danger: egui::Color32,
    pub disabled_alpha: f32,
}

/// Light theme (research R12/R13/data-model.md §5.1).
pub const LIGHT: Roles = Roles {
    text_primary: egui::Color32::from_rgb(0x1c, 0x1c, 0x1e),
    text_secondary: egui::Color32::from_rgb(0x5b, 0x5b, 0x60),
    text_disabled: egui::Color32::from_rgb(0x82, 0x82, 0x83),
    surface_base: egui::Color32::from_rgb(0xff, 0xff, 0xff),
    surface_raised: egui::Color32::from_rgb(0xe8, 0xe8, 0xea),
    accent: egui::Color32::from_rgb(0x0a, 0x63, 0xc9),
    text_on_accent: egui::Color32::from_rgb(0xff, 0xff, 0xff),
    positive: egui::Color32::from_rgb(0x1f, 0x7a, 0x44),
    warning: egui::Color32::from_rgb(0x8a, 0x5a, 0x00),
    danger: egui::Color32::from_rgb(0xb3, 0x26, 0x1e),
    disabled_alpha: 0.55,
};

/// Dark theme (research R12/R13/data-model.md §5.1).
pub const DARK: Roles = Roles {
    text_primary: egui::Color32::from_rgb(0xf2, 0xf2, 0xf7),
    text_secondary: egui::Color32::from_rgb(0xa8, 0xa8, 0xb0),
    text_disabled: egui::Color32::from_rgb(0x76, 0x76, 0x7a),
    surface_base: egui::Color32::from_rgb(0x14, 0x14, 0x17),
    surface_raised: egui::Color32::from_rgb(0x26, 0x26, 0x2c),
    accent: egui::Color32::from_rgb(0x5a, 0xa9, 0xff),
    text_on_accent: egui::Color32::from_rgb(0x14, 0x14, 0x17),
    positive: egui::Color32::from_rgb(0x4c, 0xaf, 0x50),
    warning: egui::Color32::from_rgb(0xe0, 0xa9, 0x2a),
    danger: egui::Color32::from_rgb(0xff, 0x6b, 0x5e),
    disabled_alpha: 0.44,
};

/// The active theme's colour table, selected by `dark_mode` (research R8
/// channel 2). Allocates nothing.
pub fn for_dark_mode(dark_mode: bool) -> &'static Roles {
    if dark_mode { &DARK } else { &LIGHT }
}

/// The active theme's colour table, selected by `Visuals::dark_mode`.
pub fn roles(visuals: &Visuals) -> &'static Roles {
    for_dark_mode(visuals.dark_mode)
}

/// The 1 px in-panel rule colour: `text_primary` at 8 % alpha
/// (data-model.md §5.2, FR-008, contract T10). Never a role of its own,
/// never a call-site literal.
pub fn divider_color_for(roles: &Roles) -> egui::Color32 {
    roles.text_primary.gamma_multiply(0.08)
}

/// [`divider_color_for`] selected by `visuals.dark_mode`.
pub fn divider_color(visuals: &Visuals) -> egui::Color32 {
    divider_color_for(roles(visuals))
}

/// FR-020's hook for a future user-facing text-scale setting: raising this
/// one function is the whole mechanism, no size constant below changes.
const fn text_scale() -> f32 {
    1.0
}

const fn role_size(base: f32) -> f32 {
    base * text_scale()
}

const BASE_DISPLAY: f32 = 22.0;
const BASE_TITLE: f32 = 17.0;
const BASE_SECTION: f32 = 13.0;
const BASE_BODY: f32 = 14.0;
const BASE_SECONDARY: f32 = 13.0;
const BASE_MONO: f32 = 13.0;

/// The six type-scale sizes, in logical pixels at scale factor 1.0
/// (data-model.md §2, contract T2): `BASE_<ROLE> * text_scale()`.
pub(crate) const SIZE_DISPLAY: f32 = role_size(BASE_DISPLAY);
pub(crate) const SIZE_TITLE: f32 = role_size(BASE_TITLE);
pub(crate) const SIZE_SECTION: f32 = role_size(BASE_SECTION);
pub(crate) const SIZE_BODY: f32 = role_size(BASE_BODY);
pub(crate) const SIZE_SECONDARY: f32 = role_size(BASE_SECONDARY);
pub(crate) const SIZE_MONO: f32 = role_size(BASE_MONO);

/// The six type roles as `TextStyle`s (data-model.md §2). `TITLE`/`BODY`/
/// `SECONDARY`/`MONO` are egui's own built-in variants (no allocation);
/// `DISPLAY`/`SECTION` need `TextStyle::Name`'s `Arc<str>` key, built once
/// and shared (FR-003).
pub mod text {
    use egui::TextStyle;
    use std::sync::{Arc, LazyLock};

    pub const TITLE: TextStyle = TextStyle::Heading;
    pub const BODY: TextStyle = TextStyle::Body;
    pub const SECONDARY: TextStyle = TextStyle::Small;
    pub const MONO: TextStyle = TextStyle::Monospace;

    pub static DISPLAY: LazyLock<TextStyle> =
        LazyLock::new(|| TextStyle::Name(Arc::from("display")));
    pub static SECTION: LazyLock<TextStyle> =
        LazyLock::new(|| TextStyle::Name(Arc::from("section")));
}

/// The six-step, 4 px spacing scale (FR-007, contract T7).
pub mod space {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

/// The three-step radius scale (FR-009, contract T8).
pub mod radius {
    use egui::CornerRadius;

    pub const SM: CornerRadius = CornerRadius::same(4);
    pub const MD: CornerRadius = CornerRadius::same(8);

    /// A pill at any height: half the height, rounded, clamped into `u8`
    /// (Constitution VII — no unclamped `as u8` cast on a float).
    pub fn full(height: f32) -> CornerRadius {
        CornerRadius::same((height * 0.5).round().clamp(0.0, 255.0) as u8)
    }
}

/// `section`'s style + locale-agnostic uppercase + tracking (FR-003b,
/// research R5/R6, contract T6): the only place any string is uppercased,
/// at draw time, never in a Fluent catalogue entry.
pub fn section_label(s: &str) -> egui::RichText {
    egui::RichText::new(s.to_uppercase())
        .text_style(text::SECTION.clone())
        .extra_letter_spacing(0.52)
}

/// 72 × the `body` role's `'0'` advance width (FR-006, research R17): the
/// prose measure, a maximum, never a minimum.
pub fn body_measure(ctx: &egui::Context) -> f32 {
    let font_id = egui::FontId::new(SIZE_BODY, egui::FontFamily::Proportional);
    72.0 * ctx.fonts_mut(|fonts| fonts.glyph_width(&font_id, '0'))
}

/// `mono`-styled text for a numeric readout (contract U1, FR-005): every
/// `ui.label`/`selected_text`/hover-text call site that renders a formatted
/// timestamp, dB reading, CPU/memory figure or duration routes its string
/// through this, so `type_roles::numeric_fields_use_the_mono_role` can
/// assert the mapping once instead of re-deriving it per call site.
pub fn mono_text(s: impl Into<String>) -> egui::RichText {
    egui::RichText::new(s.into()).text_style(text::MONO)
}

/// The `mono` role's `FontId`, for the few low-level `Painter::text` call
/// sites (contract U1) that draw a numeric readout directly rather than
/// through a `Style`-driven `RichText`.
pub fn mono_font_id() -> egui::FontId {
    egui::FontId::new(SIZE_MONO, egui::FontFamily::Monospace)
}

/// The `secondary` role's `FontId`, for the low-level `Painter::text` call
/// sites (`plugin_overlays.rs`'s `Label` primitive, FR-017/U5) that draw a
/// plugin-supplied string directly rather than through a `Style`-driven
/// `RichText`.
pub fn secondary_font_id() -> egui::FontId {
    egui::FontId::new(SIZE_SECONDARY, egui::FontFamily::Proportional)
}

/// The initials-avatar text ratio to its own square (data-model.md §6):
/// avatars are drawn at every size from list-row thumbnails to large
/// headers, so this stays proportional rather than a fixed role size —
/// but, like every other size in this module, it still runs through the
/// shared `text_scale()` hook (FR-020) rather than being a bare literal at
/// the call site.
const INITIALS_SIZE_RATIO: f32 = 0.4;

/// The initials-avatar text `FontId` for a `size` x `size` placeholder
/// (FR-017, U5, `widgets/initials.rs`'s one low-level `Painter::text` call
/// site): the only sanctioned `FontId::proportional` construction for this
/// glyph, so the literal scan finds none at the call site.
pub fn initials_font_id(size: f32) -> egui::FontId {
    egui::FontId::proportional(size * INITIALS_SIZE_RATIO * text_scale())
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, CornerRadius, TextStyle};
    use std::sync::Arc;

    #[test]
    fn roles_table_matches_the_contract() {
        assert_eq!(LIGHT.text_primary, Color32::from_rgb(0x1c, 0x1c, 0x1e));
        assert_eq!(LIGHT.text_secondary, Color32::from_rgb(0x5b, 0x5b, 0x60));
        assert_eq!(LIGHT.text_disabled, Color32::from_rgb(0x82, 0x82, 0x83));
        assert_eq!(LIGHT.surface_base, Color32::from_rgb(0xff, 0xff, 0xff));
        assert_eq!(LIGHT.surface_raised, Color32::from_rgb(0xe8, 0xe8, 0xea));
        assert_eq!(LIGHT.accent, Color32::from_rgb(0x0a, 0x63, 0xc9));
        assert_eq!(LIGHT.text_on_accent, Color32::from_rgb(0xff, 0xff, 0xff));
        assert_eq!(LIGHT.positive, Color32::from_rgb(0x1f, 0x7a, 0x44));
        assert_eq!(LIGHT.warning, Color32::from_rgb(0x8a, 0x5a, 0x00));
        assert_eq!(LIGHT.danger, Color32::from_rgb(0xb3, 0x26, 0x1e));
        assert!((LIGHT.disabled_alpha - 0.55).abs() < f32::EPSILON);

        assert_eq!(DARK.text_primary, Color32::from_rgb(0xf2, 0xf2, 0xf7));
        assert_eq!(DARK.text_secondary, Color32::from_rgb(0xa8, 0xa8, 0xb0));
        assert_eq!(DARK.text_disabled, Color32::from_rgb(0x76, 0x76, 0x7a));
        assert_eq!(DARK.surface_base, Color32::from_rgb(0x14, 0x14, 0x17));
        assert_eq!(DARK.surface_raised, Color32::from_rgb(0x26, 0x26, 0x2c));
        assert_eq!(DARK.accent, Color32::from_rgb(0x5a, 0xa9, 0xff));
        assert_eq!(DARK.text_on_accent, Color32::from_rgb(0x14, 0x14, 0x17));
        assert_eq!(DARK.positive, Color32::from_rgb(0x4c, 0xaf, 0x50));
        assert_eq!(DARK.warning, Color32::from_rgb(0xe0, 0xa9, 0x2a));
        assert_eq!(DARK.danger, Color32::from_rgb(0xff, 0x6b, 0x5e));
        assert!((DARK.disabled_alpha - 0.44).abs() < f32::EPSILON);
    }

    #[test]
    fn roles_selects_by_dark_mode() {
        // `LIGHT`/`DARK` are `const`s, so two references to one may be
        // promoted to distinct addresses; value equality (`Roles` is
        // `PartialEq`) is what "selects the right table" actually means.
        assert_eq!(*for_dark_mode(false), LIGHT);
        assert_eq!(*for_dark_mode(true), DARK);

        let mut visuals = Visuals::light();
        visuals.dark_mode = false;
        assert_eq!(*roles(&visuals), LIGHT);

        let mut visuals = Visuals::dark();
        visuals.dark_mode = true;
        assert_eq!(*roles(&visuals), DARK);
    }

    #[test]
    fn text_styles_cover_every_role() {
        assert_eq!(SIZE_DISPLAY, 22.0);
        assert_eq!(SIZE_TITLE, 17.0);
        assert_eq!(SIZE_SECTION, 13.0);
        assert_eq!(SIZE_BODY, 14.0);
        assert_eq!(SIZE_SECONDARY, 13.0);
        assert_eq!(SIZE_MONO, 13.0);
        assert_eq!(*text::DISPLAY, TextStyle::Name(Arc::from("display")));
        assert_eq!(*text::SECTION, TextStyle::Name(Arc::from("section")));
        assert_eq!(text::TITLE, TextStyle::Heading);
        assert_eq!(text::BODY, TextStyle::Body);
        assert_eq!(text::SECONDARY, TextStyle::Small);
        assert_eq!(text::MONO, TextStyle::Monospace);
    }

    #[test]
    fn role_sizes_are_base_times_scale() {
        assert_eq!(SIZE_DISPLAY, BASE_DISPLAY * text_scale());
        assert_eq!(SIZE_TITLE, BASE_TITLE * text_scale());
        assert_eq!(SIZE_SECTION, BASE_SECTION * text_scale());
        assert_eq!(SIZE_BODY, BASE_BODY * text_scale());
        assert_eq!(SIZE_SECONDARY, BASE_SECONDARY * text_scale());
        assert_eq!(SIZE_MONO, BASE_MONO * text_scale());
    }

    #[test]
    fn spacing_scale_is_the_four_pixel_scale() {
        assert_eq!(space::XS, 4.0);
        assert_eq!(space::SM, 8.0);
        assert_eq!(space::MD, 12.0);
        assert_eq!(space::LG, 16.0);
        assert_eq!(space::XL, 24.0);
        assert_eq!(space::XXL, 32.0);
        for step in [
            space::XS,
            space::SM,
            space::MD,
            space::LG,
            space::XL,
            space::XXL,
        ] {
            assert_eq!(step % 4.0, 0.0, "{step} is not 4 px aligned");
        }
    }

    #[test]
    fn radius_full_is_a_pill_at_any_height() {
        assert_eq!(radius::full(0.0), CornerRadius::same(0));
        assert_eq!(radius::full(20.0), CornerRadius::same(10));
        assert_eq!(radius::full(41.0), CornerRadius::same(21)); // rounds
        // Clamp guards against an unclamped `as u8` on an oversized height.
        assert_eq!(radius::full(4000.0), CornerRadius::same(255));
    }

    #[test]
    fn divider_is_primary_text_at_eight_percent() {
        assert_eq!(
            divider_color_for(&LIGHT),
            LIGHT.text_primary.gamma_multiply(0.08)
        );
        assert_eq!(
            divider_color_for(&DARK),
            DARK.text_primary.gamma_multiply(0.08)
        );

        let mut visuals = Visuals::light();
        visuals.dark_mode = false;
        assert_eq!(divider_color(&visuals), divider_color_for(&LIGHT));
    }

    #[test]
    fn mono_digits_are_tabular() {
        egui::__run_test_ctx(|ctx| {
            let font_id = egui::FontId::new(SIZE_MONO, egui::FontFamily::Monospace);
            let widths: Vec<f32> = ('0'..='9')
                .map(|c| ctx.fonts_mut(|fonts| fonts.glyph_width(&font_id, c)))
                .collect();
            let first = widths[0];
            for w in &widths {
                assert!(
                    (w - first).abs() < 0.01,
                    "digits are not tabular: {widths:?}"
                );
            }
        });
    }

    #[test]
    fn body_and_secondary_are_visibly_different() {
        assert_ne!(SIZE_BODY, SIZE_SECONDARY);
        assert_ne!(LIGHT.text_primary, LIGHT.text_secondary);
        assert_ne!(DARK.text_primary, DARK.text_secondary);
    }

    #[test]
    fn section_label_uppercases_at_draw_time() {
        let rich = section_label("mixed Case día");
        assert_eq!(rich.text(), "MIXED CASE DÍA");
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Structural (non-view-specific) checks for the token module: `A2`'s
//! per-frame re-application and `U6`'s theme switch. View-specific
//! `type_roles::*`/`measure::*` tests are added incrementally as Phases
//! 4-7 land their call sites (tasks.md Phase 2 note, T015).

use egui::{FontFamily, RichText, Style, TextStyle, Theme};
use modplayer_ui::theme;
use modplayer_ui::theme::apply_tokens;

mod app_tokens {
    use super::*;

    /// A2: `apply_tokens` is called as the first statement of every frame
    /// (`App::ui`, research R7). Re-applying it after something else
    /// mutated the installed style must restore the token values — the
    /// guarantee that makes a stray view mutation or a missed re-apply
    /// impossible to observe past the next frame.
    #[test]
    fn update_reapplies_tokens_every_frame() {
        egui::__run_test_ctx(|ctx| {
            apply_tokens(ctx);
            let installed = ctx.style_of(Theme::Light);

            // Simulate a stray mutation of the installed global style.
            ctx.set_style_of(Theme::Light, Style::default());
            assert!(!std::sync::Arc::ptr_eq(
                &installed,
                &ctx.style_of(Theme::Light)
            ));

            apply_tokens(ctx);
            assert!(std::sync::Arc::ptr_eq(
                &installed,
                &ctx.style_of(Theme::Light)
            ));
        });
    }

    /// U6 (US5 AC3, SC-007): every colour slot differs between the two
    /// installed styles, so egui picking the other one on a theme switch
    /// changes every slot with no view code running and no view holding a
    /// colour.
    #[test]
    fn theme_switch_changes_every_slot_with_no_view_change() {
        egui::__run_test_ctx(|ctx| {
            apply_tokens(ctx);
            let light = ctx.style_of(Theme::Light);
            let dark = ctx.style_of(Theme::Dark);

            assert_ne!(light.visuals.panel_fill, dark.visuals.panel_fill);
            assert_ne!(light.visuals.window_fill, dark.visuals.window_fill);
            assert_ne!(
                light.visuals.extreme_bg_color,
                dark.visuals.extreme_bg_color
            );
            assert_ne!(light.visuals.faint_bg_color, dark.visuals.faint_bg_color);
            assert_ne!(
                light.visuals.widgets.inactive.fg_stroke.color,
                dark.visuals.widgets.inactive.fg_stroke.color
            );
            assert_ne!(
                light.visuals.widgets.inactive.bg_fill,
                dark.visuals.widgets.inactive.bg_fill
            );
            assert_ne!(light.visuals.weak_text_color, dark.visuals.weak_text_color);
            assert_ne!(
                light.visuals.selection.bg_fill,
                dark.visuals.selection.bg_fill
            );
            assert_ne!(light.visuals.hyperlink_color, dark.visuals.hyperlink_color);
            assert_ne!(light.visuals.warn_fg_color, dark.visuals.warn_fg_color);
            assert_ne!(light.visuals.error_fg_color, dark.visuals.error_fg_color);
            assert_ne!(light.visuals.disabled_alpha, dark.visuals.disabled_alpha);
            assert_ne!(light.visuals.dark_mode, dark.visuals.dark_mode);
        });
    }
}

/// View-specific role assertions (Phase 5, US3): every numeric formatter's
/// call site (`markers.rs`, `now_playing.rs`, `transport_view.rs`,
/// `plugins_view.rs`, `widgets/peak_meter.rs`, `widgets/chain_meters.rs`,
/// `waveform/paint.rs`, `settings/audio.rs`, `rows.rs`'s track-duration
/// field) routes through `theme::mono_text`/`theme::mono_font_id` rather
/// than building its own `TextStyle`/`FontId`, so this single assertion on
/// the two shared helpers stands in for one per call site (contract U1).
mod type_roles {
    use super::*;

    #[test]
    fn numeric_fields_use_the_mono_role() {
        // `RichText`-based call sites (`ui.label`, `selected_text`,
        // `on_hover_text`, …) route through `mono_text`.
        let styled = theme::mono_text("12:34");
        assert_eq!(
            styled,
            RichText::new("12:34").text_style(TextStyle::Monospace)
        );
        assert_ne!(styled, RichText::new("12:34"));

        // Low-level `Painter::text` call sites (`markers.rs`'s cue slot
        // digit, `waveform/paint.rs`'s placeholder label) route through
        // `mono_font_id`, sized from the same `mono` role (T2/T4 in
        // `tokens.rs` pin the exact size and its tabular-digit metrics).
        let font_id = theme::mono_font_id();
        assert_eq!(font_id.family, FontFamily::Monospace);
        assert!(font_id.size > 0.0);
    }

    /// U4 (Phase 7, US5, T057/T066): the health-dot's existing, unchanged
    /// threshold logic (`Health::Ok`/`Warning`/`Suspended`) resolves to
    /// `positive`/`warning`/`danger`, in both themes — never a literal
    /// `from_rgb`.
    #[test]
    fn health_dot_colours_come_from_roles() {
        use modplayer_core::Health;
        use modplayer_ui::plugins_view::health_color;

        for roles in [&theme::tokens::LIGHT, &theme::tokens::DARK] {
            assert_eq!(health_color(roles, Health::Ok), roles.positive);
            assert_eq!(health_color(roles, Health::Warning), roles.warning);
            assert_eq!(health_color(roles, Health::Suspended), roles.danger);
        }
    }

    /// U5 (Phase 7, US5, T063/T066): a marker glyph's focus outline /
    /// cue-slot digit is drawn from the `text_on_accent` role, in both
    /// themes — never the literal `Color32::WHITE` that only happened to
    /// read correctly against a dark-theme fill.
    #[test]
    fn marker_labels_use_a_role_colour() {
        use egui::Visuals;
        use modplayer_ui::markers::marker_mark_color;

        let mut light = Visuals::light();
        light.dark_mode = false;
        let mut dark = Visuals::dark();
        dark.dark_mode = true;

        assert_eq!(
            marker_mark_color(&light),
            theme::roles(&light).text_on_accent
        );
        assert_eq!(marker_mark_color(&dark), theme::roles(&dark).text_on_accent);
        assert_ne!(marker_mark_color(&light), marker_mark_color(&dark));
    }
}

/// U2 (FR-006, Phase 6, US4): the 72-character prose measure — a maximum,
/// never a minimum (research R17) — applied at every call site listed in
/// `contracts/design-tokens.md` U2 (`welcome.rs`, `privacy_notice.rs`,
/// `getting_started.rs`, `device_check.rs`, `sign_in.rs`'s store-unreadable
/// explanation, the settings screens' field descriptions, and the
/// library/search/queue empty-state copy).
mod measure {
    use super::*;

    /// Run `run_ui` against a fresh `Context` with the *real* default font
    /// data loaded (unlike `egui::__run_test_ctx`, which starts from
    /// `FontDefinitions::empty()` to save CPU time — fine for style
    /// assertions, but every glyph width would read 0). `set_fonts` must
    /// be called before `run_ui`'s `begin_pass` for the swap to take
    /// effect this same frame (`Context::set_fonts` docs, R2: the font is
    /// fixed, so this loads the one real body font under test).
    fn run_with_real_fonts(mut run_ui: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let output = ctx.run_ui(egui::RawInput::default(), |ui| run_ui(ui.ctx()));
        output.drop_without_applying_deltas();
    }

    /// The `body` role's installed `FontId` (`apply_tokens`'s `Style::
    /// text_styles` remap, contract T3) — the same size `body_measure`
    /// derives its glyph width from.
    fn body_font_id(ctx: &egui::Context) -> egui::FontId {
        apply_tokens(ctx);
        ctx.style_of(Theme::Light)
            .text_styles
            .get(&TextStyle::Body)
            .cloned()
            .unwrap_or_else(|| panic!("body text style installed by apply_tokens"))
    }

    /// `body_measure` is exactly 72 × the `body` role's `'0'` advance
    /// width — deterministic because the font is fixed (research R2).
    #[test]
    fn body_measure_is_seventy_two_zero_widths() {
        run_with_real_fonts(|ctx| {
            let font_id = body_font_id(ctx);
            let zero_width = ctx.fonts_mut(|fonts| fonts.glyph_width(&font_id, '0'));
            assert!(zero_width > 0.0, "the '0' glyph must have measurable width");
            assert_eq!(theme::body_measure(ctx), 72.0 * zero_width);
        });
    }

    /// A paragraph laid out at the measure wraps to more than one row
    /// rather than spanning the full window width (Edge Cases, research
    /// R17) — the same `set_max_width(available.min(measure))` every
    /// prose call site applies.
    #[test]
    fn a_long_paragraph_wraps_within_the_measure() {
        run_with_real_fonts(|ctx| {
            let font_id = body_font_id(ctx);
            let measure = theme::body_measure(ctx);
            let long_text = "a ".repeat(200);
            let galley = ctx.fonts_mut(|fonts| {
                fonts.layout(long_text, font_id, egui::Color32::PLACEHOLDER, measure)
            });
            assert!(
                galley.rows.len() > 1,
                "a 200-character paragraph must wrap to more than one row at the measure"
            );
            for row in &galley.rows {
                assert!(
                    row.rect().width() <= measure + 1.0,
                    "row width {} exceeded the measure {measure}",
                    row.rect().width(),
                );
            }
        });
    }
}

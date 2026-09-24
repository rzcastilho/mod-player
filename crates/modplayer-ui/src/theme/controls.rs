// SPDX-License-Identifier: MIT OR Apache-2.0

//! Control variant values (015-control-variants, data-model.md §2–§7):
//! button variants and their paint (§2), interaction-state fills and the
//! focus ring (§3), the widget-state slot map (§4), the switch's metrics
//! and state table (§5), meter bands and scale marks (§6), and the
//! constants the host widgets in `widgets/controls.rs` consume (§7).
//! Every colour, alpha, stroke width and metric this feature introduces
//! lives here (FR-019) — no call site writes a literal.

use egui::{Color32, Stroke, Vec2, vec2};

use super::tokens::{Roles, divider_color_for, space};

// ---------------------------------------------------------------------
// §2 — Button variants (FR-001)
// ---------------------------------------------------------------------

/// The four button variants (FR-001). `Default` is the do-nothing variant
/// (plan.md design note 3): it reproduces exactly what
/// `theme::style::recolor_widget` already installs, so FR-005's ~60
/// untouched call sites need no edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Default,
    Quiet,
    Destructive,
}

/// A variant's resting paint (data-model.md §2). `fill` is
/// [`Color32::TRANSPARENT`] and `outline` is [`Stroke::NONE`] where the
/// variant has none of that component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VariantPaint {
    pub fill: Color32,
    pub outline: Stroke,
    pub label: Color32,
}

/// The exact table of data-model.md §2 (contract B2). Every colour is one
/// of the ten 014 roles or [`Color32::TRANSPARENT`]; every stroke width is
/// a constant of this module.
pub fn variant_paint(roles: &Roles, variant: Variant) -> VariantPaint {
    match variant {
        Variant::Primary => VariantPaint {
            fill: roles.accent,
            outline: Stroke::NONE,
            label: roles.text_on_accent,
        },
        Variant::Default => VariantPaint {
            fill: roles.surface_raised,
            outline: Stroke::new(DEFAULT_OUTLINE_WIDTH, divider_color_for(roles)),
            label: roles.text_primary,
        },
        Variant::Quiet => VariantPaint {
            fill: Color32::TRANSPARENT,
            outline: Stroke::NONE,
            label: roles.text_primary,
        },
        Variant::Destructive => VariantPaint {
            fill: Color32::TRANSPARENT,
            outline: Stroke::new(DESTRUCTIVE_OUTLINE_WIDTH, roles.danger),
            label: roles.danger,
        },
    }
}

/// `Default`'s 1 px outline width (data-model.md §2).
pub const DEFAULT_OUTLINE_WIDTH: f32 = 1.0;
/// `Destructive`'s 1 px outline width (data-model.md §2).
pub const DESTRUCTIVE_OUTLINE_WIDTH: f32 = 1.0;

// ---------------------------------------------------------------------
// §3 — Interaction-state values (FR-009, FR-010, FR-011)
// ---------------------------------------------------------------------

/// `text.primary` at 4 % alpha (FR-009, § 5.4 "full-row hover fill at 4 %
/// foreground"). A translucent overlay: painted directly it blends over
/// whatever is beneath, and [`theme::contrast::composite`](super::contrast::composite)
/// is used where an opaque equivalent is needed (data-model.md §4).
pub fn hover_fill(roles: &Roles) -> Color32 {
    roles.text_primary.gamma_multiply(HOVER_ALPHA)
}

/// `text.primary` at 8 % alpha — exactly double [`hover_fill`] (FR-011).
pub fn pressed_fill(roles: &Roles) -> Color32 {
    roles.text_primary.gamma_multiply(PRESSED_ALPHA)
}

/// The overlay alphas (contract I1, I2). The *ordering* (`0 < hover <
/// pressed`) is the normative property (FR-011); these two numbers are
/// not.
pub const HOVER_ALPHA: f32 = 0.04;
pub const PRESSED_ALPHA: f32 = 0.08;

/// `Stroke::new(focus_ring_width(roles), accent)` (§ 5.4 "focus ring = 2 px
/// `accent`"; 017-high-contrast-appearance FR-008 thickens it to 3 px).
pub fn focus_ring(roles: &Roles) -> Stroke {
    Stroke::new(focus_ring_width(roles), roles.accent)
}

/// The focus ring's normal-mode width (FR-010).
pub const FOCUS_RING_WIDTH: f32 = 2.0;
/// FR-008 (017-high-contrast-appearance): high contrast thickens the
/// ring.
pub const FOCUS_RING_WIDTH_HIGH_CONTRAST: f32 = 3.0;

/// FR-008: `2.0` normal, `3.0` high contrast — read only here, never at a
/// call site (FR-017).
///
/// ```
/// # use modplayer_ui::theme::controls::focus_ring_width;
/// # use modplayer_ui::theme::tokens::LIGHT;
/// assert_eq!(focus_ring_width(&LIGHT), 2.0);
/// ```
pub const fn focus_ring_width(roles: &Roles) -> f32 {
    if roles.high_contrast {
        FOCUS_RING_WIDTH_HIGH_CONTRAST
    } else {
        FOCUS_RING_WIDTH
    }
}
/// The gap between a control's edge and the ring: `rect.expand(FOCUS_RING_GAP)`
/// with `StrokeKind::Outside` (FR-010) — one pixel of underlying surface
/// sits between control and ring.
pub const FOCUS_RING_GAP: f32 = 1.0;

/// The Library tab strip's active-tab underline (016-list-row-and-panel-
/// components, data-model.md §9, contracts/tab-strip.md T3): `accent`, no
/// new colour, no eleventh role.
pub fn tab_underline(roles: &Roles) -> Stroke {
    Stroke::new(TAB_UNDERLINE_WIDTH, roles.accent)
}

/// The underline's thickness (FR-034) — never a literal at the call site.
pub const TAB_UNDERLINE_WIDTH: f32 = 2.0;

// ---------------------------------------------------------------------
// §5 — The switch (FR-007)
// ---------------------------------------------------------------------

/// The switch's geometry (data-model.md §5), every field derived from the
/// 014 spacing scale. `track.x` is the pill's width, `track.y` its height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchMetrics {
    pub track: Vec2,
    pub thumb: f32,
    pub inset: f32,
}

/// `track` = `space::XXL` × `space::LG` (a 2:1 pill), `thumb` =
/// `space::MD`, `inset` = `(track.y − thumb) / 2` (data-model.md §5).
pub fn switch_metrics() -> SwitchMetrics {
    let track = vec2(space::XXL, space::LG);
    let thumb = space::MD;
    SwitchMetrics {
        track,
        thumb,
        inset: (track.y - thumb) / 2.0,
    }
}

/// The switch track's outline width (data-model.md §5).
pub const SWITCH_OUTLINE_WIDTH: f32 = 1.0;

/// Off = `surface.raised` + 1 px `divider`; On = `accent` + no outline
/// (data-model.md §5).
pub fn switch_track(roles: &Roles, on: bool) -> (Color32, Stroke) {
    if on {
        (roles.accent, Stroke::NONE)
    } else {
        (
            roles.surface_raised,
            Stroke::new(SWITCH_OUTLINE_WIDTH, divider_color_for(roles)),
        )
    }
}

/// Off = `text.secondary` (assumption D3, plan.md § Complexity Tracking:
/// already ≥ 4.5:1 against `surface.raised`); On = `text.on-accent`
/// (data-model.md §5).
pub fn switch_thumb(roles: &Roles, on: bool) -> Color32 {
    if on {
        roles.text_on_accent
    } else {
        roles.text_secondary
    }
}

// ---------------------------------------------------------------------
// §7 — `DESTRUCTIVE_GAP` (FR-006)
// ---------------------------------------------------------------------

/// `space::LG` (16 px). Contract B9/V12: at least twice
/// `style.spacing.item_spacing.x`, stated as a ratio so a later
/// spacing-scale change cannot silently void the rule.
pub const DESTRUCTIVE_GAP: f32 = space::LG;

// ---------------------------------------------------------------------
// §6 — Meters (FR-012, FR-012a, FR-013)
// ---------------------------------------------------------------------

/// The three meter bands (FR-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Positive,
    Warning,
    Danger,
}

/// The warning band's lower boundary (FR-012).
pub const BAND_WARNING_DB: f32 = -6.0;

/// Fixed-order band selection (contract M2, data-model.md §6): `danger`
/// when `db >= danger_boundary_db`, else `warning` when
/// `db >= BAND_WARNING_DB`, else `positive`. Total over `f32` — a `NaN`
/// input satisfies neither `>=` comparison and falls through to
/// `Positive` (Constitution VII: no `unwrap`/`expect`, no partial
/// function).
///
/// ```
/// # use modplayer_ui::theme::controls::{band, Band, BAND_WARNING_DB};
/// assert_eq!(band(f32::NAN, 0.0), Band::Positive);
/// assert_eq!(band(BAND_WARNING_DB, 0.0), Band::Warning);
/// ```
pub fn band(db: f32, danger_boundary_db: f32) -> Band {
    if db >= danger_boundary_db {
        Band::Danger
    } else if db >= BAND_WARNING_DB {
        Band::Warning
    } else {
        Band::Positive
    }
}

/// Resolves only to the `positive`/`warning`/`danger` 014 roles (contract
/// M5).
pub fn band_color(roles: &Roles, band: Band) -> Color32 {
    match band {
        Band::Positive => roles.positive,
        Band::Warning => roles.warning,
        Band::Danger => roles.danger,
    }
}

/// The −6 dB / 0 dB scale-mark stroke width, both meters (FR-013).
pub const SCALE_MARK_WIDTH: f32 = 1.0;
/// The peak meter's ceiling-tick stroke width — thicker than a scale
/// mark, so the two kinds stay distinguishable (FR-013, contract K2).
pub const CEILING_MARK_WIDTH: f32 = 2.0;

/// One rule, both meters, both themes (contract K3): `surface.base` where
/// the fill has reached the mark's position, `text.secondary` where it
/// has not.
pub fn mark_color(roles: &Roles, filled: bool) -> Color32 {
    if filled {
        roles.surface_base
    } else {
        roles.text_secondary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::style::build_style;
    use crate::theme::tokens::{DARK, LIGHT};
    use egui::Theme as EguiTheme;

    // -------------------------------------------------------------
    // T004 — Variant / VariantPaint / variant_paint (B1, B2)
    // -------------------------------------------------------------

    #[test]
    fn variant_set_is_exactly_four() {
        let variants = [
            Variant::Primary,
            Variant::Default,
            Variant::Quiet,
            Variant::Destructive,
        ];
        assert_eq!(variants.len(), 4);
        // Exhaustive, no wildcard: a fifth `Variant` member fails to
        // compile here rather than silently passing (B1).
        for v in variants {
            match v {
                Variant::Primary | Variant::Default | Variant::Quiet | Variant::Destructive => {}
            }
        }
    }

    #[test]
    fn variant_paint_matches_the_table() {
        for roles in [&LIGHT, &DARK] {
            let primary = variant_paint(roles, Variant::Primary);
            assert_eq!(primary.fill, roles.accent);
            assert_eq!(primary.outline, Stroke::NONE);
            assert_eq!(primary.label, roles.text_on_accent);

            let default = variant_paint(roles, Variant::Default);
            assert_eq!(default.fill, roles.surface_raised);
            assert_eq!(
                default.outline,
                Stroke::new(DEFAULT_OUTLINE_WIDTH, divider_color_for(roles))
            );
            assert_eq!(default.label, roles.text_primary);

            let quiet = variant_paint(roles, Variant::Quiet);
            assert_eq!(quiet.fill, Color32::TRANSPARENT);
            assert_eq!(quiet.outline, Stroke::NONE);
            assert_eq!(quiet.label, roles.text_primary);

            let destructive = variant_paint(roles, Variant::Destructive);
            assert_eq!(destructive.fill, Color32::TRANSPARENT);
            assert_eq!(
                destructive.outline,
                Stroke::new(DESTRUCTIVE_OUTLINE_WIDTH, roles.danger)
            );
            assert_eq!(destructive.label, roles.danger);
        }
    }

    // -------------------------------------------------------------
    // T005 — hover_fill / pressed_fill / focus_ring (I1, I2, F1, F2)
    // -------------------------------------------------------------

    #[test]
    fn hover_fill_is_four_percent_foreground() {
        for roles in [&LIGHT, &DARK] {
            assert_eq!(hover_fill(roles), roles.text_primary.gamma_multiply(0.04));
        }
    }

    #[test]
    fn pressed_fill_is_double_hover() {
        for roles in [&LIGHT, &DARK] {
            assert_eq!(pressed_fill(roles), roles.text_primary.gamma_multiply(0.08));
            assert!((PRESSED_ALPHA - 2.0 * HOVER_ALPHA).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn focus_ring_is_two_px_accent() {
        for roles in [&LIGHT, &DARK] {
            let ring = focus_ring(roles);
            assert_eq!(ring.width, FOCUS_RING_WIDTH);
            assert_eq!(ring.width, 2.0);
            assert_eq!(ring.color, roles.accent);
        }
    }

    #[test]
    fn focus_ring_is_offset() {
        assert_eq!(FOCUS_RING_GAP, 1.0);
    }

    // -------------------------------------------------------------
    // T006 — switch metrics, off/on table, thumb-position delta
    // (S1, S2, S3)
    // -------------------------------------------------------------

    #[test]
    fn switch_metrics_come_from_the_scale() {
        let metrics = switch_metrics();
        assert_eq!(metrics.track, vec2(space::XXL, space::LG));
        assert_eq!(metrics.track, vec2(32.0, 16.0));
        assert_eq!(metrics.thumb, space::MD);
        assert_eq!(metrics.thumb, 12.0);
        assert_eq!(metrics.inset, (metrics.track.y - metrics.thumb) / 2.0);
        assert_eq!(metrics.inset, 2.0);
    }

    #[test]
    fn switch_states_match_the_table() {
        for roles in [&LIGHT, &DARK] {
            let (off_track, off_outline) = switch_track(roles, false);
            assert_eq!(off_track, roles.surface_raised);
            assert_eq!(
                off_outline,
                Stroke::new(SWITCH_OUTLINE_WIDTH, divider_color_for(roles))
            );
            assert_eq!(switch_thumb(roles, false), roles.text_secondary);

            let (on_track, on_outline) = switch_track(roles, true);
            assert_eq!(on_track, roles.accent);
            assert_eq!(on_outline, Stroke::NONE);
            assert_eq!(switch_thumb(roles, true), roles.text_on_accent);
        }
    }

    #[test]
    fn switch_thumb_position_carries_state() {
        // NFR-6.4 / contract S3: the thumb's centre moves between states
        // by `track.x - thumb - 2 * inset` > 0 — state is not carried by
        // colour alone. The widget computes the actual positions
        // (`track.left() + inset` / `track.right() - inset - thumb`); this
        // pins the metric that makes that delta positive.
        let metrics = switch_metrics();
        let delta = metrics.track.x - metrics.thumb - 2.0 * metrics.inset;
        assert!(delta > 0.0, "thumb travel must be positive, got {delta}");
        assert_eq!(delta, 16.0);
    }

    // -------------------------------------------------------------
    // T007 — DESTRUCTIVE_GAP >= 2 * item_spacing.x (B9)
    // -------------------------------------------------------------

    #[test]
    fn destructive_gap_is_twice_item_spacing() {
        for theme in [EguiTheme::Light, EguiTheme::Dark] {
            let style = build_style(theme, false);
            assert!(
                DESTRUCTIVE_GAP >= 2.0 * style.spacing.item_spacing.x,
                "DESTRUCTIVE_GAP ({DESTRUCTIVE_GAP}) must be >= 2x item_spacing.x ({})",
                style.spacing.item_spacing.x
            );
        }
    }

    // -------------------------------------------------------------
    // T008 — band()'s fixed-order selection (M1–M5)
    // -------------------------------------------------------------

    #[test]
    fn band_boundaries() {
        assert_eq!(BAND_WARNING_DB, -6.0);
    }

    #[test]
    fn band_selects_in_the_fixed_order() {
        let boundary = 0.0;
        assert_eq!(band(-12.0, boundary), Band::Positive);
        assert_eq!(band(-6.0, boundary), Band::Warning);
        assert_eq!(band(-3.0, boundary), Band::Warning);
        assert_eq!(band(0.0, boundary), Band::Danger);
        assert_eq!(band(3.0, boundary), Band::Danger);
        // Total over f32: NaN satisfies neither `>=` and falls through.
        assert_eq!(band(f32::NAN, boundary), Band::Positive);
    }

    #[test]
    fn boundary_belongs_to_the_higher_band() {
        // A boundary value belongs to the band above it, matching today's
        // `peak_db >= ceiling_db` test.
        assert_eq!(band(BAND_WARNING_DB, 0.0), Band::Warning);
        assert_eq!(band(-6.0001, 0.0), Band::Positive);
        assert_eq!(band(0.0, 0.0), Band::Danger);
    }

    #[test]
    fn low_ceiling_empties_the_warning_band() {
        // A danger boundary at or below -6 dBFS empties the warning band:
        // positive below the boundary, danger at or above it.
        for boundary in [-6.0, -9.0, -20.0] {
            assert_eq!(band(boundary - 1.0, boundary), Band::Positive);
            assert_eq!(band(boundary, boundary), Band::Danger);
            assert_eq!(band(boundary + 1.0, boundary), Band::Danger);
        }
    }

    #[test]
    fn band_colours_come_from_roles() {
        for roles in [&LIGHT, &DARK] {
            assert_eq!(band_color(roles, Band::Positive), roles.positive);
            assert_eq!(band_color(roles, Band::Warning), roles.warning);
            assert_eq!(band_color(roles, Band::Danger), roles.danger);
        }
    }

    // -------------------------------------------------------------
    // T009 — mark_color's fill/track rule (K3)
    // -------------------------------------------------------------

    #[test]
    fn mark_colour_follows_the_fill() {
        for roles in [&LIGHT, &DARK] {
            assert_eq!(mark_color(roles, true), roles.surface_base);
            assert_eq!(mark_color(roles, false), roles.text_secondary);
        }
    }

    // -------------------------------------------------------------
    // T053 — TAB_UNDERLINE_WIDTH / tab_underline (contracts/tab-strip.md
    // T2/T3, FR-034, FR-024)
    // -------------------------------------------------------------

    /// T2: the underline's thickness is a named constant beside
    /// `FOCUS_RING_WIDTH`, never a literal at the call site.
    #[test]
    fn tab_underline_width_is_a_named_constant() {
        assert_eq!(TAB_UNDERLINE_WIDTH, 2.0);
    }

    /// T3: the underline's colour is the `accent` role — no new colour, no
    /// eleventh role — in both themes.
    #[test]
    fn tab_underline_is_accent_stroke_both_themes() {
        for roles in [&LIGHT, &DARK] {
            let stroke = tab_underline(roles);
            assert_eq!(stroke.width, TAB_UNDERLINE_WIDTH);
            assert_eq!(stroke.color, roles.accent);
        }
    }
}

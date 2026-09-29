// SPDX-License-Identifier: MIT OR Apache-2.0

//! The waveform's own per-appearance token table (022-waveform-legibility,
//! data-model.md §3): four fill tones (played/unplayed × peak/average),
//! the cased playhead's two strokes, and the hover scrub indicator's line
//! and label colours. Kept separate from [`super::tokens::Roles`] (which
//! 014's ten-role contract and tests already pin exactly) rather than
//! adding fields to it — [`waveform_roles`] is the one selection site atop
//! that existing scale, mirroring 017's `Roles::high_contrast` pattern
//! (FR-017/FR-012).

use egui::Color32;

use super::tokens::{DARK, DARK_HIGH_CONTRAST, LIGHT, LIGHT_HIGH_CONTRAST, Roles};

/// The waveform's per-appearance colour tokens (data-model.md §3). One
/// `static` table per appearance, selected by [`waveform_roles`]. Fields
/// that equal a [`Roles`] value are initialised from that constant (e.g.
/// `playhead_core: LIGHT.text_primary`) rather than re-typed, so they
/// cannot drift from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveformRoles {
    /// Outer peak bar, left of the displayed playhead (FR-001, FR-002).
    pub played_peak: Color32,
    /// Inner ±RMS band, left of the displayed playhead (FR-001, FR-002).
    pub played_average: Color32,
    /// Outer peak bar, right of the displayed playhead (FR-001, FR-002).
    pub unplayed_peak: Color32,
    /// Inner ±RMS band, right of the displayed playhead (FR-001, FR-002).
    pub unplayed_average: Color32,
    /// The playhead's inner, 2 px stroke (FR-003) — `= Roles::text_primary`.
    pub playhead_core: Color32,
    /// The playhead's outer, 1 px-per-side casing (FR-003) —
    /// `= Roles::surface_base`.
    pub playhead_casing: Color32,
    /// The hover scrub line (FR-006, FR-019) — a distinct field from
    /// [`Self::playhead_core`] (W3), even where both resolve to the same
    /// value in high contrast (there, width/casing/z-order distinguish
    /// them instead).
    pub hover_line: Color32,
    /// The hover label's text colour (FR-006, FR-011).
    pub hover_label_text: Color32,
    /// The hover label's pill background (FR-006, FR-011).
    pub hover_label_bg: Color32,
}

/// Light theme (data-model.md §3 table).
pub static LIGHT_WAVEFORM: WaveformRoles = WaveformRoles {
    played_peak: Color32::from_rgb(0x8F, 0xBC, 0xEB),
    played_average: LIGHT.accent,
    unplayed_peak: Color32::from_rgb(0xD4, 0xD4, 0xD8),
    unplayed_average: Color32::from_rgb(0x8E, 0x8E, 0x93),
    playhead_core: LIGHT.text_primary,
    playhead_casing: LIGHT.surface_base,
    hover_line: LIGHT.text_secondary,
    hover_label_text: LIGHT.text_primary,
    hover_label_bg: LIGHT.surface_raised,
};

/// Dark theme (data-model.md §3 table).
pub static DARK_WAVEFORM: WaveformRoles = WaveformRoles {
    played_peak: Color32::from_rgb(0x2C, 0x5A, 0x8C),
    played_average: DARK.accent,
    unplayed_peak: Color32::from_rgb(0x34, 0x34, 0x3B),
    unplayed_average: Color32::from_rgb(0x74, 0x74, 0x7C),
    playhead_core: DARK.text_primary,
    playhead_casing: DARK.surface_base,
    hover_line: DARK.text_secondary,
    hover_label_text: DARK.text_primary,
    hover_label_bg: DARK.surface_raised,
};

/// Light theme, high contrast (data-model.md §3 table). `played_average`
/// is a darkened tone distinct from [`LIGHT_HIGH_CONTRAST`]'s own
/// `accent` (data-model.md §3's "LHC darkened" note) so the fill keeps its
/// worst-case contrast margin against the cased playhead (W2).
pub static LIGHT_HIGH_CONTRAST_WAVEFORM: WaveformRoles = WaveformRoles {
    played_peak: Color32::from_rgb(0x6F, 0xA3, 0xDE),
    played_average: Color32::from_rgb(0x00, 0x50, 0xA8),
    unplayed_peak: Color32::from_rgb(0xB4, 0xB4, 0xBA),
    unplayed_average: Color32::from_rgb(0x5B, 0x5B, 0x60),
    playhead_core: LIGHT_HIGH_CONTRAST.text_primary,
    playhead_casing: LIGHT_HIGH_CONTRAST.surface_base,
    hover_line: LIGHT_HIGH_CONTRAST.text_primary,
    hover_label_text: LIGHT_HIGH_CONTRAST.text_primary,
    hover_label_bg: LIGHT_HIGH_CONTRAST.surface_raised,
};

/// Dark theme, high contrast (data-model.md §3 table).
pub static DARK_HIGH_CONTRAST_WAVEFORM: WaveformRoles = WaveformRoles {
    played_peak: Color32::from_rgb(0x3A, 0x6F, 0xA8),
    played_average: Color32::from_rgb(0x74, 0xB6, 0xFF),
    unplayed_peak: Color32::from_rgb(0x44, 0x44, 0x4C),
    unplayed_average: Color32::from_rgb(0xA8, 0xA8, 0xB0),
    playhead_core: DARK_HIGH_CONTRAST.text_primary,
    playhead_casing: DARK_HIGH_CONTRAST.surface_base,
    hover_line: DARK_HIGH_CONTRAST.text_primary,
    hover_label_text: DARK_HIGH_CONTRAST.text_primary,
    hover_label_bg: DARK_HIGH_CONTRAST.surface_raised,
};

/// The waveform's token table for the already-selected `roles` (data-model
/// §3): compares `roles` by value against the four `tokens::{LIGHT, DARK,
/// LIGHT_HIGH_CONTRAST, DARK_HIGH_CONTRAST}` tables (`Roles: PartialEq`),
/// falling back to the light table for any other value — a pure function
/// of the caller's already-selected `Roles` (017's single-selection-site
/// pattern, FR-017), never a second, independent choice.
///
/// ```
/// # use modplayer_ui::theme::tokens::{LIGHT, DARK_HIGH_CONTRAST};
/// # use modplayer_ui::theme::waveform::waveform_roles;
/// let light = waveform_roles(&LIGHT);
/// assert_eq!(light.playhead_core, LIGHT.text_primary);
///
/// let dark_hc = waveform_roles(&DARK_HIGH_CONTRAST);
/// assert_eq!(dark_hc.playhead_casing, DARK_HIGH_CONTRAST.surface_base);
/// ```
#[must_use]
pub fn waveform_roles(roles: &Roles) -> &'static WaveformRoles {
    if *roles == DARK {
        &DARK_WAVEFORM
    } else if *roles == LIGHT_HIGH_CONTRAST {
        &LIGHT_HIGH_CONTRAST_WAVEFORM
    } else if *roles == DARK_HIGH_CONTRAST {
        &DARK_HIGH_CONTRAST_WAVEFORM
    } else {
        &LIGHT_WAVEFORM
    }
}

/// The playhead's inner stroke width (FR-003: >= 2 px), data-model.md
/// §3.1.
pub const PLAYHEAD_CORE_WIDTH: f32 = 2.0;

/// The playhead's total casing width, 1 px each side of the core
/// (FR-003), data-model.md §3.1.
pub const PLAYHEAD_CASING_WIDTH: f32 = 4.0;

/// The hover scrub line's width (FR-006, FR-019: strictly less than
/// [`PLAYHEAD_CORE_WIDTH`]), data-model.md §3.1.
pub const HOVER_LINE_WIDTH: f32 = 1.0;

/// The placeholder band's alpha over `text_secondary` (005, moved literal
/// — unchanged value), data-model.md §3.1.
pub const PLACEHOLDER_ALPHA: f32 = 0.4;

/// The overview detail-window highlight's alpha over `accent` (005, moved
/// literal — unchanged value), data-model.md §3.1.
pub const DETAIL_HIGHLIGHT_ALPHA: f32 = 0.25;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_roles_selects_by_value() {
        assert_eq!(*waveform_roles(&LIGHT), LIGHT_WAVEFORM);
        assert_eq!(*waveform_roles(&DARK), DARK_WAVEFORM);
        assert_eq!(
            *waveform_roles(&LIGHT_HIGH_CONTRAST),
            LIGHT_HIGH_CONTRAST_WAVEFORM
        );
        assert_eq!(
            *waveform_roles(&DARK_HIGH_CONTRAST),
            DARK_HIGH_CONTRAST_WAVEFORM
        );
    }

    /// Fields initialised straight from a `Roles` constant never drift
    /// from it (data-model.md §3's "cannot drift" note).
    #[test]
    fn roles_derived_fields_match_their_source() {
        for (roles, w) in [
            (&LIGHT, &LIGHT_WAVEFORM),
            (&DARK, &DARK_WAVEFORM),
            (&LIGHT_HIGH_CONTRAST, &LIGHT_HIGH_CONTRAST_WAVEFORM),
            (&DARK_HIGH_CONTRAST, &DARK_HIGH_CONTRAST_WAVEFORM),
        ] {
            assert_eq!(w.playhead_core, roles.text_primary);
            assert_eq!(w.playhead_casing, roles.surface_base);
            assert_eq!(w.hover_label_text, roles.text_primary);
            assert_eq!(w.hover_label_bg, roles.surface_raised);
        }
        assert_eq!(LIGHT_WAVEFORM.hover_line, LIGHT.text_secondary);
        assert_eq!(DARK_WAVEFORM.hover_line, DARK.text_secondary);
    }
}

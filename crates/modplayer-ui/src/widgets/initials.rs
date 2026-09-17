// SPDX-License-Identifier: MIT OR Apache-2.0

//! The initials-placeholder helper and widget (contracts/ui-surface.md
//! §6, data-model.md §4 "Initials rule", FR-020): drawn wherever artwork
//! is `Failed` (or absent) — up to two uppercase letters (the first
//! alphanumeric character of each of the name's first two words), or the
//! music glyph `♪` when the name has no letter or digit at all.

use egui::{Align2, Color32, CornerRadius, FontId, Sense, Ui, Vec2};

/// The glyph shown when a name has no letter or digit to derive initials
/// from (FR-020).
pub const MUSIC_GLYPH: char = '♪';

/// What [`initials_placeholder`] draws for a given name (data-model.md
/// §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initials {
    /// 1-2 uppercase characters.
    Letters(String),
    Glyph,
}

impl Initials {
    /// The text drawn inside the placeholder square.
    pub fn text(&self) -> String {
        match self {
            Self::Letters(letters) => letters.clone(),
            Self::Glyph => MUSIC_GLYPH.to_string(),
        }
    }
}

/// Derive the initials for `name` (contracts/ui-surface.md §6): the first
/// alphanumeric character of each of the first two words, uppercased —
/// `Glyph` when `name` has no letter/digit anywhere (e.g. empty, or
/// punctuation-only).
///
/// ```
/// use modplayer_ui::widgets::initials::{initials, Initials};
///
/// assert_eq!(initials("Dancing Queen"), Initials::Letters("DQ".to_string()));
/// assert_eq!(initials("Beyoncé"), Initials::Letters("B".to_string()));
/// assert_eq!(initials("..."), Initials::Glyph);
/// ```
pub fn initials(name: &str) -> Initials {
    let letters: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    if letters.is_empty() {
        Initials::Glyph
    } else {
        Initials::Letters(letters)
    }
}

/// Draw a `size` x `size` initials placeholder for `name` (FR-020): a
/// neutral square with the derived [`Initials`] centred. Used wherever
/// artwork is absent or failed to load (`ArtworkCache`, `rows::ListRow`).
pub fn initials_placeholder(ui: &mut Ui, name: &str, size: f32) {
    let (rect, _response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius::from((size * 0.15) as u8),
            ui.visuals().extreme_bg_color,
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            initials(name).text(),
            FontId::proportional(size * 0.4),
            text_color(ui),
        );
    }
}

fn text_color(ui: &Ui) -> Color32 {
    ui.visuals().text_color()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_words_yield_two_uppercase_initials() {
        assert_eq!(initials("Abbey Road"), Initials::Letters("AR".to_string()));
    }

    #[test]
    fn single_word_yields_one_letter() {
        assert_eq!(initials("Thriller"), Initials::Letters("T".to_string()));
    }

    #[test]
    fn skips_leading_punctuation_within_a_word() {
        assert_eq!(initials("(Even) Now"), Initials::Letters("EN".to_string()));
    }

    #[test]
    fn no_letters_or_digits_falls_back_to_the_glyph() {
        assert_eq!(initials("..."), Initials::Glyph);
        assert_eq!(initials(""), Initials::Glyph);
        assert_eq!(initials("   "), Initials::Glyph);
    }

    #[test]
    fn digits_count_as_initials() {
        assert_eq!(initials("21 Grams"), Initials::Letters("2G".to_string()));
    }

    #[test]
    fn more_than_two_words_only_uses_the_first_two() {
        assert_eq!(
            initials("The Dark Side"),
            Initials::Letters("TD".to_string())
        );
    }

    #[test]
    fn glyph_text_is_the_music_glyph() {
        assert_eq!(Initials::Glyph.text(), MUSIC_GLYPH.to_string());
    }
}

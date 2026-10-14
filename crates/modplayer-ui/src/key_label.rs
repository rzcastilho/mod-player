// SPDX-License-Identifier: MIT OR Apache-2.0

//! Key label model: the visible, spoken and searchable forms of a chord
//! (029-keyboard-shortcut-reference, data-model §2, contracts K7-K9).
//!
//! The structure of a label comes from core
//! ([`Chord::display_parts`](modplayer_core::Chord::display_parts));
//! this module localises its words through Fluent, spells it out for
//! assistive technology, and builds the aliases the controls filter
//! matches against.

use modplayer_core::{Chord, KeyToken, Platform, tr};

/// A chord label in its three forms.
///
/// ```
/// use modplayer_core::{Chord, Platform};
/// use modplayer_ui::key_label::KeyLabel;
///
/// let chord = Chord::parse("Primary+Shift+L").unwrap();
/// let label = KeyLabel::new(chord, Platform::Other);
/// assert_eq!(label.visible, "Ctrl+Shift+L");
/// assert_eq!(label.spoken, "Ctrl+Shift+L");
/// assert!(label.matches_key("l"));
/// assert!(label.matches_key("ctrl+shift+l"));
/// assert!(!label.matches_key("shift"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyLabel {
    /// What the chip paints.
    pub visible: String,
    /// Spelled out, no symbol characters: the accessible name.
    pub spoken: String,
    /// Lower-cased, de-duplicated filter aliases: the canonical key name,
    /// the visible key label, and the whole visible chord.
    pub aliases: Vec<String>,
}

impl KeyLabel {
    /// Build the label of `chord` for `platform`.
    pub fn new(chord: Chord, platform: Platform) -> KeyLabel {
        let parts = chord.display_parts(platform);
        let sep = parts.separator();

        let visible_of = |token: KeyToken| match token {
            KeyToken::Symbol(s) => s.to_string(),
            KeyToken::Word(w) => tr(w.fluent_key()),
        };
        let visible = parts.tokens().map(visible_of).collect::<Vec<_>>().join(sep);

        let mut spoken_parts: Vec<String> = parts
            .mods
            .iter()
            .map(|token| match token {
                KeyToken::Symbol("\u{2318}") => tr("key-spoken-command"),
                KeyToken::Symbol("\u{21e7}") => tr("key-word-shift"),
                KeyToken::Symbol("\u{2325}") => tr("key-spoken-option"),
                KeyToken::Symbol(s) => (*s).to_string(),
                KeyToken::Word(w) => tr(w.spoken_key()),
            })
            .collect();
        spoken_parts.push(match parts.key {
            KeyToken::Word(w) => tr(w.spoken_key()),
            KeyToken::Symbol(s) => match spoken_symbol_key(chord.key.as_str()) {
                Some(key) => tr(key),
                None => s.to_string(),
            },
        });
        let spoken = spoken_parts.join("+");

        let mut aliases: Vec<String> = Vec::new();
        for alias in [
            chord.key.as_str().to_lowercase(),
            visible_of(parts.key).to_lowercase(),
            visible.to_lowercase(),
        ] {
            if !alias.is_empty() && !aliases.contains(&alias) {
                aliases.push(alias);
            }
        }

        KeyLabel {
            visible,
            spoken,
            aliases,
        }
    }

    /// Whether `query` (already trimmed and lower-cased) equals one of
    /// the aliases. An empty query never matches.
    pub fn matches_key(&self, query: &str) -> bool {
        !query.is_empty() && self.aliases.iter().any(|alias| alias == query)
    }
}

/// The `key-spoken-*` Fluent key for a key whose visible form is a
/// symbol character that screen readers would not pronounce usefully.
fn spoken_symbol_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "Left" => "key-spoken-arrow-left",
        "Right" => "key-spoken-arrow-right",
        "Up" => "key-spoken-arrow-up",
        "Down" => "key-spoken-arrow-down",
        "Colon" => "key-spoken-colon",
        "Comma" => "key-spoken-comma",
        "Minus" => "key-spoken-minus",
        "Period" => "key-spoken-period",
        "Plus" => "key-spoken-plus",
        "Equals" => "key-spoken-equals",
        "Semicolon" => "key-spoken-semicolon",
        "Backslash" => "key-spoken-backslash",
        "Slash" => "key-spoken-slash",
        "Pipe" => "key-spoken-pipe",
        "Questionmark" => "key-spoken-questionmark",
        "Exclamationmark" => "key-spoken-exclamationmark",
        "OpenBracket" => "key-spoken-open-bracket",
        "CloseBracket" => "key-spoken-close-bracket",
        "OpenCurlyBracket" => "key-spoken-open-curly",
        "CloseCurlyBracket" => "key-spoken-close-curly",
        "Backtick" => "key-spoken-backtick",
        "Quote" => "key-spoken-quote",
        _ => return None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn label(encoded: &str, platform: Platform) -> KeyLabel {
        KeyLabel::new(Chord::parse(encoded).unwrap(), platform)
    }

    /// K9: spoken forms contain no symbol characters.
    #[test]
    fn spoken_forms_are_spelled_out() {
        assert_eq!(
            label("Primary+Shift+L", Platform::Mac).spoken,
            "Command+Shift+L"
        );
        assert_eq!(label("Space", Platform::Mac).spoken, "Space");
        assert_eq!(
            label("Primary+Left", Platform::Mac).spoken,
            "Command+Left Arrow"
        );
        assert_eq!(label("OpenBracket", Platform::Mac).spoken, "Left Bracket");
        assert_eq!(
            label("Primary+Shift+L", Platform::Other).spoken,
            "Ctrl+Shift+L"
        );
        assert_eq!(label("Alt+Enter", Platform::Mac).spoken, "Option+Return");
    }

    #[test]
    fn visible_forms_follow_the_platform() {
        assert_eq!(
            label("Primary+Shift+L", Platform::Mac).visible,
            "\u{2318}\u{21e7}L"
        );
        assert_eq!(label("Shift+Space", Platform::Other).visible, "Shift+Space");
        assert_eq!(label("Left", Platform::Mac).visible, "\u{2190}");
    }

    #[test]
    fn aliases_cover_name_key_label_and_chord() {
        let mac = label("Primary+Left", Platform::Mac);
        assert!(mac.matches_key("left"));
        assert!(mac.matches_key("\u{2190}"));
        assert!(mac.matches_key("\u{2318}\u{2190}"));
        let other = label("Primary+Shift+L", Platform::Other);
        assert!(other.matches_key("l"));
        assert!(other.matches_key("ctrl+shift+l"));
        assert!(!other.matches_key("ctrl"));
        assert!(!other.matches_key(""));
        let bracket = label("OpenBracket", Platform::Other);
        assert!(bracket.matches_key("["));
        assert!(bracket.matches_key("openbracket"));
        let mut sorted = other.aliases.clone();
        sorted.dedup();
        assert_eq!(sorted, other.aliases);
    }

    #[test]
    fn matching_is_exact_not_substring() {
        let enter = label("Enter", Platform::Other);
        assert!(!enter.matches_key("e"));
        assert!(enter.matches_key("enter"));
    }
}

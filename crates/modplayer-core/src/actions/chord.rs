// SPDX-License-Identifier: MIT OR Apache-2.0

//! `KeyName`, `Mods`, `Chord` and `Platform`: the closed key vocabulary
//! and chord grammar `[Primary+][Shift+][Alt+]<KeyName>` (007,
//! data-model.md §1.3, research R2/R14).

use super::ChordParseError;

/// The closed key-name vocabulary: exactly egui 0.36's `Key::name()`
/// output (research R2), so core stays free of an egui dependency while
/// a UI test (`key_name_table_matches_egui_key_all`, T054) pins the
/// bijection against `egui::Key::ALL`. Grouped as egui's own `Key` enum
/// is grouped; every value is unique (no duplicates to "dedup").
pub const KEY_NAMES: &[&str] = &[
    // Commands
    "Down",
    "Left",
    "Right",
    "Up",
    "Escape",
    "Tab",
    "Backspace",
    "Enter",
    "Insert",
    "Delete",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Copy",
    "Cut",
    "Paste",
    // Punctuation
    "Space",
    "Colon",
    "Comma",
    "Minus",
    "Period",
    "Plus",
    "Equals",
    "Semicolon",
    "Backslash",
    "Slash",
    "Pipe",
    "Questionmark",
    "Exclamationmark",
    "OpenBracket",
    "CloseBracket",
    "OpenCurlyBracket",
    "CloseCurlyBracket",
    "Backtick",
    "Quote",
    // Digits
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    // Letters
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    // Function keys
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "F13",
    "F14",
    "F15",
    "F16",
    "F17",
    "F18",
    "F19",
    "F20",
    "F21",
    "F22",
    "F23",
    "F24",
    "F25",
    "F26",
    "F27",
    "F28",
    "F29",
    "F30",
    "F31",
    "F32",
    "F33",
    "F34",
    "F35",
    // Multimedia / physical modifier / international keys (emitted only
    // as the `physical_key` half of an `Event::Key`, R3).
    "BrowserBack",
    "ShiftLeft",
    "ShiftRight",
    "ControlLeft",
    "ControlRight",
    "AltLeft",
    "AltRight",
    "SuperLeft",
    "SuperRight",
    "IntlBackslash",
];

/// One key from the closed [`KEY_NAMES`] vocabulary. Constructed only
/// through [`KeyName::parse`], so a `KeyName` is always a value the UI
/// crate can round-trip through `egui::Key::from_name`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyName(&'static str);

impl KeyName {
    /// `s` must exactly match (case-sensitive) one entry of
    /// [`KEY_NAMES`]; `None` otherwise.
    pub fn parse(s: &str) -> Option<KeyName> {
        KEY_NAMES
            .iter()
            .find(|&&name| name == s)
            .map(|&name| KeyName(name))
    }

    /// The canonical, `'static` spelling.
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

/// The three platform-uniform modifiers a chord may combine (NFR-9.2).
/// `primary` is `Ctrl` on Windows/Linux, `Cmd` on macOS (R14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Mods {
    pub primary: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Mods {
    /// Whether no modifier is held.
    pub fn is_none(&self) -> bool {
        !self.primary && !self.shift && !self.alt
    }
}

/// A platform-uniform key combination: any subset of {Primary, Shift,
/// Alt} plus exactly one [`KeyName`] (FR-003, DM-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Chord {
    pub mods: Mods,
    pub key: KeyName,
}

/// Which platform's modifier glyphs [`Chord::display`] should use
/// (R14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Other,
}

/// A modifier token's canonical position in the grammar
/// `[Primary+][Shift+][Alt+]<KeyName>` — `Primary` must precede `Shift`,
/// which must precede `Alt`.
fn modifier_rank(token: &str) -> Option<u8> {
    match token {
        "Primary" => Some(0),
        "Shift" => Some(1),
        "Alt" => Some(2),
        _ => None,
    }
}

impl Chord {
    /// Build a chord directly from its parts (no grammar involved —
    /// used by the UI adapter, which already has a matched `Mods` and
    /// `KeyName` in hand).
    pub fn new(mods: Mods, key: KeyName) -> Chord {
        Chord { mods, key }
    }

    /// Parse `[Primary+][Shift+][Alt+]<KeyName>`: modifiers in that
    /// fixed order, each at most once, case-sensitive, no whitespace,
    /// followed by exactly one [`KeyName`] (FR-003, R2).
    ///
    /// ```
    /// # use modplayer_core::actions::Chord;
    /// if let Ok(chord) = Chord::parse("Primary+Shift+Right") {
    ///     assert_eq!(chord.encode(), "Primary+Shift+Right");
    /// }
    /// ```
    pub fn parse(s: &str) -> Result<Chord, ChordParseError> {
        if s.is_empty() {
            return Err(ChordParseError::Empty);
        }

        let tokens: Vec<&str> = s.split('+').collect();
        let (mod_tokens, key_tokens) = tokens.split_at(tokens.len() - 1);

        let mut mods = Mods::default();
        let mut min_next_rank: u8 = 0;
        let mut consumed = mod_tokens.len();

        for (i, &token) in mod_tokens.iter().enumerate() {
            let Some(rank) = modifier_rank(token) else {
                return Err(ChordParseError::UnknownModifier(token.to_string()));
            };
            let already_seen = match rank {
                0 => mods.primary,
                1 => mods.shift,
                _ => mods.alt,
            };
            if already_seen {
                return Err(ChordParseError::DuplicateModifier);
            }
            if rank < min_next_rank {
                // A recognised modifier name, but out of canonical
                // order (e.g. "Shift+Primary+A"): stop treating tokens
                // as modifiers here; everything from this token onward
                // becomes the key candidate below.
                consumed = i;
                break;
            }
            match rank {
                0 => mods.primary = true,
                1 => mods.shift = true,
                _ => mods.alt = true,
            }
            min_next_rank = rank + 1;
        }

        let key_candidate = mod_tokens[consumed..]
            .iter()
            .chain(key_tokens.iter())
            .copied()
            .collect::<Vec<_>>()
            .join("+");

        match KeyName::parse(&key_candidate) {
            Some(key) => Ok(Chord { mods, key }),
            None => Err(ChordParseError::UnknownKey(key_candidate)),
        }
    }

    /// The canonical persisted encoding — the exact inverse of
    /// [`Chord::parse`] for any chord `parse` can produce.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        if self.mods.primary {
            out.push_str("Primary+");
        }
        if self.mods.shift {
            out.push_str("Shift+");
        }
        if self.mods.alt {
            out.push_str("Alt+");
        }
        out.push_str(self.key.as_str());
        out
    }

    /// A human-readable rendering for the shortcut map and tests
    /// (R14): macOS glyphs with no separator (`⌘⇧→`), or
    /// `Ctrl`/`Shift`/`Alt`/name joined by `+` elsewhere. Never empty.
    pub fn display(&self, platform: Platform) -> String {
        match platform {
            Platform::Mac => {
                let mut out = String::new();
                if self.mods.primary {
                    out.push('⌘');
                }
                if self.mods.shift {
                    out.push('⇧');
                }
                if self.mods.alt {
                    out.push('⌥');
                }
                out.push_str(mac_key_symbol(self.key.as_str()));
                out
            }
            Platform::Other => {
                let mut parts = Vec::new();
                if self.mods.primary {
                    parts.push("Ctrl");
                }
                if self.mods.shift {
                    parts.push("Shift");
                }
                if self.mods.alt {
                    parts.push("Alt");
                }
                parts.push(self.key.as_str());
                parts.join("+")
            }
        }
    }
}

/// macOS's symbol for a key name where one exists (R14); every other
/// name (letters, digits, function keys, unlisted punctuation) displays
/// as its own name.
fn mac_key_symbol(name: &str) -> &str {
    match name {
        "Left" => "←",
        "Right" => "→",
        "Up" => "↑",
        "Down" => "↓",
        "Space" => "␣",
        "Enter" => "⏎",
        "Tab" => "⇥",
        "Escape" => "⎋",
        "Plus" => "+",
        "Equals" => "=",
        "Minus" => "-",
        "Slash" => "/",
        other => other,
    }
}

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

    /// The platform-specific structure of this chord's label (029,
    /// data-model §1.2): modifier tokens in the fixed order primary,
    /// shift, alt, then the key token. Total over every [`KeyName`] and
    /// never empty.
    ///
    /// ```
    /// # use modplayer_core::actions::{Chord, Platform, KeyToken, KeyWord};
    /// let c = Chord::parse("Shift+Space").unwrap();
    /// let parts = c.display_parts(Platform::Mac);
    /// assert_eq!(parts.mods, vec![KeyToken::Symbol("\u{21e7}")]);
    /// assert_eq!(parts.key, KeyToken::Word(KeyWord::Space));
    /// ```
    pub fn display_parts(&self, platform: Platform) -> DisplayParts {
        let mut mods = Vec::new();
        match platform {
            Platform::Mac => {
                if self.mods.primary {
                    mods.push(KeyToken::Symbol("\u{2318}"));
                }
                if self.mods.shift {
                    mods.push(KeyToken::Symbol("\u{21e7}"));
                }
                if self.mods.alt {
                    mods.push(KeyToken::Symbol("\u{2325}"));
                }
            }
            Platform::Other => {
                if self.mods.primary {
                    mods.push(KeyToken::Word(KeyWord::Ctrl));
                }
                if self.mods.shift {
                    mods.push(KeyToken::Word(KeyWord::Shift));
                }
                if self.mods.alt {
                    mods.push(KeyToken::Word(KeyWord::Alt));
                }
            }
        }
        DisplayParts {
            mods,
            key: key_token(self.key, platform),
            platform,
        }
    }

    /// A human-readable rendering (en-US words): macOS symbols with no
    /// separator (`\u{2318}\u{21e7}\u{2192}`, `\u{21e7}Space`), or
    /// `Ctrl`/`Shift`/`Alt`/key joined by `+` elsewhere. Never empty.
    ///
    /// ```
    /// # use modplayer_core::actions::{Chord, Platform};
    /// let c = Chord::parse("Primary+Shift+L").unwrap();
    /// assert_eq!(c.display(Platform::Other), "Ctrl+Shift+L");
    /// ```
    pub fn display(&self, platform: Platform) -> String {
        self.display_parts(platform).to_en_string()
    }
}

/// One token of a chord label: a printed symbol, or a word that must be
/// localised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyToken {
    Symbol(&'static str),
    Word(KeyWord),
}

/// The closed set of words that appear in chord labels. Each has a
/// Fluent key (`key-word-*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyWord {
    Space,
    Return,
    Enter,
    Esc,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Copy,
    Cut,
    Paste,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Ctrl,
    Shift,
    Alt,
    BrowserBack,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    SuperLeft,
    SuperRight,
    IntlBackslash,
}

impl KeyWord {
    /// Every variant, for exhaustive string-coverage tests.
    pub const ALL: [KeyWord; 32] = [
        KeyWord::Space,
        KeyWord::Return,
        KeyWord::Enter,
        KeyWord::Esc,
        KeyWord::Tab,
        KeyWord::Backspace,
        KeyWord::Delete,
        KeyWord::Insert,
        KeyWord::Home,
        KeyWord::End,
        KeyWord::PageUp,
        KeyWord::PageDown,
        KeyWord::Copy,
        KeyWord::Cut,
        KeyWord::Paste,
        KeyWord::ArrowLeft,
        KeyWord::ArrowRight,
        KeyWord::ArrowUp,
        KeyWord::ArrowDown,
        KeyWord::Ctrl,
        KeyWord::Shift,
        KeyWord::Alt,
        KeyWord::BrowserBack,
        KeyWord::ShiftLeft,
        KeyWord::ShiftRight,
        KeyWord::ControlLeft,
        KeyWord::ControlRight,
        KeyWord::AltLeft,
        KeyWord::AltRight,
        KeyWord::SuperLeft,
        KeyWord::SuperRight,
        KeyWord::IntlBackslash,
    ];

    /// The Fluent key of the visible word.
    ///
    /// ```
    /// # use modplayer_core::actions::KeyWord;
    /// assert_eq!(KeyWord::PageUp.fluent_key(), "key-word-page-up");
    /// ```
    pub fn fluent_key(self) -> &'static str {
        match self {
            KeyWord::Space => "key-word-space",
            KeyWord::Return => "key-word-return",
            KeyWord::Enter => "key-word-enter",
            KeyWord::Esc => "key-word-esc",
            KeyWord::Tab => "key-word-tab",
            KeyWord::Backspace => "key-word-backspace",
            KeyWord::Delete => "key-word-delete",
            KeyWord::Insert => "key-word-insert",
            KeyWord::Home => "key-word-home",
            KeyWord::End => "key-word-end",
            KeyWord::PageUp => "key-word-page-up",
            KeyWord::PageDown => "key-word-page-down",
            KeyWord::Copy => "key-word-copy",
            KeyWord::Cut => "key-word-cut",
            KeyWord::Paste => "key-word-paste",
            KeyWord::ArrowLeft => "key-word-left",
            KeyWord::ArrowRight => "key-word-right",
            KeyWord::ArrowUp => "key-word-up",
            KeyWord::ArrowDown => "key-word-down",
            KeyWord::Ctrl => "key-word-ctrl",
            KeyWord::Shift => "key-word-shift",
            KeyWord::Alt => "key-word-alt",
            KeyWord::BrowserBack => "key-word-browser-back",
            KeyWord::ShiftLeft => "key-word-shift-left",
            KeyWord::ShiftRight => "key-word-shift-right",
            KeyWord::ControlLeft => "key-word-control-left",
            KeyWord::ControlRight => "key-word-control-right",
            KeyWord::AltLeft => "key-word-alt-left",
            KeyWord::AltRight => "key-word-alt-right",
            KeyWord::SuperLeft => "key-word-super-left",
            KeyWord::SuperRight => "key-word-super-right",
            KeyWord::IntlBackslash => "key-word-intl-backslash",
        }
    }

    /// The Fluent key of the spoken word. A word's spoken form reuses
    /// its visible string.
    ///
    /// ```
    /// # use modplayer_core::actions::KeyWord;
    /// assert_eq!(KeyWord::Esc.spoken_key(), KeyWord::Esc.fluent_key());
    /// ```
    pub fn spoken_key(self) -> &'static str {
        self.fluent_key()
    }

    /// The en-US text of the word (what [`Chord::display`] prints).
    pub fn en_us(self) -> &'static str {
        match self {
            KeyWord::Space => "Space",
            KeyWord::Return => "Return",
            KeyWord::Enter => "Enter",
            KeyWord::Esc => "Esc",
            KeyWord::Tab => "Tab",
            KeyWord::Backspace => "Backspace",
            KeyWord::Delete => "Delete",
            KeyWord::Insert => "Insert",
            KeyWord::Home => "Home",
            KeyWord::End => "End",
            KeyWord::PageUp => "Page Up",
            KeyWord::PageDown => "Page Down",
            KeyWord::Copy => "Copy",
            KeyWord::Cut => "Cut",
            KeyWord::Paste => "Paste",
            KeyWord::ArrowLeft => "Left",
            KeyWord::ArrowRight => "Right",
            KeyWord::ArrowUp => "Up",
            KeyWord::ArrowDown => "Down",
            KeyWord::Ctrl => "Ctrl",
            KeyWord::Shift => "Shift",
            KeyWord::Alt => "Alt",
            KeyWord::BrowserBack => "Browser Back",
            KeyWord::ShiftLeft => "Left Shift",
            KeyWord::ShiftRight => "Right Shift",
            KeyWord::ControlLeft => "Left Ctrl",
            KeyWord::ControlRight => "Right Ctrl",
            KeyWord::AltLeft => "Left Alt",
            KeyWord::AltRight => "Right Alt",
            KeyWord::SuperLeft => "Left Super",
            KeyWord::SuperRight => "Right Super",
            KeyWord::IntlBackslash => "Intl Backslash",
        }
    }
}

/// The structure of a chord's label for one platform (see
/// [`Chord::display_parts`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayParts {
    /// Modifier tokens (at most 3), in order primary, shift, alt.
    pub mods: Vec<KeyToken>,
    /// The key token.
    pub key: KeyToken,
    /// Mac joins parts with no separator; other platforms with `+`.
    pub platform: Platform,
}

impl DisplayParts {
    /// All tokens, modifiers first.
    pub fn tokens(&self) -> impl Iterator<Item = KeyToken> + '_ {
        self.mods.iter().copied().chain(std::iter::once(self.key))
    }

    /// The separator placed between tokens.
    pub fn separator(&self) -> &'static str {
        match self.platform {
            Platform::Mac => "",
            Platform::Other => "+",
        }
    }

    /// Join the tokens using en-US words.
    pub fn to_en_string(&self) -> String {
        let texts: Vec<&str> = self
            .tokens()
            .map(|t| match t {
                KeyToken::Symbol(s) => s,
                KeyToken::Word(w) => w.en_us(),
            })
            .collect();
        texts.join(self.separator())
    }
}

/// The key token for a key name on a platform (data-model §1.2).
fn key_token(key: KeyName, platform: Platform) -> KeyToken {
    let mac = platform == Platform::Mac;
    match key.as_str() {
        "Space" => KeyToken::Word(KeyWord::Space),
        "Enter" if mac => KeyToken::Word(KeyWord::Return),
        "Enter" => KeyToken::Word(KeyWord::Enter),
        "Escape" => KeyToken::Word(KeyWord::Esc),
        "Tab" => KeyToken::Word(KeyWord::Tab),
        "Left" if mac => KeyToken::Symbol("\u{2190}"),
        "Right" if mac => KeyToken::Symbol("\u{2192}"),
        "Up" if mac => KeyToken::Symbol("\u{2191}"),
        "Down" if mac => KeyToken::Symbol("\u{2193}"),
        "Left" => KeyToken::Word(KeyWord::ArrowLeft),
        "Right" => KeyToken::Word(KeyWord::ArrowRight),
        "Up" => KeyToken::Word(KeyWord::ArrowUp),
        "Down" => KeyToken::Word(KeyWord::ArrowDown),
        "PageUp" => KeyToken::Word(KeyWord::PageUp),
        "PageDown" => KeyToken::Word(KeyWord::PageDown),
        "Backspace" => KeyToken::Word(KeyWord::Backspace),
        "Delete" => KeyToken::Word(KeyWord::Delete),
        "Insert" => KeyToken::Word(KeyWord::Insert),
        "Home" => KeyToken::Word(KeyWord::Home),
        "End" => KeyToken::Word(KeyWord::End),
        "Copy" => KeyToken::Word(KeyWord::Copy),
        "Cut" => KeyToken::Word(KeyWord::Cut),
        "Paste" => KeyToken::Word(KeyWord::Paste),
        "Colon" => KeyToken::Symbol(":"),
        "Comma" => KeyToken::Symbol(","),
        "Minus" => KeyToken::Symbol("-"),
        "Period" => KeyToken::Symbol("."),
        "Plus" => KeyToken::Symbol("+"),
        "Equals" => KeyToken::Symbol("="),
        "Semicolon" => KeyToken::Symbol(";"),
        "Backslash" => KeyToken::Symbol("\\"),
        "Slash" => KeyToken::Symbol("/"),
        "Pipe" => KeyToken::Symbol("|"),
        "Questionmark" => KeyToken::Symbol("?"),
        "Exclamationmark" => KeyToken::Symbol("!"),
        "OpenBracket" => KeyToken::Symbol("["),
        "CloseBracket" => KeyToken::Symbol("]"),
        "OpenCurlyBracket" => KeyToken::Symbol("{"),
        "CloseCurlyBracket" => KeyToken::Symbol("}"),
        "Backtick" => KeyToken::Symbol("`"),
        "Quote" => KeyToken::Symbol("'"),
        "BrowserBack" => KeyToken::Word(KeyWord::BrowserBack),
        "ShiftLeft" => KeyToken::Word(KeyWord::ShiftLeft),
        "ShiftRight" => KeyToken::Word(KeyWord::ShiftRight),
        "ControlLeft" => KeyToken::Word(KeyWord::ControlLeft),
        "ControlRight" => KeyToken::Word(KeyWord::ControlRight),
        "AltLeft" => KeyToken::Word(KeyWord::AltLeft),
        "AltRight" => KeyToken::Word(KeyWord::AltRight),
        "SuperLeft" => KeyToken::Word(KeyWord::SuperLeft),
        "SuperRight" => KeyToken::Word(KeyWord::SuperRight),
        "IntlBackslash" => KeyToken::Word(KeyWord::IntlBackslash),
        // Letters, digits and function keys print as their own name.
        _ => KeyToken::Symbol(key.as_str()),
    }
}

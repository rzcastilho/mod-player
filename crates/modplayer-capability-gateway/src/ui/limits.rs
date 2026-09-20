// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every spec-fixed number the `ui.*` surfaces enforce (data-model.md
//! §1.2, spec Assumptions). Each constant lives exactly once, here —
//! validators (`crate::ui`) and the runtime/core layers above them read
//! it from this module rather than repeating the literal.

use std::time::Duration;

/// FR-003: the most panels one plugin may have registered at once.
pub const MAX_PANELS_PER_PLUGIN: usize = 16;
/// FR-003: the most widgets one panel may declare.
pub const MAX_WIDGETS_PER_PANEL: usize = 100;
/// FR-002: the most items a `list` widget (or `list`/`choice` value) may
/// carry.
pub const MAX_LIST_ITEMS: usize = 256;
/// FR-002: the longest a widget/field/action label may be, in `char`s.
pub const MAX_LABEL_CHARS: usize = 256;
/// FR-002 / FR-007: the longest a `text` widget's content (or
/// `update_widget` string value) may be, in `char`s.
pub const MAX_TEXT_CHARS: usize = 1_024;
/// FR-003: the longest a panel title may be, in `char`s.
pub const MAX_PANEL_TITLE_CHARS: usize = 64;
/// FR-015: the most overlay primitives one plugin may have registered at
/// once (post-merge count).
pub const MAX_OVERLAY_PRIMITIVES: usize = 500;
/// FR-014: the longest an overlay `label` primitive's text may be, in
/// `char`s.
pub const MAX_OVERLAY_LABEL_CHARS: usize = 64;
/// FR-010: the most shortcut actions one plugin may register.
pub const MAX_ACTIONS_PER_PLUGIN: usize = 64;
/// FR-017: the most fields one plugin's settings schema may declare.
pub const MAX_SETTINGS_FIELDS: usize = 100;
/// FR-017: the most options a `choice` settings field may declare.
pub const MAX_CHOICE_OPTIONS: usize = 64;
/// FR-017: the longest a `string` settings field's value may be, in
/// `char`s.
pub const MAX_STRING_FIELD_CHARS: usize = 1_024;
/// FR-019: the longest a notification's text may be, in `char`s.
pub const MAX_NOTIFY_TEXT_CHARS: usize = 200;
/// FR-020 (PL-8.1): the most `notify` calls admitted per rolling window.
pub const NOTIFY_LIMIT: usize = 6;
/// FR-020: the `notify` rolling window's length.
pub const NOTIFY_WINDOW: Duration = Duration::from_secs(60);
/// FR-020a: the most calls admitted per rolling window in the `ui`
/// category (every `ui.*` request except `notify`).
pub const UI_LIMIT: usize = 100;
/// FR-020a: the `ui` category's rolling window length.
pub const UI_WINDOW: Duration = Duration::from_secs(1);
/// FR-014a: the most glyphs a manifest's `[glyphs]` table may declare.
pub const MAX_GLYPHS: usize = 32;
/// FR-014a: an icon's largest accepted square dimension, in pixels.
pub const ICON_MAX_PX: u32 = 128;
/// FR-014a: an icon file's largest accepted size, in bytes.
pub const ICON_MAX_BYTES: usize = 256 * 1024;
/// FR-014a: a glyph's largest accepted square dimension, in pixels.
pub const GLYPH_MAX_PX: u32 = 32;
/// FR-014a: a glyph file's largest accepted size, in bytes.
pub const GLYPH_MAX_BYTES: usize = 64 * 1024;

/// FR-002a: every `UiId` (panel/widget/overlay/action/settings-field id)
/// must match `[a-z][a-z0-9_]{0,63}` — checked by [`crate::ui::UiId::
/// parse`] rather than a regex crate (Constitution X: no new crate).
pub const ID_GRAMMAR: &str = "[a-z][a-z0-9_]{0,63}";
/// FR-014a: a manifest `[glyphs]` key must match `[a-z][a-z0-9_]{0,31}`.
pub const GLYPH_KEY_GRAMMAR: &str = "[a-z][a-z0-9_]{0,31}";

/// The longest byte length [`ID_GRAMMAR`] allows (1 leading + 63
/// trailing).
pub const MAX_ID_CHARS: usize = 64;
/// The longest byte length [`GLYPH_KEY_GRAMMAR`] allows (1 leading + 31
/// trailing).
pub const MAX_GLYPH_KEY_CHARS: usize = 32;

/// Whether `s` matches `[a-z][a-z0-9_]{0,63}` ([`ID_GRAMMAR`]).
#[must_use]
pub fn matches_id_grammar(s: &str) -> bool {
    matches_grammar(s, MAX_ID_CHARS)
}

/// Whether `s` matches `[a-z][a-z0-9_]{0,31}` ([`GLYPH_KEY_GRAMMAR`]).
#[must_use]
pub fn matches_glyph_key_grammar(s: &str) -> bool {
    matches_grammar(s, MAX_GLYPH_KEY_CHARS)
}

fn matches_grammar(s: &str, max_len: usize) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > max_len {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn id_grammar_accepts_and_rejects() {
        assert!(matches_id_grammar("tempo"));
        assert!(matches_id_grammar("a"));
        assert!(matches_id_grammar("snap_to_beat_2"));
        assert!(!matches_id_grammar(""));
        assert!(!matches_id_grammar("Tempo"));
        assert!(!matches_id_grammar("1tempo"));
        assert!(!matches_id_grammar("tempo-x"));
        assert!(!matches_id_grammar(&"a".repeat(65)));
    }

    #[test]
    fn glyph_key_grammar_caps_at_32() {
        assert!(matches_glyph_key_grammar(&"a".repeat(32)));
        assert!(!matches_glyph_key_grammar(&"a".repeat(33)));
    }
}

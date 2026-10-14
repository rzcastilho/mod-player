# Contract: Key Labels (K1–K10)

Scope: `modplayer-core::actions::chord::{Chord::display_parts, Chord::display, DisplayParts, KeyToken, KeyWord}` and `modplayer-ui::key_label::KeyLabel`. Covers FR-001, FR-002, FR-012, SC-001. Table: [data-model.md §1.2](../data-model.md).

| # | Rule | Test |
|---|---|---|
| K1 | `display_parts(p)` is defined for every `KEY_NAMES` entry × 8 `Mods` × 2 `Platform`. `display(p)` is never empty | core `actions.rs` proptest `display_parts_total` |
| K2 | `Space` displays as `Space` on both platforms. No label contains `␣`, `⏎`, `⎋` or `⇥` | core unit test `space_and_specials_are_words`; existing `⇧␣` assertion updated to `⇧Space` |
| K3 | Mac modifier order is `⌘⇧⌥` with no separator; arrows are `← → ↑ ↓`; Enter → `Return`; Escape → `Esc` | core `chord_display_mac_and_other` (updated) |
| K4 | Other platforms: `Ctrl+Shift+Alt+<key>`; arrows are `Left/Right/Up/Down`; Enter → `Enter`; Escape → `Esc` | same |
| K5 | Punctuation displays as its printed character on both platforms (`OpenBracket` → `[` … `Questionmark` → `?`); `PageUp` → `Page Up`; letters are upper-case; digits and F-keys are verbatim | core table test, one assertion per row of §1.2 |
| K6 | Every `Symbol` token is ASCII printable or one of `⌘ ⇧ ⌥ ← → ↑ ↓` | core proptest `symbols_in_covered_set` |
| K7 | **Glyph coverage**: for all 2 176 chords, `KeyLabel::new(c, p).visible` is fully covered by `FontId::proportional(body)` after `theme::apply_tokens` (`has_glyphs`), contains no U+FFFD or U+25A1, and is non-empty | ui `tests/key_labels.rs::every_key_label_is_covered_by_app_fonts` |
| K8 | Under en-US, `KeyLabel::visible == Chord::display(p)` for all 2 176 chords | ui `tests/key_labels.rs::visible_label_matches_core_display` |
| K9 | `spoken` contains no symbol characters. Mac: `Command+Shift+L`, `Space`, `Command+Left Arrow`, `Left Bracket`. Other: `Ctrl+Shift+L` | ui unit tests in `key_label.rs` |
| K10 | Encoding, parsing, persistence, conflict detection and capture are unchanged: the existing `encode`/`parse` round-trip proptest and the `keymap`/registry tests pass without edits, except for display-string assertions | existing core/ui suites |

Font install (research R3):

| # | Rule | Test |
|---|---|---|
| K11 | `theme::fonts::ensure_installed` appends `ModPlayerKeys` then `Hack` to the Proportional and Monospace families and leaves the existing entries and their order unchanged | ui unit test on the built `FontDefinitions` |
| K12 | Every glyph that resolved before the font install resolves to the same font afterwards: existing coverage tests (`row_action_glyphs_covered_by_fonts`, `palette_current_glyph_covered_by_mono_font`, the panel-card disclosure test) still pass | existing tests |
| K13 | The font asset ships with `OFL.txt` and `SOURCE.md` next to it | review + `quickstart.md` check |

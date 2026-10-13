# Tasks: Keyboard Shortcut Reference

**Input**: Design documents from `/specs/029-keyboard-shortcut-reference/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R14), data-model.md, contracts/ (K1–K13, C1–C17, S1–S18, F1–F5), quickstart.md
**Tests**: Included. Constitution VIII and the contracts map every rule to a named test.
**Organization**: By user story. US1 = legible labels + controls page (P1), US2 = key filter (P2), US3 = shortcut sheet + printable (P2).

## Format: `- [ ] [ID] [P?] [Story?] Description with file path`

Paths: core = `crates/modplayer-core/`, ui = `crates/modplayer-ui/`, strings = `locales/`.

---

## Phase 1: Setup

- [X] T001 Record the SC-005 baseline FIRST, on untouched code: add a headless 1280×800 harness to `crates/modplayer-ui/tests/controls.rs` that measures the controls page content height (full host catalog, default bindings, empty filter) and pin it as `const PRE_029_CONTROLS_HEIGHT` (research R12). Commit before any other change. [Req: FR-9.2.2]
- [X] T002 [P] Create `crates/modplayer-ui/assets/fonts/` with `OFL.txt` (Noto Sans Symbols 2 licence) and `SOURCE.md` (upstream URL, version, subset command `pyftsubset NotoSansSymbols2-Regular.ttf --unicodes=U+2318,U+21E7,U+2325`; note the full-font fallback if `pyftsubset` is missing) (K13). [Req: NFR-9.2]
- [X] T003 Generate `crates/modplayer-ui/assets/fonts/ModPlayerKeys-Regular.ttf` (3-glyph subset ⌘ ⇧ ⌥) per `SOURCE.md`; verify the glyphs are present. [Req: NFR-9.2]
- [X] T004 [P] Add empty module declarations `pub mod key_label; pub mod shortcut_groups; pub mod shortcut_sheet;` in `crates/modplayer-ui/src/lib.rs`, with stub files `key_label.rs`, `shortcut_groups.rs`, `shortcut_sheet.rs` (module doc comment only) so the crate compiles. [Req: FR-7.4.1]

---

## Phase 2: Foundational (blocks all stories)

**Core label table, strings, fonts, `KeyLabel`, shared grouping.**

- [X] T005 Implement `KeyWord`, `KeyToken`, `DisplayParts` and `Chord::display_parts` in `crates/modplayer-core/src/actions/chord.rs` per data-model §1.2 (Space/Return/Enter/Esc words, mac `⌘⇧⌥` + arrows, punctuation as printed char, `Page Up`, Tab as word); add `KeyWord::fluent_key()` / `spoken_key()`. Doc comments with runnable examples. [Req: NFR-9.2]
- [X] T006 Rewrite `Chord::display` over `display_parts` and remove `mac_key_symbol` in `crates/modplayer-core/src/actions/chord.rs`. Encoding, parsing and capture stay untouched (FR-012). [Req: NFR-9.2, FR-9.2.2]
- [X] T007 [P] Re-export `DisplayParts`, `KeyToken`, `KeyWord` in `crates/modplayer-core/src/actions/mod.rs` and `crates/modplayer-core/src/lib.rs`. [Req: NFR-9.2]
- [X] T008 Update `crates/modplayer-core/tests/actions.rs`: change `⇧␣` assertion to `⇧Space`, update `chord_display_mac_and_other` (K3/K4), add `space_and_specials_are_words` (K2), a per-row table test for §1.2 (K5), proptests `display_parts_total` (K1) and `symbols_in_covered_set` (K6). Confirm the encode/parse round-trip proptest is unedited (K10). [Req: NFR-9.2, FR-9.2.2]
- [X] T009 [P] Create `locales/en-US/shortcuts.ftl` with every key in contracts/fluent-strings.md "New keys" (nav, sheet, printable, `controls-row`, `key-word-*`, `key-spoken-*`). [Req: NFR-7.1]
- [X] T010 [P] Create `locales/pt-BR/shortcuts.ftl` with the pt-BR values for the same keys. [Req: NFR-7.1]
- [X] T011 [P] Edit `locales/en-US/app.ftl`: trim shortcut prose from `getting-started-section-loop` and `getting-started-key-tempo`, add `getting-started-shortcuts`. [Req: FR-1.4.1, NFR-7.1]
- [X] T012 [P] Create `locales/pt-BR/app.ftl` holding only the getting-started keys (title, section-loop, key-tempo, tutorial, dismiss, shortcuts). [Req: FR-1.4.1, NFR-7.1]
- [X] T013 Extend `crates/modplayer-ui/tests/fluent_keys.rs` with F1–F4: `shortcuts_029_keys_resolve`, `shortcuts_029_keys_have_en_us_and_pt_br_parity`, `every_key_word_has_a_string`, `getting_started_has_no_shortcut_prose`; extend the literal scan to the new modules (F5). [Req: NFR-7.1, FR-1.4.1]
- [X] T014 Implement `crates/modplayer-ui/src/theme/fonts.rs`: `ensure_installed` appends `ModPlayerKeys` (via `include_bytes!`) then `Hack` to Proportional and Monospace families, once per `Context`, existing entries and order untouched (K11). Doc example. [Req: NFR-9.2]
- [X] T015 Hook `fonts::ensure_installed` into `apply_tokens_for` in `crates/modplayer-ui/src/theme/mod.rs`; add `mod fonts`. [Req: NFR-9.2]
- [X] T016 [P] Unit test for K11 (family order unchanged, new fonts appended) in `crates/modplayer-ui/src/theme/fonts.rs`; confirm K12 by running the existing glyph-coverage tests (`row_action_glyphs_covered_by_fonts`, `palette_current_glyph_covered_by_mono_font`, panel-card disclosure test). [Req: NFR-9.2]
- [X] T017 Implement `KeyLabel { visible, spoken, aliases }` and `matches_key` in `crates/modplayer-ui/src/key_label.rs`, localising `KeyWord`s via `tr`, building the spelled-out form and the alias set (canonical name, visible key label, whole visible chord, lower-cased). Doc example; unit tests for K9 (`Command+Shift+L`, `Space`, `Command+Left Arrow`, `Left Bracket`; `Ctrl+Shift+L`). [Req: NFR-9.2, NFR-6.2]
- [X] T018 Create `crates/modplayer-ui/tests/key_labels.rs` with `every_key_label_is_covered_by_app_fonts` (K7: all 136 `KEY_NAMES` × 8 `Mods` × 2 `Platform`, `has_glyphs`, no U+FFFD/U+25A1, non-empty) and `visible_label_matches_core_display` (K8). [Req: NFR-9.2]
- [X] T019 Move `RowSnapshot` and the grouping out of `crates/modplayer-ui/src/settings/controls.rs` into `crates/modplayer-ui/src/shortcut_groups.rs`: `ShortcutRow`, `ShortcutGroup`, `snapshot` (catalog order, then plugin groups, C2), `TWO_COLUMN_MIN_WIDTH = 720.0`, `column_break` (weight `1 + rows`, most balanced group boundary), and the shared row-content painter (name, "(inactive)", chips via `KeyLabel`, conflict `⚠` note, "Not bound"). Doc examples. `controls.rs` calls the new module; behaviour unchanged at this step. [Req: FR-7.4.1]
- [X] T020 [P] Unit tests plus proptest `column_break_is_optimal_boundary` in `crates/modplayer-ui/src/shortcut_groups.rs` (C4). [Req: FR-7.4.1]

**Checkpoint**: `cargo test -p modplayer-core -p modplayer-ui` green; labels are legible; no visible UI change yet beyond fonts/labels.

---

## Phase 3: User Story 1 — Legible bindings on the controls page (P1) 🎯 MVP

**Goal**: Readable chips, catalog-ordered groups, two columns ≥ 720 px, compact rows with hover/focus-revealed controls.
**Independent test**: At ≥ 720 px Play/pause reads "Space", all chips pass glyph coverage, groups sit in two columns, `×` / Add / Reset take zero width until hover, focus or capture.

### Tests (US1)

- [X] T021 [P] [US1] Layout tests in `crates/modplayer-ui/tests/controls.rs`: `layout_order_filter_and_reset_all_above_columns` (C1), `groups_follow_catalog_then_plugin_order` (C2), `two_columns_at_or_above_720`, `one_column_below_720` (C3, headless 1280 and 700 px), `many_bindings_wrap_without_elision` (C5, five bindings at 700 px). [Req: FR-7.4.1]
- [X] T022 [P] [US1] Row tests in `crates/modplayer-ui/tests/controls.rs`: `row_at_rest_shows_name_chips_conflict_only`, `unbound_row_shows_not_bound` (C6), `trigger_kind_label_removed` (C7), `chip_text_is_key_label` (C8), `controls_hidden_at_rest_take_zero_width` (C9). [Req: FR-7.4.1, FR-9.2.2, NFR-6.2]
- [X] T023 [P] [US1] Reveal tests in `crates/modplayer-ui/tests/controls.rs`: `hover_reveals_controls` (C10), `row_element_is_focusable_and_named` (C11), `keyboard_reveal_tab_walk` (C12), `capture_keeps_controls_visible_when_pointer_leaves` (C13). [Req: NFR-6.1, NFR-6.2]
- [X] T024 [P] [US1] `controls_height_at_most_60_percent_of_baseline` (C17) in `crates/modplayer-ui/tests/controls.rs`. [Req: FR-9.2.2]
- [X] T025 [P] [US1] Add a 40 % pseudo-expansion variant (`i18n::with_pseudo_expansion(40, …)`) of the C5 and C9 tests in `crates/modplayer-ui/tests/controls.rs`. [Req: NFR-7.1]

### Implementation (US1)

- [X] T026 [US1] In `crates/modplayer-ui/src/settings/controls.rs`, lay out groups with `ui.columns(2, …)` when `ui.available_width() >= TWO_COLUMN_MIN_WIDTH`, splitting at `column_break`, else a single column. The filter box and "Reset all to defaults" stay full width above the columns (C1–C4). [Req: FR-7.4.1]
- [X] T027 [US1] In `crates/modplayer-ui/src/settings/controls.rs`, paint chips from `KeyLabel::visible` with accessible name `controls-binding-chip { $binding = spoken }`; `×` uses `controls-remove-binding` with the spoken form; chips wrap in the row with no elision (C5, C8). [Req: FR-9.2.2, NFR-6.2]
- [X] T028 [US1] In `crates/modplayer-ui/src/settings/controls.rs`, drop the trigger kind label; draw a kind label only for `ActionKind::Continuous` (C7). [Req: FR-7.4.1]
- [X] T029 [US1] In `crates/modplayer-ui/src/settings/controls.rs`, add a focusable row element (`Sense::focusable_noninteractive`, accessible name `controls-row { $action } { $bindings }`) and the reveal logic (R4). It uses the previous-frame row rect plus derived focus membership, and keeps the controls revealed during capture or a rejection. Unrevealed controls are not added at all. Request a repaint when the reveal state changes (C9–C13). [Req: NFR-6.1, NFR-6.2]
- [X] T030 [US1] Verify C17: run T024; if above 60 %, tighten row spacing through theme tokens (never relax the target). [Req: FR-9.2.2]
- [X] T031 [P] [US1] Extend `crates/modplayer-ui/tests/accessibility.rs` for the new chip and row names; confirm the sweep passes. [Req: NFR-6.2]

**Checkpoint**: US1 independently demoable (MVP). Controls page legible, two-column, compact.

---

## Phase 4: User Story 2 — Filter by key name (P2)

**Goal**: Filter finds actions by key alias and outlines the matching chip.
**Independent test**: "L", "space", "←" (mac), "ctrl+shift+l" list the bound actions with the chip outlined; "e" matches no Enter/Escape bindings via keys.

- [X] T032 [P] [US2] Tests in `crates/modplayer-ui/tests/controls.rs`: `filter_by_key_alias_lists_bound_actions` ("L", "l", "space", "Space", "left", mac "←", "ctrl+shift+l"), `filter_key_match_is_exact` ("e"), existing name/group substring filter tests untouched (C14, C15), `filter_no_match_message`, `key_match_chip_outlined` (C16). [Req: FR-7.4.1]
- [X] T033 [P] [US2] Add `key_match_outline_visible_in_hc` to `crates/modplayer-ui/tests/high_contrast.rs` (C16). [Req: FR-7.4.1, NFR-6.2]
- [X] T034 [US2] Extend the filter in `crates/modplayer-ui/src/shortcut_groups.rs`: the trimmed, lower-cased query matches the group heading or action label (substring), OR a binding whose `KeyLabel` alias equals it exactly. Return per-chip hit flags (C14, C15). [Req: FR-7.4.1]
- [X] T035 [US2] In `crates/modplayer-ui/src/settings/controls.rs`, outline key-matched chips with the theme accent/focus stroke (outline, not fill) and show `controls-no-match` when nothing matches (C16). [Req: FR-7.4.1]

**Checkpoint**: US1 + US2 work independently.

---

## Phase 5: User Story 3 — Shortcut sheet and printable version (P2)

**Goal**: Read-only live sheet from the nav rail and getting-started link; printable HTML export.
**Independent test**: Open from the nav rail with the card dismissed; change a binding and see it update next frame; export lists every group.

### Tests (US3)

- [X] T036 [P] [US3] Nav tests in `crates/modplayer-ui/tests/shell_navigation.rs`: `keyboard_shortcuts_item_after_settings_never_selected` (S1), `shortcuts_item_opens_sheet_without_changing_section` (S2), `shortcuts_item_present_after_card_dismissed` (S3). [Req: FR-7.4.1, NFR-6.1]
- [X] T037 [P] [US3] Getting-started link tests (Role::Link, `OpenShortcuts` outcome, no shortcut prose in the rendered card, SC-006) in `crates/modplayer-ui/tests/first_launch.rs` and `crates/modplayer-ui/tests/library_view.rs` (S4). [Req: FR-1.4.1]
- [X] T038 [P] [US3] Create `crates/modplayer-ui/tests/shortcut_sheet.rs`: `sheet_single_instance_non_modal` (S6), `sheet_matches_controls_groups_and_rows` (S7), `sheet_has_no_editing_controls` (S8), `sheet_two_columns_when_wide` (S9), `sheet_reflects_binding_change_next_frame` (S10), `sheet_esc_closes_when_focused_only` (S11), `sheet_starts_closed` (S12). [Req: FR-7.4.1, NFR-6.1]
- [X] T039 [P] [US3] Printable tests in `crates/modplayer-ui/tests/shortcut_sheet.rs`: `printable_writes_file_and_returns_url` (S14, temp dir via env), `printable_html_escapes_plugin_labels` (S16), `printable_write_failure_shows_inline_error` (S18), plus a structure check (inline `<style>` with `@media print`, no `<script>`, no external `src`/`href`, `<html lang>`, one `<h2>` + `<table>` per group) (S15). [Req: FR-7.4.1]
- [X] T040 [P] [US3] Add a `credential_leak.rs`-style assertion that the printable builder takes no session or settings (S17) in `crates/modplayer-ui/tests/credential_leak.rs`. [Req: FR-7.4.1]

### Implementation (US3)

- [X] T041 [P] [US3] Implement `file_url(&Path)` in `crates/modplayer-ui/src/shortcut_sheet.rs` with unit tests for the POSIX, Windows (`C:\Users\A B\…` → `file:///C:/Users/A%20B/…`) and non-ASCII percent-encoding cases (R9). [Req: FR-7.4.1, NFR-9.2]
- [X] T042 [P] [US3] Implement `html_escape` and `printable_html(groups, locale, date)` in `crates/modplayer-ui/src/shortcut_sheet.rs`: self-contained, inline print CSS, black-on-white, `shortcuts-printable-*` strings, built only from `ShortcutGroup`s and the date. Doc example (S15–S17). [Req: FR-7.4.1, NFR-7.1]
- [X] T043 [US3] Implement `ShortcutSheetState { open, export_error }`, `SheetOutcome`, `show()` and `write_printable` in `crates/modplayer-ui/src/shortcut_sheet.rs`. `show()` is a non-modal `egui::Window` (id `shortcut-sheet`, title `shortcuts-sheet-title`) that rebuilds from the live registry every frame, uses the shared row painter without editing controls, applies the two-column rule to its own width, and has a close button and Esc handling only when focus is inside the sheet's layer (R7). It has an "Open printable version" button and an inline `shortcuts-printable-error` (S6–S14, S18). [Req: FR-7.4.1, NFR-6.1]
- [X] T044 [P] [US3] In `crates/modplayer-ui/src/shell.rs`, add `Shell::shortcut_sheet_requested` and, in `nav_rail`, a `theme::divider` plus a never-selected `nav_item` named `nav-keyboard-shortcuts` after Settings (S1–S3). [Req: FR-7.4.1, NFR-6.1]
- [X] T045 [P] [US3] In `crates/modplayer-ui/src/getting_started.rs`, add `GettingStartedOutcome::OpenShortcuts` and a `Role::Link` "Keyboard shortcuts" control (`getting-started-shortcuts`); dismiss and tutorial behave as before (S4). [Req: FR-1.4.1, NFR-6.2]
- [X] T046 [US3] Wire `crates/modplayer-ui/src/app.rs`: hold `ShortcutSheetState`; consume the shell flag and `OpenShortcuts` to open the sheet in the same frame; draw the sheet each frame; on `SheetOutcome::OpenPrintable(url)` call `ctx.open_url(OpenUrl::new_tab(url))` (the only place that does); the section is unchanged (S2, S4, S14). [Req: FR-1.4.1, FR-7.4.1]
- [X] T047 [P] [US3] Extend `crates/modplayer-ui/tests/accessibility.rs` and `crates/modplayer-ui/tests/high_contrast.rs` for the sheet, nav item and link (S13). [Req: NFR-6.2]

**Checkpoint**: all three stories work independently.

---

## Phase 6: Polish & cross-cutting

- [X] T048 [P] Extend `crates/modplayer-ui/tests/design_token_literals.rs` to scan `key_label.rs`, `shortcut_groups.rs`, `shortcut_sheet.rs` and `theme/fonts.rs` for colour literals. [Req: NFR-6.2]
- [X] T049 [P] Run the 40 % pseudo-expansion check for the sheet and nav item (no overlap or truncation); fix as needed in `crates/modplayer-ui/src/shortcut_sheet.rs`. [Req: NFR-7.1]
- [X] T050 Run the gates from `quickstart.md`: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo doc --no-deps`, `cargo deny check`. Fix failures. [Req: NFR-6.1, NFR-7.1, NFR-9.2]
  - 2026-10-10: all green — fmt ok; clippy (`--all-features -D warnings`) 0 diagnostics; `cargo test --workspace --no-fail-fast` 2 463 passed / 0 failed (2 466 after the T051 fixes and their regression tests); doctests 122 passed; `cargo doc --no-deps` exits 0 (remaining warnings all pre-date 029); `cargo deny check` advisories/bans/licenses/sources ok. Fixed three stale/host-dependent tests: `controls.rs::chips_display_platform_glyphs` now expects the C8 accessible name (`KeyLabel::spoken`, not `Chord::display`); `shortcut_sheet.rs::fresh_ctx` pins `OperatingSystem::Nix` (egui defaults `os()` to the build target, so labels differed on macOS); `fluent_keys.rs` literal scan now allows file names, BCP 47 tags, HTML entities and skips `'"'` char literals. `modplayer-account --test refresh::expiry_without_refresh…` flaked once under full-suite load (untouched by 029; 5/5 isolated reruns and the final full run pass).
- [X] T051 Execute the manual scenarios M1–M13 from `quickstart.md` on macOS (Quartz recipe) and record sign-off per the constitution. [Req: NFR-6.1, NFR-9.2]
  - 2026-10-10, macOS host, real Keychain session, debug build. Driven with Quartz `CGEventPost` and `screencapture`; evidence is in `target/manual-walk/` (gitignored). Launched with `open -n` because the agent shell is a non-Aqua launchd session. The second pass ran on the rebuilt app with a fresh `MODPLAYER_CONFIG_DIR`.
  - M1 **pass** (`m1-controls.png`). Two columns; group order Transport → Markers → Loop → Cues → Navigation → Effects → Key & Tempo → Section Loop; Space chip; ⌘ ⇧ ← → ↑ ↓ render; no trigger labels; no edit controls at rest. No default binding uses ⌥, so that glyph rests on `key_labels.rs`.
  - M2 **pass** (`m2-hover/leave.png`). Controls reveal and hide with no sideways shift. The revealed buttons are taller than chips, so rows below move down about 6 pt; the spec doesn't forbid this.
  - M3 **pass** (`m3-*.png`). Focus ring on the row element; Tab order is filter → Reset all → row → `×` → Add binding → Reset → next row; the previous row hides.
  - M4 **pass** (`m4-capture.png`, `m4-esc.png`). Capture stays visible with the pointer off the row; Esc cancels it.
  - M5 **deviation, fixed** (`m5-*.png`, `n7-960.png`). The one-column case can't be reached live: minimum window is 960 × 640, and the dock only appears on Now Playing (R15); quickstart M5 has been updated. Found right-column conflict notes running under the floating scroll bar; fixed with a gutter-narrowed child UI plus `groups_keep_clear_of_scroll_bar_gutter`.
  - M6 **pass after fix** (`m6-*.png`, `n8-space.png`). `L`, `space` and `←` list the bound actions with chips outlined; `e` outlines only the E chip (no Enter/Escape match). Found a lone matching group drawn in the right column; fixed by the `column_break` tie rule plus `lone_filtered_group_sits_in_left_column`, and re-verified live.
  - M7 **pass** (`m7-sheet.png`). The sheet opens over Library and Library stays selected; read-only; groups match; "Not bound" shown.
  - M8 **pass** (`m8-*.png`). Binding Play to K shows in the sheet on the next frame, and two-step Reset all clears it, without reopening.
  - M9 **pass after fix** (`n3-link.png`, `n4-esc.png`, `n5-*.png`). The card has no "Shortcuts:" clause and its link opens the sheet. Found that Esc could never close the sheet for a mouse user; after the fix, clicking the sheet then Esc closes it, while Esc with no click or after an outside click leaves it open. The × button closes it. Covered by `sheet_esc_closes_after_clicking_its_body`.
  - M10 **pass** (`n6-m10.png`). After dismissing the card, the nav item still opens the sheet.
  - M11 **pass** (`m11-browser.png`). Chrome opened `file:///…/T/modplayer-shortcuts.html`: 8 groups, ⌘-style `<kbd>` labels, black on white, `section{break-inside:avoid}` for print. The source has no account or device data (the only `@` is CSS `@media`). Fixed per-table column misalignment with `table-layout:fixed`. Print preview not opened, to avoid driving a dialog in the maintainer's browser.
  - M12 **pass** (`m12-error.png`). With the file at `chmod 444`, the sheet shows the inline `shortcuts-printable-error` and the app keeps running; `chmod 644` restored.
  - M13 **pass** (`m13-*.png`). In high contrast, chips, key-match outlines, focus rings and the sheet are legible. High contrast was turned back off afterwards.
  - Not re-verified live: the gutter fix (rebuilding needs another Keychain approval); the headless test fails without the fix and passes with it.
- [X] T052 Confirm FR-012: `git diff` shows no change to chord encode/parse/persist/conflict/capture code, only display and presentation. [Req: FR-9.2.2]

---

## Dependencies & order

- Phase 1 → Phase 2 → stories. **T001 must precede any code change.** T003 follows T002. T015 follows T014. T019 follows T017.
- Phase 2 internal: T005 → T006 → T008; T017 needs T005 and T015; T018 needs T017; T019 needs T017.
- **US1** (T021–T031) needs Phase 2. **US2** (T032–T035) needs US1 (same page and `shortcut_groups`). **US3** (T036–T047) needs Phase 2 only (shared row painter); it is independent of US1/US2 UI work and can run in parallel with them.
- T043 needs T041, T042 and T019. T046 needs T043, T044, T045.
- Polish needs the desired stories done.

## Parallel examples

- Phase 2 strings: T009, T010, T011, T012 together, while T005–T008 proceed in core.
- US1 tests T021–T025 together, all in `tests/controls.rs`. Write them as separate functions, or serialise the edits if merge-conflict-prone.
- US3: T041 and T042 together, then T044 and T045 together. US3 can run alongside US1/US2 with a second implementer.

## Implementation strategy

1. **MVP = Phase 1 + 2 + US1.** It fixes UX-32 tofu, compacts the page and gives the two-column layout. Stop and validate: run the glyph test and view the page at 1280 and 700 px.
2. Add US2 (small, builds on the `KeyLabel` aliases).
3. Add US3, then Polish, gates and manual sign-off.
4. Commit after each phase; keep the T001 baseline commit separate.

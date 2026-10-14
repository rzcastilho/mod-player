# Implementation Plan: Keyboard Shortcut Reference

**Branch**: `029-keyboard-shortcut-reference` (git: `feature/029-keyboard-shortcut-reference`) | **Date**: 2026-10-09 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/029-keyboard-shortcut-reference/spec.md`

## Summary

Make every key binding legible and easy to find (UI/UX review UX-10, UX-32).

**Labels.** Today the Settings › Controls chips draw tofu boxes: egui's default Proportional fonts lack `⇧ ⌥ ← → ↑ ↓ ␣ ⏎ ⎋` (measured, research R2). The fix has three parts:

- Core gets a structured `Chord::display_parts`, the single source of the clarified label table: Space/Return/Esc become words, macOS keeps `⌘⇧⌥` and arrows, and punctuation shows as its printed character.
- A new UI `key_label` module localises the words, builds a spelled-out accessible form ("Command+Shift+L"), and builds the filter aliases.
- A new `theme::fonts` appends a 3-glyph Noto Sans Symbols 2 subset (OFL) and egui's already-embedded Hack to the fallback chains. This is strictly additive: no glyph that resolves today changes font.

A 2 176-chord test proves that every label is covered by the fonts.

**Controls page.** The row grouping moves into a shared `shortcut_groups` module (catalog categories, then plugin groups). Groups lay out in two columns at ≥ 720 px, split at the most balanced group boundary. A row shows only its name, chips, conflict note and "Not bound". The `×`, Add binding and Reset controls are not added until the row is hovered, keyboard-focused (through a new focusable row element), or capturing. The filter also matches exact key aliases and outlines the matched chips.

**Shortcut sheet.** A new read-only, non-modal `egui::Window` rebuilds from the live registry every frame. It opens from a new nav-rail "Keyboard shortcuts" item (an action, not a section) and from a link on the getting-started card. The card loses its "Shortcuts: …" prose. The sheet's "Open printable version" writes an escaped, self-contained HTML file to the temp directory, and `App` opens it through `open_url` (`file://` is supported because the `webbrowser` crate is not hardened).

Chord encoding, parsing, persistence, conflicts and capture are unchanged (FR-012).

## Technical Context

**Language/Version**: Rust 1.95.0 (`rust-toolchain.toml`, workspace `rust-version = "1.95"`)

**Primary Dependencies**: egui/eframe 0.36 (existing: `Window`, `ui.columns`, `Sense::focusable_noninteractive`, `FontDefinitions`, AccessKit, `ctx.open_url`), fluent-templates through `modplayer_core::{tr, tr_args}`, `chrono` 0.4 (existing direct dependency since 028, used for the printable generation date). **No new crate dependency.** New binary asset: Noto Sans Symbols 2 subset (SIL OFL 1.1), embedded with `include_bytes!`.

**Storage**: No persisted change. The sheet's open state and export error live in `App` for the session only. The printable file is a transient overwrite at `std::env::temp_dir()/modplayer-shortcuts.html`, holding action/group/binding labels only.

**Testing**: `cargo test`. Headless egui `Context::run_ui` + AccessKit harness. New `tests/key_labels.rs` (glyph coverage over every `KEY_NAMES` × 8 `Mods` × 2 `Platform`) and `tests/shortcut_sheet.rs`. Extended `tests/controls.rs` (layout, reveal, filter, SC-005 height baseline), `shell_navigation.rs`, `fluent_keys.rs` (+ pt-BR parity), `accessibility.rs`, `high_contrast.rs`, `design_token_literals.rs`, getting-started tests. Core `tests/actions.rs` display assertions updated, plus new proptests for label totality and symbol coverage. Unit tests for the pure functions `column_break`, `KeyLabel`, `file_url`, `printable_html` and the escaping helper. Doc tests on new pub items.

**Target Platform**: macOS, Windows and Linux desktop (egui). Labels branch on `ctx.os().is_mac()` → `Platform`, the only platform difference, and it is presentation only. Manual scenarios run on macOS per the constitution recipe.

**Project Type**: Desktop application (Cargo workspace, one crate per component).

**Performance Goals**: No measurable UI cost. Per frame, O(actions ≈ 44 host + plugin) snapshots, which the page already builds, plus at most 4 `tr` lookups per chip. Fonts are installed once per `Context` (flag check afterwards). The sheet adds one more snapshot only while it is open. Nothing touches the audio thread.

**Constraints**: Minimum window 960×640. Two columns only at content width ≥ 720 px. No chip elision. Theme tokens only (no colour literals), legible in high contrast. Every new element is keyboard-operable and has an accessible name. Strings in en-US + pt-BR. Behaviour identical across OSes. Only `App` calls `open_url`. The printable HTML is escaped and contains no credentials.

**Scale/Scope**: 136 key names × 8 modifier sets × 2 platforms = 2 176 labels. 44 host actions in 6 categories plus plugin groups. New files: 4 UI modules (`key_label.rs`, `shortcut_groups.rs`, `shortcut_sheet.rs`, `theme/fonts.rs`), 1 font asset directory, 2 test files, 3 `.ftl` files (en-US `shortcuts.ftl`, pt-BR `shortcuts.ftl`, pt-BR `app.ftl`). Edited: core `chord.rs` / `actions/mod.rs` / `lib.rs`; UI `settings/controls.rs`, `shell.rs`, `getting_started.rs`, `app.rs`, `theme/mod.rs`, `lib.rs`; en-US `app.ftl`; about 8 existing test files.

All Technical Context items are resolved. No open unknowns remain (see [research.md](./research.md) R1–R14).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: **VI (printable file content), VII, VIII, X**, plus Governance › Manual Scenario Sign-Off and traceability. The others are N/A, with the reason given below.

- [x] **I. Real-Time Path Is Sacred**: N/A. No engine or audio-thread code is touched. All new code runs on the UI thread (label rendering, layout, a file write on a button press).
- [x] **II. Plugins Are Guests**: Pass. No change to the plugin runtime or gateway. Plugin-contributed action and group labels (untrusted literals) are only displayed. In the printable HTML they are **HTML-escaped** (S16), so a plugin cannot inject markup into a page the system browser opens.
- [x] **III. Host Primitives, Plugin Behaviors**: N/A. The action catalog, categories and registry are reused as-is. No primitive is added, and plugins gain no new capability.
- [x] **IV. Audio Source Replaceable and Isolated**: N/A. No audio-source crate is touched. Tests use the existing `FakeBackend` and synthetic host.
- [x] **V. No Audio Ever Leaves the Engine**: N/A. The only file written is an HTML list of action names and key labels. No sample buffers, no cache.
- [x] **VI. Security and Privacy by Default**: Pass. The printable file is built only from `ShortcutGroup`s and a date (S17). It holds no account, credential, device or path data, is written to the OS temp dir, and its errors are logged without content. There is no network access and no telemetry.
- [x] **VII. Rust Quality Gates**: Pass. No new crate and no new dependency (a `file_url` helper is used instead of adding `url`, R9). The font is an OFL asset with its licence alongside, and `cargo deny` is unaffected. `forbid(unsafe_code)` is unaffected. No `unwrap`/`expect` in library code: the write returns `Result` and becomes an inline error. New pub items (`Chord::display_parts`, `KeyToken`, `KeyWord`, `KeyLabel`, `column_break`, `printable_html`, `file_url`, `fonts::ensure_installed`) carry doc comments with runnable examples. fmt, clippy `-D warnings`, test, doc and deny are all in the quickstart.
- [x] **VIII. Test What the NFRs Promise**: Pass. Each contract rule (K1–K13, C1–C17, S1–S18, F1–F5) maps to a named test. Regression tests cover UX-32 tofu (K7), the old "Shortcuts:" prose (S5/F4) and the trigger label (C7). Proptests cover label totality, symbol coverage and `column_break` optimality. The existing chord encode/parse proptest is unchanged. No NFR-10.3 suite is affected.
- [x] **IX. One Plugin API Definition**: N/A. The plugin API schema and manifests are untouched. Plugin action registration is unchanged.
- [x] **X. Simplicity, Portability, User's Override**: Pass. No new trait and no feature flag. It reuses `nav_item`, `divider`, `section_heading`, `Window`, `columns`, the token roles and the request-flag pattern. Behaviour and shortcuts are identical on all OSes: only the label glyphs follow platform conventions, which is presentation, as clarified. There is no native menu or platform adapter. Every new control (nav item, row element, link, sheet buttons) is keyboard-operable and has an accessible name. Chips expose spelled-out names. All strings are externalised in en-US + pt-BR (F1–F2). Plugin-disable and transport-focus override are untouched.
- [x] **Governance › Manual Scenario Sign-Off**: Pass. Quickstart M1–M13 are executed by the implementing agent with the macOS Quartz recipe.
- [x] **Governance › area-maintainer sign-off (GOV-3.2)**: N/A. The engine, Capability Gateway and Plugin Runtime crates are not modified. Core changes are confined to `modplayer-core::actions::chord` (presentation).
- [x] **Governance › traceability**: Pass. Contracts cite FR-001–FR-013 and SC-001–SC-006. The sources are UX-10 and UX-32 (UI/UX review §3.1, §3.7) through the spec, and constitution X ↔ NFR-6.1/6.2/7.1, NFR-9.2.

**Post-design re-check (after Phase 1)**: Unchanged. The data model adds presentation-only core types and UI session state. The contracts are UI, string and file-content contracts. The only new artifact outside code is an OFL font subset. **PASS.** There are no violations, so Complexity Tracking lists none (assumptions are recorded below).

## Project Structure

### Documentation (this feature)

```text
specs/029-keyboard-shortcut-reference/
├── plan.md                  # This file
├── research.md              # Phase 0: R1–R14 decisions
├── data-model.md            # Phase 1: DisplayParts/KeyWord table, KeyLabel, ShortcutGroup, reveal + sheet state, printable doc, fonts
├── quickstart.md            # Phase 1: automated gates + manual M1–M13
├── contracts/
│   ├── key-labels.md        # K1–K13: label table, glyph coverage, font install
│   ├── controls-page.md     # C1–C17: groups, columns, row anatomy, reveal, filter, SC-005
│   ├── shortcut-sheet.md    # S1–S18: nav item, card link, sheet window, printable HTML
│   └── fluent-strings.md    # F1–F5: new/changed keys, en-US + pt-BR values
├── checklists/              # from specify/clarify
└── tasks.md                 # Phase 2 (/speckit-tasks; not created here)
```

### Source Code (repository root)

```text
crates/modplayer-core/src/
├── actions/chord.rs             # display_parts(), DisplayParts, KeyToken, KeyWord (+ fluent_key/spoken_key); display() rewritten over parts; mac_key_symbol replaced
├── actions/mod.rs               # re-export DisplayParts, KeyToken, KeyWord
└── lib.rs                       # re-export
crates/modplayer-core/tests/
└── actions.rs                   # update ␣ assertions; table + proptests K1–K6

crates/modplayer-ui/
├── assets/fonts/                # NEW dir: ModPlayerKeys-Regular.ttf (Noto Sans Symbols 2 subset), OFL.txt, SOURCE.md
└── src/
    ├── lib.rs                   # pub mod key_label, shortcut_groups, shortcut_sheet
    ├── key_label.rs             # NEW: KeyLabel {visible, spoken, aliases}, matches_key
    ├── shortcut_groups.rs       # NEW: ShortcutRow/Group, snapshot, filter (+key hits), column_break, TWO_COLUMN_MIN_WIDTH, shared row-content painter (name, chips, conflict, Not bound)
    ├── shortcut_sheet.rs        # NEW: ShortcutSheetState, SheetOutcome, show(), printable_html, html_escape, file_url, write_printable
    ├── settings/controls.rs     # use shortcut_groups; two-column layout; row element + reveal; chips via KeyLabel; key filter + outline; kind label only for Continuous
    ├── shell.rs                 # nav_rail: divider + "Keyboard shortcuts" nav_item; Shell::shortcut_sheet_requested
    ├── getting_started.rs       # GettingStartedOutcome::OpenShortcuts + link-role control
    ├── app.rs                   # ShortcutSheetState; consume shell flag & card outcome; draw sheet; open_url for printable
    └── theme/
        ├── fonts.rs             # NEW: ensure_installed (append ModPlayerKeys + Hack)
        └── mod.rs               # apply_tokens_for → fonts::ensure_installed
crates/modplayer-ui/tests/
├── key_labels.rs                # NEW: K7, K8, K11
├── shortcut_sheet.rs            # NEW: S6–S18
├── controls.rs                  # C1–C17 (+ PRE_029_CONTROLS_HEIGHT baseline)
├── shell_navigation.rs          # S1–S3
├── fluent_keys.rs               # F1–F5, S5
├── first_launch.rs / library_view.rs  # S4 getting-started link
├── accessibility.rs             # new names; sweep passes
├── high_contrast.rs             # key-match outline, sheet in HC
└── design_token_literals.rs     # scan new modules

locales/
├── en-US/shortcuts.ftl          # NEW
├── en-US/app.ftl                # getting-started prose trimmed; getting-started-shortcuts
├── pt-BR/shortcuts.ftl          # NEW
└── pt-BR/app.ftl                # NEW: getting-started keys only
```

**Structure Decision**: The existing Cargo workspace, with no new crate. Core changes are limited to `crates/modplayer-core/src/actions/` (presentation of `Chord`). All UI work goes in the existing `crates/modplayer-ui/src/`: three new top-level modules beside `getting_started.rs` and `shell.rs`, plus `theme/fonts.rs` in the existing `theme/` directory. Tests go in the existing `crates/modplayer-ui/tests/` and `crates/modplayer-core/tests/`, and strings in the existing `locales/en-US/` and `locales/pt-BR/`. The single new directory is `crates/modplayer-ui/assets/fonts/`, which holds the bundled OFL font subset and its licence (no asset directory exists yet; `include_bytes!` reads it from `src/theme/fonts.rs`). Every other directory listed exists today. The engine, gateway, plugin runtime, account and audio crates are not modified.

## Implementation Outline (for /speckit-tasks)

1. **Baseline first**: add the SC-005 height harness to `tests/controls.rs` and record `PRE_029_CONTROLS_HEIGHT` on the untouched code (R12).
2. **Core labels** (US1): `KeyWord`/`KeyToken`/`DisplayParts`, `display_parts`, and `display` rewritten over them. Update the `␣` tests and add the table test and proptests (K1–K6, K10).
3. **Strings**: en-US/pt-BR `shortcuts.ftl`, pt-BR `app.ftl`, trimmed en-US getting-started prose. `fluent_keys.rs` tables and parity tests (F1–F5).
4. **Fonts**: subset the asset (`pyftsubset NotoSansSymbols2-Regular.ttf --unicodes=U+2318,U+21E7,U+2325`), add `theme/fonts.rs`, and hook it into `apply_tokens_for` (K11–K13).
5. **`key_label`**: `KeyLabel` and its aliases. `tests/key_labels.rs` with the 2 176-chord coverage test (K7–K9).
6. **`shortcut_groups`**: move `RowSnapshot` and the grouping out of `controls.rs`; add `column_break` + proptest, the filter with key hits, and the shared row-content painter.
7. **Controls page** (US1/US2): two columns, the row element + reveal (R4), chips through `KeyLabel`, Continuous-only kind label, key filter + outline (C1–C17).
8. **Sheet** (US3): `shortcut_sheet::show`, `App` state, nav-rail item + flag, getting-started link outcome, Esc handling (S1–S13).
9. **Printable**: `printable_html`, `html_escape`, `file_url`, write + inline error, and `App` `open_url` (S14–S18).
10. **Cross-cutting**: accessibility sweep, high contrast, token-literal scan, 40 % pseudo-expansion.
11. **Gates + manual** M1–M13.

## Complexity Tracking

There are no Constitution Check violations to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| None | n/a | n/a |

**Assumptions recorded** (headless run, so no clarification was possible):

- **Label table location**: structured `display_parts` in core, localised in the UI (R1). Rejected: calling `tr` inside core `display` (locale-dependent core tests and logs) and a second table in the UI (two sources of truth).
- **Glyph source**: append a 3-glyph Noto Sans Symbols 2 subset plus egui's existing Hack to the fallback chains (R3). Rejected: inserting fonts earlier in the chain (could change existing glyphs such as `⚠`), a Noto Sans Math subset (lacks `⌘ ⌥`), and textual Cmd/Opt on macOS (contradicts the clarified table). Fallback if `pyftsubset` is unavailable: embed the full font (~670 KB) and note it in `SOURCE.md`.
- **Tab key**: shows the word "Tab" on all platforms. The old macOS `⇥` is dropped. The clarified table names only `⌘ ⇧ ⌥` and arrows as macOS symbols, so `⇥` falls under "every other key uses its textual name".
- **Enter on non-macOS**: shows "Enter". The spec fixes only macOS ("Return"). Windows and Linux keycaps say Enter.
- **Arrows on non-macOS**: textual "Left/Right/Up/Down", consistent with "Windows/Linux keep the existing textual form".
- **Column-balance weight**: `1 + rows` per group (R5). Rejected: measuring rendered heights (non-deterministic in tests).
- **Hover reveal mechanism**: previous-frame row rect + derived focus membership. Controls that aren't revealed are not added at all, so they take zero width (R4). Rejected: invisible-but-laid-out controls (non-zero width).
- **Esc scope**: the sheet consumes Esc only when focus is inside the sheet's layer (R7), so Esc elsewhere keeps its existing meaning (capture cancel, modal close).
- **Printable URL**: a local `file_url` helper with tests, instead of adding `url` as a direct dependency (R9). Only write failures are observable. `open_url` failures are logged by egui-winit and cannot be surfaced.
- **pt-BR getting-started strings**: pt-BR has no `app.ftl` today. A new `pt-BR/app.ftl` holding only the getting-started keys satisfies FR-011/SC-006 parity. `tr` still resolves en-US, as in 024 and 028.
- **SC-005 baseline**: measured on the pre-feature code in the same headless harness and pinned as a constant (R12). If the ≤ 60 % target misses, row spacing is tightened with tokens. The target is not relaxed.

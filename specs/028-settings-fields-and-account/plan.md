# Implementation Plan: Settings Fields, Placeholders, and Account Summary

**Branch**: `028-settings-fields-and-account` (git: `feature/028-settings-fields-and-account`) | **Date**: 2026-09-30 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/028-settings-fields-and-account/spec.md`

## Summary

Turn the Settings categories this feature owns (Audio, Playback, Appearance, Language, Account, plus the Offline / Privacy & diagnostics placeholders) from undifferentiated label-over-control stacks into grouped, legible decisions (UI/UX review §3.7, UX-33..36). A new shared builder, `settings/field.rs::row`, draws every field as label → indented secondary help → control line (control with its unit rendered inside the value box via Fluent formatters + an optional per-field **Reset**) → range caption read from the same clamp constants, and returns the field rect used for search highlight and scroll. Fields sit in `panel_card` groups ("Output", "Level protection", "Connect device", "Markers", "Theme", "Language"). Reset is offered for exactly eight fields when current ≠ `AudioSettings::default()` and writes through each field's existing setter. Choosing a descriptor search result now scrolls to, focuses and outlines the whole field (≥ 2 px accent-role stroke, clears after 3 s or first key/pointer press). Offline and Privacy & diagnostics show an honest "not available yet" card and carry a "Coming soon" text badge in the category row (status from a new `SettingsCategory::is_available()`). Account leads with one summary card (identity strong; tier and last-verified as label–value lines with fallbacks; last-verified as a localised local date/time via `chrono::Local`, already in the lockfile). The sign-out dialog becomes `clamp(viewport − 64, 420, 560)` px wide with wrapping content; Cancel-default focus/Escape/backdrop behaviour is kept and locked by tests. No value domain, default, persistence format or account logic changes (FR-017).

## Technical Context

**Language/Version**: Rust 1.95.0 (`rust-toolchain.toml`, workspace `rust-version = "1.95"`)

**Primary Dependencies**: egui/eframe 0.36 (existing: `Slider`/`DragValue` `custom_formatter`/`custom_parser`, `Modal`, `LayoutJob`, AccessKit), fluent-templates via `modplayer_core::{tr, tr_args}`, `time` 0.3 (existing, `OffsetDateTime`/`UtcOffset`), **`chrono` 0.4 `clock` feature — new direct dependency of `modplayer-ui` only, crate already in `Cargo.lock` (0.4.45 via `oauth2`)** for DST-correct local offset (R8).

**Storage**: N/A — no persistence change. Reset writes existing `settings.toml` keys through existing setters / reload-mutate-save. Highlight and deferred focus are session-only UI state on `SettingsScreen`.

**Testing**: `cargo test` — egui headless `Context::run_ui` + AccessKit bounds harness (new `tests/settings_fields.rs`, `tests/settings_search_highlight.rs`, `tests/settings_account.rs`; extended `settings_category_row.rs`, `fluent_keys.rs` + pt-BR parity, `high_contrast.rs`, `design_token_literals.rs`, `accessibility.rs`); `i18n::with_pseudo_expansion(40, …)` for SC-007; unit tests for pure fns (`format_last_verified`, `signout_dialog_width`, value parser, highlight state machine, `is_available`); doc tests on new pub items.

**Target Platform**: macOS, Windows, Linux desktop (egui); manual scenarios on macOS per constitution recipe.

**Project Type**: Desktop application (Cargo workspace, one crate per component).

**Performance Goals**: No measurable UI cost: per frame O(fields ≤ 8) comparisons against a cached defaults snapshot; no settings-file I/O per frame (loads only on a write, as today); highlight repaint scheduled once via `request_repaint_after`.

**Constraints**: Minimum window 960×640 (`crates/modplayer/src/main.rs`); 40 % text expansion without truncation/overlap; theme roles/tokens only (no colour literals), distinguishable in high contrast; every new element keyboard-operable with accessible name; strings in en-US + staged pt-BR; 020 category-row contracts (single line, More overflow, pinned selected) must still hold; zero engine/real-time change.

**Scale/Scope**: 5 owned categories + 2 placeholder categories, 14 descriptor fields, 8 resettable fields, 2 sign-out consequence items. Files: 1 new UI module (`settings/field.rs`), 7 UI modules edited, 2 core files edited (additive), 1 `Cargo.toml`, 2 en-US `.ftl` edited, 2 pt-BR `.ftl` new, 3 new + 5 extended test files.

All Technical Context items resolved; no open unknowns remain (see [research.md](./research.md) R1–R12).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: **VI (read-only account data), VII, VIII, X** and Governance › Manual Scenario Sign-Off / traceability. Others N/A with reason.

- [x] **I. Real-Time Path Is Sacred** — N/A: no engine or audio-thread code; `set_ceiling`/`confirm_device` are the existing UI-thread controller setters already used by manual changes, reset only calls them with the default value.
- [x] **II. Plugins Are Guests** — N/A: plugin runtime, gateway and plugin settings fields untouched; plugin search hits keep current behaviour (F20).
- [x] **III. Host Primitives, Plugin Behaviors** — N/A: no markers/loops/effects/transport primitive changes; nudge-step setter reused unchanged.
- [x] **IV. Audio Source Replaceable and Isolated** — N/A: no audio-source crate touched; tests use `FakeBackend` + `ScriptedHost`/`SyntheticHost`.
- [x] **V. No Audio Ever Leaves the Engine** — N/A: presentation of settings/account metadata only; no sample buffers, cache or file output.
- [x] **VI. Security and Privacy by Default** — Pass: account summary reads only `display_name`, `tier`, `last_validated_at` already exposed by `AccountSession`; no credential/token is read or displayed; sign-out semantics and cleared stores unchanged (S5); no telemetry, network or new data collection.
- [x] **VII. Rust Quality Gates** — Pass: no new workspace crate; one new direct dependency (`chrono`, `default-features = false, features = ["clock"]`) justified in R8 — `time` cannot soundly resolve a DST-correct local offset in a multi-threaded process, and chrono is already compiled via `oauth2`, so `cargo deny` sees no new license/advisory/duplicate; `forbid(unsafe_code)` unaffected; no `unwrap`/`expect` in library code (parser returns `Option`, offset resolver falls back to UTC); new pub items (`NUDGE_STEP_MS_RANGE`, `is_available`, `field::row`, `signout_dialog_width`) carry doc comments with runnable examples; fmt/clippy `-D warnings`/test/doc/deny in quickstart.
- [x] **VIII. Test What the NFRs Promise** — Pass: test-first per contract rules F1–F29, A1–A8, S1–S6 (test map in each contract); regression tests for the fixed defects (raw `OffsetDateTime` shown, dialog too narrow, unexplained empty categories, literal `" ms"`); pure-fn unit tests for arithmetic/formatting. No NFR-10.3 suite affected; proptest not required (no manifest parsing, marker arithmetic or state serialization change — `NUDGE_STEP_MS_RANGE` only replaces a literal inside an existing clamp).
- [x] **IX. One Plugin API Definition** — N/A: plugin API schema and manifests untouched.
- [x] **X. Simplicity, Portability, User's Override** — Pass: no new trait or feature flag (builder is a plain fn + struct, R1); reuses `panel_card`, `button`/`Variant`, `switch`, `section_label`, `body_measure`, `focus_ring_width`, existing setters and search/focus plumbing; identical on all OSes; every new element keyboard-operable with accessible name (F11, F23, F28, S4); all strings externalised en-US + staged pt-BR (fluent-strings.md, R10); user override (plugin disable/transport focus) untouched.
- [x] **Governance › Manual Scenario Sign-Off** — Pass: quickstart M1–M9, executed by the implementing agent with the macOS Quartz recipe against the live signed-in account (M7 stops short of confirming sign-out unless re-sign-in is planned).
- [x] **Governance › area-maintainer sign-off (GOV-3.2)** — N/A: engine, Capability Gateway and Plugin Runtime crates are not modified (engine types only read; R3 deliberately keeps the safe-volume cap range in the UI to avoid an engine change).
- [x] **Governance › traceability** — Pass: contracts cite FR-001–FR-017 / SC-001–SC-007; source UX-33..36 (UI/UX review §3.7) via spec; constitution X ↔ NFR-6.1/6.2/7.1.

**Post-design re-check (after Phase 1)**: unchanged — data model adds one core constant, one core predicate and UI-local state; contracts are UI + string contracts; the only dependency addition is an already-locked crate. **PASS.** No violations → Complexity Tracking lists none (assumptions recorded below).

## Project Structure

### Documentation (this feature)

```text
specs/028-settings-fields-and-account/
├── plan.md                        # This file
├── research.md                    # Phase 0: R1–R12 decisions
├── data-model.md                  # Phase 1: defaults source, resettable catalogue, highlight state, account view model
├── quickstart.md                  # Phase 1: automated gates + manual M1–M9
├── contracts/
│   ├── settings-fields.md         # Phase 1: F1–F29 (groups, anatomy, units, ranges, reset, highlight, placeholders, row badge)
│   ├── account-and-signout.md     # Phase 1: A1–A8, S1–S6
│   └── fluent-strings.md          # Phase 1: new/removed keys, en-US + pt-BR values
├── checklists/                    # from specify/clarify
└── tasks.md                       # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
Cargo.toml                          # [workspace.dependencies] chrono = { version = "0.4", default-features = false, features = ["clock"] }

crates/modplayer-core/src/
├── settings/model.rs               # pub const NUDGE_STEP_MS_RANGE; clamp_nudge_step_ms uses it
├── settings/mod.rs                 # re-export NUDGE_STEP_MS_RANGE
└── settings_registry.rs            # SettingsCategory::is_available(); invariant test vs DESCRIPTORS

crates/modplayer-ui/
├── Cargo.toml                      # chrono.workspace = true
└── src/settings/
    ├── field.rs                    # NEW: FieldSpec, FieldOutput, row(), reset button, range caption, highlight paint, value formatter/parser
    ├── mod.rs                      # SettingsScreen: defaults, highlight, pending_focus; search → highlight; Offline/Privacy unavailable cards
    ├── audio.rs                    # Output / Level protection cards; units, captions, resets; SAFE_VOLUME_CAP_RANGE
    ├── playback.rs                 # Connect device / Markers cards; device-name + nudge resets; Fluent ms unit; NUDGE_STEP_MS_RANGE
    ├── appearance.rs               # Theme card; theme + high-contrast resets; persist_theme / persist_high_contrast
    ├── language.rs                 # Language card (no reset)
    ├── account.rs                  # summary card, fallbacks, format_last_verified, local_offset_at, signout_dialog_width, wrapped modal
    └── category_row.rs             # category_item_job with "Coming soon" badge; measure/draw/menu/a11y name

crates/modplayer-ui/tests/
├── settings_fields.rs              # NEW
├── settings_search_highlight.rs    # NEW
├── settings_account.rs             # NEW
├── settings_category_row.rs        # extend: badges, 40 % expansion
├── fluent_keys.rs                  # key tables + pt-BR settings/account parity
├── high_contrast.rs                # highlight + badge in HC
├── design_token_literals.rs        # include settings/field.rs
└── accessibility.rs                # sweep still passes; new names

locales/
├── en-US/settings.ftl              # new keys
├── en-US/account.ftl               # new keys; remove account-display-name / -last-validated / -never-validated
├── pt-BR/settings.ftl              # NEW: this feature's keys only
└── pt-BR/account.ftl               # NEW: this feature's keys only
```

**Structure Decision**: Existing Cargo workspace; no new crate. All rendering lives in the existing `crates/modplayer-ui/src/settings/` module (one new file, `field.rs`), tests in the existing `crates/modplayer-ui/tests/`, strings in the existing `locales/en-US/` and `locales/pt-BR/` directories (two new pt-BR files). Core changes are additive and confined to `crates/modplayer-core/src/settings/model.rs` and `crates/modplayer-core/src/settings_registry.rs`. The engine crate (`crates/modplayer-engine`) and account crate (`crates/modplayer-account`) are read, not modified. All listed directories exist today.

## Implementation Outline (for /speckit-tasks)

1. **Core (additive)**: `NUDGE_STEP_MS_RANGE` (+ clamp uses it, test unchanged behaviour); `SettingsCategory::is_available` + invariant test.
2. **Strings first**: en-US additions/removals, new pt-BR files, `fluent_keys.rs` tables + parity test (red → green).
3. **`field.rs`**: builder, value formatter/parser (unit tests), reset button, caption, highlight painter; add to literal scan.
4. **Screens**: Audio, Playback, Appearance, Language rewritten onto cards + builder; reset wiring per data-model §3.3; deferred focus.
5. **Search highlight**: `FieldHighlight` state machine (unit tests), scroll/focus/paint, repaint scheduling; Account button targets.
6. **Placeholders + row badge**: unavailable cards; `category_item_job` for measure/inline/pinned/menu; a11y names; 020 tests + expansion.
7. **Account**: summary card, fallbacks, `format_last_verified` + `local_offset_at` (chrono dep), `signout_dialog_width`, wrapped modal; tests A1–A8/S1–S6.
8. **Cross-cutting tests**: 40 % expansion, HC, keyboard order, accessibility sweep.
9. **Gates + manual** M1–M9.

## Complexity Tracking

No Constitution Check violations — nothing to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| None | — | — |

**Assumptions recorded (headless run — no clarification possible)**:

- **Local date/time source**: `chrono::Local` per instant (R8). Rejected: `time`'s `local-offset` (unsound/`IndeterminateOffset` once multi-threaded; a startup capture is DST-wrong) and showing UTC/RFC 3339 (spec requires human-readable local).
- **Date format**: Fluent pattern `{ $day } { $month } { $year }, { $time }` with 12 localised short-month keys, 24 h clock (matches spec example "30 Sep 2026, 17:59"). Rejected: numeric ISO date (less readable, not the spec example); ICU formatting (heavy dependency).
- **Safe-volume cap range constant**: UI-local `SAFE_VOLUME_CAP_RANGE` in `audio.rs` (R3). Rejected: `VolumePercent::MIN/MAX` in the engine (GOV-3.2 sign-off for a UI-only need).
- **Buffer-preset reset without a preferred device**: not offered (no write path — manual change is a no-op then too). Rejected: offering a Reset that silently does nothing. Edge only: first-launch always confirms a device.
- **Device-name "changed" test**: `cached.device_name.is_some()` (a custom name set, even one equal to the hostname default). Rejected: comparing effective strings (would call `hostname` each frame).
- **Account card title**: new `account-summary-title` "Signed-in account" — the spec names the block "summary" but gives no title; a titled `panel_card` gives AT a heading. Rejected: untitled `Frame` (no heading for screen readers).
- **Unit via Fluent message with `$value`** rather than suffix string (R2), so pt-BR can space/place the unit; en-US values match spec examples exactly.
- **Highlight target for Account** descriptors: the Re-check / Sign out button itself (clarification), outlined by the same painter.
- **Removed keys** (`account-display-name`, `account-last-validated`, `account-never-validated`) are deleted only after a workspace grep shows no other caller; `placeholder-settings-category` and `tier-unknown` stay (other callers exist).

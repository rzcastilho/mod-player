# Tasks: Settings Fields, Placeholders, and Account Summary

**Input**: Design documents from `/specs/028-settings-fields-and-account/`

**Prerequisites**: plan.md, spec.md, research.md (R1–R12), data-model.md, contracts/ (settings-fields F1–F29, account-and-signout A1–A8/S1–S6, fluent-strings), quickstart.md

**Tests**: INCLUDED. Constitution VIII and plan.md require test-first per contract rules (write the test, see it fail, then implement).

**Organization**: Grouped by user story. Paths are relative to the repo root. `UI` = `crates/modplayer-ui`, `CORE` = `crates/modplayer-core`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US6, from spec.md
- **Traceability**: each task ends with `Implements:` listing the spec requirement IDs (FR-001–FR-017) and source NFR IDs it delivers. Constitution Principle X maps to NFR-6.1 / NFR-6.2 (accessible names, keyboard operability) and NFR-7.1 (externalized en-US + pt-BR strings); UX-33..36 (UI/UX review §3.7) are traced via spec.md.
- Toolchain: `export RUSTUP_TOOLCHAIN=1.95.0`. Prefix shell commands with `rtk`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependency and additive core items that later phases read.

- [X] T001 Add `chrono = { version = "0.4", default-features = false, features = ["clock"] }` to `[workspace.dependencies]` in `Cargo.toml`, and `chrono.workspace = true` to `crates/modplayer-ui/Cargo.toml`. Verify `cargo deny check` reports no new crate (chrono 0.4.45 is already in `Cargo.lock` via `oauth2`). — Implements: FR-012, FR-017
- [X] T002 [P] In `crates/modplayer-core/src/settings/model.rs` add `pub const NUDGE_STEP_MS_RANGE: RangeInclusive<u16> = 1..=1000;` with a doc comment and runnable example. Make `clamp_nudge_step_ms` use it. Re-export it from `crates/modplayer-core/src/settings/mod.rs`. Add a unit test that clamp behaviour is unchanged (0→1, 1001→1000, 10→10). — Implements: FR-004, FR-017
- [X] T003 [P] In `crates/modplayer-core/src/settings_registry.rs` add `SettingsCategory::is_available(self) -> bool` (`false` for `Offline` and `PrivacyDiagnostics`, `true` otherwise) with a doc-tested example. Add a unit test for the invariant: `!c.is_available()` ⇒ no `DESCRIPTORS` entry has `category == c` (F25). — Implements: FR-009, FR-010

**Checkpoint**: `rtk cargo test -p modplayer-core` green.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Strings, the shared field builder and the screen state. Every story uses these.

**⚠️ CRITICAL**: No user story starts before this phase is done.

- [X] T004 [P] Add to `locales/en-US/settings.ftl` all new keys from `contracts/fluent-strings.md`: `settings-group-*` (6), `setting-value-{dbfs,percent,ms}`, `setting-range-{dbfs,percent,ms}`, `settings-reset`, `settings-reset-a11y`, `settings-coming-soon`, `settings-category-coming-soon-a11y`, `settings-unavailable-offline`, `settings-unavailable-privacy-diagnostics`. — Implements: FR-015, NFR-7.1 (strings for FR-001, FR-004, FR-005, FR-006, FR-009, FR-010)
- [X] T005 [P] Add to `locales/en-US/account.ftl` the new keys: `account-summary-title`, `account-identity-unavailable`, `account-tier-unverified`, `account-last-verified`, `account-last-verified-at`, `account-never-verified`, `date-month-short-1` … `date-month-short-12`. Do NOT remove the old keys yet (see T045). — Implements: FR-012, FR-015, NFR-7.1
- [X] T006 [P] Create `locales/pt-BR/settings.ftl` with the pt-BR values from `contracts/fluent-strings.md` (this feature's keys only). — Implements: FR-015, NFR-7.1
- [X] T007 [P] Create `locales/pt-BR/account.ftl` with the pt-BR values from `contracts/fluent-strings.md` (this feature's keys only). — Implements: FR-015, NFR-7.1
- [X] T008 Extend `crates/modplayer-ui/tests/fluent_keys.rs`: add the new keys to the key tables and add a pt-BR parity test covering the new `settings.ftl` and `account.ftl` (F26). Confirm it fails before T004–T007 land and passes after. — Implements: FR-015, NFR-7.1
- [X] T009 [P] Write failing unit tests in a new `crates/modplayer-ui/src/settings/field.rs` for the pure helpers: (a) value formatter renders one decimal for dBFS ("-1.0"), integer for % and ms; (b) value parser accepts a number with or without unit and returns `None` on garbage (F8); (c) `ResetState` computed `Offered` only when current ≠ default. — Implements: FR-003, FR-004, FR-005
- [X] T010 Implement `crates/modplayer-ui/src/settings/field.rs` (R1, R2): `FieldSpec`, `FieldOutput`, `ResetState` (data-model §3.1–3.2). `pub fn row(ui, spec, add_control) -> FieldOutput` draws label → help (indented `space::SM`, `text::SECONDARY`, `roles.text_secondary`, max width `theme::body_measure`) → control line (control + optional Reset `Variant::Secondary`, text `settings-reset`, a11y name `settings-reset-a11y {$field}`) → range caption. It returns the whole-field `rect`. Add the value formatter/parser via `tr_args` (pre-formatted number strings). Doc comments with runnable examples on all `pub` items. Make T009 pass. Register `mod field;` in `crates/modplayer-ui/src/settings/mod.rs`. — Implements: FR-001, FR-002, FR-003, FR-004, FR-006
- [X] T011 Add the `SettingsScreen` state in `crates/modplayer-ui/src/settings/mod.rs` (data-model §3.5): `defaults: AudioSettings` (= `AudioSettings::default()`), `highlight: Option<FieldHighlight>`, `pending_focus: Option<&'static str>`. Merge `pending_focus` with the existing `focus_target` handling. No behaviour change yet. — Implements: FR-005, FR-006, FR-007
- [X] T012 [P] Extend `crates/modplayer-ui/tests/design_token_literals.rs` so its scan covers `crates/modplayer-ui/src/settings/field.rs` (F29). Must pass after T010. — Implements: FR-016

**Checkpoint**: `rtk cargo test -p modplayer-ui --test fluent_keys --test design_token_literals` green. Builder compiles and is unit-tested.

---

## Phase 3: User Story 1 — Read a category as a set of decisions (Priority: P1) 🎯 MVP

**Goal**: Fields sit in titled cards with indented help, inline units and range captions.

**Independent Test**: Open Audio. Two cards ("Output", "Level protection"), indented help, and every slider value shows its unit with a range caption beneath.

### Tests for US1 (write first, must fail)

- [X] T013 [P] [US1] Create `crates/modplayer-ui/tests/settings_fields.rs` using the existing egui headless + AccessKit harness at 960×640. Add tests for: F1 (Audio = "Output" card then "Level protection" card with the right fields in order), F2 (Playback, Appearance, Language groups), F3 (card headers are `Role::Heading` with un-uppercased names), F5/F6 (help left edge ≥ label left edge + 8 px; width ≤ body measure), F7 (ceiling "-1.0 dBFS", cap "50%", nudge "10 ms" inside the control text; no separate unit label; buffer preset keeps "(~N ms)"), F9 (captions "-6.0 to -0.1 dBFS", "0 to 100%", "1 to 1000 ms" equal values built from `CeilingDb::MIN/MAX`, `SAFE_VOLUME_CAP_RANGE`, `NUDGE_STEP_MS_RANGE`). Also a F4 guard that Controls/Plugins/Developer/About still pass their existing tests. — Implements: FR-001, FR-002, FR-003, FR-004

### Implementation for US1

- [X] T014 [US1] Rewrite `crates/modplayer-ui/src/settings/audio.rs` onto `panel_card` + `field::row`: card "Output" (output device, buffer preset, Test output device) and card "Level protection" (limiter ceiling, safe-volume switch, safe-volume cap), keeping today's field order. Add UI-local `pub(crate) const SAFE_VOLUME_CAP_RANGE` (R3). Ceiling slider uses `custom_formatter`/`custom_parser` with `setting-value-dbfs`; cap uses `setting-value-percent`. Captions via `setting-range-*` from the shared constants. No reset wiring yet (Phase 4). — Implements: FR-001, FR-002, FR-003, FR-004, FR-017
- [X] T015 [P] [US1] Rewrite `crates/modplayer-ui/src/settings/playback.rs`: card "Connect device" (device name) and card "Markers" (nudge step). Remove the literal `" ms"` suffix and use `setting-value-ms` plus `NUDGE_STEP_MS_RANGE` for the `DragValue` range and caption. — Implements: FR-001, FR-003, FR-004, FR-017
- [X] T016 [P] [US1] Rewrite `crates/modplayer-ui/src/settings/appearance.rs`: card "Theme" (theme, high contrast) using `field::row`. Keep `persist_theme` / `persist_high_contrast` unchanged. — Implements: FR-001, FR-002, FR-017
- [X] T017 [P] [US1] Rewrite `crates/modplayer-ui/src/settings/language.rs`: card "Language" (locale) using `field::row`, no reset. — Implements: FR-001, FR-002
- [X] T018 [US1] Make T013 pass and confirm the existing `settings_plugins.rs`, `controls.rs` and `developer_removed.rs` stay green (F4). Run `rtk cargo test -p modplayer-ui`. — Implements: FR-001

**Checkpoint**: US1 is demoable on its own. Audio, Playback, Appearance and Language read as grouped decisions.

---

## Phase 4: User Story 2 — Reset a changed field (Priority: P1)

**Goal**: Exactly eight fields offer a per-field Reset when current ≠ default. Reset writes through the existing setter and returns focus to the control.

**Independent Test**: Change the ceiling, see Reset only on that row, activate it, see the default restored and Reset gone.

### Tests for US2 (write first, must fail)

- [X] T019 [US2] Extend `crates/modplayer-ui/tests/settings_fields.rs` with tests for F10–F15: Reset absent at defaults for all 8 ids; present after a change, with a11y name "Reset {field} to default" (F11); activation by click and by Enter/Space restores only that field (F12, F15, compare the settings diff); next frame Reset is absent and focus is on the control (F12); hidden again when changed back manually or hand-edited to default (F13); cap Reset offered while the switch is off (F13); no Reset on output device, locale, Test output device, Re-check, Sign out, Controls, Developer or plugin fields (F14); buffer preset Reset hidden when no preferred device exists. Tab order is control then its Reset (F28). — Implements: FR-005, FR-006, FR-016, NFR-6.1, NFR-6.2

### Implementation for US2

- [X] T020 [US2] In `crates/modplayer-ui/src/settings/field.rs` add the static `ResettableField` catalogue per data-model §3.3 (8 ids, default source, "offered when" predicate, write-path selector). Wire `FieldOutput.reset_clicked` and set `pending_focus` to the field id on activation. — Implements: FR-005, FR-006
- [X] T021 [US2] Wire resets in `crates/modplayer-ui/src/settings/audio.rs`: `audio.buffer_preset` via `confirm_device(preferred, default)` (only when `preferred_device().is_some()`), `audio.limiter_ceiling` via `set_ceiling`, `audio.safe_volume_enabled` and `audio.safe_volume_cap` via `persist_safe_volume`. Compare against `self.defaults` with no per-frame file I/O. — Implements: FR-005, FR-006, FR-017
- [X] T022 [P] [US2] Wire resets in `crates/modplayer-ui/src/settings/appearance.rs`: `appearance.theme` via `persist_theme` + `theme::apply`, and `appearance.high_contrast` via `set_high_contrast` + persist. — Implements: FR-005, FR-006, FR-017
- [X] T023 [P] [US2] Wire resets in `crates/modplayer-ui/src/settings/playback.rs`: `playback.device_name` (offered iff `cached.device_name.is_some()`) via `set_device_name("")`, and `markers.nudge_step_ms` via `set_nudge_step_ms`. — Implements: FR-005, FR-006, FR-017
- [X] T024 [US2] Apply `pending_focus` in `crates/modplayer-ui/src/settings/mod.rs` so the control with that id takes focus on the next frame, then clear it. Make T019 pass. — Implements: FR-006, NFR-6.2

**Checkpoint**: US1 and US2 are both independently verifiable.

---

## Phase 5: User Story 3 — Jump to a matched field from search (Priority: P2)

**Goal**: A descriptor search result opens its category, scrolls to, focuses and outlines the whole field. The highlight clears after 3 s or on the first key or pointer press.

**Independent Test**: Search "nudge", choose the result. Playback opens, the field is in view, focused and outlined, and the outline is gone after 3 s.

### Tests for US3 (write first, must fail)

- [X] T025 [P] [US3] Add unit tests in `crates/modplayer-ui/src/settings/mod.rs` for the `FieldHighlight` state machine (data-model §3.4): armed false on the selecting frame, armed next frame, cleared at ≥ 3.0 s, on a key press, on a pointer-button press, on category change, and restarted by a new result. — Implements: FR-008
- [X] T026 [P] [US3] Create `crates/modplayer-ui/tests/settings_search_highlight.rs` for F16–F20: result text "{category} › {field}" and plugin hits unchanged; selecting a descriptor in Audio, Playback, Appearance, Language and Account selects the category, puts the field rect fully inside the content viewport, focuses the control and draws the highlight in one activation; stroke ≥ 2 px in the accent role; Controls and Developer targets focus only; empty query gives no highlight; Account targets (`account.recheck_subscription`, `account.sign_out`) highlight the button (A8). — Implements: FR-007, FR-008

### Implementation for US3

- [X] T027 [US3] Implement `FieldHighlight` in `crates/modplayer-ui/src/settings/mod.rs`: create it when a descriptor result in Audio, Playback, Appearance, Language or Account is chosen; arm it the next frame; clear per the transitions; call `ctx.request_repaint_after` once so expiry happens without input. — Implements: FR-007, FR-008
- [X] T028 [US3] In `crates/modplayer-ui/src/settings/field.rs` add the highlight painter: stroke width `max(2, focus_ring_width)` in `roles.accent` plus a translucent accent fill over `FieldOutput.rect` (no colour literals). When the field is the highlight target, call `ui.scroll_to_rect(rect, ..)` and request focus on the control. — Implements: FR-007, FR-008, FR-016
- [X] T029 [US3] Make Account descriptor targets highlight the Re-check / Sign out button itself, using the same painter in `crates/modplayer-ui/src/settings/account.rs`. Make T025 and T026 pass. — Implements: FR-007, FR-008

**Checkpoint**: US3 works on top of US1/US2 without affecting them.

---

## Phase 6: User Story 4 — Honest placeholder categories (Priority: P2)

**Goal**: Offline and Privacy & diagnostics state "not available yet" and carry a "Coming soon" badge in the category row (inline, pinned, More menu).

**Independent Test**: Open Offline. It shows its header and the not-available sentence, and its row entry carries the badge with a11y name "Offline, coming soon".

### Tests for US4 (write first, must fail)

- [X] T030 [P] [US4] Extend `crates/modplayer-ui/tests/settings_category_row.rs` for F22–F24: both categories remain selectable; badge text appears inline, in the pinned slot and in the "More" menu; complete categories have no badge; a11y name "{category}, coming soon"; width counted in `measure_category_width`; the 020 contracts (single line, overflow into More, pinned selected item, equal row height) still hold with badges, including at 40 % expansion (`i18n::with_pseudo_expansion(40, …)`). Keep the proptest regressions file valid. — Implements: FR-010, FR-015, NFR-6.1, NFR-7.1
- [X] T031 [P] [US4] Add F21 tests (in `crates/modplayer-ui/src/settings/mod.rs` unit tests or `settings_fields.rs`): Offline body shows a card titled `settings-cat-offline` and the `settings-unavailable-offline` sentence, Privacy & diagnostics likewise, and `placeholder-settings-category` no longer appears in either. — Implements: FR-009

### Implementation for US4

- [X] T032 [US4] In `crates/modplayer-ui/src/settings/category_row.rs` add `category_item_job(ui, category) -> LayoutJob`: `section_label(label)` plus, when `!is_available()`, `space::XS` and `settings-coming-soon` in secondary style and role. Use it in `measure_category_width`, the inline draw, the pinned draw and the More menu items. Set the a11y name to `settings-category-coming-soon-a11y {$category}` for unavailable categories (data-model §3.8). — Implements: FR-010, NFR-6.1
- [X] T033 [US4] In `crates/modplayer-ui/src/settings/mod.rs` render Offline and Privacy & diagnostics as `panel_card(tr("settings-cat-…"))` plus one wrapped sentence from their per-category key. Keep `placeholder-settings-category` for the Plugins management placeholder. Make T030 and T031 pass. — Implements: FR-009

**Checkpoint**: US4 done. No unexplained empty screens (SC-003).

---

## Phase 7: User Story 5 — Account summary first (Priority: P2)

**Goal**: Account leads with one summary card (identity strong, tier and last-verified as label–value lines), with Re-check then Sign out beneath.

**Independent Test**: Open Account signed in. The summary card is first and fully visible at 960×640, with fallbacks for missing data.

### Tests for US5 (write first, must fail)

- [X] T034 [P] [US5] Add unit tests in `crates/modplayer-ui/src/settings/account.rs` for A3/A4: `format_last_verified(OffsetDateTime, UtcOffset)` gives "30 Sep 2026, 17:59" for a fixed offset, handles day rollover across offsets and midnight as "00:05", never produces the RFC 3339 / `Display` form, and pads `HH:MM` but not the day. Test the empty and whitespace-only identity fallback. — Implements: FR-012
- [X] T035 [P] [US5] Create `crates/modplayer-ui/tests/settings_account.rs` (fake `AccountService` fixtures as used in `first_launch.rs` / `accessibility.rs`) for A1, A2, A5–A7: in `Active` and `Expired` the first block is a `panel_card` titled `account-summary-title` holding identity, tier, last-verified in order; fallbacks "Name unavailable", "Not verified yet", "Never verified online"; Re-check (standard) then Sign out (destructive) beneath; Re-check enabled as today; summary rect inside the scroll viewport at offset 0 at 960×640 (A6); `account()` = `None` while Active/Expired shows the signed-out view (A7). — Implements: FR-011, FR-012

### Implementation for US5

- [X] T036 [US5] In `crates/modplayer-ui/src/settings/account.rs` implement `local_offset_at(OffsetDateTime) -> UtcOffset` using `chrono::Local` (fallback to UTC, no `unwrap`/`expect`) and the pure `format_last_verified` using `account-last-verified-at` with `$day`, `$month` (`date-month-short-N`), `$year`, `$time`. Doc comments with runnable examples. — Implements: FR-012, NFR-7.1
- [X] T037 [US5] In the same file render the summary card per data-model §3.6: identity (body-strong, fallback key), tier line (`account-tier` label + `tier-premium` / `tier-free` / `account-tier-unverified`), last-verified line (`account-last-verified` label + value or `account-never-verified`). Place Re-check help + button, then the divider and the destructive Sign out, beneath. Same layout for `Expired`. Make T034 and T035 pass. — Implements: FR-011, FR-012

**Checkpoint**: US5 done. Sign-out dialog changes come next.

---

## Phase 8: User Story 6 — Sign-out confirmation fully visible (Priority: P2)

**Goal**: The dialog is `clamp(viewport − 64, 420, 560)` px wide, every item wraps, and Cancel stays the default.

**Independent Test**: At 960×640 with 40 % expansion, every consequence item is visible inside the dialog and focus starts on Cancel.

### Tests for US6 (write first, must fail)

- [X] T038 [P] [US6] Add unit and doc tests for `signout_dialog_width` (S1): `960 → 560`, `500 → 436`, `400 → 420`. — Implements: FR-013
- [X] T039 [US6] Extend `crates/modplayer-ui/tests/settings_account.rs` for S2–S6: no `…` in any node name or painted galley; at 960×640 with `with_pseudo_expansion(40)` the modal's outer rect ⊆ viewport and every consequence node's bounds ⊆ modal rect, with no internal scroll area; Cancel focused once on open and Enter/Space close it while still signed in; Escape and backdrop click close as Cancel and only an explicit Sign out calls `account.sign_out()`; button order Cancel, Sign out; Sign out is `Variant::Destructive`. — Implements: FR-013, FR-014, FR-015, NFR-6.2

### Implementation for US6

- [X] T040 [US6] In `crates/modplayer-ui/src/settings/account.rs` add `pub(crate) fn signout_dialog_width(viewport_w: f32) -> f32` (`clamp(w − 2·space::XXL, 420, 560)`, doc-tested). Rebuild the `Modal` with that width: wrapping title, intro and one wrapped bullet per `signout_categories()` entry with a hanging indent, no `Label::truncate`. Keep the Cancel initial-focus-once, Escape/backdrop = Cancel, and Destructive Sign out. Make T038 and T039 pass. — Implements: FR-013, FR-014, FR-017

**Checkpoint**: All six stories are independently functional.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [X] T041 [P] Extend `crates/modplayer-ui/tests/settings_fields.rs` for F27 and SC-007: with `with_pseudo_expansion(40, …)` at 960×640, no label, unit value, caption, group header, Reset or badge node in an owned category is clipped or overlaps its neighbour. — Implements: FR-015, NFR-7.1
- [X] T042 [P] Extend `crates/modplayer-ui/tests/high_contrast.rs` (F18 HC): highlight outline, badge, group cards and destructive styling stay distinguishable in high contrast. — Implements: FR-016
- [X] T043 [P] Extend `crates/modplayer-ui/tests/accessibility.rs`: the AccessKit sweep passes with the new names (Reset, badge, summary card, modal). — Implements: FR-016, NFR-6.1, NFR-6.2
- [X] T044 [P] Keyboard order check (F28) added to `crates/modplayer-ui/tests/settings_fields.rs`: control then its Reset, groups top to bottom. — Implements: FR-006, FR-016, NFR-6.2
- [X] T045 Workspace grep for callers of `account-display-name`, `account-last-validated` and `account-never-validated`. If none remain, delete them from `locales/en-US/account.ftl` and move their `fluent_keys.rs` entries to the new keys. Keep `tier-unknown` and `placeholder-settings-category`. — Implements: FR-015, NFR-7.1
- [X] T046 Grep `crates/modplayer-ui/src/settings/` for leftover user-visible string literals, including the old `" ms"`, and confirm none remain (F26). — Implements: FR-015, NFR-7.1
- [X] T047 Run the automated gates from `quickstart.md`: `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`, `rtk cargo test --workspace`, `rtk cargo test --doc -p modplayer-ui -p modplayer-core`, `rtk cargo deny check`. Fix any failures. — Implements: FR-001–FR-017 (automated gates)
- [X] T048 Manual Scenario Sign-Off (constitution): the implementing agent executes quickstart M1–M9 on macOS with the Quartz `CGEventPost` + `screencapture` recipe against the live signed-in account. Do not confirm sign-out in M7. Record pass or deviation plus evidence path per scenario on this task, and put any behavioural deviation in `quickstart.md` and `research.md`. — Implements: FR-001–FR-017, NFR-6.1, NFR-6.2, NFR-7.1
  - Walk run 2026-09-30, macOS, 960×640 window, live account (re-signed in through the OAuth Retry). Evidence is in `target/manual-walk/shots/m/` (gitignored). Live `settings.toml` was backed up first and restored afterwards.
  - **M1**: deviation, fixed. Cards, indented help, "-1.0 dBFS" / "50%" and both captions were correct, but the slider rails were invisible on the card fill. Fixed in `field::row` with a regression test (research R13.1). Re-walk passed (`m1-audio-fixed.png`).
  - **M2**: pass. Ceiling -3.0 → Reset only on that row → -1.0, Reset gone, focus ring on the slider, and only `limiter_ceiling_db` changed. With the switch off, cap 79 → Reset → 50. The switch Reset restored `enabled = true`, and the file matched its pre-step state exactly (`m2-*.png`).
  - **M3**: pass. "25 ms" / "250 ms" shown inside the box, caption "1 to 1000 ms", Reset → 10 ms. Device name "Test Rig" → Reset removed the key and showed the default name, with focus back on the field (`m3-*.png`).
  - **M4**: pass. Result "Playback › Marker nudge step". Tab to the result then Enter opened Playback with the field outlined and the control focused. The outline was gone by ~3.7 s; pressing a key cleared it at once (`m4-*.png`). Enter in the search box itself does not activate a result (existing behaviour, quickstart note).
  - **M5**: deviation, fixed. The Offline and Privacy cards and sentences were correct, and the badge showed inline, in the More menu and in the pinned slot. On the selected item the badge measured ~1.04:1. Fixed with a regression test (R13.2), re-walk passed (`m5-badge-sel-fixed.png`, `m5-privacy.png`). VoiceOver was not run; the a11y name is covered by tests.
  - **M6**: partial. The summary card comes first and is fully visible at 960×640, with Re-check and then Sign out below it. The live profile/tier check failed (account crate, out of scope), so the fallbacks showed. Populated values could not be observed (R13, `m6-account.png`).
  - **M7**: deviation, fixed. The dialog is ~560 px with both bullets wrapped and visible, but it opened with nothing focused. Fixed with a regression test (R13.3). Re-walk: Cancel focused on every open, Enter, Escape and backdrop each close it, still signed in, and sign-out was never confirmed (`m7-open-fixed.png`, `m7-strip.png`).
  - **M8**: pass. In high contrast, cards, captions, badge and rails stay distinguishable, and the highlight outline is ~3 px (`m8-hc-*.png`).
  - **M9**: pass. Tab order: category row → More → output device → buffer preset → its Reset → Test output device → ceiling slider (and its value box) → safe-volume switch → cap. Space on Reset restored "Balanced" and returned focus to the combo (`m9-*.png`).

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (P1)**: no dependencies. T002 and T003 run in parallel.
- **Foundational (P2)**: needs Setup for T001 only (T010 does not need chrono; T036 does). Blocks all stories.
- **US1 (P3)** → **US2 (P4)**: US2 edits the same screen files as US1, so it runs after US1.
- **US3 (P5)**: needs Foundational and the `field::row` rect. Account targets in T029 need the US5 button layout for full verification, so do T029 after T037 or re-verify then.
- **US4 (P6)**: independent of US1–US3 (touches `category_row.rs` and `mod.rs`); needs T003.
- **US5 (P7)** and **US6 (P8)**: independent of US1–US4 except for shared `account.rs` and `mod.rs`. US6 follows US5 (same file, same test file).
- **Polish (P9)**: after all desired stories.

### Story dependencies

- US1 has no story dependency (MVP).
- US2 depends on US1 layout.
- US3 depends on Foundational (and US1 for field rects).
- US4, US5 are independent.
- US6 follows US5 (same files).

### Within each story

Tests first and failing, then implementation, then the checkpoint run.

### Parallel opportunities

- Setup: T002 ‖ T003.
- Foundational: T004 ‖ T005 ‖ T006 ‖ T007 ‖ T009 ‖ T012.
- US1: T015 ‖ T016 ‖ T017 after T014 (different files).
- US2: T022 ‖ T023.
- US3: T025 ‖ T026. US4: T030 ‖ T031. US5: T034 ‖ T035.
- Different stories can go in parallel across developers once Foundational is done, except where they share files (see above).
- Polish: T041 ‖ T042 ‖ T043 ‖ T044.

### Parallel example: US1

```text
T013 (tests) → T014 (audio.rs) → then in parallel: T015 playback.rs, T016 appearance.rs, T017 language.rs → T018
```

---

## Implementation Strategy

### MVP first (US1 only)

1. Setup, Foundational, then US1 (T001–T018).
2. Stop and validate: Audio shows two cards, indented help, inline units and range captions.

### Incremental delivery

1. MVP (US1) → US2 (Reset) → US3 (search highlight), each demoable.
2. US4, US5 and US6 can land in any order; US6 after US5.
3. Polish, gates and manual sign-off last.

### Notes

- The only core changes are additive (`NUDGE_STEP_MS_RANGE`, `is_available`). The engine and account crates are read, not modified.
- Commit after each task or logical group. Do not start an implementation task until its test is red.

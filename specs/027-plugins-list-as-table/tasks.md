# Tasks: Plugins List as a Real Table

**Input**: Design documents from `/specs/027-plugins-list-as-table/`

**Prerequisites**: plan.md, spec.md, research.md (R1–R13), data-model.md, contracts/plugins-table.md (T1–T20), contracts/fluent-strings.md, quickstart.md

**Tests**: INCLUDED. plan.md (Constitution VIII) mandates test-first per contract T1–T20, a proptest for layout arithmetic, an overflow regression test and the FR-009 colour-literal guard. Write each test first and confirm it fails before implementing.

**Organization**: Grouped by user story. `crates/modplayer-ui/src/plugins_view.rs` and `crates/modplayer-ui/tests/plugins_view.rs` are shared by all stories, so tasks touching them are NOT marked [P] against each other.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an incomplete task
- **[Story]**: US1–US4 (spec.md); none for Setup / Foundational / Polish
- Toolchain: `export RUSTUP_TOOLCHAIN=1.95.0`; run commands with the `rtk` prefix

## Path Conventions

Cargo workspace. Core model: `crates/modplayer-core/src/plugins/view.rs`. UI: `crates/modplayer-ui/src/{plugins_view,app,plugin_panels}.rs`. UI tests: `crates/modplayer-ui/tests/`. Locales: `locales/{en-US,pt-BR}/plugins.ftl`.

---

## Phase 1: Setup

**Purpose**: Baseline and safety checks before changing anything.

- [X] T001 Run `rtk cargo test -p modplayer-core plugins::view` and `rtk cargo test -p modplayer-ui --test plugins_view --test accessibility --test fluent_keys --test design_token_literals` to record the green baseline; note any pre-existing failures in `specs/027-plugins-list-as-table/research.md` (append a "Baseline" line)
- [X] T002 [P] Grep the workspace (`crates/`, `modplayer/`, `locales/`) for callers of `plugins-col-version`, `plugins-col-cpu`, `plugins-col-memory`, `plugins-cpu`, `plugins-memory`, `plugins-list-separator`; record which are safe to remove (only `plugins_view.rs` and tests expected) in `specs/027-plugins-list-as-table/research.md`
- [X] T003 [P] Read `crates/modplayer-ui/src/search_view.rs` (026) and `crates/modplayer-ui/src/app.rs` `Section::Plugins` arm to confirm the owned-`ScrollArea` / `SectionMemory` / `ViewKey::Plugins` pattern to copy (no edits)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core model fields, strings and wiring every story needs.

**⚠️ CRITICAL**: No user story work starts until this phase is complete.

- [X] T004 Write failing unit tests in `crates/modplayer-core/src/plugins/view.rs`: Suspended lifecycle → `suspend_cause == Some(cause)` for each `SuspendCause`; Active/other → `None`; `Budgets::DEFAULT` → `cpu_budget_pct == 10.0` and `memory_budget_bytes == 67_108_864`; zero `window` → `cpu_budget_pct == 0.0`; invariant `suspend_cause.is_some() ⇒ health == Some(Health::Suspended)`
- [X] T005 Add `suspend_cause: Option<SuspendCause>`, `cpu_budget_pct: f32`, `memory_budget_bytes: u64` to `PluginRow` and derive them in `row()` from `record.lifecycle` / `record.budgets` in `crates/modplayer-core/src/plugins/view.rs` (data-model §1, R6, R7); doc comments with runnable examples, no `unwrap`/`expect`; fix all `PluginRow` constructors in the workspace (`rtk cargo check --workspace --all-targets`) (depends on T004)
- [X] T006 [P] Edit `locales/en-US/plugins.ftl` per contracts/fluent-strings.md: change `plugins-health-ok` → "healthy", `plugins-health-warning` → "degraded"; add all new keys (`plugins-col-resource`, `-col-actions`, `plugins-resource-cpu/-memory/-cpu-none/-memory-none`, `plugins-over-budget`, `plugins-suspended-reason-unknown`, `plugins-permissions-show/-hide`, `plugins-panels-count/-show/-hide`, `plugins-panel-{show,hide,enable,disable}-a11y`, `plugins-restart-a11y`). Do NOT yet remove the old keys
- [X] T007 [P] Create `locales/pt-BR/plugins.ftl` with pt-BR values for exactly the changed and new keys in contracts/fluent-strings.md (header comment mirrors `locales/pt-BR/library.ftl`: not yet a runtime locale, exists for the parity test)
- [X] T008 Update `crates/modplayer-ui/tests/fluent_keys.rs`: add every new key (with variables) to the key table and add a pt-BR parity test asserting each contract key exists in both `en-US/plugins.ftl` and `pt-BR/plugins.ftl` with identical variables (depends on T006, T007)
- [X] T009 Add `pub struct PluginsViewState { permissions_open: HashSet<PluginId>, panels_open: HashSet<PluginId> }` (`Default`, doc comment + example) to `crates/modplayer-ui/src/plugins_view.rs`, with a `prune(&mut self, rows: &[PluginRow])` method using `retain` (data-model §4, R8)
- [X] T010 Change the `plugins_view::show` signature to `show(ui, controller, state: &mut PluginsViewState, section_memory: &mut SectionMemory)` (contracts/plugins-table.md "Entry point"); add a `PluginsViewState` field to `App` in `crates/modplayer-ui/src/app.rs`; update the `Section::Plugins` arm to pass `&mut self.plugins_view_state` and `&mut self.section_memory`, keep the 500 ms `request_repaint_after`, and drop the outer `ScrollArea`/recording for this arm. Keep the current row rendering temporarily so the workspace compiles and `rtk cargo test -p modplayer-ui` stays green (depends on T005, T009)

**Checkpoint**: Core fields, strings and wiring ready; `rtk cargo test --workspace` green.

---

## Phase 3: User Story 1 - Scan many plugins in aligned columns (Priority: P1) 🎯 MVP

**Goal**: Seven-column table, header and cells sharing x-extents, 17 plugins fit at 960 pt and ≥ 560 pt, truncation with tooltips, vertical scroll under a fixed header.

**Independent Test**: 17 fixture plugins at 960 pt and 560 pt: zero overflowing or overlapping cells, header aligned with columns, no horizontal scroll.

### Tests for User Story 1 (write first, confirm FAIL)

- [X] T011 [P] [US1] Add `PluginsColumns::layout` unit tests and a proptest (`w ∈ [560, 4000]`: Σ widths + gaps ≤ w, each width ≥ MIN, contiguous with exact `GAP`; `w ∈ [0, 560)`: widths ≥ 0, last `max_x ≤ w`; `MIN` sums with gaps to exactly `HOST_CONTENT_FLOOR`) in `crates/modplayer-ui/src/plugins_view.rs` `#[cfg(test)]` (L1–L4, R2, R12)
- [X] T012 [US1] Add the AccessKit harness tests `seventeen_fixtures_fit_at_min_window_width` (960) and `seventeen_fixtures_fit_at_section_floor` (560), `header_aligns_with_columns`, `long_name_truncates_with_tooltip_and_full_accessible_name`, `invalid_row_span`, and the empty-state test (T1, `plugins-empty`, no header, no scroll area) in `crates/modplayer-ui/tests/plugins_view.rs` using the 17 packages in `plugins/fixtures/` (T1–T7, T16, R12)
- [X] T013 [US1] Add `plugins_view_has_no_colour_literals` in `crates/modplayer-ui/tests/plugins_view.rs`: `include_str!` of `src/plugins_view.rs`, strip `//` comments and `use` lines, assert no `Color32::from_`, `Color32::[A-Z]` (except `TRANSPARENT`), `rgb(`, `hex_color!`; also assert `health_color` returns `roles.positive/warning/danger` for normal and high-contrast `Roles` (T19, R11). Should pass immediately; it guards every later task

### Implementation for User Story 1

- [X] T014 [US1] Implement `Column`, `ColumnExtent`, `PluginsColumns { extents, gap }`, constants `GAP = theme::space::XS`, `MIN = [104, 56, 44, 80, 48, 128, 76]`, `WEIGHT = [3, 1, 0, 2, 0, 1, 1]`, `layout(available_width)` (L1–L4, zero-window guarded) and `span(from, to)` in `crates/modplayer-ui/src/plugins_view.rs`, with doc comments and runnable examples (data-model §3, R2) (makes T011 pass)
- [X] T015 [US1] Implement `truncating_cell(ui, rect, text, style) -> Response` in `crates/modplayer-ui/src/plugins_view.rs`: `Label::truncate()` placed in the cell rect via `scope_builder(UiBuilder::new().max_rect(..))`; `on_hover_text(full)` only when truncated; full text as the accessible label (T6, R4)
- [X] T016 [US1] Rebuild `show` in `crates/modplayer-ui/src/plugins_view.rs`: heading `plugins-title`; empty state → `plugins-empty` only (T1); compute columns once per frame from `ui.available_width() − (scroll.bar_width + scroll.bar_outer_margin)` (R3a); draw the fixed seven-label header outside the scroll area using the column rects (T2, T3); call `state.prune(rows)`; rows inside a vertical-only `ScrollArea` from `section_memory.scroll_area(&ViewKey::Plugins)` with `scroll_bar_visibility(AlwaysVisible)` (T5, R3). Register `ViewKey::Plugins` in `SectionMemory` if missing (depends on T010, T014, T015)
- [X] T017 [US1] Render each row inside `row_frame` with cells placed in their column rects: Name (name + secondary-style version, `plugins-dash` version for invalid rows, T7), Source (`plugins-source-bundled` or source), Enabled (existing `switch`, `plugins-enable-toggle`, inert for invalid, T8), Health (`●` via `health_color` + word via a new pure `health_word_key(h)`, T9 word only — reason added in US3), and placeholder cells for Permissions / Resource / Actions filled in by US2–US4 (still in `crates/modplayer-ui/src/plugins_view.rs`). Invalid rows: one truncating `plugins-invalid-manifest` label over `span(Health, Resource)`, Actions empty (T16, R13) (depends on T016)
- [X] T018 [US1] Remove the legacy 8-label header, the `ui.horizontal` row code and the indented panel line from `crates/modplayer-ui/src/plugins_view.rs`; panel controls are re-homed in US4 (temporarily keep them rendering in the Actions cell so `panel_controls_listed` stays green) (depends on T017)
- [X] T019 [US1] Make T012 and T013 pass; fix overflow/alignment findings in `crates/modplayer-ui/src/plugins_view.rs` only (constants stay in `PluginsColumns`); update `crates/modplayer-ui/tests/accessibility.rs` for the `plugins-health-ok` "healthy" value and removed CPU/Memory header assertions; run `rtk cargo test -p modplayer-ui --test plugins_view --test accessibility --test design_token_literals` (depends on T018)

**Checkpoint**: US1 independently testable — aligned, non-overflowing 7-column table; MVP.

---

## Phase 4: User Story 2 - Read permissions as a list (Priority: P1)

**Goal**: Permissions as count + disclosure; expansion lists one `permission-*` explanation per line inside the row.

**Independent Test**: Expand a plugin with several permissions; one per line, inside the row and window; collapse works; 0 permissions → "0", no control.

### Tests for User Story 2 (write first, confirm FAIL)

- [X] T020 [US2] Add `permissions_count_and_disclosure`, `permissions_expand_one_per_line` (catalog order, every explanation present and unclipped, SC-005), `zero_permissions_no_disclosure`, and `expansion_survives_repaint_and_health_change` (state keyed by `PluginId`, pruned for vanished ids) in `crates/modplayer-ui/tests/plugins_view.rs` (T10, T14, T18)

### Implementation for User Story 2

- [X] T021 [US2] Implement the Permissions cell in `crates/modplayer-ui/src/plugins_view.rs`: count in mono digits; `count > 0` → disclosure button with visible text `"{count} ▸"` / `"{count} ▾"` and accessible name `plugins-permissions-show`/`-hide` (`$count`, `$plugin`) toggling `state.permissions_open`; `count == 0` → `"0"` label only (T10)
- [X] T022 [US2] Implement the in-row expansion area in `crates/modplayer-ui/src/plugins_view.rs` inside the same `row_frame`, below the cells line, from `Name.min_x` to the row's right edge: for open permissions, one wrapping `Label` per `permission-*` explanation in catalog order (T14, R9); leave a slot for the panels list (US4)
- [X] T023 [US2] Make T020 pass; run `rtk cargo test -p modplayer-ui --test plugins_view` (depends on T021, T022)

**Checkpoint**: US1 + US2 both work independently.

---

## Phase 5: User Story 3 - Health words and restart a suspended plugin (Priority: P2)

**Goal**: healthy/degraded/suspended words with theme-role colours, suspension reason and Restart in the row.

**Independent Test**: Suspend a plugin; its row shows "suspended" (danger role), the reason, and a Restart that calls `plugin_restart`.

### Tests for User Story 3 (write first, confirm FAIL)

- [X] T024 [P] [US3] Add unit tests for `health_word_key` (Ok/Warning/Suspended keys) and `suspension_reason` (`(Some(Suspended), Some(c))` → each `plugin-suspended-cause-{hang|cpu-share|memory|did-not-start}`; `(Some(Suspended), None)` → `plugins-suspended-reason-unknown`; otherwise `None`) in `crates/modplayer-ui/src/plugins_view.rs` `#[cfg(test)]` (data-model §2)
- [X] T025 [US3] Add `health_words_healthy_degraded_suspended` and `suspended_row_shows_reason_and_restart_restarts` (reason text in row, Restart present only when suspended, click calls `plugin_restart`, no Restart for healthy/degraded/auto-disabled rows) in `crates/modplayer-ui/tests/plugins_view.rs`; update the health-word assertions in `crates/modplayer-ui/tests/accessibility.rs` and keep `notification_actions_call_facade` green (T9, T13, US3-1/2, SC-003)

### Implementation for User Story 3

- [X] T026 [US3] If the `SuspendCause` → `plugin-suspended-cause-*` key mapping in `crates/modplayer-ui/src/plugin_panels.rs` is private, expose it `pub(crate)` for reuse (R6); otherwise no change
- [X] T027 [US3] Implement `suspension_reason(health, cause) -> Option<String>` in `crates/modplayer-ui/src/plugins_view.rs` using the shared mapping (makes T024 pass)
- [X] T028 [US3] Extend the Health cell in `crates/modplayer-ui/src/plugins_view.rs`: after the word on the same truncating line, append the suspension reason for Suspended rows (tooltip with full text when truncated); colour only through `health_color` (T9, FR-008)
- [X] T029 [US3] Add the Restart button in the Actions cell in `crates/modplayer-ui/src/plugins_view.rs` (`horizontal_wrapped` within the cell rect): visible `plugin-panel-restart`, accessible `plugins-restart-a11y` (`$plugin`), calls `controller.plugin_restart(row.id)`, only when `health == Some(Suspended)` (T13)
- [X] T030 [US3] Make T025 pass and keep T013 green; run `rtk cargo test -p modplayer-ui --test plugins_view --test accessibility --test high_contrast` (depends on T027, T028, T029)

**Checkpoint**: US1–US3 independently functional.

---

## Phase 6: User Story 4 - Resource use against budgets, row-owned controls (Priority: P2)

**Goal**: CPU/memory against budgets in mono digits with over-budget text; panel controls inside the row (inline for 1 panel, "Panels (N)" disclosure for ≥ 2).

**Independent Test**: An active row shows `CPU 1.2 % / 10 %` and `Mem 3.2 MB / 64 MB`; all controls sit in the row's frame; 2-panel plugin exposes one line per panel on disclosure.

### Tests for User Story 4 (write first, confirm FAIL)

- [X] T031 [P] [US4] Add unit tests for `cpu_line(row)` / `memory_line(row)` in `crates/modplayer-ui/src/plugins_view.rs` `#[cfg(test)]`: permille 120 with budget 10 → `"CPU 1.2 % / 10 %"` not over; permille 1000 → over; 3.2 MiB with 64 MiB budget → `"Mem 3.2 MB / 64 MB"`; memory ≥ budget → over; `None` → the `-none` dash strings (T11, T12)
- [X] T032 [US4] Add `resource_cell_shows_budgets_and_dashes`, `single_panel_controls_inline_in_row`, `multi_panel_disclosure` (synthetic 2-panel registry), and panel-button accessible-name assertions (`plugins-panel-*-a11y` carry plugin and panel title) in `crates/modplayer-ui/tests/plugins_view.rs`; adapt the existing `panel_controls_listed` (T11–T15)

### Implementation for User Story 4

- [X] T033 [US4] Implement `cpu_line` / `memory_line` with over-budget flag (used = `cpu_pct_of_share × cpu_budget_pct / 100`, 1 dp; budget `{:.0}`; memory `bytes / 1_048_576`) in `crates/modplayer-ui/src/plugins_view.rs`, using `plugins-resource-*` keys with `$used`/`$budget` (no "64 MB" literal) (makes T031 pass; R7)
- [X] T034 [US4] Implement the Resource cell in `crates/modplayer-ui/src/plugins_view.rs`: two right-aligned mono lines, `plugins-resource-*-none` when not Active, over-budget line appends `plugins-over-budget` and uses `roles.warning` (T11, T12)
- [X] T035 [US4] Implement panel controls in the Actions cell in `crates/modplayer-ui/src/plugins_view.rs`: 0 panels → none; 1 panel → inline Show/Hide and Enable/Disable buttons; ≥ 2 → `plugins-panels-count` disclosure (accessible `plugins-panels-show`/`-hide`) toggling `state.panels_open`. Buttons keep visible `plugin-panel-*` text, accessible names `plugins-panel-*-a11y` (`$title`, `$plugin`), semantics unchanged (`plugin_panel_show` / `plugin_panel_close` / `plugin_panel_set_disabled`) (T13, T15)
- [X] T036 [US4] Fill the panels slot of the expansion area in `crates/modplayer-ui/src/plugins_view.rs`: one line per panel (truncating title, Show/Hide, Enable/Disable), after the permissions list, inside the row frame (T14); remove the temporary panel rendering left by T018
- [X] T037 [US4] Make T032 pass; run `rtk cargo test -p modplayer-ui --test plugins_view --test accessibility` (depends on T033–T036)

**Checkpoint**: All four stories independently functional.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T038 [P] Add `tab_order_within_row` to `crates/modplayer-ui/tests/accessibility.rs`: Enabled → permissions disclosure → panel controls/disclosure → Restart → expansion-area controls, rows top→bottom, header labels not focusable (T17, FR-010)
- [X] T039 Remove now-unused keys `plugins-col-version`, `plugins-col-cpu`, `plugins-col-memory`, `plugins-cpu`, `plugins-memory`, `plugins-list-separator` from `locales/en-US/plugins.ftl` and `crates/modplayer-ui/tests/fluent_keys.rs`, only those T002 confirmed have no remaining caller (depends on T019, T037)
- [X] T040 [P] Confirm `no_uninstall_control`, `toggle_disables_in_one_action`, `notification_actions_call_facade` (T20, FR-011) and the pseudo-locale expansion (`apply_pseudo_expansion`) still fit at 960 pt in `crates/modplayer-ui/tests/plugins_view.rs`; add a pseudo-expansion fit case if absent
- [X] T041 Run all gates from quickstart.md: `rtk cargo fmt --check`; `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`; `rtk cargo test --workspace`; `rtk cargo deny check`; fix findings
- [X] T042 Execute quickstart manual scenarios M1–M10 via the macOS Quartz recipe with `MODPLAYER_PLUGIN_FIXTURES=1` (Constitution › Manual Scenario Sign-Off); record pass/deviation and screenshot path per scenario under this task in `specs/027-plugins-list-as-table/tasks.md`; log deviations in `research.md`
  - **Executed 2026-09-30: 8 PASS, 2 PASS after fix (M1 dot glyph, M3 double tooltip), 1 DEVIATION (M7 memory gauge, pre-existing).** Debug build with `RUSTUP_TOOLCHAIN=1.95.0`, `MODPLAYER_PLUGIN_FIXTURES=1`, and `MODPLAYER_PLUGIN_STATE_DIR=target/manual-walk/state`. Live sign-in came from the Keychain (no prompt). Window 960×668 outer (the minimum), later widened to 1500 pt. Dark theme. Track: "Bring Me To Life" (Evanescence). Driven by `target/manual-walk/qw.py` (CGEvent + `screencapture -l`) and `tabwalk.py` (tracks where the focus ring appears after each Tab). All evidence is in `target/manual-walk/` (gitignored).
    - **M1 PASS after fix**: 20 rows sorted by name. The fixed header sits above aligned columns. No overflow, no horizontal scrollbar, and a vertical scroll under the fixed header (`m1-960.png`, `m1-960-scrolled.png`). **Deviation, fixed**: the health `●` drew as a tofu box `□` because the glyph is absent from the app fonts. It is now painted, and `health_dot_is_painted_not_a_font_glyph` pins that (research.md › Manual walk deviations). Header labels "Enab…"/"Permi…" truncate at 960 pt and show their full text as a tooltip (T2).
    - **M2 PASS**: at 1500 pt, the extra width goes to Name/Source/Health/Actions. Reasons show in full ("it used too much memory"), panel buttons sit on one line, and alignment holds (`m2-wide.png`). No separate dock is visible in this layout.
    - **M3 PASS after fix**: a truncated name and the invalid row's reason show the full text as a tooltip, and a short name shows none (`m3-name-tooltip-fixed.png`, `m3-invalid-tooltip.png`, `m3-short-no-tooltip.png`, `m3-header-tooltip.png`). **Deviation, fixed**: two tooltips stacked because egui's own elided-label tooltip also fired. It is now off (`show_tooltip_when_elided(false)`), and the test asserts exactly one.
    - **M4 PASS**: Focus fixture A `4 ▸` → `4 ▾` lists 4 explanations one per line inside the row frame, and clicking again collapses them. Never-ready shows a plain `0` with no control (`m4-expand-a.png`, `m4-collapse-crop.png`).
    - **M5 PASS**: healthy rows show a green dot and "healthy". `throw` turned "degraded" with an amber dot once playback fed it `position` events, and `hang` showed "degraded" after repeated aborts (`m5-playing.png`, `m7-hang-20-crop.png`).
    - **M6 PASS**: `leak` → "suspended it used too much memory" + Restart + toast. Restart → "healthy", Restart gone, and its toast dismissed (`m5-playing.png`, `m6-restart-1.png`). Leak re-suspends about 2 s later while playback continues, as designed. `hang` (driven by rapid play/pause) → "suspended it stopped responding" + Restart. Restart → healthy, and the Hang toast was removed (`m6-hang-before.png`, `m6-hang-restarted.png`).
    - **M7 DEVIATION (pre-existing)**: CPU lines are mono, right-aligned, and aligned across rows (e.g. `CPU 2.0 % / 10 %`, `CPU 4.1 % / 10 %`), and non-active rows show `CPU —`/`Mem —`. But every `Mem` reads `0.0 MB / 64 MB`, including `leak`: `PluginGauges::set_used_bytes` has no caller in the runtime (since 009). That is out of 027's no-runtime-change scope and needs a follow-up issue. "over budget" was not caught on screen (`hang` went from degraded to suspended between repaints). Unit tests cover it (T12).
    - **M8 PASS**: Key & Tempo shows Hide/Disable in the Actions cell, wrapped inside the cell at 960 pt and inline at 1500 pt. Hide → Show, Disable → Enable, and both were restored (`m8-hidden-crop.png`, `m8-disabled-crop.png`, `m8-restored.png`).
    - **M9 PASS**: Focus A and Focus B stayed expanded for 4 s of 500 ms repaints (`m9-two-expanded.png`). Expansion also persisted through the relaunch-free health changes during M5/M6.
    - **M10 PASS**: Tab order is nav → notifications → per row Enabled → permissions → panel Hide → Disable → Restart, rows top to bottom. Never-ready goes Enabled → Restart, and the invalid row's inert switch is skipped (`tabwalk.py` log). Shift+Tab reverses. Space/Enter operate switches, disclosures and buttons (`m10-space-crop.png`, `m10-enter-crop.png`, `m10-perm-space2-crop.png`). In high contrast the focus ring stays visible, the words carry state, and buttons are outlined (`m10-hc.png`, `m10-hc-row-crop.png`). High contrast was turned back off (`hc-off-crop.png`).
    - Gates after fixes: `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace` (2293 passed, 11 ignored), and `cargo deny check` all green.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (1)**: none. **Foundational (2)**: after Setup; blocks all stories.
- **US1 (3)**: after Foundational. Supplies table skeleton, columns and row frame.
- **US2, US3, US4**: after Foundational AND US1 skeleton (T016–T018), because all fill cells in `plugins_view.rs`. Their tests and pure fns (T020, T024, T031) can be written earlier.
- **Polish (7)**: after all desired stories.

### Story dependencies

- US1 → none beyond Foundational. US2 needs US1 row/cell placement. US3 needs US1 Health cell and Actions cell. US4 needs US1 Actions cell and US2's expansion area (T022). US3 and US4 both edit the Actions cell → do sequentially (US3 then US4) or coordinate.

### Within a story

Tests first (fail) → pure fns → rendering → make tests pass.

### Parallel Opportunities

- Setup: T002, T003.
- Foundational: T006 ‖ T007 (and T004 ‖ T006/T007, different files).
- US1: T011 (src unit tests) ‖ T012/T013 (tests file) once T010 is done; T011 and T014 share a file so stay ordered.
- US3: T024 ‖ T025 (different files). US4: T031 ‖ T032 (different files).
- Polish: T038 ‖ T040.

### Parallel example: Foundational

```text
Task: "T004 Write failing unit tests in crates/modplayer-core/src/plugins/view.rs"
Task: "T006 Edit locales/en-US/plugins.ftl"
Task: "T007 Create locales/pt-BR/plugins.ftl"
```

---

## Implementation Strategy

### MVP first (US1)

1. Setup → Foundational → US1 (T011–T019).
2. STOP and validate: 17 fixtures at 960 and 560 pt, header aligned, no overflow (SC-001/SC-002).

### Incremental delivery

1. Add US2 (permissions) → test. 2. Add US3 (health reason + Restart) → test. 3. Add US4 (resource budgets + panel controls) → test. 4. Polish, gates, manual M1–M10.

### Notes

- Core change is limited to three `PluginRow` fields; no runtime, gateway or plugin API change (no GOV-3.2 sign-off).
- No new dependency (R1 rejects `egui_extras`).
- Commit after each task or logical group.

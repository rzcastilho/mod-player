---

description: "Task list for 015-control-variants"

---

# Tasks: Button, Toggle, and Meter Variants

**Input**: Design documents from `/specs/015-control-variants/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included. Constitution Principle VIII (Test What the NFRs Promise) and this plan's Design Note 1 require every suite to land **red** against `data-model.md`'s values before the module and call-site conversions that satisfy it — 014's Phase 2 inverted this once and had to record the deviation; do not repeat it here.

**Organization**: Tasks are grouped by user story (US1–US5, priorities P1/P1/P2/P2/P3 per spec.md) after a Setup phase and a Foundational phase. The Foundational phase carries almost all of this feature's *values* (data-model.md §2–§7): `theme::controls`, the re-differentiated `Style` slots, and the three host widgets (`button`, `switch`, `row_frame`) that every story's call-site edits depend on. Each user story phase is then a set of **call-site line edits** (plan.md §"The call-site payload is small and fully enumerated") plus the integration tests that pin that story's rules.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no unresolved same-file dependency)
- **[Story]**: US1–US5, per spec.md's user stories. Setup/Foundational/Polish carry no story label.
- Every task names its exact file path(s) and the contract rule id(s) (B/S/A/L, I/F/N/Y, M/K/R) it writes toward.

## Path Conventions

Single crate, `crates/modplayer-ui/`. All source paths are relative to the repository root. No other crate is touched (plan.md § Project Structure).

---

## Phase 1: Setup

**Purpose**: Create the two new files this feature owns, with no logic yet, so Phase 2's tests have somewhere to fail from.

- [X] T001 Create `crates/modplayer-ui/src/theme/controls.rs`: SPDX header, module doc pointing at data-model.md §2–§7, no public items yet (Constitution VII)
- [X] T002 [P] Create `crates/modplayer-ui/src/widgets/controls.rs`: SPDX header, module doc pointing at data-model.md §7, no public items yet (Constitution VII)
- [X] T003 Wire `pub mod controls;` + re-exports into `crates/modplayer-ui/src/theme/mod.rs` and `crates/modplayer-ui/src/widgets/mod.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The presentation value set (data-model.md §2–§6) and the three host widgets (§7) that every user story's call sites consume. **No user story call-site edit can compile until this phase is done.**

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Tests (write first — must fail red before Implementation below)

- [X] T004 Unit tests for `Variant`/`VariantPaint`/`variant_paint()` matching data-model.md §2's table exactly (B1, B2) in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T005 Unit tests for `hover_fill`/`pressed_fill`/`focus_ring` derivations (I1, I2, F1, F2) in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T006 Unit tests for switch metrics, the off/on state table, and the thumb-position delta (S1, S2, S3) in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T007 Unit test asserting `DESTRUCTIVE_GAP >= 2 * style.spacing.item_spacing.x` (B9) in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T008 Unit tests for `band()`'s fixed-order selection, including a boundary value and a danger boundary at or below −6 dBFS (M1–M5) in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T009 Unit test for `mark_color`'s fill/track rule (K3) in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T010 [P] Extend the widget-slot test module for the re-differentiation (I4) and assert 014's `no_geometry_or_interaction_field_changes` still passes **verbatim** (V6, Y1) in `crates/modplayer-ui/src/theme/style.rs`
- [X] T011 [P] Write behaviour tests: `button()`/`switch()` resolve state from `Response`'s own fields, **never** `widget_state()` (I7); each `SwitchKind` emits the right `WidgetInfo` (A1) in `crates/modplayer-ui/src/widgets/controls.rs`
- [X] T012 [P] Create `crates/modplayer-ui/tests/control_variants.rs` — the module-level integration suite, written **before** the values and widgets it pins (T013–T025), with: four variants pairwise-distinct triples (B3), variant colours come from roles (B4), a switch is not mistakable for any button variant (S4), the widget module uses only token values — no literal (L1), disabled controls show no hover/focus/pressed feedback (A5, F6, FR-020)

**Red gate**: T004–T012 must be run and observed **failing** — against the empty `theme/controls.rs` and `widgets/controls.rs` from Phase 1 — before any task below is started. A suite that compiles green here has not pinned anything (Constitution VIII; plan.md design note 1).

### Implementation (makes T004–T012 pass)

- [X] T013 Implement `Variant` enum + `VariantPaint` struct + `variant_paint()` per data-model.md §2 in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T014 Implement `hover_fill()`/`pressed_fill()`/`focus_ring()` + `FOCUS_RING_WIDTH`/`FOCUS_RING_GAP` per data-model.md §3 in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T015 Implement `SwitchMetrics`/`switch_metrics()`/`switch_track()`/`switch_thumb()` + `SWITCH_OUTLINE_WIDTH` per data-model.md §5 in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T016 Implement `DESTRUCTIVE_GAP` constant per data-model.md §7 in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T017 Implement `Band` enum, `BAND_WARNING_DB`, `band()`, `band_color()` per data-model.md §6 in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T018 Implement `mark_color()`, `SCALE_MARK_WIDTH`, `CEILING_MARK_WIDTH` per data-model.md §6 in `crates/modplayer-ui/src/theme/controls.rs`
- [X] T019 [P] Re-differentiate the five `Visuals::widgets` slots inside `build_style`'s single construction site per data-model.md §4 (FR-011a, design note 4 "one construction site") in `crates/modplayer-ui/src/theme/style.rs`

### Host widgets (depend on T013–T019; turn T011/T012 green)

- [X] T020 Implement `button(ui, variant, text)` using `next_auto_id()` + `read_response` from last pass (research R4), compositing the overlay into the fill it hands `egui::Button` in `crates/modplayer-ui/src/widgets/controls.rs`
- [X] T021 Implement `SwitchKind` enum + `switch(ui, kind, on, label)`, emitting `WidgetInfo::selected(WidgetType::Checkbox | SelectableLabel, …)` per `kind` (data-model.md §8) in `crates/modplayer-ui/src/widgets/controls.rs`
- [X] T022 Implement `row_frame()`: reserve a shape index before content, `set` it after — zero layout change (research R5) in `crates/modplayer-ui/src/widgets/controls.rs`
- [X] T023 Implement `destructive_gap()` (`ui.add_space(DESTRUCTIVE_GAP)`) in `crates/modplayer-ui/src/widgets/controls.rs`
- [X] T024 Implement `paint_focus_ring(ctx)`: read `Memory::focused()` + `Context::read_response`, stroke one ring into a foreground layer, return silently when nothing is focused or the widget isn't visible (F1, F2, F4, F5) in `crates/modplayer-ui/src/widgets/controls.rs`
- [X] T025 [P] Wire `widgets::controls::paint_focus_ring(ui.ctx())` as the **last statement** of `App::ui` in `crates/modplayer-ui/src/app.rs`

**Checkpoint**: `theme::controls`, the re-differentiated `Style`, the three host widgets, and the focus-ring pass all compile and pass their own tests. Every user story below is now just call-site edits.

---

## Phase 3: User Story 1 - A destructive action is unmistakable before the click (Priority: P1) 🎯 MVP

**Goal**: Every FR-003 destructive site (Clear all markers + Yes, Effect Chain Remove, both plugin-panel-disable sites, Sign out + its modal) renders the `destructive` variant and sits ≥16 px from its nearest non-destructive neighbour.

**Independent Test**: Display the Markers panel header, the Effect Chain row, a plugin's panel row, or Settings › Account, and confirm the destructive button is visually distinct (danger outline + danger label) from every other button in that screen and separated from its nearest neighbour by the `lg` gap.

### Tests for User Story 1 (write first — must fail red)

- [X] T026 [US1] Add `destructive_sites_are_exactly_fr003` (B6) to `crates/modplayer-ui/tests/control_variants.rs`
- [X] T027 [US1] Add `destructive_gap_at_named_instances` (B10) to `crates/modplayer-ui/tests/control_variants.rs`

### Implementation for User Story 1

- [X] T028 [US1] Convert `markers-clear-all`→destructive + gap before it, `markers-clear-yes`→destructive + gap before `markers-clear-no` (:843, :853) in `crates/modplayer-ui/src/markers.rs`
- [X] T029 [P] [US1] Convert `effects-remove`→destructive + gap before it (:163) in `crates/modplayer-ui/src/effects_view.rs`
- [X] T030 [P] [US1] Convert `plugin-panel-disable`→destructive + gap, only while the label reads "Disable" (:161) in `crates/modplayer-ui/src/plugins_view.rs`
- [X] T031 [P] [US1] Convert `plugin-panel-disable`→destructive + gap, only while the label reads "Disable" (:291, the dock's own site — plan.md D4) in `crates/modplayer-ui/src/plugin_panels.rs`
- [X] T032 [P] [US1] Convert `account-sign-out`→destructive (:60) and the `signout-confirm` modal's confirming control→destructive, **removing** its existing `error_fg_color` call-site colour (:146) in `crates/modplayer-ui/src/settings/account.rs`

**Checkpoint**: `tests/control_variants.rs`'s destructive rules (B6, B10) pass; every destructive action in the app is visually distinct and gapped. This is the MVP.

---

## Phase 4: User Story 2 - Toggles look like toggles, not labels that happen to highlight (Priority: P1)

**Goal**: Every persistent boolean on/off control app-wide (FR-008 + FR-008a) renders the switch; every one-of-N selection control (FR-008b) is left unconverted.

**Independent Test**: Toggle Queue/Effects/Transport, a plugin's Enabled checkbox, and an Effect Chain Bypass control, and confirm each on-state renders the switch's pill-plus-thumb visual, distinguishable from a highlighted button.

### Tests for User Story 2 (write first — must fail red)

- [X] T033 [US2] Create `crates/modplayer-ui/tests/control_inventory.rs` with `every_boolean_control_is_a_switch` (S5) and `no_selection_control_became_a_switch` (S6) — the source-level inventory (SC-010)

### Implementation for User Story 2 — FR-008 acceptance set

- [X] T034 [US2] Convert the Queue/Effects/Transport disclosure controls→switch (`SwitchKind::Toggle`, :145/:152/:159) in `crates/modplayer-ui/src/now_playing.rs`
- [X] T035 [P] [US2] Convert the plugin Enabled checkbox→switch (`SwitchKind::Checkbox`, :183) in `crates/modplayer-ui/src/plugins_view.rs`
- [X] T036 [P] [US2] Convert `effects-bypass`→switch (`SwitchKind::Toggle`, :140) in `crates/modplayer-ui/src/effects_view.rs`

### Implementation for User Story 2 — FR-008a app-wide boolean set

- [X] T037 [US2] Convert Formant/Mute/Mono-sum/Phase-invert/Channel-swap `toggle_value`s→switch (:290/:343/:549/:558/:567) in `crates/modplayer-ui/src/effects_view.rs` (same file as T036 — sequential)
- [X] T038 [P] [US2] Convert Shuffle→switch (`SwitchKind::Toggle`, :27) in `crates/modplayer-ui/src/queue_view.rs` — data-model.md §8 and `accessibility.rs::queue_shuffle_toggle_reports_its_toggled_state` (unmodified) pin `Role::Button` for `queue-shuffle` (it renders via `selectable_label` today, design note 9); `SwitchKind::Checkbox` as literally written in this task line would regress that pinned role, so `Toggle` is used instead
- [X] T039 [P] [US2] Convert the loop-arm `Checkbox`→switch (:757) in `crates/modplayer-ui/src/markers.rs` (same file as T028 — sequential)
- [X] T040 [P] [US2] Convert the safe-volume `Checkbox`→switch (:166) in `crates/modplayer-ui/src/settings/audio.rs`
- [X] T041 [P] [US2] Convert the boolean plugin-setting fields→switch (:167) in `crates/modplayer-ui/src/settings/plugins.rs`
- [X] T042 [US2] Convert the plugin-contributed checkbox host line→switch (:363) in `crates/modplayer-ui/src/plugin_panels.rs` (same file as T031 — sequential)

### Verification for User Story 2

- [X] T043 [US2] Regenerate and diff `modplayer-capability-gateway`'s API reference; confirm **zero diff** against `crates/modplayer-capability-gateway/api/v1.toml` and `docs/plugin-api/v1.md` (S7, Principle IX untriggered)

**Checkpoint**: `tests/control_inventory.rs` (S5, S6) passes — every named boolean is a switch, every named selection control is unconverted; `api_reference.rs` shows no diff (S7).

---

## Phase 5: User Story 3 - Interactive rows and controls give feedback (Priority: P2)

**Goal**: Every interactive row (queue, plugin, marker, search/library/detail) gets the hover fill through `row_frame`; the focus ring and the selection indicator stay separately identifiable everywhere.

**Independent Test**: Rest the pointer on a list row and a toggle and confirm the surface visibly changes in both; move keyboard focus onto a control that is also the current selection and confirm the ring and the selection remain two separate signals; press and hold a button or toggle and confirm a distinct pressed appearance.

### Tests for User Story 3 (write first — must fail red)

- [X] T044 [US3] Create `crates/modplayer-ui/tests/interaction_states.rs` with `three_states_are_three_increasing_fills` (I3), `row_frame_adds_no_layout` (I6), `host_controls_do_not_use_widget_state` (I7), `focus_ring_is_painted_once_app_wide` (F4), `no_ring_without_a_visible_focus` (F5), `ring_and_selection_do_not_overlap` (F3)

### Implementation for User Story 3

- [X] T045 [US3] Wire row hover via `row_frame()` into `crates/modplayer-ui/src/rows.rs` (:502-510), reusing the row's existing rect + response
- [X] T046 [P] [US3] Wire row hover via `row_frame()` into the queue rows in `crates/modplayer-ui/src/queue_view.rs` (same file as T038 — sequential)
- [X] T047 [P] [US3] Wire row hover via `row_frame()` into the plugin rows in `crates/modplayer-ui/src/plugins_view.rs` (same file as T030/T035 — sequential)
- [X] T048 [P] [US3] Wire row hover via `row_frame()` into the marker rows in `crates/modplayer-ui/src/markers.rs` (same file as T028/T039 — sequential)

**Checkpoint**: `tests/interaction_states.rs` passes; hover/focus/pressed feedback (built entirely on Phase 2's foundational plumbing) is live on every interactive row and control app-wide.

---

## Phase 6: User Story 4 - Meters communicate level state before it becomes a problem (Priority: P2)

**Goal**: The Now Playing peak meter and the Effect Chain's pre-/post-chain level pair render `positive`/`warning`/`danger` segmented bands with −6/0 dB scale marks, and the level pair's RMS sub-bar stops being dimmed.

**Independent Test**: Drive a peak meter's level from silence past its ceiling and confirm the fill progresses positive → warning → danger with both scale marks visible, and that its readout stays in `mono`.

### Tests for User Story 4 (write first — must fail red)

- [X] T049 [US4] Create `crates/modplayer-ui/tests/meter_bands.rs` with `fill_is_segmented_by_db_position` (M6), `rightmost_column_is_danger_over_the_boundary` (M7), `each_meter_uses_its_own_boundary` (M8), `rms_bands_and_is_not_dimmed` (M9), `both_meters_draw_both_marks` (K1), `ceiling_tick_is_two_px` (K2), `ceiling_tick_is_not_warn_fg` (K5), `zero_db_mark_is_inset` (K6), `mark_positions` (K7)

### Implementation for User Story 4

- [X] T050 [US4] Segment the peak meter's fill by band, add the −6/0 dB marks (1 px) plus the ceiling tick (2 px) via `mark_color`, and **remove** the `warn_fg_color` ceiling tick (FR-012, FR-013, K5) in `crates/modplayer-ui/src/widgets/peak_meter.rs`
- [X] T051 [P] [US4] Segment `level_pair`'s peak **and** RMS sub-bars by band with a fixed 0 dBFS boundary, add both scale marks, **remove** the `gamma_multiply(0.7)` RMS dim, and stop reading `selection.bg_fill` on either sub-bar (FR-012, FR-012a) in `crates/modplayer-ui/src/widgets/chain_meters.rs`

**Checkpoint**: `tests/meter_bands.rs` passes; both meters band correctly and keep their `mono` readouts and accessible values unchanged (R1, R2 — guarded by the existing, unmodified suites).

---

## Phase 7: User Story 5 - Primary and quiet actions stay out of each other's way (Priority: P3)

**Goal**: The Welcome screen's "I understand, continue" is the app's one `primary` button; the Queue panel's four row actions render `quiet`.

**Independent Test**: Display the Welcome screen and confirm exactly one primary button; display the Queue panel and confirm Move up/Move down/Play next/Remove all render quiet.

### Tests for User Story 5 (write first — must fail red)

- [ ] T052 [US5] Add `welcome_has_exactly_one_primary` (B5) and `queue_row_actions_are_quiet` (B7) to `crates/modplayer-ui/tests/control_variants.rs`

### Implementation for User Story 5

- [ ] T053 [US5] Convert `welcome-acknowledge`→primary (:108) in `crates/modplayer-ui/src/welcome.rs`
- [ ] T054 [P] [US5] Convert `queue-move-up`/`-move-down`/`-play-next`/`-remove`→quiet (:84/:87/:90/:93); `queue-remove` stays **quiet**, not destructive (FR-004) in `crates/modplayer-ui/src/queue_view.rs` (same file as T038/T046 — sequential)

**Checkpoint**: All five user stories pass independently. `tests/control_variants.rs` is fully green (B1–B11, S4, A5, L1).

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Confirm nothing this feature touched regressed, and close out Governance's manual sign-off.

- [ ] T055 [P] Run the unmodified regression net and confirm every suite still passes: `accessibility.rs`, `fluent_keys.rs`, `design_token_literals.rs` (0 hits — SC-007), `design_token_contrast.rs`, `design_token_roles.rs` (SC-006), `plugins_view.rs`, `plugin_panels.rs`, `settings_plugins.rs`, `queue_view.rs`, `effects_view.rs`, `markers.rs`, `rows.rs`, `now_playing.rs`, `controls.rs`, `actions.rs` (A2, SC-008)
- [ ] T056 [P] `RUSTUP_TOOLCHAIN=1.95.0 cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo deny check`
- [ ] T057 [P] `scripts/check-license-headers.sh` — confirm SPDX headers on `crates/modplayer-ui/src/theme/controls.rs` and `crates/modplayer-ui/src/widgets/controls.rs`
- [ ] T058 Execute quickstart.md's manual scenarios M1–M10 (Governance › Manual Scenario Sign-Off): locate the window, drive with `CGEventPost`, capture with `screencapture`, sample with `target/manual-walk/pixel.py`; record pass/deviation with evidence on this task; verify `LaunchStep::Main` is reached before M2–M6/M8–M10, and if the sign-in gate blocks any, record it **not executed** with the reason — never fabricate a signed-in state
- [ ] T059 If any manual scenario was recorded not-executed, write the deviation back into `quickstart.md` and `research.md`, and note it in `plan.md` § Complexity Tracking, per Governance

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. **BLOCKS every user story** — `theme::controls`, the re-differentiated `Style`, and the three host widgets are what every call-site edit calls.
- **User Stories (Phases 3–7)**: All depend on Foundational. Priority order is P1 (US1, US2) → P2 (US3, US4) → P3 (US5); US1 is the MVP.
- **Polish (Phase 8)**: Depends on every user story phase being complete.

### User Story Dependencies

- **US1 (P1)**: Foundational only. No dependency on US2–US5.
- **US2 (P1)**: Foundational only. Independent of US1, but `plugin_panels.rs` (T031) and `markers.rs`/`queue_view.rs`/`effects_view.rs` are touched by more than one story — see **Same-file notes** below.
- **US3 (P2)**: Foundational only (uses `row_frame` from T022). Touches `markers.rs`, `queue_view.rs`, `plugins_view.rs` after US1/US2 if run in priority order.
- **US4 (P2)**: Foundational only. Fully isolated to `widgets/peak_meter.rs` and `widgets/chain_meters.rs` — no file overlap with any other story.
- **US5 (P3)**: Foundational only. `queue_view.rs` overlaps US2 (T038) and US3 (T046).

### Same-file notes (run sequentially, not in parallel, across stories if interleaved)

- `markers.rs`: T028 (US1) → T039 (US2) → T048 (US3)
- `queue_view.rs`: T038 (US2) → T046 (US3) → T054 (US5)
- `plugins_view.rs`: T030 (US1) → T035 (US2) → T047 (US3)
- `plugin_panels.rs`: T031 (US1) → T042 (US2)
- `effects_view.rs`: T029 (US1) → T036 (US2) → T037 (US2)

If stories are implemented strictly in priority order (US1 → US2 → US3 → US4 → US5, the recommended path), these resolve themselves naturally — no rebasing needed.

### Within Each User Story

- Tests are written and confirmed **red** before implementation (Constitution VIII).
- Call-site edits within a story are independent of each other except where the same-file notes above apply.

### Parallel Opportunities

- T001/T002 (Setup) — different files.
- T004–T009 are all the same file (`theme/controls.rs`) and are **not parallel** with each other; T010 (`style.rs`), T011 (`widgets/controls.rs`) and T012 (`tests/control_variants.rs`) are each a different file and parallel with all of them and with each other.
- T013–T018 are the same file and **not parallel** with each other; T019 (`style.rs`) is parallel.
- T020–T024 are the same file and **not parallel** with each other (and all follow T011's tests in that file); T025 (`app.rs`) is parallel.
- Within US1: T029–T032 are four different files and fully parallel; T028 is a fifth, independent file.
- Within US2: T035/T036/T038/T040/T041 are five different files and parallel with each other; T037 must follow T036 (same file); T042 must follow T031 from US1 (same file).
- Within US3: T046/T047/T048 are parallel with each other but each must follow that file's earlier story task (see same-file notes); T045 (`rows.rs`) is untouched elsewhere and fully independent.
- Within US4: T050 and T051 are two different files and fully parallel — this story has no cross-story file overlap at all.
- Phase 8: T055/T056/T057 are independent verification commands and parallel; T058 depends on the whole feature being built; T059 depends on T058's outcome.

---

## Parallel Example: User Story 1

```bash
# After T026-T027 (tests, red) and Foundational are done, launch the four independent call-site edits together:
Task: "Convert effects-remove -> destructive + gap (:163) in crates/modplayer-ui/src/effects_view.rs"
Task: "Convert plugin-panel-disable -> destructive + gap (:161) in crates/modplayer-ui/src/plugins_view.rs"
Task: "Convert plugin-panel-disable -> destructive + gap (:291) in crates/modplayer-ui/src/plugin_panels.rs"
Task: "Convert account-sign-out + signout-confirm -> destructive (:60, :146) in crates/modplayer-ui/src/settings/account.rs"
# T028 (markers.rs) runs independently alongside these four.
```

## Parallel Example: User Story 4 (fully isolated)

```bash
Task: "Segment peak meter fill + marks in crates/modplayer-ui/src/widgets/peak_meter.rs"
Task: "Segment level_pair fill + marks, remove RMS dim in crates/modplayer-ui/src/widgets/chain_meters.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational — `theme::controls`, re-differentiated `Style`, the three host widgets, the focus-ring pass. **This is most of the feature's real work**; every story after it is call-site edits.
3. Complete Phase 3: User Story 1.
4. **STOP and VALIDATE**: run `tests/control_variants.rs`'s B6/B10, confirm every destructive site is gapped and distinct.
5. This is shippable: the app's one safety-relevant gap (`UX-04`) is closed.

### Incremental Delivery

1. Setup + Foundational → foundation ready (module + widgets pass their own tests).
2. Add US1 → validate → MVP (destructive actions unmistakable).
3. Add US2 → validate (every boolean is a switch).
4. Add US3 → validate (hover/focus/pressed live app-wide).
5. Add US4 → validate (meters band and mark correctly) — fully isolated, can run any time after Foundational.
6. Add US5 → validate (primary/quiet hierarchy) — closes out `tests/control_variants.rs`.
7. Phase 8: regression net, gates, manual sign-off.

### Parallel Team Strategy

Once Foundational is done: Developer A takes US1, Developer B takes US4 (fully isolated, no file overlap with anything). US2, US3, US5 share `markers.rs`/`queue_view.rs`/`plugins_view.rs`/`plugin_panels.rs` with US1 and each other — sequence per the **Same-file notes** above if split across people, or keep one owner per file until all its stories land.

---

## Notes

- `Default` is the do-nothing variant (design note 3): **no task touches an FR-005 call site.** The ~60 buttons not named in FR-002–FR-004 already render `default` via `recolor_widget` and stay untouched.
- Every number this feature introduces lives in `theme/controls.rs` (L1, L2); if an implementation task finds itself typing a number into `widgets/controls.rs`, a view file, or a meter, the number belongs in `theme/controls.rs` instead (design note 2).
- `next_auto_id()` and the widget it names must stay adjacent (research R4) — do not insert an allocation between them in T020/T021.
- No task introduces animation, easing, or a transition duration (FR-021, Y1).
- Commit after each task or logical group; stop at any checkpoint to validate that story independently.
- T058/T059 (manual scenarios) are the implementing agent's own responsibility per Governance — not automatable, not skippable, and never satisfied by automated evidence alone.

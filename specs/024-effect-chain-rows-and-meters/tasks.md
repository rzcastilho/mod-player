---

description: "Task list for Effect Chain Rows and Meters (024)"
---

# Tasks: Effect Chain Rows and Meters

**Input**: Design documents from `/specs/024-effect-chain-rows-and-meters/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Included — plan.md and quickstart.md are test-first (Constitution VIII); write each area's test(s) before the implementation that makes them pass.

**Organization**: Phase 3 = US1 (P1, rows), Phase 4 = US2 (P2, header/meters/spectrum), Phase 5 = US3 (P3, empty state). This feature is presentation-only (FR-014): no new crate, module, or data model — all work is in `crates/modplayer-ui/src/effects_view.rs`, `crates/modplayer-ui/src/widgets/chain_meters.rs`, their `tests/`, `locales/en-US/effects.ftl`, and a new `locales/pt-BR/effects.ftl` (FR-015, NFR-7.1).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on an incomplete task)
- **[Story]**: US1 / US2 / US3 — omitted for Setup, Foundational and Polish
- Every task names its exact file path

## Path Conventions

Single Cargo workspace, single crate (`modplayer-ui`):
- Source: `crates/modplayer-ui/src/effects_view.rs`, `crates/modplayer-ui/src/widgets/chain_meters.rs`
- Tests: `crates/modplayer-ui/tests/effects_view.rs`, `tests/accessibility.rs`, `tests/meter_bands.rs`, `tests/fluent_keys.rs`
- Strings: `locales/en-US/effects.ftl`, `locales/pt-BR/effects.ftl` (new)

---

## Phase 1: Setup

**Purpose**: Confirm the toolchain and baseline before any change.

- [X] T001 Run `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui` on a clean checkout to record the pre-change baseline (all green) before touching `effects_view.rs` / `chain_meters.rs` — no file changes, just a recorded baseline for the FR-014 "no behaviour change" regression check in Polish. **Baseline (2026-09-29): 728 passed, 39 suites, 0 failed.**

**Checkpoint**: Baseline green; safe to start Foundational.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The Fluent keys every story's UI text depends on, added first per plan.md's suggested order ("(1) Fluent keys + `fluent_keys.rs`").

**⚠️ CRITICAL**: US1/US2/US3 all reference keys added here (handle name, budget labels, empty-state text, spectrum labels). Complete before any story's implementation task (tests that assert on key *values* may be written in parallel, see below).

- [X] T002 Update changed key **values** (ids unchanged) in `locales/en-US/effects.ftl`: `effects-chain-cpu` → `Chain CPU: { $pct } % of real-time budget`, `effects-overloads` → `Budget overruns: { $count }`, `effects-cpu` → `{ $pct } % of budget` (contracts/fluent-strings.md "Changed values") — implements FR-6.2.5, NFR-7.1 (spec FR-006)
- [X] T003 Add new keys to `locales/en-US/effects.ftl`: `effects-chain-cpu-hint`, `effects-reorder-handle-node` (`$kind`, `$position`), `effects-reorder-handle-hint`, `effects-empty-explanation`, `effects-empty-add`, `effects-spectrum-tick-100`, `effects-spectrum-tick-1k`, `effects-spectrum-tick-10k`, `effects-spectrum-ref-0db`, `effects-spectrum-ref-minus30`, `effects-spectrum-ref-minus60` (contracts/fluent-strings.md "New keys"); then create `locales/pt-BR/effects.ftl` with pt-BR values for the same 11 new keys and the 3 changed keys (same ids and placeholders) — implements FR-6.2.2, FR-6.2.5, FR-6.4.2, NFR-6.2, NFR-7.1 (spec FR-006, FR-008, FR-010, FR-013, FR-015) — depends on T002 (same file)
- [X] T004 Update `crates/modplayer-ui/tests/fluent_keys.rs`: move `effects-chain-cpu`/`effects-overloads`/`effects-cpu` into (or confirm) the `pct`/`count`-arg exhaustiveness lists, and add the 11 new keys to their matching list (plain / `pct`-arg / `count`-arg / new `kind`+`position`-arg list), per contracts/fluent-strings.md; add a parity test (via the existing `defined_keys` helper over `include_str!`) asserting every one of this feature's 14 new/changed keys is defined in both `locales/en-US/effects.ftl` and `locales/pt-BR/effects.ftl` — implements NFR-7.1 (spec FR-015) — depends on T003

**Checkpoint**: `rtk cargo test -p modplayer-ui --test fluent_keys` passes with the new/changed keys recognized; string surface ready for all three stories.

---

## Phase 3: User Story 1 - A node's row reads as four things, not one run-on line (Priority: P1) 🎯 MVP

**Goal**: Each row renders as four visually distinct, atomic zones (identity, state, parameters, actions) in that tab order, with a visibly draggable, uniquely-named handle, unit suffixes inside value boxes, and a destructive-gap before Remove — no change to row *behavior* (FR-001–FR-005, FR-013 handle/focus parts, FR-016 wrap rule).

**Independent Test**: Add two nodes of different kinds, open the panel, confirm identity/state/parameters/actions are four separate visual groups per row (quickstart §1 `rows_render_four_zones_in_order`).

### Tests for User Story 1 ⚠️ write first, confirm they fail against the current single-line row

- [X] T005 [P] [US1] Unit tests for `zone_breaks` (first-frame fallback to Parameters break, fit-based breaks, zone-0-never-breaks invariant) in `crates/modplayer-ui/src/effects_view.rs` (data-model §4.1, contract R1.2/R1.3)
- [X] T006 [P] [US1] Integration test `rows_render_four_zones_in_order` in `crates/modplayer-ui/tests/effects_view.rs`: with two different-kind nodes, assert handle < Bypass < first parameter control < Remove in x/y AccessKit order, and that auto-bypassed/mode notes render after Bypass (state zone), not after the first parameter (FR-001, SC-001)
- [X] T007 [P] [US1] Integration test `unit_suffix_inside_value_for_all_six_kinds` in `crates/modplayer-ui/tests/effects_view.rs`: for each of the six node kinds, assert every unit-bearing control's rendered value text carries its suffix inside the control (e.g. pitch shift `+2` → `"2.00 st"`) and unitless params (Q, resonance, width, balance) are unchanged (FR-002, SC-002)
- [X] T008 [P] [US1] Integration test `handle_name_identifies_node_and_position` in `crates/modplayer-ui/tests/effects_view.rs`: handle AccessKit name equals `tr_args("effects-reorder-handle-node", kind, position)` = `"Reorder {kind}, position {n}"`, and starts with `tr("effects-reorder-handle")` (FR-013, contract R2.4)
- [X] T009 [US1] Update `handle_names_update_after_reorder`, `drag_drop_reorders_and_calls_move_to`, `arrow_up_on_focused_handle_moves_node_and_keeps_focus` in `crates/modplayer-ui/tests/effects_view.rs` to look up handles by the `"Reorder"` name *prefix* instead of an exact match (they now carry kind+position), and assert names update in the frame after a reorder (FR-004, FR-013, SC-003) — touches the same file as T006–T008, run after them
- [X] T010 [P] [US1] Integration test `tab_order_runs_identity_state_params_actions` in `crates/modplayer-ui/tests/effects_view.rs`: focus traversal within one row visits handle → Bypass → parameter controls (existing order) → Remove → next row's handle (FR-013, contract R3, SC-007)
- [X] T011 [P] [US1] Integration test `rows_fit_panel_at_960_px` in `crates/modplayer-ui/tests/effects_view.rs`: at a 960×640 window with several nodes (including one with a long kind/owner label), assert no two control rects overlap and every rect lies inside the panel card's rect (FR-016, contract R5)

### Implementation for User Story 1

- [X] T012 [US1] Implement `zone_breaks(available: f32, widths: [Option<f32>; 4], gap: f32) -> [bool; 4]` as a pure helper in `crates/modplayer-ui/src/effects_view.rs` per data-model §4.1 (first-frame None ⇒ Parameters breaks; fit rule; zone 0 never breaks) — makes T005 pass — implements FR-6.2.2, NFR-6.1 (spec FR-001, FR-016)
- [X] T013 [US1] Add the `effects-zone-width` per-node-per-zone temp-memory id (`Id::new("effects-zone-width").with((node_id, zone_idx))`) to `crates/modplayer-ui/src/effects_view.rs`: write each zone's measured width after drawing it, read last frame's widths to feed `zone_breaks` (data-model §3) — implements FR-6.2.2 (spec FR-016)
- [X] T014 [US1] Restructure `show_row` in `crates/modplayer-ui/src/effects_view.rs` into the four zones in order — Identity (handle, `"{index+1}."`, kind label, owner label), State (Bypass toggle, `effects-cpu` mono figure, `effects-auto-bypassed`/`effects-mode-note` notes moved here), Parameters (existing per-kind controls unchanged), Actions (`destructive_gap` + Remove) — using `zone_breaks` for line placement, a `ui.separator()` + `theme::space::LG` between zones on the same line, and right-aligning Actions when it shares a line with Parameters (contract R1, R1.1–R1.5) — depends on T012, T013 — implements FR-6.2.2, FR-6.2.5, NFR-6.1 (spec FR-001, FR-002, FR-005, FR-013)
- [X] T015 [US1] Truncate the identity zone's kind/owner labels to a max width of 180 px with full text on hover when elided, in `crates/modplayer-ui/src/effects_view.rs` (contract R1.6, research R10) — depends on T014 — implements FR-6.2.2 (spec FR-016)
- [X] T016 [US1] Keep the Parameters zone's separator slot allocated even when a kind has no controls, in `crates/modplayer-ui/src/effects_view.rs` (contract R1.7, Edge Cases) — depends on T014 — implements FR-6.2.2 (spec FR-001)
- [X] T017 [US1] Replace the handle glyph `"⋮"` with `"⠿"` (fallback `"⋮⋮"` if unrendered, research R3) in `roles.text_secondary`, visible without hover, in `crates/modplayer-ui/src/effects_view.rs` (contract R2.1) — implements NFR-6.1, NFR-6.4 (spec FR-003)
- [X] T018 [US1] Add `CursorIcon::Grab` on pointer hover over the handle and `CursorIcon::Grabbing` while its drag is active, and a hover tooltip `tr("effects-reorder-handle-hint")`, in `crates/modplayer-ui/src/effects_view.rs` (contract R2.2, R2.5) — depends on T017 — implements NFR-6.1, NFR-6.2 (spec FR-003)
- [X] T019 [US1] Change the handle's AccessKit name to `tr_args("effects-reorder-handle-node", kind, position)` in `crates/modplayer-ui/src/effects_view.rs`, keeping role `Button` and the focus ring from `paint_focus_ring` (contract R2.4) — makes T008 pass, depends on T017 — implements NFR-6.2 (spec FR-013)
- [X] T020 [US1] Update `crates/modplayer-ui/tests/accessibility.rs` handle-name-prefix lookup and any header-label assertions it duplicates, to match the new handle name and moved notes (FR-013 regression guard)

**Checkpoint**: `rtk cargo test -p modplayer-ui` green for T005–T011; User Story 1 is independently demoable (quickstart M3–M5, M8, M10).

---

## Phase 4: User Story 2 - The chain's budget figures and meters read as measurements (Priority: P2)

**Goal**: Header CPU/overrun figures read as budget-relative with a hover explanation; level-pair readouts are fixed-width; the spectrum gains a dB gutter with 0/−30/−60 reference lines, a 100/1k/10k (+minor) tick strip, and positive/warning/danger bar segmentation with a 0 dBFS danger cap (FR-006–FR-009).

**Independent Test**: Open the panel under load; header text identifies CPU/overload figures as budget-relative; spectrum shows frequency ticks and an amplitude reference alongside bars (quickstart §1 `header_figures_are_budget_labeled`, spectrum axis tests).

### Tests for User Story 2 ⚠️ write first

- [X] T021 [P] [US2] Unit test `format_db_is_fixed_width` in `crates/modplayer-ui/src/widgets/chain_meters.rs`: assert `format_db` returns exactly 8 characters for −∞, −60.0, −12.3, −6.0 and 0.0 dB (data-model §4.2, FR-007)
- [X] T022 [P] [US2] Unit tests `freq_to_frac_matches_bar_mapping`, `db_to_frac_inverts_bar_height`, `tick_label_spans_never_overlap_160_to_420` in `crates/modplayer-ui/src/widgets/chain_meters.rs`: verify `freq_to_frac(100/1000/10000)` ≈ 0.233/0.566/0.900, `db_to_frac(0/-30/-60)` = 1.0/0.5/0.0, and label spans for plot widths from 160 to 420 px never overlap each other or exceed the plot (data-model §4.3, FR-008, SC-005)
- [X] T023 [P] [US2] Unit test `spectrum_segments_band_boundaries` in `crates/modplayer-ui/src/widgets/chain_meters.rs`: for linear values ≤0.001, 0.1, 0.7, ≥1.0 assert the segment set matches data-model §4.4 (none / positive-only / positive+warning / positive+warning+danger-cap) using `controls::BAND_WARNING_DB` (−6) and `SCALE_MAX_DB` (0) as boundaries (FR-009)
- [X] T024 [P] [US2] Add a spectrum case to `crates/modplayer-ui/tests/meter_bands.rs` asserting the spectrum's positive/−6 dBFS/warning/0 dBFS/danger convention replaces the old single `selection.bg_fill` color (FR-009, contract S2)
- [X] T025 [P] [US2] Integration/unit test `header_figures_are_budget_labeled` in `crates/modplayer-ui/tests/effects_view.rs` and `crates/modplayer-ui/tests/accessibility.rs`: header text matches `tr_args("effects-chain-cpu", pct)` / `tr_args("effects-overloads", count)`, hover text on the CPU figure equals `tr("effects-chain-cpu-hint")`, and the idle chain shows `"  0 %"` / `"0"` still labeled (FR-006, contract H1–H3, H6, SC-004)

### Implementation for User Story 2

- [X] T026 [US2] Update `show_header` in `crates/modplayer-ui/src/effects_view.rs` to render `effects-chain-cpu`/`effects-overloads` with the new values via `theme::mono_text`, `pct = format!("{:>3.0}", total_cost_pct)`, and attach the `effects-chain-cpu-hint` hover text to the CPU figure; leave the over-budget badge unchanged (contract H1–H4) — makes T025 pass, depends on T002 — implements FR-6.2.5, NFR-6.2 (spec FR-006)
- [X] T027 [US2] Update the row state-zone CPU figure to `tr_args("effects-cpu", pct)` = `"{ $pct } % of budget"`, `{:>3.0}`, mono, in `crates/modplayer-ui/src/effects_view.rs` (contract H5) — depends on T002; coordinate with T014 (same zone) if not yet landed — implements FR-6.2.5 (spec FR-001, FR-006)
- [X] T028 [US2] Change `format_db` in `crates/modplayer-ui/src/widgets/chain_meters.rs` to always return 8 characters (right-aligned, `" -inf dB"` … `"  0.0 dB"`) and update `level_pair`'s readout composition to `"{peak}: {format_db}"` / `"{rms}: {format_db}"` via `theme::mono_text` (contract L1) — makes T021 pass, no change to bars/banding/scale marks (contract L2) or the `ProgressIndicator` name shape (contract L3) — implements FR-6.4.1 (spec FR-007)
- [X] T029 [US2] Implement the spectrum axis constants and pure helpers (`SPECTRUM_MIN_HZ`/`MAX_HZ`, `MAJOR_TICKS_HZ`, `MINOR_TICKS_HZ`, `REFERENCE_DB`, `freq_to_frac`, `db_to_frac`, `tick_label_spans`) in `crates/modplayer-ui/src/widgets/chain_meters.rs` per data-model §4.3 — makes T022 pass — implements FR-6.4.2 (spec FR-008)
- [X] T030 [US2] Implement `spectrum_segments(value: f32) -> ([(Band, f32, f32); 3], usize)` in `crates/modplayer-ui/src/widgets/chain_meters.rs` per data-model §4.4, using `controls::BAND_WARNING_DB`/`SCALE_MAX_DB` — makes T023/T024 pass, depends on T029 (shared constants) — implements FR-6.4.1, FR-6.4.2, NFR-6.4 (spec FR-009)
- [X] T031 [US2] Redraw `spectrum` in `crates/modplayer-ui/src/widgets/chain_meters.rs`: allocate the rect as `available_width().clamp(160, 420)` outer width, add a left dB gutter with `effects-spectrum-ref-0db`/`-minus30`/`-minus60` labels (mono, small, right-aligned, vertically centred, `mark_color(roles, false)`), draw the 0/−30/−60 reference lines after the bars via `db_to_frac`, replace bar coloring with `spectrum_segments`/`band_color` (no `selection.bg_fill`), and add the bottom tick strip (major ticks 100/1k/10k labeled via `effects-spectrum-tick-*`, minor ticks at 50/200/500/2k/5k unlabeled) via `freq_to_frac`/`tick_label_spans` — per contract S1–S6, depends on T003, T029, T030 — implements FR-6.4.2, NFR-6.4 (spec FR-008, FR-009)
- [X] T032 [US2] Confirm the spectrum's AccessKit name/value (`"{effects-spectrum}, 64"`, peak-frequency value) is unchanged by T031 and add/update the assertion in `crates/modplayer-ui/tests/accessibility.rs` (contract S8) — depends on T031

**Checkpoint**: `rtk cargo test -p modplayer-ui` green for T021–T025; User Stories 1 AND 2 both independently demoable (quickstart M6, M7).

---

## Phase 5: User Story 3 - An empty chain explains itself (Priority: P3)

**Goal**: Zero-node chain shows explanatory text and one `Primary` "Add effect node" button instead of the bare kind dropdown; activating it reveals the existing add control and focuses the kind combo; state resets when the chain empties again (FR-010–FR-012).

**Independent Test**: Open the panel with zero nodes; confirm explanatory text and a single primary action replace the bare dropdown (quickstart §1 `empty_chain_shows_explanation_and_single_primary`).

### Tests for User Story 3 ⚠️ write first

- [X] T033 [P] [US3] Integration test `empty_chain_shows_explanation_and_single_primary` in `crates/modplayer-ui/tests/effects_view.rs`: zero nodes ⇒ `tr("effects-empty-explanation")` text and exactly one `(Role::Button, tr("effects-empty-add"))` styled `Primary` are present; no kind combo visible; header/meters/spectrum still render (FR-010, contract R4.1–R4.2, R4.4, SC-006)
- [X] T034 [P] [US3] Integration test `empty_primary_reveals_combo_and_focuses_it` in `crates/modplayer-ui/tests/effects_view.rs`: activating the empty-state button (click, Enter, and Space, each as a sub-case) reveals the kind combo + `effects-add-node` label + "Add" confirm styled `Primary` in the next frame, explanation still shown, and keyboard focus lands on the kind combo; still exactly one `Primary` control (FR-011, contract R4.3–R4.4)
- [X] T035 [P] [US3] Integration test `empty_state_resets_after_last_node_removed` in `crates/modplayer-ui/tests/effects_view.rs`: with ≥1 node the ordinary "Add node…" row renders (default-styled "Add", no explanation); removing the last node in one frame shows the collapsed explanation + primary button (not the revealed combo) with no stale content from the other state (FR-012, contract R4.5, Edge Cases "zero to one / back to zero")

### Implementation for User Story 3

- [X] T036 [US3] Add the `now-playing-effect-chain-add-revealed` and `now-playing-effect-chain-add-focus-pending` temp-memory bools to `crates/modplayer-ui/src/effects_view.rs` (data-model §3): set both `true` when the empty-state primary is activated; clear both once `view.nodes.is_empty()` becomes false, so a later empty chain starts collapsed again — implements NFR-6.1 (spec FR-011, FR-012)
- [X] T037 [US3] Implement the collapsed empty state in `show` / a new `show_empty_state` in `crates/modplayer-ui/src/effects_view.rs`: when `view.nodes.is_empty()` and not revealed, render header/level-pairs/spectrum as usual, then `tr("effects-empty-explanation")` wrapped label + one `button(Variant::Primary, tr("effects-empty-add"))`, no kind combo (contract R4.1–R4.2) — depends on T036 — implements NFR-6.2 (spec FR-010)
- [X] T038 [US3] Implement the revealed sub-state in `show_empty_state`: on activation (click/Enter/Space) show the existing kind combo + `effects-add-node` label + "Add" confirm drawn as `Variant::Primary`, request focus on the kind combo for one frame via `now-playing-effect-chain-add-focus-pending` (contract R4.3–R4.4) — depends on T036, T037 — implements NFR-6.1, NFR-6.2 (spec FR-011)
- [X] T039 [US3] Ensure the ≥1-node path renders the ordinary "Add node…" row with "Add" as the default (non-Primary) button as today, and that the reveal/focus flags are cleared on this path so the panel re-collapses if it empties again (contract R4.5) — depends on T036 — implements NFR-6.1 (spec FR-012)
- [X] T040 [US3] Update fixtures in `crates/modplayer-ui/tests/effects_view.rs` / `tests/accessibility.rs` that add a node starting from an empty chain, to first activate `effects-empty-add` before selecting a kind and confirming "Add" (research R9, plan.md Implementation Notes item 4) — audited both files: every existing fixture adds nodes through `controller.chain_add_node(kind)` directly (never through the UI combo/"Add" click), so none needed that change; the one fixture that *did* rely on empty-chain UI text (`e_and_header_toggle_panel_and_it_survives_track_change`'s open/closed body marker, which was `effects-add-node`) is updated to the collapsed empty state's own `effects-empty-explanation` marker, since that test's controller never adds a node

**Checkpoint**: `rtk cargo test -p modplayer-ui` green for T033–T035; all three user stories independently demoable (quickstart M1, M2, M9).

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Full-suite regression, formatting/lint gates, and the mandatory manual scenario sign-off.

- [X] T041 Run `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings` and `rtk cargo deny check`; fix any findings introduced by T002–T040 (quickstart §1, Constitution VII) — **Result**: `cargo fmt --check` found 4 unformatted spots in `chain_meters.rs`/`accessibility.rs`/`fluent_keys.rs` (long line-wraps this feature added); fixed with `cargo fmt`. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: no issues. `cargo deny check`: exit 0, `advisories ok, bans ok, licenses ok, sources ok` (only pre-existing duplicate-version/no-license-field warnings on unrelated deps, none introduced by this feature).
- [X] T042 Run `rtk cargo test -p modplayer-ui` then `rtk cargo test --workspace`; confirm every pre-existing test not touched by T009/T020/T040 is still green, unchanged, proving FR-014 (no node-behaviour change) — depends on all of Phase 3–5 — **Result (2026-09-29, `RUSTUP_TOOLCHAIN=1.95.0`)**: first run 2120 passed / 1 failed — `now_playing.rs::reserved_height_keeps_volume_meter_and_queue_card_in_a_960x640_viewport` (021 C13): with an empty chain the Queue card's bottom edge landed at y=655 in the 640 px viewport, because the empty-state explanation (R4.2) and the spectrum tick strip (+12 px, S5) make the Effect Chain card taller. Measured: removing the tick strip alone still overflowed by 3 px; removing the explanation alone passed. **Deviation (user-approved)**: that test is narrowed from "whole Queue card in the viewport" to "Queue card heading in the viewport". Its master-volume and peak-meter guards are unchanged. Logged in research.md R14. No node-behaviour test changed (FR-014 holds). After the change: `cargo test -p modplayer-ui` 753 passed / 0 failed (39 suites); `cargo test --workspace --no-fail-fast` all green except one flaky `modplayer-account/tests/reads.rs::playback_state_204_maps_to_none` failure. That crate is untouched by this branch, and the test passed on 3 of 3 reruns. `cargo fmt --check` and `cargo clippy … -D warnings` are green after the edit.
- [X] T043 [P] Execute manual scenarios M1–M10 from [quickstart.md](./quickstart.md) on macOS (`RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer`, Quartz window capture); record pass/deviation with screenshot paths for each M# directly under this task, and log any spec deviation in quickstart.md/research.md (Constitution "Manual Scenario Sign-Off") — **Status (2026-09-29): EXECUTED; M8, D1 and D2 fixed and re-verified in the app (last bullet). Signed off, with M6 band colours unobservable with real music and covered by unit tests.**
  - Setup: live sign-in (known issue #31 means the app needed a relaunch before the library loaded). The window was 960×668 outer, i.e. a 960×640 content area, and was driven on both the 1x external display and the 2x built-in display. Evidence is in `target/manual-walk/` (gitignored). A pre-024 baseline was built from `9faff60` into `target/baseline-9faff60` / `target/baseline-target` for comparison.
  - **M1 PASS**: 0-node chain shows "Chain CPU:   0 % of real-time budget", "Budget overruns: 0", both level pairs, the spectrum with 0 dB/−30/−60 and 100/1k/10k labels, the explanation, and one filled "Add effect node" button. No kind combo. (`m9-m1-empty.png`)
  - **M2 PASS**: Tab to "Add effect node" then Enter shows the kind combo with a focus ring and a filled "Add"; still exactly one Primary control. (`m2-sheet.png`)
  - **M3 PASS with observation**: after adding Pitch shift then Gain, the explanation disappears and the ordinary add row has a default-styled "Add". Rows read identity │ state │ params │ actions with separators, and units sit inside the value ("0.0 st", "12.0 dB"). Observation: the Gain row fits identity+state+params on one line, so Remove wraps onto its own line, left-aligned. That is R1.4-conformant but looks orphaned, and row frames differ in width. (`m3-a-pitch.png`, `z-m3-c.png`)
  - **M4 PASS**: open-hand cursor on hover, closed fist while dragging; after the drop, rows swap and renumber 1., 2. (`z-m4-hover.png`, `z-m4-cursor.png`, `z-m4-after.png`)
  - **M5 PASS**: handle focused via Tab; ↑ moves the node to position 1 and ↓ moves it back; focus stays on its handle and numbers update. VoiceOver not run (optional). (`m5-sheet.png`) Clicking a handle does not focus it; only Tab does.
  - **M6 PARTIAL**: played "Wherever I May Roam" with Gain +12 dB. Level readouts stay fixed-width: the "rms:" column does not move between "peak: −12.6 dB" and "peak:   0.0 dB" (`m6-d-sheet.png`). Spectrum amber/red segmentation was **not observed**: no band exceeded about −12 dBFS with real music (`m6-e-spectrum.png`). Band colouring rests on `spectrum_segments_band_boundaries` and `tests/meter_bands.rs`.
  - **M7 PASS**: the Chain CPU tooltip explains the callback-time share and the 90 %/100 % overrun rule. (`z-m7.png`)
  - **M8 FAIL (024 regression)**: with Equalizer + Stereo tools added at 960×640, the Stereo tools Parameters zone (Width, Balance, Mono sum, Phase invert, Channel swap) is wider than the centre column. It overflows past the window edge ("Ch…" clipped), the Effect Chain card loses its right border, and the Queue rows' "Remove" labels below get clipped too. Cause: the Parameters zone is one atomic `ui.horizontal` (R1). Before 024 the row was `horizontal_wrapped`, so controls wrapped individually. `rows_fit_panel_at_960_px` misses it because it renders `effects_view::show` alone across the full 960 pt, while the real column is ~740 pt after the nav rail and card insets. (`m8-c-stereo.png`, `m8-b-eq-scrolled.png`) The Equalizer row itself fits.
  - **M9 PASS**: removing the last node brings back the explanation and the *collapsed* "Add effect node" (not the revealed combo). (`m9-m1-empty.png`)
  - **M10 PASS (3 clean runs)**: disclosure → handle → Bypass → slider → value → (Formant) → Mode → Remove → next handle → … → kind combo → Add, on both displays and after a reorder (`v-tab-sheet.png`, `v-tab-sheet-reordered.png`, `r-tab-sheet.png`). A one-time **focus trap** (Tab stuck on the Time stretch "100 %" value box) seen in an earlier *offline* session (issue #31, "Reconnecting…") did not reproduce here, nor headless, nor on the baseline (`tab-sheet2.png`, `base-tab-sheet2.png`). Logged as unreproduced.
  - **Deviation D1** (research R3 claim wrong; glyph pre-dates 024): the handle glyph "⠿" (U+283F) renders as tofu "☐", because egui's bundled font lacks it. The baseline shows the same tofu (`base-np.png`), so the R3 fallback is needed.
  - **Deviation D2**: the handle hint tooltip's ↑/↓ render as tofu: "Drag to reorder, or focus and press ☐ / ☐".
  - Pre-existing, not 024: slider rails are invisible on the row `Frame::group` fill (knob only), also in the baseline. Cosmetic: header figures run together ("…budget Budget overruns"), and the empty-state explanation sits flush under the tick labels.
  - Cleanup: every node added during the walk was removed and the Queue/Markers panels were restored. The plugin-owned Pitch shift/Time stretch nodes are re-created by their plugins at launch.
  - **Fixes (2026-09-29, re-verified in the running app)**:
    - **M8**: `show_parameters_zone` is now `horizontal_wrapped`, and `widgets::controls::switch` starts a fresh row inside a wrapping layout when the whole switch would cross the row edge (egui can't wrap its nested `horizontal` by itself). Stereo tools now wraps "Channel swap" onto its own line inside the card (`fx-m8.png`). New regression test `now_playing.rs::effect_rows_fit_the_real_centre_column_at_960x640` renders the real shell (nav rail + `CentralPanel`) at 960×640 with Gain + Equalizer + Stereo tools, and asserts that no AccessKit node and no panel card crosses the window's right edge. It failed before the fix.
    - **D1**: the handle glyph is replaced by a painted 2×3 dot grip (`effects_view::paint_grip`), so there is no font dependency (`fx-0.png`).
    - **D2**: `effects-reorder-handle-hint` now uses words ("…press the Up or Down arrow key" / "…a seta para cima ou para baixo") (`z-fx-hint.png`). New guard `effects_view.rs::handle_hint_uses_only_glyphs_the_bundled_fonts_have` checks every shipped locale's hint against the bundled fonts; it fails on the old `↑ / ↓`.
    - Gates: `cargo fmt --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` are clean, and `cargo test --workspace --no-fail-fast` gives 2123 passed / 0 failed.
- [X] T044 Re-read `plan.md` Constitution Check post-design re-check and confirm every gate still passes after implementation (no engine/core change, no new crate/dependency, all strings externalized in en-US and pt-BR, all Constitution VII gates incl. `cargo deny check` green) — depends on T041, T042 — **Result (2026-09-29)**: every gate still passes. Checked with `git diff 9faff60..HEAD`: code changes are limited to `modplayer-ui` (`effects_view.rs`, `widgets/chain_meters.rs`, tests) plus `locales/{en-US,pt-BR}/effects.ftl`. No engine/core crate is touched, and no `Cargo.toml` or `Cargo.lock` change means no new crate or dependency (I, III, IV, V, X). Added non-test source has no `unwrap`/`expect`, and every new user-visible string is a Fluent key with en-US ↔ pt-BR parity checked by `fluent_keys.rs` (VII, X, FR-015). `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`) are all green, and T042's tests are green (VIII). Note: research.md R12 still says the "en-US only" decision, but the plan's final Constitution X text and the shipped `locales/pt-BR/effects.ftl` superseded it.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. BLOCKS all user stories that reference new/changed Fluent keys (i.e., all three — every story's Fluent-key-consuming tests need T004 to compile/pass).
- **User Stories (Phase 3–5)**: All depend on Foundational (Phase 2). Per plan.md's suggested order, US1 lands before US2/US3, but US1/US2/US3 are independently shippable (plan.md "Implementation Notes"): US2's `chain_meters.rs` work does not touch `effects_view.rs` beyond `show_header` (T026), and US3's empty-state work in `effects_view.rs` is additive to US1's row rework.
- **Polish (Phase 6)**: Depends on all desired user stories being complete.

### User Story Dependencies

- **User Story 1 (P1)**: After Phase 2. No dependency on US2/US3.
- **User Story 2 (P2)**: After Phase 2. Independent of US1 except sharing `effects_view.rs`'s `show_header`/row state-zone figure (T026/T027) — sequence with US1's T014 if both are in flight to avoid a merge conflict in the same function.
- **User Story 3 (P3)**: After Phase 2. Builds additively on US1's `show` restructuring (T014) for the row list vs. empty-state branch, but does not require US1's zone work to be finished first — `show_empty_state` is a separate branch of `show`.

### Within Each User Story

- Tests (T005–T011, T021–T025, T033–T035) are written first and must fail against the current code before their matching implementation task.
- Pure helpers (`zone_breaks`, `format_db`, spectrum axis helpers, `spectrum_segments`) before the rendering code that calls them.
- Rendering/structure changes before AccessKit-name/focus fixups that depend on the new structure.
- Story complete (its Checkpoint) before relying on it from a later story's manual scenario.

### Parallel Opportunities

- T002/T003 are sequential (same file); T004 can start once T003 lands.
- All of T005–T011 are `[P]` (distinct test functions, mostly same file `tests/effects_view.rs` but non-overlapping — treat as logically parallel authorship, land as one commit if serializing on the file).
- T021–T025 are `[P]` (three in `chain_meters.rs`, one in `meter_bands.rs`, one across `effects_view.rs`/`accessibility.rs`).
- T033–T035 are `[P]` (same file, non-overlapping test functions).
- T043 (manual scenarios) can run in parallel with T041/T042 once a build exists, but sign-off recording depends on the code being final.

---

## Parallel Example: User Story 1

```bash
# Tests for User Story 1 (author together, distinct functions):
Task: "zone_breaks unit tests in crates/modplayer-ui/src/effects_view.rs"
Task: "rows_render_four_zones_in_order in crates/modplayer-ui/tests/effects_view.rs"
Task: "unit_suffix_inside_value_for_all_six_kinds in crates/modplayer-ui/tests/effects_view.rs"
Task: "handle_name_identifies_node_and_position in crates/modplayer-ui/tests/effects_view.rs"
Task: "tab_order_runs_identity_state_params_actions in crates/modplayer-ui/tests/effects_view.rs"
Task: "rows_fit_panel_at_960_px in crates/modplayer-ui/tests/effects_view.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1: Setup (baseline).
2. Phase 2: Foundational (Fluent keys + `fluent_keys.rs`).
3. Phase 3: User Story 1 (four-zone rows, handle affordance/name, focus order, 960 px fit).
4. **STOP and VALIDATE**: `cargo test -p modplayer-ui`, quickstart M3–M5/M8/M10.
5. Demo: rows read as four groups — this alone resolves review finding `UX-25`'s lead complaint.

### Incremental Delivery

1. Setup + Foundational → Fluent surface ready.
2. Add User Story 1 → validate independently → demo (MVP).
3. Add User Story 2 (header/meters/spectrum) → validate independently → demo.
4. Add User Story 3 (empty state) → validate independently → demo.
5. Polish: full regression, lint/fmt, manual M1–M10 sign-off.

### Parallel Team Strategy

1. Team completes Setup + Foundational together (small, sequential, single file).
2. Once Foundational is done:
   - Developer A: User Story 1 (`effects_view.rs` rows/handle).
   - Developer B: User Story 2 (`widgets/chain_meters.rs` + `show_header`).
   - Developer C: User Story 3 (`effects_view.rs` empty state).
3. A and C touch `effects_view.rs`: land T014 (US1 row restructuring) before T037–T039 (US3 empty-state branch) to avoid a merge conflict in `show`, or rebase C's branch onto A's once T014 lands.

---

## Notes

- No new crate, module, trait, or persisted data (FR-014, Constitution X) — every task edits one of the files named in Path Conventions.
- `[P]` tasks are different files or non-overlapping functions within one file; serialize same-file tasks that touch the same function (e.g., T012→T014, T017→T018/T019).
- Every user-visible string added or changed is a Fluent key in both en-US and pt-BR (T002/T003) checked by `fluent_keys.rs` incl. parity (T004) — no literal strings in Rust source (FR-015, contract "Rules").
- Verify each story's tests fail before implementing (T005–T011, T021–T025, T033–T035 before their implementation tasks).
- Stop at each Checkpoint to validate that story independently before starting the next.
- T043 (manual scenarios M1–M10) is mandatory per the constitution's Manual Scenario Sign-Off governance rule — not optional polish.

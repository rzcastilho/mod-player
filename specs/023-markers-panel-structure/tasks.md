---

description: "Task list for 023-markers-panel-structure"
---

# Tasks: Markers Panel Structure

**Input**: Design documents from `/specs/023-markers-panel-structure/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/ui-markers-panel.md](./contracts/ui-markers-panel.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The spec requires automated verification (FR-025), the plan's Testing section and Constitution Principle VIII require tests written first, and research R15 names the exact test list. Every test task below writes a specific test named in [contracts/ui-markers-panel.md §9](./contracts/ui-markers-panel.md#9-tests-pinning-this-contract) and must fail (red) before its paired implementation task.

**Organization**: Tasks are grouped by user story (US1–US4, spec priorities P1/P1/P2/P3), per the plan's own "Implementation Order" section. US1 (grouping) must land before US2–US4 because their rows render inside US1's groups. US3 and US4 are independent of US2 and of each other.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies). Most tasks here land in the same two files (`src/markers.rs`, `tests/markers.rs`) in a fixed order, so **most tasks are intentionally not [P]** — marking them so would cause edit conflicts.
- **[Story]**: US1–US4, mapping to spec.md's four user stories.

## Path Conventions

Single Cargo workspace, one crate touched: `crates/modplayer-ui/`. All paths below are relative to the repository root.

---

## Phase 1: Setup

**Purpose**: Confirm a green baseline before touching anything.

- [X] T001 Run `rtk cargo fmt --check && rtk cargo clippy --workspace --all-targets --all-features -- -D warnings && rtk cargo test --workspace && rtk cargo deny check` (quickstart §1) and confirm all four gates pass on `023-markers-panel-structure` before any edit in this feature.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The `PanelModel`/`PanelIntent` types, the new `WaveformState` fields, the glyph constants and the Fluent strings that every user story's rendering code depends on.

**⚠️ CRITICAL**: No user story task may start until this phase is complete.

### Tests for Foundational (write first, confirm they fail)

- [X] T002 [P] Write `PanelModel` unit tests (partition by kind, empty `TrackMarkers`, counts) and the `proptest panel_model_invariants` (P1–P4: multiset equality, kind-to-group matching, `visual_order` is a permutation, A-before-B inside a block) in `crates/modplayer-ui/src/markers.rs` `#[cfg(test)]` (data-model §1; contract §9 `panel_model_partitions_by_kind`). Confirm it fails to compile/pass — the types do not exist yet.
- [X] T003 [P] Extend `crates/modplayer-ui/tests/fluent_keys.rs` to assert the 9 new keys resolve and are exercised: `markers-group-loop`, `markers-group-points`, `markers-group-cues`, `markers-group-heading`, `markers-name-placeholder`, `markers-cue-empty`, `markers-jump`, `markers-nudge-earlier`, `markers-nudge-later`, `markers-remove`, `markers-palette` (contract P10; FR-024). Confirm it fails — the keys are not in `playback.ftl` yet.
- [X] T004 [P] Add `row_action_glyphs_covered_by_fonts` test in `crates/modplayer-ui/tests/markers.rs`, asserting `ctx.fonts(|f| f.has_glyphs(..))` for each `ROW_ACTION_*_GLYPH` after `theme::apply` (research R6). Confirm it fails — the constants do not exist yet.

### Implementation for Foundational

- [X] T005 [P] Add `pub enum PanelFocus { Row(MarkerId), NewLoopRegion }` and the two new fields `rename_focus_pending: bool` (default `false`) and `panel_focus: Option<PanelFocus>` (default `None`) to `WaveformState` in `crates/modplayer-ui/src/waveform/state.rs`, both reset on track change alongside existing fields; doc comments on the new public items, each with an `# Examples` doctest that runs under `cargo test --doc` (e.g. constructing `WaveformState::default()` and asserting `rename_focus_pending == false` / `panel_focus == None`, and matching a `PanelFocus::NewLoopRegion`) (data-model §5; Constitution VII).
- [X] T006 [P] Add glyph constants `ROW_ACTION_JUMP_GLYPH = "↦"`, `ROW_ACTION_NUDGE_EARLIER_GLYPH = "◂"`, `ROW_ACTION_NUDGE_LATER_GLYPH = "▸"`, `ROW_ACTION_REMOVE_GLYPH = "✕"` with doc comments, each carrying an `# Examples` doctest that runs under `cargo test --doc` (asserting the constant's value, e.g. `assert_eq!(ROW_ACTION_JUMP_GLYPH, "↦")`), in `crates/modplayer-ui/src/theme/markers.rs` (research R6). This should make T004 pass; if any glyph fails the font-coverage assertion, substitute a covered glyph from the same family and record the change in research.md R6.
- [X] T007 [P] Add the `## Markers panel — structure (023)` section (11 keys per contract P10, including `markers-group-heading` and `markers-palette`) to `locales/en-US/playback.ftl`. This should make T003 pass.
- [X] T008 Define the private types `RowData`, `LoopBlock`/`RegionCells`, `CueSlotRow` (`Occupied`/`Empty` variants) and `PanelModel` (`loop_blocks`, `points`, `cues: [CueSlotRow; 8]`, `total`) plus `PanelModel::snapshot(&TrackMarkers, sample_rate)` and its derived methods (`is_empty`, `loop_count`, `point_count`, `cue_count`, `show_points_group`, `show_loop_group`, `show_cues_group`, `visual_order`) in `crates/modplayer-ui/src/markers.rs` (data-model §1–3; research R1, R3). This should make T002 pass. Depends on T005 (uses no state, but lands in the same file after the test scaffold).
- [X] T009 Define `PanelIntent` (`NewLoopRegion`, `Focus`, `OpenRename`, `CommitRename`, `CancelRename`, `Recolor`, `Jump`, `Nudge`, `Remove`, `ClearRequest`/`ClearConfirm`/`ClearCancel`) and `apply_intents(&mut Vec<PanelIntent>, &mut controller, &mut waveform)` with its two-pass order (rename intents first, then the rest in emission order) and a `ctx.request_repaint()` when anything was applied, in `crates/modplayer-ui/src/markers.rs` (data-model §4; research R2). Depends on T005 (new `WaveformState` fields), T008 (`PanelModel::visual_order` for the `Remove` focus target).

**Checkpoint**: `PanelModel`, `PanelIntent`, the glyph constants, the new `WaveformState` fields and the Fluent keys all exist, compile and pass T002–T004. User story implementation can now begin.

---

## Phase 3: User Story 1 - See loop regions, points, and cues as three distinct groups (Priority: P1) 🎯 MVP

**Goal**: The panel shows three labelled, counted groups — Loop Region, Points, Cues — in that order, with loop cells following a region's last present boundary, replacing today's flat list.

**Independent Test**: Load a track with an armed loop region, two point markers, and one cue point; confirm the panel shows exactly three labelled groups in order with accurate counts, and that the region's arm/repeat/crossfade render inside the Loop Region group.

### Tests for User Story 1 (write first, confirm they fail)

- [X] T010 [US1] Write `panel_groups_in_order_with_counts` test (three headings, order, counts for a mixed track) and `points_group_omitted_when_empty` / `loop_group_always_when_populated` tests (Loop and Cues always render once populated; Points renders only when non-empty) in `crates/modplayer-ui/tests/markers.rs` (contract P1/P2; FR-001, FR-004; US1 AS1/AS3/AS4; SC-002).
- [X] T011 [US1] Write `loop_block_cells_follow_last_boundary` test, including a B-only region showing its cells (today it shows none), and a two-region track where both blocks render ordered by earliest present boundary in `crates/modplayer-ui/tests/markers.rs` (contract P1/P3; FR-003; Clarification 12).
- [X] T012 [US1] Write `cues_group_shows_eight_slots_muted_empty_rows` and `empty_cue_rows_are_not_tab_stops` tests in `crates/modplayer-ui/tests/markers.rs` (contract P1/P8; FR-015/FR-016; SC-008).
- [X] T013 [US1] Write `empty_panel_shows_message_and_new_loop_only` (exactly `markers-empty` + "New loop region"; no headings, cue rows or Clear all) and `status_line_above_loop_group` tests in `crates/modplayer-ui/tests/markers.rs` (contract P1; FR-020, FR-021; SC-007).

### Implementation for User Story 1

- [X] T014 [US1] Rewrite the `panel()` entry point to build `PanelModel::snapshot` once, render the status line (only if `waveform.marker_status.is_some()`), then branch to the empty layout or the three group renderers, in `crates/modplayer-ui/src/markers.rs` (contract P1; research R1).
- [X] T015 [US1] Implement `show_loop_group`: heading (`markers-group-loop`, count = `loop_count()`), "New loop region" as its first line (Default variant), then one block per `LoopBlock` (A row if present, B row if present, then the loop cells after the last present boundary) in `crates/modplayer-ui/src/markers.rs` (contract P1/P2/P3-cells; FR-001–FR-004; research R3, R4, R13).
- [X] T016 [US1] Implement `show_points_group`: heading (`markers-group-points`, count = `point_count()`), rendered only when `show_points_group()` is true, rows ordered by position in `crates/modplayer-ui/src/markers.rs` (FR-001, FR-005).
- [X] T017 [US1] Implement `show_cues_group`: heading (`markers-group-cues`, count = occupied-slot count), then all 8 `CueSlotRow`s in slot order — an occupied slot as a full row, an empty slot as the muted, non-interactive `markers-cue-empty { $slot }` row — in `crates/modplayer-ui/src/markers.rs` (FR-002, FR-016; research R12).
- [X] T018 [US1] Implement `show_empty`: `markers-empty` message followed by "New loop region" only, and remove today's top line (`New loop region` + `Clear all markers` together) and its old empty-state rendering in `crates/modplayer-ui/src/markers.rs` (FR-020; research baseline table).
- [X] T019 [US1] Remove the now-superseded flat marker loop and `MarkerRowData` plumbing that T014–T018 replace, in `crates/modplayer-ui/src/markers.rs`.

**Checkpoint**: T010–T013 pass. The panel shows three correctly ordered, correctly counted groups; User Story 1 is independently testable and demoable (rows still show only swatch/name/position at this point — row actions and rename arrive in US2).

---

## Phase 4: User Story 2 - Act on any marker row without leaving the panel (Priority: P1)

**Goal**: Every populated row supports click-to-rename, colour picking, jump, nudge earlier/later, and remove, entirely from the panel, matching the keyboard's existing behaviour and reflected on the same rendered frame.

**Independent Test**: Create a point marker; rename it by clicking its name; recolour it from the palette; click jump and confirm play state is preserved; click nudge later and confirm the configured step; click remove and confirm it disappears — all without leaving the panel.

### Tests for User Story 2 (write first, confirm they fail)

- [X] T020 [US2] Write `row_actions_present_quiet_and_named` and `row_tab_order_swatch_name_actions` tests in `crates/modplayer-ui/tests/markers.rs` (contract P3/P7; FR-008, FR-014; Clarification 6; SC-009).
- [X] T021 [US2] Write `row_jump_seeks_preserving_play_state` (Playing and Paused, each marker kind) and `row_jump_noop_while_dragging` tests in `crates/modplayer-ui/tests/markers.rs` (contract P6; FR-009).
- [X] T022 [US2] Write `row_nudge_matches_keyboard_step`, including clamp-at-0, in `crates/modplayer-ui/tests/markers.rs` (contract P6; FR-010).
- [X] T023 [US2] Write `row_remove_matches_keyboard_delete` (region-incomplete, last-boundary, armed-disarm cases) and `row_remove_moves_focus_next_prev_new_loop` tests in `crates/modplayer-ui/tests/markers.rs` (contract P6; FR-011; Clarification 8).
- [X] T024 [US2] Write `swatch_opens_palette_popover_and_picks` and `palette_esc_and_click_outside_no_change` tests in `crates/modplayer-ui/tests/markers.rs` (contract P4; FR-013; Clarification 2). `swatch_opens_palette_popover_and_picks` also asserts, as the regression test for the 1-based colour-label fix (research R7; Constitution VIII), that the popover's 8 `Role::RadioButton` nodes carry the labels `markers-color` with `$index` 1..8 in palette order (never 0).
- [X] T025 [US2] Write `click_name_opens_rename_placeholder_when_unnamed`, `rename_commits_on_focus_loss_esc_cancels`, `rename_reflected_in_lane_glyph_name_next_frame`, and `rename_commits_before_other_row_action` tests in `crates/modplayer-ui/tests/markers.rs` (contract P5; FR-012; Clarification 3; SC-004; Edge Case).
- [X] T026 [US2] Write `enter_on_row_action_does_not_open_rename` test in `crates/modplayer-ui/tests/markers.rs` (contract P7; research R8).

### Implementation for User Story 2

- [X] T027 [US2] Implement the populated-row layout: swatch (16×16 button), role label, name cell, then a trailing column built with a pre-allocated rect and a left-to-right child `Ui` (so creation order equals visual and Tab order) holding position, clamped ⚠, jump, nudge−, nudge+, remove, in `crates/modplayer-ui/src/markers.rs` (contract P3; research R5).
- [X] T028 [US2] Wire the jump/nudge-earlier/nudge-later/remove buttons as Quiet variant with the T006 glyphs, `.on_hover_text` and AccessKit label = `tr(markers-jump|nudge-earlier|nudge-later|remove)`, each pushing its `PanelIntent`, in `crates/modplayer-ui/src/markers.rs` (contract P3 cells 6–9; FR-008).
- [X] T029 [US2] Implement `apply_intents` handling for `Jump` (no-op while `marker_drag` matches the id; otherwise `seek_frames`), `Nudge` (`nudge_marker(id, ∓1, 1)`), and `Remove` (`delete_marker`, then compute the next/previous/`NewLoopRegion` target from `visual_order()` and store it in `waveform.panel_focus`, clearing `focused_marker`/`rename` if they pointed at the removed id) in `crates/modplayer-ui/src/markers.rs` (data-model §4; research R10, R11; FR-009–FR-011).
- [X] T030 [US2] Implement click-to-rename: the name cell is a Quiet button (or the `markers-name-placeholder` when empty) that pushes `OpenRename`; the `TextEdit` calls `request_focus()` only while `rename_focus_pending` then clears it; `Esc` is checked before `lost_focus` so Esc cancels and anything else (Enter or focus loss) commits, in `crates/modplayer-ui/src/markers.rs` (contract P5; research R9).
- [X] T031 [US2] Implement the swatch popover: `egui::Popup` with `CloseOnClickOutside`, `Role::RadioGroup` labelled `markers-palette`, 8 `Role::RadioButton` swatches labelled `markers-color { $index }` (1-based) with the current one marked by a `text_primary` outline + "✓", pick → `Recolor` intent + popup close, keyboard-open focuses the current swatch, ←/→/Tab move between swatches, `Esc`/click-outside close with no change, in `crates/modplayer-ui/src/markers.rs` (contract P4; research R7).
- [X] T032 [US2] Wire row focus: any focusable row control (swatch, name cell, jump, nudge−, nudge+, remove) gaining focus pushes `Focus(id)`; register `marker_claims()` and the horizontal-arrow `EventFilter` on each; add the Enter-alias gate to `handle_focused_marker_keys` via a per-frame `RenameOnEnter` temp-memory id set written only by lane glyphs and name cells, in `crates/modplayer-ui/src/markers.rs` (contract P7; research R8).
- [X] T033 [US2] Apply `waveform.panel_focus` on the next frame: the target row's swatch (or "New loop region") calls `request_focus()` and the field is cleared; if the target id no longer exists, fall through to the first populated row, else "New loop region", in `crates/modplayer-ui/src/markers.rs` (research R11).

**Checkpoint**: T020–T026 pass. User Stories 1 and 2 both work independently: grouped rows with full in-panel actions, rename and recolour, keyboard parity.

---

## Phase 5: User Story 3 - Clear-all is visibly and spatially separate from New loop region (Priority: P2)

**Goal**: "New loop region" is the first line of the Loop Region group; "Clear all markers" moves to a destructive footer below the Cues group, keeping its existing two-step confirmation.

**Independent Test**: Open the panel on a track with several markers; confirm "New loop region" is at the top of the Loop Region group and "Clear all markers" is in a separate destructive footer below Cues, separated by other panel content; confirm the two-step confirmation still gates removal.

### Tests for User Story 3 (write first, confirm they fail)

- [X] T034 [US3] Write `clear_all_in_footer_destructive_not_adjacent` and `clear_all_two_step_unchanged` tests in `crates/modplayer-ui/tests/markers.rs` (contract P9; FR-018/FR-019; SC-005/SC-006).

### Implementation for User Story 3

- [X] T035 [US3] Move the existing `clear_all_controls` body (unchanged "Clear N markers?" confirm/cancel logic) into a `show_footer` that renders a right-aligned, destructive-variant row below the Cues group, and only when the panel is not empty, in `crates/modplayer-ui/src/markers.rs` (contract P9; FR-018/FR-019; research R13).
- [X] T036 [US3] Confirm "New loop region" (T015/T018) uses the explicit `Variant::Default` control and sits directly under the Loop Region header (populated) or directly under the empty-state message (empty), with nothing from FR-018's footer adjacent to it, in `crates/modplayer-ui/src/markers.rs` (FR-017; research R13).

**Checkpoint**: T034 passes. User Story 3 is independently testable: the destructive action reads as separate and styled correctly, and its confirmation is unchanged.

---

## Phase 6: User Story 4 - The panel tells you what to do when it has nothing, or room, to show (Priority: P3)

**Goal**: A fully empty track shows one instruction plus "New loop region" only; once the panel has any content, the Loop Region group always shows (even at count 0) and the Cues group always shows all 8 slots.

**Independent Test**: Open the panel on a brand-new track and confirm the single empty-state message plus "New loop region"; then create one cue in slot 3 and confirm all 8 cue slots render, slot 3 populated and the other 7 visibly muted and labelled by number.

### Tests for User Story 4 (write first, confirm they fail)

- [X] T037 [US4] Write a test asserting the Loop Region group renders with header count 0 and its "New loop region" control when the track has only Points or only Cues content (no loop region at all) in `crates/modplayer-ui/tests/markers.rs` (US4 AS4; FR-004).

### Implementation for User Story 4

- [X] T038 [US4] Verify `show_empty` (T018) and `show_loop_group`/`show_cues_group` (T015/T017) already satisfy FR-020 (fully empty ⇒ message + New loop region only) and FR-004/FR-016 (any content ⇒ Loop group always, Cues group always shows all 8 slots) exactly; adjust the gating conditions in `crates/modplayer-ui/src/markers.rs` if T037 or T013 surfaces a gap.

**Checkpoint**: All four user stories are independently functional and testable.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Confirm the zero-new-literal and fully-externalized-string constraints, run the full regression suite, and execute the manual scenarios.

- [X] T039 [P] Run `rtk cargo test -p modplayer-ui --test design_token_literals` and confirm the baseline is unchanged — no new colour, font-size or spacing literal was introduced outside `crates/modplayer-ui/src/theme/**` (FR-023).
- [X] T040 [P] Review `crates/modplayer-ui/src/now_playing.rs` and confirm the `panel()` call site and signature are unchanged (no edit expected; plan.md Project Structure).
- [X] T041 Run `rtk cargo test -p modplayer-ui --test accessibility --test controls --test control_variants --test now_playing` and confirm no regression.
- [X] T042 Run the full gate suite: `rtk cargo fmt --check && rtk cargo clippy --workspace --all-targets --all-features -- -D warnings && rtk cargo test --workspace && rtk cargo deny check` (quickstart §1).
- [X] T043 Execute manual scenarios M1–M11 from [quickstart.md §3](./quickstart.md#3-manual-scenarios), capturing each to `target/manual-walk/023-Mn.png`, and record pass/deviation with the screenshot path in this file.
  - **Attempt 2026-09-29: BLOCKED.** Debug build succeeds (`RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer`; the shell exports `1.93.1`), but the macOS session is locked (`CGSessionCopyCurrentDictionary()["CGSSessionScreenIsLocked"] == True`), so no window can be driven or captured with `screencapture -l`. The 022 walk helpers (`mw.py`, `drag.py`) went away with that worktree and need rewriting under `target/manual-walk/`. A rebuilt debug binary may also raise the SecurityAgent Keychain prompt seen in 022 T030. The agent will not type the login password. **Unblock**: a human unlocks the session, launches `./target/debug/modplayer` once, and chooses *Always Allow* on any Keychain prompt; M1–M11 can then run. All automated coverage for these scenarios is green (T042: 2094 passed, 11 ignored).
  - **Executed 2026-09-29 (second attempt): 10 PASS, 1 DEVIATION (M5 ✓ glyph).** The session was unlocked and no Keychain prompt appeared. After the user signed in, the app stayed "offline" (Library empty, Search offline, reconnect toast) until it was relaunched twice. This is filed as [#31](https://github.com/rzcastilho/mod-player/issues/31) and is unrelated to this feature. Driven by `CGEvent` injection + `screencapture -l` (`target/manual-walk/mw.py`; `tabwalk.py` diffs frames to follow Tab focus). Window 1680×1000, dark theme. Track: "Bring Me To Life" (Evanescence); M10 ran on "Bring Me To Life – Remastered 2023" after a queue advance. Nudge step configured at 25 ms (`023-settings-playback.png`).
    - **M1 PASS**: only "No markers — press I to set A" + "New loop region"; no headings, cue rows or Clear all. `target/manual-walk/023-M1.png`
    - **M2 PASS**: LOOP REGION 1 → A, B, then Arm/Repeat/Crossfade; POINTS 2; CUES 1 with 8 slots, where slot 3 is populated and 1, 2, 4–8 are muted "Cue n — empty · Shift+n to set". `023-M2-full.png`, `023-M2-cues.png`
    - **M3 PASS**: arming shows "Armed (waiting for the playhead)", then "Looping indefinitely", inside the loop block. The block does not move and there is no extra region marker. `023-M3-L.png`, `023-M3.png`. (One injected `L` was dropped at first; a retest with a row button focused armed normally: `023-M3-L-focused-crop.png`.)
    - **M4 PASS**: clicking the name opens a focused field with the text selected. "Chorus" commits on click-away; "Verse" + Esc leaves "Chorus". `023-M4-open.png`, `023-M4-commit-crop.png`, `023-M4-typing-crop.png`, `023-M4-esc-crop.png`. Lane glyphs show no hover tooltip; the name is exposed only as the glyph's AccessKit label (`glyph_accessible_name`). Synthetic hover could not show it, and the AX tree could not be read (System Events returned no descriptions), so the lane half is judged from code, not observed.
    - **M5 DEVIATION**: picking the 6th colour recolours the swatch and both lane glyphs (teal). Reopening marks teal as current, and Esc closes with no change. `023-M5-open.png`, `023-M5-picked.png`, `023-M5-reopen-crop.png`, `023-M5-esc-crop.png`. **But the "current" mark renders as a tofu box □, not ✓** (`023-M5-check-zoom.png`, and again in high contrast in `023-M11-hc-popover.png`). `"\u{2713}"` (`src/markers.rs:1349`) is not in the configured fonts, and `row_action_glyphs_covered_by_fonts` only checks the four `ROW_ACTION_*` glyphs. **Follow-up**: move ✓ into a `theme::markers` constant, add it to the font-coverage test, and substitute a covered glyph (or paint the check) per research R6. **Fixed** in the same change set: `PALETTE_CURRENT_GLYPH = "✔"` (U+2714, covered by the monospace font), pinned by `palette_current_glyph_covered_by_mono_font`; research.md R7 records the substitution. Re-checked on screen after the rebuild: the current swatch now shows a real check mark (`023-fix-popover.png`, `023-fix-check-zoom.png`).
    - **M6 PASS**: jump while paused moved the playhead to Marker 1 and stayed paused (resume started at ~3:03); jump to Cue 3 while playing landed ~3:10 and kept playing. `023-M6-paused.png`, `023-M6-resume.png`, `023-M6-playing.png`. (Observed: the header clock did not refresh while paused until playback resumed; not investigated.)
    - **M7 PASS**: B went from 2:58.488 to 2:58.613 after 5 clicks on nudge-later (+125 ms = 5 × 25 ms). A point at 0:01.913 clamps at 0:00.000 after 85 nudge-earlier clicks. `023-M7-B-crop.png`, `023-M7-clamp-crop.png`
    - **M8 PASS**: Tab order is New loop region → A: swatch → name → jump → nudge− → nudge+ → remove → B: swatch… (`tabwalk.py` stops 2–14). Tab goes from Cue 3's controls directly to "Clear all markers", skipping the empty cue rows (`023-M8-cues-a.png`, `023-M8-cues-b.png`). Enter on A's remove deletes A only: Arm loop is disabled and focus moves to B's swatch, with no rename field (`023-M8-preremove-crop.png`, `023-M8-remove-crop.png`). Observed: the page does not auto-scroll to a focused control below the fold; not investigated.
    - **M9 PASS**: "Clear all markers" is red, bottom-right under Cues, far from "New loop region". Confirm is "Clear N markers?"; "No" keeps everything and "Yes" returns to the M1 state. `023-M9-confirm.png`, `023-M9-no.png`, `023-M9-yes.png`
    - **M10 PASS**: at POINTS 64, `M` shows "The track already has the maximum number of markers." under the card header, above LOOP REGION; grouping is unchanged. `023-M10.png`
    - **M11 PASS except the M5 glyph**: in dark + high contrast, headings, muted cue rows, quiet row actions and outlined buttons are legible, and the popover is readable. The current-colour mark is the same tofu □ as in M5. High contrast was turned back off afterwards. `023-M11-hc.png`, `023-M11-hc-popover.png`, `023-M11-restored.png`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies.
- **Foundational (Phase 2)**: Depends on Setup. BLOCKS every user story (T008/T009 define the types every group renderer and intent handler calls).
- **US1 (Phase 3)**: Depends on Foundational. BLOCKS US2, US3, US4 — their rows and footer render inside US1's group functions.
- **US2 (Phase 4)**: Depends on US1 (row shells from T015–T017). Independent of US3/US4.
- **US3 (Phase 5)**: Depends on US1 only (needs the group layout to move the footer below it). Can proceed in parallel with US2.
- **US4 (Phase 6)**: Depends on US1 only. Can proceed in parallel with US2/US3.
- **Polish (Phase 7)**: Depends on all four user stories being complete.

### Within Each Phase

- Tests are written first and must fail before their paired implementation task.
- Within Foundational and within each user story, implementation tasks touching `crates/modplayer-ui/src/markers.rs` are strictly ordered (same file); only tasks in different files (state.rs / theme/markers.rs / playback.ftl / tests/fluent_keys.rs vs tests/markers.rs) can run in parallel, and are marked `[P]`.

### Parallel Opportunities

- T002, T003, T004 (Foundational tests: three different files) can run in parallel.
- T005, T006, T007 (Foundational implementation: `waveform/state.rs`, `theme/markers.rs`, `playback.ftl`) can run in parallel.
- Once US1 (Phase 3) is checkpointed, US2, US3 and US4 can be staffed and worked in parallel by different people, since all three only depend on US1 — though within `markers.rs` itself, merges must be sequenced by whoever lands first.
- T039 and T040 in Polish can run in parallel (different files, read-only checks).

---

## Parallel Example: Foundational

```bash
# Launch all three Foundational tests together (different files):
Task: "PanelModel unit tests + proptest panel_model_invariants in crates/modplayer-ui/src/markers.rs"
Task: "Extend crates/modplayer-ui/tests/fluent_keys.rs for the 9 new markers-* keys"
Task: "row_action_glyphs_covered_by_fonts test in crates/modplayer-ui/tests/markers.rs"

# Then launch all three Foundational implementation tasks together (different files):
Task: "PanelFocus enum + WaveformState fields in crates/modplayer-ui/src/waveform/state.rs"
Task: "ROW_ACTION_*_GLYPH constants in crates/modplayer-ui/src/theme/markers.rs"
Task: "New Fluent section in locales/en-US/playback.ftl"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational (CRITICAL — blocks every story).
3. Complete Phase 3: User Story 1.
4. **STOP and VALIDATE**: run T010–T013; confirm the three groups, counts and empty state independently.
5. Demo: a track with a region, points and cues shows the correct grouped structure (rows are read-only at this point — no row actions yet).

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. US1 → validate independently → grouped, counted panel (MVP).
3. US2 → validate independently → full row interaction (rename, recolour, jump, nudge, remove, keyboard parity).
4. US3 → validate independently → destructive action visibly and spatially separated.
5. US4 → validate independently → empty-state message and always-8-cue-slots rule confirmed.
6. Polish → literal scan, full regression, manual M1–M11 with evidence.

### Suggested Single-Agent Order

Given the single-crate, mostly-single-file nature of this feature, US2 and US3/US4 are best done sequentially even though they are logically independent, to avoid repeated merge conflicts in `markers.rs`: **US1 → US2 → US3 → US4 → Polish**, exactly as plan.md's "Implementation Order" lays out.

---

## Notes

- `[P]` tasks touch different files with no dependency on each other.
- `[Story]` maps every Phase 3–6 task to its user story for traceability back to spec.md.
- Every test task names the exact test(s) from [contracts/ui-markers-panel.md §9](./contracts/ui-markers-panel.md#9-tests-pinning-this-contract); write it, confirm red, then do the paired implementation task(s) until green.
- FR-022 (no core/engine/keyboard/persistence change) and FR-023 (zero new literal outside `theme/**`) apply to every implementation task in this file, not just T039.
- Commit after each task or logical group (see the optional `before_tasks`/`after_tasks` git-commit hooks in `.specify/extensions.yml`).
- Stop at any Checkpoint to validate a story independently before continuing.

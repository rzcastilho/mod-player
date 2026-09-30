# Implementation Plan: Markers Panel Structure

**Branch**: `023-markers-panel-structure` | **Date**: 2026-09-29 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/023-markers-panel-structure/spec.md`

## Summary

This feature gives the Now Playing Markers card the structure a practice session needs (`UX-24`). Today it is one flat list with "Clear all markers" sitting beside "New loop region". All changes are UI-only, inside `crates/modplayer-ui`. The core model, controller, engine and persistence are untouched (FR-022).

1. **Three ordered groups with counted headings**: Loop Region, then Points, then Cues. A pure per-frame `PanelModel` snapshot of `TrackMarkers` partitions markers by `MarkerKind`, orders them by position and derives the counts. The Loop and Cues groups always render once anything exists. Points renders only when non-empty. Cues always shows all 8 slots, with free slots as muted, non-interactive rows (research R3, R4, R12).
2. **Loop Region blocks**: each region renders as its own block. The block shows its A row, then its B row, then its loop cells (arm, repeat, crossfade, wraps, armed-inactive, clamped). The cells now follow the *last* present boundary, so a region with only a B marker finally shows them (research, baseline table).
3. **Richer rows**: each row shows, in order:
   - a swatch that opens an 8-colour palette popover, using the existing `recolor_marker`
   - the role label
   - a name you click once to rename, with an "Add name" placeholder when empty and commit on Enter or focus loss
   - the `mono` position and the clamped ⚠
   - four always-visible Quiet icon actions: jump (`seek_frames`), nudge earlier and nudge later (`nudge_marker(.., 1)`), and remove (`delete_marker`)

   These reuse the exact controller paths the keyboard uses, so pointer and keyboard give the same result by construction (research R5–R10).
4. **Deterministic mutation**: widgets emit `PanelIntent`s. They are applied after render, with any rename commit or cancel first. This guarantees the "rename commits before another row's action" edge case and removes today's mid-frame stale-snapshot hazard (research R2).
5. **Keyboard parity**: the focus order in each row is swatch → name → jump → nudge− → nudge+ → remove. Any row control that gains focus sets `focused_marker`, so the 006 key table works from rows. The key table's `Enter` → rename alias is gated to the lane glyphs and name cells, so Enter on an action runs only that action. After a remove, focus moves to the next row, else the previous row, else "New loop region" (research R8, R11).
6. **Destructive action separated**: "New loop region" (Default variant) is the first line of the Loop group. "Clear all markers" (Destructive) moves to a right-aligned footer below the Cues group, and its two-step confirmation is unchanged. The empty state shows only `markers-empty` and "New loop region" (research R13).
7. **Strings**: 11 new en-US Fluent keys in `playback.ftl` (contract P10). Every colour, spacing and glyph comes from `theme/**` (FR-023, FR-024).

**Assumption recorded** (headless run, nothing escalated): the spec's clarifications settled all product behaviour. Research R1–R15 records the implementation choices the spec left open. The most consequential are:

- **Frame-level reflection**: the rename or recolour shows on the lanes on the next rendered frame, with an immediate repaint requested (R9). The lanes are drawn before the panel, and 021 fixes that order.
- **Enter gate** on row actions (R8).
- **Keyboard `Delete` focus**: it keeps its 006 focus behaviour; only the new pointer and keyboard *remove action* moves focus (R11). This follows FR-022.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable; `rust-toolchain.toml`, workspace `rust-version = "1.95"`)

**Primary Dependencies**: `egui`/`eframe` 0.36 (AccessKit enabled). The palette uses `egui::Popup` with `PopupCloseBehavior::CloseOnClickOutside`, the same pattern as `settings/category_row.rs`. Existing workspace crates are used unchanged: `modplayer-core` (`PlaybackController::{seek_frames, nudge_marker, delete_marker, rename_marker, recolor_marker, select_marker, new_loop_region, clear_all_markers}`, `markers::{TrackMarkers, Marker, LoopRegion, CueSlot, PaletteIndex}`, `tr`/`tr_args`). No new crates or dependencies.

**Storage**: N/A. There are no persistence changes. The two new fields on `WaveformState` (`rename_focus_pending`, `panel_focus`) are session-only (data-model §5).

**Testing**: `cargo test --workspace`:

- `PanelModel` unit tests and a `proptest` of partition invariants in `crates/modplayer-ui/src/markers.rs` `#[cfg(test)]`. The constitution requires proptest for marker arithmetic.
- egui-harness and AccessKit-tree integration tests extending `crates/modplayer-ui/tests/markers.rs` (contract §9).
- `crates/modplayer-ui/tests/fluent_keys.rs`, extended for the new keys.
- `crates/modplayer-ui/tests/design_token_literals.rs`, whose baseline must stay unchanged.
- Manual scenarios M1–M11 in [quickstart.md](./quickstart.md).

**Target Platform**: Desktop macOS, Windows and Linux (egui/wgpu). Manual runs are on macOS.

**Project Type**: Desktop application: a single Cargo workspace with one crate per component.

**Performance Goals**: The panel stays well inside one 60 fps frame. The snapshot is O(n log n) for n ≤ 64 markers, plus 8 fixed cue rows and at most ~6 widgets per row. Row actions take effect on the first frame after input, with a repaint requested (SC-004).

**Constraints**:

- Zero change to the real-time path, the engine, core APIs, keyboard shortcuts, persistence, the 64-marker limit, or the crossfade and repeat model (FR-022).
- No colour, font-size or spacing literal outside `crates/modplayer-ui/src/theme/**` (FR-023).
- Every new string is externalized (FR-024).
- Every control is keyboard-operable and has an accessible name (NFR-6.1, NFR-6.2).
- Colour is never the only carrier of meaning (NFR-6.4).

**Scale/Scope**:

- 1 crate touched (`modplayer-ui`) and 1 locale file (`locales/en-US/playback.ftl`).
- Source edits: `src/markers.rs` (main refactor, ~+350 lines), `src/waveform/state.rs` (2 fields and `PanelFocus`), `src/theme/markers.rs` (4 glyph constants).
- Test edits: `tests/markers.rs`, `tests/fluent_keys.rs`.
- The spec's 25 FRs map to 11 manual scenarios and about 25 automated tests.

No open unknowns remain. Every implementation question is resolved in [research.md](./research.md) R1–R15.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: **III, VII, VIII, X**. Not touched: I, II, IV, V, VI, IX (reasons below).

- [x] **I. Real-Time Path Is Sacred**: N/A. The feature only changes egui panel layout on the UI thread. Every effect goes through existing `PlaybackController` methods whose queue and atomic traffic is unchanged: `seek_frames` is the same path as a waveform click or cue jump, and nudge, delete and recolour follow 006. `modplayer-engine` is not modified, so no real-time safety note is needed.
- [x] **II. Plugins Are Guests**: N/A. No plugin API, gateway or runtime change. Plugin-owned markers render in the same groups by `kind`, and their ownership checks in core are unchanged.
- [x] **III. Host Primitives, Plugin Behaviors**: Markers, regions and cues stay host primitives owned by `modplayer-core`. The panel is a read-only projection (`PanelModel`) and mutates only through existing controller methods. No marker semantics are reimplemented in the UI (research R10).
- [x] **IV. Audio Source Replaceable and Isolated**: N/A. No audio-source code is touched. All tests run on the synthetic source through the existing `active_controller` harness.
- [x] **V. No Audio Ever Leaves the Engine**: N/A. No sample data, cache or export path is involved. "Jump" is a seek, not a buffer access.
- [x] **VI. Security and Privacy by Default**: N/A. No credentials, network, telemetry or signed artefacts. Marker names are already user data persisted by 006, and nothing new is logged.
- [x] **VII. Rust Quality Gates**:
  - No new crate and no `unsafe`.
  - No `unwrap`/`expect` outside tests. Controller `Result`s are handled or ignored as today (`let _`, matching 006's panel).
  - The new public items (`PanelFocus`, the new `WaveformState` fields and the `ROW_ACTION_*_GLYPH` constants) carry doc comments with `# Examples` doctests that run under `cargo test --doc` (T005, T006). `PanelModel` is private and is covered by unit tests rather than doctests.
  - The fmt, clippy `-D warnings`, test and deny gates are listed in quickstart §1.
- [x] **VIII. Test What the NFRs Promise**:
  - Tests are written first (contract §9).
  - The proptest of `PanelModel` invariants P1–P4 covers marker arithmetic, which the constitution requires.
  - Keyboard reachability and accessible names are asserted on the AccessKit tree (NFR-6.1 and NFR-6.2).
  - The implementing agent executes manual scenarios M1–M11.
  - This feature adds no new NFR-10.3 category, such as loop-seam or latency.
- [x] **IX. One Plugin API Definition**: N/A. The plugin API schema and versions are untouched. `PanelIntent`, `PanelModel` and `PanelFocus` are internal host UI types.
- [x] **X. Simplicity, Portability, User's Override**:
  - No trait, no feature flag and no new crate. Behaviour is identical on all platforms (pure egui).
  - Every new control is keyboard-operable with an externalized accessible name (FR-014, FR-024).
  - The strings are en-US only. pt-BR is project-level work outside this feature (spec Clarification 13); every key is externalized, so pt-BR stays purely additive.
  - No new shortcut is added.

**Post-design re-check (after Phase 1)**: all gates still pass. The design added the private types `PanelModel`, `RowData`, `LoopBlock`, `CueSlotRow` and `PanelIntent`, one public enum `PanelFocus`, two state fields and four glyph constants. There are no new abstraction layers, no core API additions and no violations. Complexity Tracking below records design choices and rejected alternatives only.

## Project Structure

### Documentation (this feature)

```text
specs/023-markers-panel-structure/
├── plan.md                          # This file
├── spec.md                          # Feature spec (clarified)
├── research.md                      # Phase 0: R1–R15
├── data-model.md                    # Phase 1: PanelModel, RowData, LoopBlock, CueSlotRow, PanelIntent, WaveformState additions
├── quickstart.md                    # Phase 1: automated gates + manual M1–M11
├── contracts/
│   └── ui-markers-panel.md          # P1–P11 + §9 test map (supersedes 006 contracts/ui-markers.md §4 layout)
├── checklists/
│   └── requirements.md              # from /speckit-specify
└── tasks.md                         # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
crates/modplayer-ui/
├── src/
│   ├── markers.rs                # panel(): PanelModel snapshot → group renderers → apply_intents;
│   │                             # show_marker_row (swatch popover, click-to-rename, row actions, focus → focused_marker),
│   │                             # show_region_cells after last boundary, empty cue rows, footer Clear-all;
│   │                             # handle_focused_marker_keys: Enter-alias gate (R8); #[cfg(test)] PanelModel unit + proptest
│   ├── waveform/state.rs         # WaveformState { rename_focus_pending, panel_focus }, pub enum PanelFocus
│   ├── theme/markers.rs          # ROW_ACTION_{JUMP,NUDGE_EARLIER,NUDGE_LATER,REMOVE}_GLYPH
│   └── now_playing.rs            # no change expected (panel signature unchanged); listed for review only
└── tests/
    ├── markers.rs                # contract §9 integration tests
    └── fluent_keys.rs            # new markers-* keys
locales/en-US/playback.ftl        # new "Markers panel — structure (023)" section (contract P10)
```

**Structure Decision**: This is the existing single Cargo workspace. All code changes land in the existing crate `crates/modplayer-ui`, with strings in `locales/en-US/playback.ftl`:

- The panel stays in `crates/modplayer-ui/src/markers.rs`, which already owns the panel, the lane glyphs and the key table and shares their private helpers (research R1).
- Session UI state stays in `crates/modplayer-ui/src/waveform/state.rs`.
- Glyph tokens go in `crates/modplayer-ui/src/theme/markers.rs`.
- The tests extend `crates/modplayer-ui/tests/markers.rs` and `crates/modplayer-ui/tests/fluent_keys.rs`.

No crate or module is added (Principle X). `crates/modplayer-core` is not edited, because every mutation it needs already exists on `PlaybackController`.

## Phase 0 — Research (complete)

See [research.md](./research.md). Key decisions:

- **R1**: the panel stays in `markers.rs`, split into private helpers.
- **R2**: snapshot, then render, then apply intents, with the rename pass first.
- **R3**: explicit partition and sort rules, with a B-only region's cells rendered after B.
- **R4**: headings use the section style plus a mono count, `Role::Heading`, and an externalized accessible name.
- **R5**: row layout with creation order equal to Tab order (left-to-right child in a pre-allocated trailing rect).
- **R6**: Quiet icon actions, glyph constants in `theme/`, and a font-coverage test.
- **R7**: `egui::Popup` palette, a RadioGroup with a ✓ indicator, and `recolor_marker`.
- **R8**: row focus sets `focused_marker`, row controls register `marker_claims`, and the Enter alias is gated.
- **R9**: click-to-rename, one-shot focus request, focus-loss commit, Esc before `lost_focus`, and a repaint.
- **R10**: jump, nudge and remove reuse `seek_frames`, `nudge_marker(..,1)` and `delete_marker`, with no core change.
- **R11**: post-remove focus via the one-shot `panel_focus`.
- **R12**: empty cue rows are muted `text_secondary` static text.
- **R13**: New loop region (Default) goes first in the Loop group; Clear all goes in the footer.
- **R14**: 11 Fluent keys.
- **R15**: test strategy.

## Phase 1 — Design & Contracts (complete)

- [data-model.md](./data-model.md): `PanelModel` with invariants P1–P4, `RowData`, `LoopBlock`/`RegionCells`, `CueSlotRow`, the `PanelIntent` table and application order, the `WaveformState` additions and `PanelFocus`, egui-memory state, and visual state per row type.
- [contracts/ui-markers-panel.md](./contracts/ui-markers-panel.md): P1 layout (populated and empty), P2 headings, P3 row cells and Tab stops, P4 palette popover, P5 rename, P6 row actions, P7 keyboard and focus, P8 empty cue row, P9 footer, P10 strings, P11 unchanged surfaces, and the §9 test map.
- [quickstart.md](./quickstart.md): automated gates and manual scenarios M1–M11.
- Agent context: `.specify/scripts/bash/` has no agent-context update script, so there is nothing to run.

## Implementation Order (input for /speckit-tasks)

1. **Foundation**. This blocks all stories.
   - Write the `PanelModel` snapshot (partition, sort, counts, `visual_order`) with its unit tests and proptest, failing first.
   - Add the `PanelIntent` enum, `apply_intents` with its two-pass order, and the new `WaveformState` fields and `PanelFocus`.
   - Add the Fluent keys and extend `fluent_keys.rs`.
   - Add the glyph constants and the font-coverage test.
2. **US1 (P1): grouping**. Status line, then the Loop group (heading, New loop region, blocks with cells after the last boundary), then Points (conditional), then Cues (8 slots, empty rows muted). Remove the flat loop.
3. **US2 (P1): row actions and in-place edit**.
   - Row layout with the trailing column in creation order.
   - Jump, nudge and remove intents; post-remove focus.
   - Click-to-rename with placeholder and focus-loss commit.
   - Palette popover.
   - Row focus → `focused_marker`, `marker_claims`, arrow filter, and the Enter gate in `handle_focused_marker_keys`.
4. **US3 (P2): destructive separation**. Move `clear_all_controls` into the footer; set New loop region to the explicit Default variant.
5. **US4 (P3): empty state**. Empty layout (`markers-empty` then New loop region only), and verify the 8-slot rule.
6. **Polish**. Literal scan, full regression suite, manual M1–M11 with evidence.

US1 must land before US2–US4, because they render inside its groups. US3 and US4 are independent of US2 and can proceed in parallel with it.

## Complexity Tracking

No constitution violations. The table below records design choices and their rejected alternatives, because the spec left these to plan time:

| Decision | Why | Simpler / Other Alternative Rejected Because |
|---|---|---|
| Intent queue applied after render (R2) | Makes "rename commits before another row's action" deterministic and removes the mid-frame stale snapshot | Immediate mutation, as today, would spread the ordering rule across five call sites and keep the stale-row hazard |
| Enter-alias gate in `handle_focused_marker_keys` (R8) | Row controls must drive the key table (Clarification 6), but Enter on "remove" or the swatch must not also open a rename | Not setting `focused_marker` from rows keeps today's gap, where the key table works only from glyphs. A single roving-tabindex row contradicts Clarification 6's six tab stops. |
| Trailing column built left to right in a pre-allocated rect (R5) | Keeps widget creation order equal to visual and Tab order while staying right-aligned | Plain `right_to_left` reverses the Tab order |
| Font-glyph icons plus a coverage test (R6) | Quiet icon actions with no new paint code | Text labels compete with the name. Vector icons add a paint path for no gain. |
| `markers-group-heading` and `markers-palette` keys beyond FR-024's minimum (R4, R7) | Accessible names stay fully externalized | `format!("{label}, {count}")` would hard-code punctuation outside Fluent (NFR-7.1) |
| Panel `remove` moves focus; keyboard `Delete` keeps its 006 behaviour (R11) | Clarification 8 governs the row action. FR-022 forbids changing shortcut behaviour. | Aligning `Delete` would be a keyboard behaviour change outside scope |
| Lane reflection on the next frame with a forced repaint (R9) | 021 fixes lanes above the panel, and the model mutates during the panel pass | Re-ordering the lanes after the panel breaks 021's layout contract. A second lane pass wastes a frame of paint. |

**Assumptions recorded** (the spec left these open; no escalation needed):

- The glyph choices (↦ ◂ ▸ ✕) are provisional pending the font-coverage test. A substitution is allowed if the test fails, and must be recorded in research R6.
- The popover's swatch index in `markers-color { $index }` is 1-based. The current code passes the 0-based raw index, which is a latent 006 bug fixed here.
- A `Remove` intent whose marker was already removed earlier in the same pass is a silent no-op.

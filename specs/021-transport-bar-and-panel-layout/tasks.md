# Tasks: Sticky Transport Bar and Panel Layout

**Input**: Design documents from `/specs/021-transport-bar-and-panel-layout/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution (VIII) and every contract rule carry a test
obligation (T-B*, T-S*, T-C*, T-R*, T-Q*, T-N*, T-L*, T-K, T-QC); this feature
does not skip them.

**Organization**: Tasks are grouped by user story (spec.md priorities). Stories
2 and 3 build on User Story 1's structural rewrite of the same file
(`now_playing.rs`) and are not independently *implementable* in parallel with
it, even though each has its own independent *test*; see Dependencies below.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1–US4, mapped to spec.md's user stories
- File paths are exact and relative to the repository root
- **[Req: …]**: the spec.md requirement IDs the task implements or verifies
  (FR-001–FR-019, FR-3.2.2, NFR-6.1/6.2/6.4, NFR-7.4, SC-*), per the
  constitution's Governance traceability rule

---

## Phase 1: Setup

**Purpose**: Confirm the branch builds clean before changing it. No new
dependencies, crates, or scaffolding are needed (plan.md Technical Context).

- [X] T001 Run `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test --workspace` on the
      unmodified branch to record a clean baseline before any edit in this
      feature [Req: baseline for FR-001–FR-019]

**Checkpoint**: Baseline green.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core model and shared UI primitives that every user story's
implementation calls. No user story can be implemented until this phase is
done.

- [X] T002 [P] Add `NowPlayingPanel::Markers` variant and a `RENDER_ORDER`
      associated const `[Markers, EffectChain, Transport, Queue]` in
      `crates/modplayer-core/src/controller.rs` (data-model §1) [Req: FR-004, FR-006]
- [X] T003 Extend the `now_playing_panel_open` / `set_now_playing_panel_open`
      match arms to cover `NowPlayingPanel::Markers` in
      `crates/modplayer-core/src/controller.rs` (depends on T002) [Req: FR-006]
- [X] T004 [P] Add `markers_open: bool` to `NowPlayingPanels` and
      `RawNowPlayingPanels` (`#[serde(default = "default_true")]` on the raw
      side) and replace the derived `Default` with a hand-written one
      (`markers_open: true`, the other three `false`) in
      `crates/modplayer-core/src/settings/model.rs` (data-model §2, contract
      N1) [Req: FR-006]
- [X] T005 Unit tests T-N1 (absent key ⇒ `markers_open = true`; absent section
      ⇒ `(true, false, false, false)`) and T-N2 (independence across all four
      flags, both directions) in `crates/modplayer-core/src/settings/model.rs`
      (depends on T004) [Req: FR-006]
- [X] T006 Proptest T-N5: serialise → parse → `into_settings` round trip over
      all 16 combinations of the four bools in
      `crates/modplayer-core/src/settings/model.rs` (depends on T004) [Req: FR-006]
- [X] T007 Unit test T-N3: controller set → reload the store → read back, for
      `NowPlayingPanel::Markers`, extending the existing all-variants test near
      `crates/modplayer-core/src/controller.rs:5158` (depends on T002, T003) [Req: FR-006]
- [X] T008 [P] Add a `NowPlaying` variant to `ViewKey` (unit `Hash` arm, like
      `Search`/`Plugins`) in `crates/modplayer-ui/src/section_memory.rs`,
      replacing the "Now Playing has no entry" doc line (data-model §5) [Req: FR-003]
- [X] T009 [P] Add pure functions `reveal_align(card: Rangef, viewport: Rangef)
      -> Option<Option<Align>>` and `identity_width(bar_width: f32) -> f32`
      with runnable doc examples in `crates/modplayer-ui/src/layout.rs`
      (data-model §6, research R4/R5) [Req: FR-002, FR-007, FR-017, NFR-7.4]
- [X] T010 Unit tests and proptests T-L1 — `reveal_align` properties P-R1–P-R3
      and `identity_width` properties P-I1–P-I2 — in
      `crates/modplayer-ui/src/layout.rs` (depends on T009) [Req: FR-002, FR-007, FR-017, NFR-7.4]
- [X] T011 [P] Add `CardResponse { rect: Rect, toggled: bool }` and
      `collapsible_panel_card(ui, header, open: &mut bool, add_contents) ->
      CardResponse` (016 card chrome unchanged; header disclosure button,
      `Role::Button`, `expanded = open`) with a doc example in
      `crates/modplayer-ui/src/widgets/controls.rs` (data-model §7, contract
      C1) [Req: FR-004, FR-006, FR-016, NFR-6.1, NFR-6.2]
- [X] T012 [P] Add fluent keys `panel-collapse` ("Collapse { $panel }") and
      `panel-expand` ("Expand { $panel }") in `locales/en-US/controls.ftl`
      (contract C1) [Req: FR-004, FR-016, NFR-6.2]

**Checkpoint**: Core model and shared widgets exist. User story phases can
begin.

---

## Phase 3: User Story 1 - The transport never disappears (Priority: P1) 🎯 MVP

**Goal**: Split the Now Playing content into a pinned transport bar and one
scroll region below it holding the status line/banner, waveform, and the four
panel cards, so every transport control stays visible and operable at any
scroll offset, panel state, or window height.

**Independent Test**: Open any one panel with content taller than the window,
scroll it to the bottom, and confirm every transport control is still visible
and responds to a click.

### Implementation for User Story 1

- [X] T013 [US1] As the first statement of `now_playing::show`, capture
      `waveform_h = ui.max_rect().height()` before the dock, bar, or scroll
      area touch `ui`, in `crates/modplayer-ui/src/now_playing.rs` (research
      R3) [Req: FR-003, FR-011]
- [X] T014 [US1] Draw `Panel::top("now-playing-transport-bar")
      .resizable(false).show_inside` after the 018 docked column, as atomic
      `wrap_group_before`-guarded groups: identity (40 px artwork +
      `identity_width`-wide title/artist, `Label::truncate()`, full text in
      `on_hover_text` and the `now-playing-bar-identity` AccessKit label),
      transport (skip back/play-pause/stop/skip forward), time
      (elapsed/remaining, `theme::mono_text`), level (master volume over the
      peak meter), an `XL` gap then the Queue/Effects/Transport toggles via
      `switch(ui, SwitchKind::Toggle, …)`, then 018's Panels toggle under its
      existing conditions, in `crates/modplayer-ui/src/now_playing.rs`
      (contract B1, B2, B4, B6, B7; research R4; depends on T009, T011, T013) [Req: FR-001, FR-002, FR-017, FR-018, FR-019, NFR-6.2, NFR-6.4, NFR-7.4]
- [X] T015 [US1] Delete `show_heading` (96 px artwork, `display`-role title,
      artist/album lines) and the old standalone control row in
      `crates/modplayer-ui/src/now_playing.rs` (research R12; depends on T014) [Req: FR-018]
- [X] T016 [US1] Build the single `ScrollArea` from
      `SectionMemory::scroll_area(&ViewKey::NowPlaying)`, `.animated(false)`,
      `.scroll_source(ScrollSource { drag: false, ..ScrollSource::ALL })`,
      containing (top → bottom) the status line / transfer banner, the
      waveform region, then the Markers / Effect Chain / Transport focus /
      Queue cards, each pair (and waveform → first card) separated by
      `space::XL`, in `crates/modplayer-ui/src/now_playing.rs` (contract S1,
      S3, C4; research R2; depends on T008, T013, T015) [Req: FR-003, FR-004, FR-005]
- [X] T017 [US1] Delete the Effect Chain's inner `ScrollArea` and the
      `effects_panel_reserved_height` calculation in
      `crates/modplayer-ui/src/now_playing.rs` and
      `crates/modplayer-ui/src/effects_view.rs` (contract S1; depends on T016) [Req: FR-003, FR-004]
- [X] T018 [P] [US1] In `pointer_zoom_or_pan`, when a Shift+vertical wheel
      produces a `Pan`, zero the consumed delta with
      `ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO)` so the new
      enclosing `ScrollArea` doesn't also scroll the page, in
      `crates/modplayer-ui/src/waveform/input.rs` (contract S5; research R11) [Req: FR-003]
- [X] T019 [P] [US1] Draw the Markers panel body inside
      `collapsible_panel_card`, gated on `controller.current_track().is_some()`,
      card chrome owned by the caller, in `crates/modplayer-ui/src/markers.rs`
      (contract C1, C2, C4) [Req: FR-004]
- [X] T020 [US1] Draw the Effect Chain body inside `collapsible_panel_card` in
      `crates/modplayer-ui/src/effects_view.rs` (contract C1, C2; depends on
      T017) [Req: FR-004]
- [X] T021 [P] [US1] Draw the Transport focus body inside
      `collapsible_panel_card` in `crates/modplayer-ui/src/transport_view.rs`
      (contract C1, C2) [Req: FR-004]
- [X] T022 [US1] Draw the Queue card's chrome via `collapsible_panel_card` in
      `crates/modplayer-ui/src/now_playing.rs`; `queue_view::show` becomes
      body-only (header controls — shuffle, repeat — stay as they are) in
      `crates/modplayer-ui/src/queue_view.rs` (contract C1, C2, C4; depends on
      T016) [Req: FR-004]
- [X] T023 [US1] On a header disclosure click, write the flag through
      `set_now_playing_panel_open` and call
      `ui.ctx().request_discard("panel header toggle")` so the bar toggle and
      the card agree in the same output, in
      `crates/modplayer-ui/src/now_playing.rs` (contract C3; research R7;
      depends on T014, T019, T020, T021, T022) [Req: FR-006]
- [X] T024 [US1] Record `Section::NowPlaying`'s scroll offset through
      `ViewKey::NowPlaying`, the same way Search and Plugins do, dropping the
      now-unused `memory_epoch` parameter of `now_playing::show`, in
      `crates/modplayer-ui/src/now_playing.rs` and
      `crates/modplayer-ui/src/app.rs` (contract S4; depends on T016) [Req: FR-003]
- [X] T025 [P] [US1] Add the `now-playing-bar-identity` fluent key
      ("{ $title }, { $artist }") in `locales/en-US/playback.ftl` (contract
      B4) [Req: FR-002, FR-016, NFR-6.2]

### Tests for User Story 1

- [X] T026 [US1] Tests T-B1 (all panels open, 960×640, scroll to max, bar
      controls stay in-bounds and clickable), T-B3 (bar height ±1 px across
      the window/scroll/panel/state/title matrix), T-B4 (200-char title, one
      line, ellipsis, full text in the AccessKit label), T-B6 (toggles report
      `Toggled`, ≥`XL` from skip forward), T-S1 (no nested scroll area; grep
      for `effects_panel_reserved_height`), T-S3 (no `Separator`/hline in the
      scroll region; `XL` gaps), T-C2 (collapsed card renders header-only),
      T-C3 (header click ⇒ bar toggle `Toggled::False` in the same output),
      T-C4 (card order/presence with and without a track), T-C6 (no volume or
      meter node below the bar); also update the 014 test that asserted a
      `display`-role Now Playing title to assert the bar's title and
      accessible name instead, in `crates/modplayer-ui/tests/now_playing.rs`
      (depends on T013–T024) [Req: FR-001, FR-002, FR-003, FR-004, FR-005, FR-006, FR-010, FR-019, SC-001]
- [X] T027 [P] [US1] Test T-B5: at 960×640 with +40% pseudo-localisation and
      at dock width 240, no bar control label is elided and no two
      interactive rects overlap, in
      `crates/modplayer-ui/tests/responsive_dock.rs` (depends on T014) [Req: FR-017, NFR-7.4]
- [X] T028 [P] [US1] Update the 016 C11 interactive-count assertion for
      Markers' new header disclosure in `crates/modplayer-ui/tests/markers.rs`
      (depends on T019) [Req: FR-004, FR-016]

**Checkpoint**: The bar is pinned, there is exactly one scroll region with the
four cards in order, and every transport control stays visible and usable at
any scroll offset — User Story 1 is independently testable and demoable.

---

## Phase 4: User Story 2 - A panel toggle behaves like navigation (Priority: P1)

**Goal**: Pressing a closed Queue/Effects/Transport toggle opens its panel and
scrolls it into view in the same interaction; pressing an open one collapses
it, even off screen. Keyboard shortcuts behave the same way.

**Independent Test**: With a panel closed and the screen scrolled away from
the top, press its toggle and confirm the panel opens and is visible with no
further scrolling; press it again and confirm it collapses.

**Depends on**: User Story 1 (the bar, the scroll region and the cards must
already exist for a toggle to have anything to open or reveal).

### Implementation for User Story 2

- [X] T029 [US2] Add `RevealRequest { panel: NowPlayingPanel, pass: u64 }` in
      egui temp memory under `Id::new("now-playing-reveal")`, with
      `now_playing::request_reveal(ctx, panel)` (writes) and
      `take_reveal(ctx, panel) -> bool` (returns `true` and clears the request
      when `ctx.cumulative_pass_nr() - pass ≤ 1`; a newer request replaces an
      older one) in `crates/modplayer-ui/src/now_playing.rs` (data-model §4;
      research R6) [Req: FR-007]
- [X] T030 [US2] Call `request_reveal` from the bar's toggle when
      `switch(...).changed()` turns a panel on, in
      `crates/modplayer-ui/src/now_playing.rs` (depends on T029; contract R1) [Req: FR-007]
- [X] T031 [US2] After each card is drawn, compute
      `layout::reveal_align(card_rect, ui.clip_rect())` from `take_reveal` and
      call `ui.scroll_to_rect(card_rect, align)` when it returns
      `Some(align)`, in `crates/modplayer-ui/src/now_playing.rs` (contract R1,
      R2, R3, R6; depends on T029) [Req: FR-007]
- [X] T032 [P] [US2] Change `toggle_effect_chain_panel` to return `bool` (the
      panel's new open state) in `crates/modplayer-ui/src/effects_view.rs` [Req: FR-009]
- [X] T033 [P] [US2] Change `toggle_transport_panel` to return `bool` (the
      panel's new open state) in `crates/modplayer-ui/src/transport_view.rs` [Req: FR-009]
- [X] T034 [US2] In `actions::invoke`, rename the unused `_ctx` parameter to
      `ctx`; for `ToggleQueue`, `ToggleEffectChain` and
      `ToggleTransportPanel`, call `request_reveal` when the toggle function
      returns `true`, in `crates/modplayer-ui/src/actions.rs` (contract R5;
      research R6; depends on T029, T032, T033) [Req: FR-009, NFR-6.1]

### Tests for User Story 2

- [X] T035 [US2] Tests T-R1 (Queue/Effects/Transport open ⇒ revealed in the
      same pass, including a 16-node-chain case where the header aligns to
      the viewport top), T-R2 (already-visible ⇒ offset unchanged), T-R4
      (collapsing an off-screen panel changes the offset only by the shorter-
      content clamp), in `crates/modplayer-ui/tests/now_playing.rs` (depends
      on T029–T031) [Req: FR-007, FR-008, SC-002]
- [X] T036 [P] [US2] Test T-R5: through
      `actions::dispatch_and_invoke` with the current bindings, the same
      open-and-reveal / collapse behaviour as the bar toggle; and the
      not-shown-section case (flag flips, no reveal, none deferred), in
      `crates/modplayer-ui/tests/actions.rs` (depends on T034) [Req: FR-009, NFR-6.1]

**Checkpoint**: Toggling a panel from the bar or its shortcut opens and reveals
it, or collapses it, in one interaction — User Story 2 is independently
testable.

---

## Phase 5: User Story 3 - The waveform grows with the window (Priority: P2)

**Goal**: The waveform's height follows 018's `H`-based formula and grows with
window height, while the transport bar's height never changes.

**Independent Test**: Note the bar and waveform heights at the default window
size; make the window taller; confirm the waveform grows and the bar doesn't.

**Depends on**: User Story 1 (the `H` capture point and the scroll-region
placement of the waveform are part of its structural rewrite).

### Implementation for User Story 3

- [X] T037 [US3] Confirm `layout::waveform_heights(waveform_h)` (018's
      unchanged formula) is called with the value captured in T013 — before
      the dock, bar or scroll area are drawn — so it stays independent of
      scroll offset and of the bar's wrapped height, in
      `crates/modplayer-ui/src/now_playing.rs` (contract S2; depends on T013,
      T016) [Req: FR-003, FR-011]

### Tests for User Story 3

- [X] T038 [P] [US3] Test T-S2: overview and detail heights at
      H ∈ {640, 820, 1200} equal `waveform_heights(H)`, independent of scroll
      offset, extending 018's D10.9 coverage, in
      `crates/modplayer-ui/tests/waveform.rs` (depends on T037) [Req: FR-003, FR-011, SC-003]

**Checkpoint**: The waveform scales with window height and the bar's height is
unaffected — User Story 3 is independently testable (T-B3 in Phase 3 already
covers the bar-height side of this).

---

## Phase 6: User Story 4 - The queue reads at a glance (Priority: P2)

**Goal**: Queue rows use the shared list-row layout (artwork, title over
artist, right-aligned position), the current row is marked by more than
colour, and row actions are quiet and always visible.

**Independent Test**: Open the Queue panel with several items including the
current one; confirm the current entry is identifiable without reading any
text, and every row shows artwork, title-over-artist, and a right-aligned
position.

**Note**: Implementable in parallel with User Stories 2 and 3 once Phase 2 and
T022 (US1's Queue-card wiring) are done — it touches `modplayer-core` and
`rows.rs`/`queue_view.rs`, not the bar or reveal logic.

### Implementation for User Story 4

- [X] T039 [P] [US4] Add `artwork_url: Option<String>` and `artwork_name:
      String` (album, else title) to `QueueRow`; fill both in `queue_view()`
      from `item.track` in `crates/modplayer-core/src/controller.rs`
      (data-model §3; research R9) [Req: FR-012, FR-3.2.2]
- [X] T040 [US4] Unit test T-QC: `queue_view()` fills `artwork_url` and
      `artwork_name` per the album-else-title rule, in
      `crates/modplayer-core/src/controller.rs` (depends on T039) [Req: FR-012]
- [X] T041 [P] [US4] Factor `draw_artwork_url(ui, cache, url, name)` out of
      the private `draw_artwork` in `crates/modplayer-ui/src/rows.rs` [Req: FR-012]
- [X] T042 [US4] Add `QueueRowAction { MoveUp, MoveDown, PlayNext, Remove }`
      and `rows::queue_row(ui, artwork, row: &QueueRow, position: usize) ->
      Option<QueueRowAction>`: 40 px artwork (placeholder via
      `artwork_name` when there's no URL or the fetch fails), for the current
      row a leading `nav_indicator`-stroke accent bar plus a ▶
      (`queue-playing-glyph`) before the title, title/artist on separate
      truncated lines, four quiet actions (Play next absent on the current
      row) each their own tab stop, a `duration_measure`-wide right-aligned
      `theme::mono_text` position column, and a `Role::ListItem` node labelled
      `queue-row-name` or `queue-row-name-current`, in
      `crates/modplayer-ui/src/rows.rs` (data-model §8; contract Q1–Q11;
      research R8; depends on T039, T041) [Req: FR-012, FR-013, FR-014, FR-015, FR-016, FR-3.2.2, NFR-6.1, NFR-6.2, NFR-6.4]
- [X] T043 [US4] `queue_view::show` renders each row via `rows::queue_row` and
      applies the returned `QueueRowAction` to `queue_move_up`,
      `queue_move_down`, `queue_play_next` or `queue_remove` with the row's
      `uid`; header controls (shuffle, repeat) and the empty-state copy are
      unchanged, in `crates/modplayer-ui/src/queue_view.rs` (contract Q8, Q9;
      depends on T022, T042) [Req: FR-015, FR-3.2.2]
- [X] T044 [P] [US4] Add fluent keys `queue-row-name`, `queue-row-name-current`
      and `queue-playing-glyph`; delete `queue-current`, in
      `locales/en-US/playback.ftl` (contract Q7, Q11) [Req: FR-013, NFR-6.2]

### Tests for User Story 4

- [X] T045 [US4] Tests T-Q1 (3-column geometry), T-Q2 (missing-artwork
      placeholder), T-Q3 (960 px + 40% pseudo-localisation: no elided action
      label, no overlapping rects), T-Q5 (current row paints the ▶ glyph and
      the `nav_indicator` bar; no other row does), T-Q6 (no `accent`-filled
      row background), T-Q7 (no painted "Now playing:" text — replaces the
      assertion at `tests/queue_view.rs:221`), T-Q8 (actions painted on every
      row without hover), T-Q9 (each action applies to its own row's `uid`;
      Play next absent on the current row), T-Q10 (tab order visits each
      row's actions in order; Enter on a focused Remove removes that row),
      T-Q11 (`ListItem` label equals `queue-row-name-current` for the current
      row), T-Q12 (single-item current-only queue renders correctly), in
      `crates/modplayer-ui/tests/queue_view.rs` (depends on T039–T044) [Req: FR-012, FR-013, FR-014, FR-015, FR-016, FR-3.2.2, NFR-6.1, NFR-6.2, NFR-6.4, NFR-7.4, SC-004, SC-005]

**Checkpoint**: Queue rows show artwork, title-over-artist and position; the
current row is marked by shape and position, not colour alone — User Story 4
is independently testable.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Whole-feature validation once every story's tests exist.

- [X] T046 [P] Test T-K: every new fluent key (`panel-collapse`,
      `panel-expand`, `now-playing-bar-identity`, `queue-row-name`,
      `queue-row-name-current`, `queue-playing-glyph`) resolves, and
      `queue-current` is gone from the key list, in
      `crates/modplayer-ui/tests/fluent_keys.rs` (depends on T012, T025, T044) [Req: FR-016, NFR-6.2]
- [X] T047 Run the full quickstart gate, in order, each with
      `RUSTUP_TOOLCHAIN=1.95.0`: `rtk cargo fmt --check`; `rtk cargo clippy
      --workspace --all-targets --all-features -- -D warnings`; `rtk cargo
      test -p modplayer-core --lib settings::model controller`; `rtk cargo
      test -p modplayer-ui --lib layout`; `rtk cargo test -p modplayer-ui
      --test now_playing --test queue_view --test responsive_dock --test
      waveform --test actions --test markers --test effects_view --test
      transport_view --test section_memory --test fluent_keys --test
      controls`; `rtk cargo test --workspace`; `rtk cargo deny check`
      (depends on T001–T046) [Req: FR-001–FR-019, FR-3.2.2, NFR-6.1, NFR-6.2, NFR-6.4, NFR-7.4]
- [X] T048 Execute manual scenarios M1–M14 from quickstart.md, under the
      constitution's Manual Scenario Sign-Off recipe (build, launch the `.app`
      wrapper, Quartz `CGEventPost` + `screencapture` into
      `target/manual-walk/021-Mx.png`). Record each result below; for any
      deviation, also record it in `quickstart.md` and `research.md` (depends
      on T047) [Req: SC-001–SC-005]

  | # | Scenario | Result | Evidence |
  |---|---|---|---|
  | M1 | Transport never disappears | Pass. Bar pinned at the bottom of a 40-row queue; Pause/Play and volume drag worked while scrolled. Found and fixed tofu chevrons and the Effect Chain overflowing the dock (R15) | target/manual-walk/021-M1-bottom.png, M1-paused.png, M1-vol.png, np4.png | |
  | M2 | Short window | Pass. Bar height unchanged at 960 × 640, overview 64 pt, detail 137 pt, region scrolls; dock auto-hides and Panels is the last control | target/manual-walk/021-M2-1200.png, M2-960.png | |
  | M3 | Waveform grows | Pass at a fixed 1679 pt width: bar edge rows identical at 820 and 996 tall; detail 176.5 → 215.5 pt | target/manual-walk/021-M3-820w.png, M3-max.png | |
  | M4 | Queue toggle reveals | Pass. Card header lands directly under the bar, toggle on | target/manual-walk/021-M4-pre.png, M4.png | |
  | M5 | Effects/Transport toggles reveal | Pass for Effects (2- and 16-node chains; with 16 nodes the header sits under the bar) and for Transport | target/manual-walk/021-M5-effects.png, M5-transport.png, M5-tall.png | |
  | M6 | Toggle collapses | Pass. Off-screen Queue collapses, toggle off, no jump | target/manual-walk/021-M6-pre.png, M6.png | |
  | M7 | Keyboard parity | Pass for Q/E/T on Now Playing. Deviation: Q on Library is a no-op because the action is `Scope::NowPlaying` (unchanged from main; spec keeps existing behaviour; quickstart corrected, R15) | target/manual-walk/021-M7-q.png, M7-q2.png, M7-e.png, M7-t.png, M7-inlib.png | |
  | M8 | Narrow window and 40% text | Pass (real strings). At 960 × 640, controls wrap whole, no label is elided, Panels is last, the title is truncated with '…', and hover shows exactly one full-text tooltip after the fix (R15). Deviation: no runtime +40 % override exists; that case is covered by `responsive_dock` | target/manual-walk/021-M2-960.png, M8.png (before fix), M8c.png (after) |
  | M9 | Header collapse | Pass. Chevron collapse flips the bar toggle in the same frame; Tab to the chevron then Space reopens it; Markers stays collapsed across relaunch, with `markers_open = false` in settings.toml | target/manual-walk/021-M9-collapsed.png, u10 (tab), M9-space2.png, M9-relaunch.png | |
  | M10 | Queue at a glance | Pass. Artwork, title over artist, right-aligned position, ▶ plus accent bar on the current row, no fill, no 'Now playing:' text, actions visible without hover, no Play next on the current row. Tab visits each row's actions in order; Enter on row 2 Remove removed God Eater and focus moved to the next row's Remove | target/manual-walk/021-M4.png, w1–w7.png, M10-removed.png |
  | M11 | Single-item queue | Pass. A 20-result search queue was cut down with Tab plus Enter on Remove until only the current track was left: one row with ▶, the accent bar and position 1, and no Play next | target/manual-walk/021-M11-focus.png, M11.png |
  | M12 | No track | Pass. Fresh `MODPLAYER_CONFIG_DIR`, signed in, nothing played: placeholder ♪ plus 'No track is playing.', no Markers card, and the Effect Chain, Transport and Queue cards and toggles all work. Deviation: transport buttons stay enabled, because contract B7 ties them to `transport_enabled()` (device/health), not to having a track; the disabled state appears when transport is unavailable (seen after re-sign-in, M13). Quickstart wording corrected | target/manual-walk/021-M12.png, M12-q.png, M12-e.png, M12-t.png |
  | M13 | Scroll retention | Pass. The offset was restored after Library → Now Playing. After sign-out and in-app sign-in, Now Playing reopened at the top. Caveat: sign-out also empties the queue, so the page was short. Observed outside 021 (account flow): after an in-session re-sign-in, playback stayed unregistered ('Sign in to play from your account') and Library showed 0 until relaunch; after relaunch it was healthy (R15) | target/manual-walk/021-M13-a.png, M13-b.png, M13-d.png, so2–so5.png, M13-e.png, M13-f.png, relaunch.png |
  | M14 | Shift+wheel pan | Pass. Shift+wheel pans the detail and the page stays; plain wheel over the waveform scrolls the page (harness needed explicit CGEvent flags, R15) | target/manual-walk/021-r6.png, r7.png, r8.png | |

**Checkpoint**: Feature complete — all four user stories pass their tests, the
full automated gate is green, and every manual scenario is recorded.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup. Blocks every user story.
- **User Story 1 (Phase 3)**: depends on Foundational only.
- **User Story 2 (Phase 4)**: depends on Foundational **and** on User Story 1
  (its reveal logic acts on the bar toggle and the cards US1 builds). Not
  parallel with US1.
- **User Story 3 (Phase 5)**: depends on Foundational **and** on User Story 1
  (the `H`-capture point and the scroll-region placement are part of US1's
  rewrite of `now_playing.rs`). Not parallel with US1.
- **User Story 4 (Phase 6)**: depends on Foundational and on T022 (US1's Queue
  card wiring). Otherwise touches only `modplayer-core`, `rows.rs` and
  `queue_view.rs` — safe to run alongside US2/US3.
- **Polish (Phase 7)**: depends on all four user stories.

This feature deviates from the generic "stories are independent after
Foundational" pattern: US1 is a structural rewrite of one file
(`now_playing.rs`) that US2 and US3 both extend, so those two are sequenced
after US1, not parallel with it. US4 is the one genuinely parallel story.

### Within Each User Story

- Tests are written after each story's implementation tasks and must fail
  before those tasks land, then pass after (contract test obligations above).
- Same-file tasks are sequential; only tasks on different files are marked
  `[P]`.

### Parallel Opportunities

- Foundational: T002, T004, T008, T009, T011, T012 (six different files) can
  run together.
- User Story 1: T018 (waveform/input.rs), T019 (markers.rs) and T021
  (transport_view.rs) can run together; T026/T027/T028 (three different test
  files) can run together once their implementation tasks land.
- User Story 2: T032 (effects_view.rs) and T033 (transport_view.rs) can run
  together.
- User Story 4 can run in parallel with US2 and US3 (see note above); within
  it, T039 and T041 (different files) can run together.
- Phase 7: T046 can run as soon as T012/T025/T044 have landed, ahead of T047.

---

## Parallel Example: Foundational

```bash
Task: "Add NowPlayingPanel::Markers + RENDER_ORDER in crates/modplayer-core/src/controller.rs"
Task: "Add markers_open to NowPlayingPanels/RawNowPlayingPanels in crates/modplayer-core/src/settings/model.rs"
Task: "Add ViewKey::NowPlaying in crates/modplayer-ui/src/section_memory.rs"
Task: "Add reveal_align + identity_width in crates/modplayer-ui/src/layout.rs"
Task: "Add CardResponse + collapsible_panel_card in crates/modplayer-ui/src/widgets/controls.rs"
Task: "Add panel-collapse/panel-expand fluent keys in locales/en-US/controls.ftl"
```

## Parallel Example: User Story 4 alongside User Story 2/3

```bash
# Once T022 (US1) and Phase 2 are done:
Task: "Add QueueRow.artwork_url/artwork_name + fill in crates/modplayer-core/src/controller.rs"
Task: "Factor draw_artwork_url out of draw_artwork in crates/modplayer-ui/src/rows.rs"
# ...while US2's T029-T034 proceed in now_playing.rs / effects_view.rs / transport_view.rs / actions.rs
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 — both Priority P1)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational (blocks everything).
3. Complete Phase 3: User Story 1 — **stop and validate**: open each panel,
   scroll to the bottom, confirm the bar stays put (SC-001).
4. Complete Phase 4: User Story 2 — **stop and validate**: press each closed
   toggle, confirm it opens and reveals with no manual scroll (SC-002).
5. This is the feature's MVP: both of the spec's P1 acceptance lines
   (`UX-21`, `UX-22`) are satisfied.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. User Story 1 → validate → the transport is always visible.
3. User Story 2 → validate → toggles open-and-reveal or collapse.
4. User Story 3 → validate → waveform scales with window height.
5. User Story 4 → validate → Queue rows read at a glance.
6. Polish → full automated gate + manual sign-off (M1–M14).

### Notes

- `[P]` tasks touch different files and have no unmet dependency.
- `[Story]` maps each task to spec.md's user stories for traceability.
- Every contract rule (B*, S*, C*, R*, Q*, N*) and NFR-backed proptest (T-L1,
  T-N5) has a task above; none were dropped.
- Commit after each task or logical group (project convention via `rtk git`).
- Stop at any checkpoint to validate that story independently.

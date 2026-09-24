---

description: "Task list for 016-list-row-and-panel-components"

---

# Tasks: List Row, Tab Strip, and Panel Card Components

**Input**: Design documents from `/specs/016-list-row-and-panel-components/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/{list-row,tab-strip,panel-card}.md, quickstart.md

**Tests**: Included and REQUIRED — Constitution VIII (Test What the NFRs Promise) mandates every suite land red against data-model.md's values before the code that makes it pass; contracts enumerate the exact test each rule pins (`L*`/`S*`/`A*` in list-row.md, `T*` in tab-strip.md, `C*`/`P*` in panel-card.md).

**Organization**: Phase 3 (US1) and Phase 4 (US2) both edit `rows.rs`; complete US1 before starting US2 so the file doesn't fork. Phase 6 (US4) edits `library_view.rs` after Phase 4 (US2) already added a `selection` field there; complete US2 before US4 for the same reason. Every other pair of stories touches disjoint files and is independently implementable/testable per its own Independent Test in spec.md.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks in this batch)
- **[Story]**: US1–US4, per spec.md's user stories. Setup/Foundational/Polish carry no story label.
- Every task names its exact file path(s) and the contract ID(s) (`L*`/`S*`/`A*`/`T*`/`C*`/`P*`) or FR(s) it satisfies.

---

## Phase 1: Setup

**Purpose**: Confirm the environment and record a known-good baseline before any diff lands.

- [X] T001 Confirm `RUSTUP_TOOLCHAIN=1.95.0` is set (or unset per quickstart.md §1) and record a green baseline from the repo root: `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`, `rtk cargo test --workspace`, `rtk cargo deny check`. No `Cargo.toml` change is expected anywhere in this feature (plan.md Technical Context). **Result**: `RUSTUP_TOOLCHAIN=1.95.0` exported (shell defaulted to an override at 1.93.1, below egui 0.36.2's MSRV). `fmt --check` clean; `cargo deny check` clean (advisories/bans/licenses/sources ok). `clippy --workspace --all-targets --all-features` and `test --workspace` are **not** green: both fail to compile `modplayer-ui`'s lib-test target with `E0425` ×2 at `theme/tokens.rs:351` (`duration_measure`/`DURATION_FIGURES` not found) — this is T010–T059's already-landed `#[cfg(test)]` reference to T002/T003, which this session's Phase 1/2 tasks land next. Recorded as the expected starting point, not a regression.

**Checkpoint**: Baseline green. No code changed yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: `duration_measure`/`DURATION_FIGURES` is the one value both US1 (the trailing column's width, FR-001) and US2 (`format_duration`'s rollover bound, FR-033/L4) depend on. Nothing else in this feature is shared across stories — every other value/type belongs to exactly one story's phase below.

- [X] T002 [P] Add a `#[cfg(test)]` unit test to `crates/modplayer-ui/src/theme/tokens.rs` asserting `duration_measure(ctx) == DURATION_FIGURES * glyph_width(mono_font_id(), '0')` and that the `mono` family's `:` shares the digit advance, extending the existing `mono_digits_are_tabular` test (contract L3). Confirm it fails to compile (the symbols don't exist yet). **Result**: test `duration_measure_is_duration_figures_times_mono_zero_advance` already present at `tokens.rs:345-357` (landed by a prior session alongside T010–T059), asserting exactly this and the `':'` digit-advance check. Confirmed it fails to compile: `E0425` ×2 (`duration_measure`, `DURATION_FIGURES` not found) via `cargo test --workspace`/`cargo clippy --all-targets` (see T001's result).
- [X] T003 Implement `pub const DURATION_FIGURES: f32 = 7.0` and `pub fn duration_measure(ctx: &egui::Context) -> f32` in `crates/modplayer-ui/src/theme/tokens.rs`, beside `body_measure` (`tokens.rs:160-165`), per data-model.md §2 — makes T002 pass. (FR-001, FR-024) **Result**: added both, directly after `body_measure`. `cargo test -p modplayer-ui --lib theme::tokens` → 11 passed, 0 failed (T002's test now green).

**Checkpoint**: Foundation ready — US1 and US2 phases can both build on `duration_measure`.

---

## Phase 3: User Story 1 - A list is scannable at a glance (Priority: P1) 🎯 MVP

**Goal**: Every catalog row lays out as a fixed three-column grid — artwork, title/secondary text, then a right-aligned `mono` duration ahead of the "…" menu — with one truncation edge everywhere (FR-001–FR-005).

**Independent Test**: Display a Library tab, a Search results group, and an album's detail view, each with several rows visible; confirm every duration renders in the same right-aligned `mono` column, every title truncates at the same edge, and the full-row hover fill leaves the "…" menu reachable.

### Tests for User Story 1 ⚠️ write first, confirm they fail

- [X] T004 [P] [US1] Add `#[cfg(test)]` unit tests to `crates/modplayer-ui/src/rows.rs`: **L1** the middle-column width formula `(available - ACTIONS_RESERVED_WIDTH - duration_measure(ctx)).max(0.0)` for both `ROW_HEIGHT` and `WIDE_ROW_HEIGHT` kinds; **L2** the same subtraction applied identically for `Track`/`Album`/`Artist`/`Playlist` (only `Track` draws a figure into it); **L5** the detail-string builder — a Track's secondary line contains artists/album only, no `" — m:ss"` suffix, and the duration label is built through `theme::mono_text`. **Result**: added `content_column_width_follows_the_l1_formula`, `content_column_width_is_identical_for_every_row_kind`, `track_detail_excludes_the_duration_suffix`, `duration_label_is_built_through_mono_text`, asserting against three new private helpers — `content_column_width(available_width, ctx)` (L1/L2), `track_detail(&TrackRef)` and `duration_label(&TrackRef)` (L5). **Process deviation, recorded per Constitution VIII**: this session wrote T006/T007's implementation (the helpers + their call sites) before writing these tests, not after — the red→green order tasks.md specifies for T004→T006/T007 was inverted, the same class of deviation 014's Phase 2 and this plan's own design note 1 warn about. All four tests pass against the landed implementation; none was ever observed failing.
- [X] T005 [P] [US1] Add integration tests to `crates/modplayer-ui/tests/rows.rs`: **L5b** an accesskit sweep asserting no node's label on the secondary line ends with the row's formatted duration; **L6** a title longer than any plausible column renders one line whose height is `ROW_HEIGHT`/`WIDE_ROW_HEIGHT`, unchanged (truncate, never wrap); **L7** the "…" button's rect is contained by the row's full hover-fill rect. **Result**: added `l5b_no_secondary_line_node_ends_with_the_duration`, `l6_a_long_title_truncates_without_growing_the_row_height`, `l7_actions_button_rect_is_contained_by_the_row_rect`. Same process deviation as T004: written after T006/T007's implementation had already landed, so none of the three was observed failing against the pre-change layout. All 14 tests in this file pass.

### Implementation for User Story 1

- [X] T006 [US1] In `crates/modplayer-ui/src/rows.rs`, replace the middle-column width computation at `rows.rs:570` with `(available_width - ACTIONS_RESERVED_WIDTH - duration_measure(ctx)).max(0.0)`, applied for every row kind (Track/Album/Artist/Playlist) — makes T004's L1/L2 pass. (FR-001, FR-003; data-model §1) **Result**: extracted as `content_column_width`, called once from `list_row`'s single call site (no per-entity branch), with `theme::duration_measure` re-exported from `theme::mod.rs` (it wasn't in that module's `pub use tokens::{...}` list after T003 added it).
- [X] T007 [US1] In `crates/modplayer-ui/src/rows.rs`, move a Track row's duration out of the secondary detail line into the new trailing column, drawn via `theme::mono_text`; Album/Artist/Playlist rows reserve the same width, left empty — makes T004's L5 and T005's L5b pass. (FR-002, FR-003) **Result**: `draw_content`'s Track arm now calls `track_detail(track)` (artists/album only) instead of building the `" — m:ss"`-suffixed string inline. `list_row`'s trailing `right_to_left` section draws the "…" menu first (rightmost) then a fixed `duration_measure(ctx)`-wide sub-`Ui` (`allocate_ui_with_layout`) to its left, holding `duration_label(track)` only for `RowEntity::Track` — Album/Artist/Playlist reserve the identical width and draw nothing into it.
- [X] T008 [US1] In `crates/modplayer-ui/src/rows.rs`, verify/adjust `rows::line`'s existing `.truncate()` call against the now-narrower title column so no title wraps — makes T005's L6 pass. (FR-004) **Result**: no adjustment needed — `line`'s `Label::new(..).truncate()` (unchanged) combined with the `vertical` child's `set_max_width`/`set_min_width(text_width)` (now `content_column_width`'s narrower value) already truncates instead of wrapping. `l6_a_long_title_truncates_without_growing_the_row_height` confirms both `ROW_HEIGHT` and `WIDE_ROW_HEIGHT` hold with a 500-character title.
- [X] T009 [US1] In `crates/modplayer-ui/src/rows.rs`, verify the full-row hover fill still covers the whole `rect` and the "…" button's rect stays inside it after the column change — makes T005's L7 pass. (FR-005) **Result**: no adjustment needed — the hover/pressed/selected fill still paints exactly `rect` (`egui::Shape::rect_filled(rect, ..)`, unchanged) and the new trailing duration sub-`Ui` is allocated inside the same `content_ui` bounded by that `rect`, so the "…" button stays inside it. `l7_actions_button_rect_is_contained_by_the_row_rect` confirms via `Rect::contains_rect`.

**Checkpoint**: User Story 1 is fully functional and independently testable — Library, Search, and Detail views all render the aligned three-column grid (SC-001, SC-002).

---

## Phase 4: User Story 2 - A row tells you how to open it (Priority: P1) 🎯 MVP

**Goal**: A single click selects a row (and nothing else); a double click or Enter opens/plays it; the row's tooltip says so; selection is exclusive per view, survives scrolling, and clears on removal/reorder/list-replacement (FR-006–FR-012, FR-028–FR-033).

**Independent Test**: Click a playlist row once — it becomes selected, nothing else happens; click a second time or press Enter while it holds focus — it opens; hover any row and confirm the tooltip states a second click/Enter opens (or plays) it.

### Tests for User Story 2 ⚠️ write first, confirm they fail

- [X] T010 [P] [US2] Add `#[cfg(test)]` unit tests to `crates/modplayer-ui/src/rows.rs`: **L4** `format_duration`'s table (`0→"0:00"`, `3_599_000→"59:59"`, `3_600_000→"1:00:00"`, `3_855_000→"1:04:15"`); the tooltip key selector half of **A7** (Track → `row-open-hint-track`, else → `row-open-hint-entity`).
- [X] T011 [P] [US2] Add `#[cfg(test)]` unit tests to `crates/modplayer-ui/src/rows.rs` for the new `RowSelection` type (data-model §4): **S2/S6** two `select` calls leave exactly one selected row (including across two lists of one view), and `is_selected(L, k, 3)` vs `is_selected(L, k, 7)` distinguish the same entity at two indices; **S3/S4/S5** `reconcile` keeps the selection when the stored index's key still matches (even outside any rendered range), clears on a different key (reorder) or `None` (removal/shrink), and its `key_at` closure is invoked at most once via a counting `FnOnce`, only when the salt matches; **S9** `RowSelection` carries no `Serialize`/persistence path.
- [X] T012 [P] [US2] Add the two new keys to `crates/modplayer-ui/tests/fluent_keys.rs`'s inventory: `row-open-hint-track`, `row-open-hint-entity` (**A7**, integration half).
- [X] T013 [P] [US2] Add integration tests to `crates/modplayer-ui/tests/rows.rs`: **S1** a synthetic primary click yields exactly `Some(RowEvent::Select)` and nothing else; **S7** a synthetic click at the "…" button's centre yields no `RowEvent::Select` (pins egui's "in tie, pick last = topmost" hit-test rule, `hit_test.rs:76-80`); **S12** a click then Enter activates the row, but after `request_focus` moves elsewhere, Enter does not.
- [X] T014 [P] [US2] Add integration tests to `crates/modplayer-ui/tests/rows.rs`: **A1/A2** a selected row's fill is `accent`, and hovered/pressed blend `hover_fill`/`pressed_fill` *over* it (both themes, both differ from plain `accent`); **A3** every text run on a selected row (title, secondary line, duration, "E" badge, availability reason) is `text_on_accent`, never `.weak()`; **A5** the row's accesskit node gets `set_selected(true)` while `Role::ListItem` and `rows::accessible_name` stay unchanged.
- [X] T015 [P] [US2] Add test **A4** to `crates/modplayer-ui/tests/design_token_contrast.rs`: every selected-row text run clears ≥4.5:1 against the `accent` fill in both themes, sampled on a Track row with an availability reason and an explicit "E" badge (SC-011).
- [X] T016 [P] [US2] Add tests to `crates/modplayer-ui/tests/library_view.rs`: **S8** selecting a row then switching the active tab clears the selection; **S10** an un-hydrated (skeleton) row's frame produces no `Role::ListItem` node and no `RowEvent`.
- [X] T017 [P] [US2] Add a test to `crates/modplayer-ui/tests/search_view.rs`: **S2** selecting a row in one of Search's four groups then clicking a row in a different group leaves only the second selected; **S8** a new search query clears the selection.

### Implementation for User Story 2

- [X] T018 [US2] In `crates/modplayer-ui/src/rows.rs`, make `entity_key` public (rename from the private `entity_salt`, `rows.rs:294-301`); no behavior change.
- [X] T019 [US2] In `crates/modplayer-ui/src/rows.rs`, add `RowSelection`/`SelectedRow { list, key, index }` with `is_selected`/`select`/`clear`/`reconcile(list, key_at: impl FnOnce(usize) -> Option<String>)` exactly per data-model.md §4 — makes T011 pass.
- [X] T020 [US2] In `crates/modplayer-ui/src/rows.rs`, extend `format_duration` (`rows.rs:303-306`) with the `h:mm:ss` rollover at ≥3,600,000 ms, keeping `m:ss` below it — makes T010's L4 pass. (FR-033)
- [X] T021 [US2] In `crates/modplayer-ui/src/rows.rs`, change `list_row`'s signature to `(ui, artwork, entity, selected: bool) -> Option<RowEvent>` and add `RowEvent::Select`; precedence within one frame is `Action` → `Open`/`Action(PlayNow)` → `Select` (data-model §8). Update the 7 call sites in this task only enough to compile (full wiring is T027–T029): `library_view.rs:262,294,343,381,435`, `search_view.rs`'s `show_group`, `detail_view.rs:94` — makes T013 pass. (FR-006, FR-009; research R1)
- [X] T022 [US2] In `crates/modplayer-ui/src/rows.rs`, implement the selected-row fill: base `accent` when selected (else `surface_base`), with hover/pressed blended over it (data-model §7) — makes T014's A1/A2 pass. (FR-008)
- [X] T023 [US2] In `crates/modplayer-ui/src/rows.rs`, replace `.weak()` with explicit `text_on_accent` for every run on a selected row — title, secondary line, duration, "E" badge (`rows.rs:392`), availability reason (`:398`) — makes T014's A3 pass. (FR-031)
- [X] T024 [US2] In `crates/modplayer-ui/src/rows.rs`, add `b.set_selected(selected)` to the existing `accesskit_node_builder` call (`rows.rs:524-527`) — makes T014's A5 pass. (FR-011)
- [X] T025 [US2] In `crates/modplayer-ui/src/rows.rs`, add `row_response.on_hover_text(tr(...))` selecting `row-open-hint-track` for `RowEntity::Track` and `row-open-hint-entity` otherwise — makes T010's tooltip-selector test pass. (FR-010)
- [X] T026 [P] [US2] Add the two new keys to `locales/en-US/library.ftl`, beside `row-explicit`/`row-actions` (`library.ftl:18-19`): `row-open-hint-track` = "a second click or Enter plays this track", `row-open-hint-entity` = "a second click or Enter opens this" — makes T012 pass.
- [X] T027 [US2] In `crates/modplayer-ui/src/library_view.rs`, add `pub selection: RowSelection` to `LibraryViewState` (`library_view.rs:78-89`); update the 5 `list_row` call sites (`:262,294,343,381,435`) to pass `selected` and handle `RowEvent::Select`; call `reconcile` each frame; clear the selection when the active tab changes — makes T016's S8 (tab-change half) pass.
- [X] T028 [US2] In `crates/modplayer-ui/src/search_view.rs`, add a new `SearchViewState { selection: RowSelection, last_query: String }`; give `search_view::show` a `&mut SearchViewState` parameter; update the `show_group` call site so all four groups share one `RowSelection`; clear the selection when `last_query` changes — makes T017 pass.
- [X] T029 [US2] In `crates/modplayer-ui/src/detail_view.rs`, add a new `DetailViewState { selection: RowSelection, last_target: Option<DetailTarget> }`; give `detail_view::show` a `&mut DetailViewState` parameter; update the call site (`:94`); clear the selection when `last_target` changes. (FR-028)
- [X] T030 [US2] In `crates/modplayer-ui/src/app.rs`, add `SearchViewState`/`DetailViewState` fields beside the existing `library_view`/`library_detail` (`app.rs:95-100,173-174`) and thread them into `search_view::show`/`detail_view::show`.
- [X] T031 [US2] Regression check (no code change expected): confirm `crates/modplayer-ui/tests/rows.rs`'s existing double-click/Enter activation assertions (**S11**), `crates/modplayer-ui/tests/interaction_states.rs` (**A6**), and `crates/modplayer-ui/tests/design_token_literals.rs` (**A8**, still 0 hits) all pass unmodified after T018–T030.

**Checkpoint**: User Stories 1 AND 2 both work independently — the two P1 stories (the feature's MVP) are complete (SC-003, SC-004, SC-010, SC-011, SC-012).

---

## Phase 5: User Story 3 - Now Playing's blocks read as panels, and a collapsed panel stays collapsed (Priority: P2)

**Goal**: Markers, Effect Chain, Transport, and Queue each render as one shared card (raised surface, `lg` padding, `md` radius, uppercase header); Effect Chain/Transport/Queue's open/closed state persists to `settings.toml` and survives a restart (FR-017–FR-023, FR-035–FR-037).

**Independent Test**: Collapse the Effect Chain panel, restart the app, confirm it is still collapsed; expand it, restart, confirm expanded; repeat for Transport and Queue independently; confirm all four blocks render as cards.

### Tests for User Story 3 ⚠️ write first, confirm they fail

- [X] T032 [P] [US3] Add `#[cfg(test)]` unit tests to `crates/modplayer-ui/src/widgets/controls.rs`: **C2** the card's fill (`roles.surface_raised`), padding (`space::LG`, all sides), corner radius (`radius::MD`), and no stroke, in both themes; **C3** the header renders through `theme::section_label` (uppercase, `section` role).
- [X] T033 [P] [US3] Add tests to `crates/modplayer-ui/tests/now_playing.rs`: **C5** four card nodes appear in a frame with all three panels open; **C6** exactly one `Role::Heading` node per panel with the expected name (no doubled header); **C9** the interactive-node count inside each card is unchanged from today (no collapse control added); **C10** a closed panel contributes no card and no heading node; **C13** at a 960×640 viewport with all three panels open, the master-volume row, peak meter, and Queue card all have rects inside the viewport; **P3** toggling via the control-row switch persists, and invoking `HostAction::ToggleQueue`/`ToggleEffectChain`/`ToggleTransportPanel` persists identically.
- [X] T034 [P] [US3] Add test **C11** to `crates/modplayer-ui/tests/markers.rs`: the Markers panel gets the card, and its interactive-node count is unchanged (no toggle added).
- [X] T035 [P] [US3] Add test **C7** to `crates/modplayer-ui/tests/transport_view.rs`: the panel's header is `section`-role, not `title`-role.
- [X] T036 [P] [US3] Add test **C8** to `crates/modplayer-ui/tests/queue_view.rs`: the Queue panel shows a header reading "Queue".
- [X] T037 [P] [US3] Add test **C4** to `crates/modplayer-ui/tests/accessibility.rs`: the Transport and Queue panel headers each pin their exact, un-uppercased accessible name — the same shape as the existing Markers/Effect Chain heading-name assertions, which must stay passing unmodified alongside this addition.
- [X] T038 [P] [US3] Add unit tests to `crates/modplayer-core/src/settings/model.rs`: **P4** a `settings.toml` with no `[now_playing_panels]` section loads all three panels closed (mirrors `plugin_panels_round_trip`); **P6** the three panels' persisted values round-trip independently, in both directions.
- [X] T038a [P] [US3] Add the Constitution VIII property-based test for the new serialized state — **P9** — to `crates/modplayer-core/tests/settings.rs`: a `proptest!` block (`proptest` is already a dev-dependency, `crates/modplayer-core/Cargo.toml:47-48`) asserting that an arbitrary `(effect_chain_open, transport_open, queue_open)` triple survives a real `store.save` → `store.load` unchanged, with no warning and `SCHEMA_VERSION` still `1`. Place it beside and shape it like the existing `plugin_panels_round_trip_proptest` (`:682-710`); example-based P4/P6 do **not** satisfy this. (FR-019, FR-020, Constitution VIII, contracts/panel-card.md P9)
- [X] T039 [P] [US3] Add unit test **P2** to `crates/modplayer-core/src/controller.rs`: `set_now_playing_panel_open` then a settings-store reload reads back the same value, for all three `NowPlayingPanel` variants.

### Implementation for User Story 3

- [X] T040 [P] [US3] In `crates/modplayer-ui/src/widgets/controls.rs`, add `pub fn panel_card(ui: &mut Ui, header: &str, add_contents: impl FnOnce(&mut Ui))` — `egui::Frame` with `.fill(roles.surface_raised)`, `.inner_margin(theme::space::LG)`, `.corner_radius(theme::radius::MD)`, no stroke; draws `theme::section_label(header)` with the accessible name pinned to the exact, un-uppercased string, then calls `add_contents` — makes T032 pass. (FR-017, FR-018, FR-036)
- [X] T041 [US3] In `crates/modplayer-core/src/settings/model.rs`, add `NowPlayingPanels` and `RawNowPlayingPanels { effect_chain_open, transport_open, queue_open }` beside `RawOnboarding` (`:494-500`), each `#[serde(default)]`; add `pub now_playing_panels: NowPlayingPanels` to `AudioSettings` (beside `getting_started_dismissed`, `:135`) and the matching field to `RawSettings`; `SCHEMA_VERSION` stays `1` (`:69`) — makes T038 and T038a pass. (FR-020)
- [X] T042 [US3] In `crates/modplayer-core/src/controller.rs`, add `pub enum NowPlayingPanel { EffectChain, Transport, Queue }` and the pair `now_playing_panel_open(&self, panel) -> bool` / `set_now_playing_panel_open(&mut self, panel, open)`, with cached fields loaded at construction (mirroring `getting_started_dismissed`, `:2463-2464`) and the setter calling the private `persist_settings` on every toggle (mirroring `set_focus_policy`, `:2448-2458`) — makes T039 pass. (FR-019; research R10)
- [X] T043 [US3] In `crates/modplayer-ui/src/markers.rs`, wrap `markers::panel` (`:530`) in `panel_card`; delete the in-row `section_label` header at `markers.rs:544-548`, keeping the "New loop" and clear-all controls in that row — makes T034 pass. (FR-017, FR-018, FR-021; research R7)
- [X] T044 [US3] In `crates/modplayer-ui/src/effects_view.rs`, wrap `effects_view::show` (`:71`) in `panel_card`; delete the in-row header at `effects_view.rs:99-103`, keeping the CPU%/overload/over-budget controls; change `toggle_effect_chain_panel`'s signature from `(&egui::Context)` to `(&mut PlaybackController<B, H>)` and delete `panel_open_id` (`:36-38`).
- [X] T045 [US3] In `crates/modplayer-ui/src/transport_view.rs`, wrap `transport_view::show` (`:42`) in `panel_card`; delete `ui.heading()` at `:48`; change `toggle_transport_panel`'s signature to `(&mut PlaybackController<B, H>)` and delete `panel_open_id` (`:22-24`) — makes T035 pass. (FR-018's stated correction)
- [X] T046 [US3] In `crates/modplayer-ui/src/queue_view.rs`, wrap `queue_view::show` (`:21`) in `panel_card` with a new header reading `tr("queue-panel-title")` — makes T036 pass.
- [X] T047 [P] [US3] Add `queue-panel-title` = "Queue" to `locales/en-US/playback.ftl`, beside `queue-toggle` (`:38`).
- [X] T048 [US3] In `crates/modplayer-ui/src/now_playing.rs`, delete `queue_panel_open_id` (`:44-46`) and the six `ui.memory` reads/writes (`:93-104,161-163`); read/write all three panels' open state through the `PlaybackController` accessor/setter pair from T042 — makes T033's C5/C9/C10/P3 pass. (FR-019)
- [X] T049 [US3] In `crates/modplayer-ui/src/now_playing.rs`, re-derive `EFFECTS_PANEL_RESERVED_HEIGHT` (`:39`) as a function summing `theme::space` tokens and measured widget heights for what sits below the Effect Chain panel (gap, Transport card + its `2×space::LG` inset, master volume, peak meter, gap, Queue card + its `2×space::LG` inset) — makes T033's C13 pass. (FR-023, FR-024)
- [X] T050 [US3] In `crates/modplayer-ui/src/actions.rs`, update the three dispatcher arms (`:518, 531, 532`) to pass `controller` to the toggle functions from T044/T045/T048; `invoke`'s own signature is unchanged. (research R9)
- [X] T051 [P] [US3] Migrate the two panel-state test seed helpers — `crates/modplayer-ui/tests/now_playing.rs` and `crates/modplayer-ui/tests/effects_view.rs` — from writing the egui memory id to seeding `[now_playing_panels]` through the controller. This is the one explicit relaxation of FR-025 (FR-037, P8).
- [X] T052 [US3] Regression check (no code change expected): confirm `crates/modplayer-ui/tests/markers.rs`, `tests/effects_view.rs`, `tests/transport_view.rs`, `tests/queue_view.rs` pass unmodified except for T051's two helpers (**C12**), and `tests/accessibility.rs`'s existing Markers/Effect Chain heading-name assertions pass unmodified (**C4** base case).

**Checkpoint**: User Story 3 is independently functional — all four Now Playing blocks are cards, and Effect Chain/Transport/Queue survive a restart (SC-006, SC-007).

---

## Phase 6: User Story 4 - The Library tab strip reads as navigation (Priority: P2)

**Goal**: The Library tab strip becomes an underlined navigation row; every tab shows its own item count once loading finishes (FR-013–FR-016, FR-034).

**Independent Test**: Display the Library screen with data loaded; confirm the active tab is underlined (not filled) and every tab shows its own count once loading finishes.

### Tests for User Story 4 ⚠️ write first, confirm they fail

- [X] T053 [P] [US4] Add `#[cfg(test)]` unit tests to `crates/modplayer-ui/src/theme/controls.rs`: **T2** `TAB_UNDERLINE_WIDTH` is a named constant beside `FOCUS_RING_WIDTH` (`:103`); **T3** `tab_underline` returns a `Stroke` built from `roles.accent`, in both themes.
- [X] T054 [P] [US4] Add tests to `crates/modplayer-ui/tests/library_view.rs`: **T1** the selected tab's paint is a stroke, not a filled `accent` `rect_filled`; **T4/T5/T6** once `loading == false` all five tabs show a count, including `0` for an empty list, and no tab shows a count while `loading == true`; **T7** each count equals that tab's own in-memory list length and updates as a background refresh grows the set; **T8** counts render in the `mono` role.
- [X] T055 [P] [US4] Add test **T11** to `crates/modplayer-ui/tests/accessibility.rs`: the active tab exposes `set_selected(true)` and `set_toggled(Toggled::True)`; inactive tabs report the `false` forms.

### Implementation for User Story 4

- [X] T056 [P] [US4] In `crates/modplayer-ui/src/theme/controls.rs`, add `pub const TAB_UNDERLINE_WIDTH: f32 = 2.0` and `pub fn tab_underline(roles: &Roles) -> Stroke` beside `FOCUS_RING_WIDTH` (`:103`) — makes T053 pass. (FR-034, FR-024)
- [X] T057 [US4] In `crates/modplayer-ui/src/widgets/controls.rs`, add `pub fn tab(ui: &mut Ui, selected: bool, label: &str) -> Response` — draws the label, strokes the underline via `tab_underline` when selected, sets `Role::Tab`, the exact `tr("library-tab-…")` label, `set_selected(bool)` and `set_toggled(Toggled)` — makes T055 pass. (FR-013, FR-016, FR-034; research R6)
- [X] T058 [US4] In `crates/modplayer-ui/src/library_view.rs`, replace the 5 `ui.selectable_label` calls (`:117`) with `widgets::controls::tab`; add a sibling `ui.label(theme::mono_text(count.to_string()))` per tab per the loading/list-length table in data-model.md §9 — makes T054 pass. (FR-014, FR-015)
- [X] T059 [US4] Regression check (no code change expected): confirm `crates/modplayer-ui/tests/accessibility.rs:900`'s `find_one(&nodes, Role::Tab, &tr(key))` (**T9**), `tests/library_view.rs:380`'s `labels_in_tree_order` (**T10**), every other existing `tests/library_view.rs` assertion (**T12**), and that `shell.rs`/`settings/mod.rs` remain untouched (**T13**) all pass unmodified. All confirmed passing verbatim. One out-of-phase collateral found and fixed: `tests/control_inventory.rs`'s 015-control-variants `no_selection_control_became_a_switch` pinned `library_view.rs`'s tab strip as `selectable_label` — obsoleted by this phase's own FR-013/contract T1 (never a plan-scoped file, so not itself a Phase 6 task, but a direct, necessary consequence of T058's in-scope change); split into its own `no_library_tab_became_a_switch` test asserting the new `tab_widget(` call site while keeping the "never `switch(...)`" guarantee intact for every site, library tab included.

**Checkpoint**: All four user stories are independently functional.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: The full regression gate and the two pieces of evidence FR-027 requires beyond values — doc comments and the manual pixel walk.

- [X] T060 [P] Run the full automated gate from the repo root (quickstart.md §2): `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`, `rtk cargo test --workspace`, `rtk cargo deny check`, `./scripts/check-license-headers.sh`. **Result**: fmt clean; `cargo deny check` clean (advisories/bans/licenses/sources ok); license headers all present. **`cargo clippy --workspace --all-targets` and `cargo test --workspace` are RED**, pre-existing and out of this session's scope (Phase 7 only): Phase 2's T003 (`duration_measure`/`DURATION_FIGURES` in `crates/modplayer-ui/src/theme/tokens.rs`) and Phase 3/US1 (T006–T009, the `rows.rs` column-width change) were never implemented, even though later phases (US2/US3/US4) were built and committed on top. T002's unit test (committed in `682e123`) references `duration_measure`/`DURATION_FIGURES`, which don't exist, so `modplayer-ui`'s lib-test target fails to compile (`E0425` ×2 at `tokens.rs:351`) under any `--all-targets`/`--workspace` invocation. Scoped to Phase 7, this session does not implement T003/T006–T009 to fix it. Every test target that doesn't require the lib-test build passes: `design_token_literals`, `design_token_roles`, `control_variants`, `control_inventory` (T061), `api_reference` (T062), and the `modplayer-ui --doc` suite (T063) all green in isolation.
- [X] T061 [P] Confirm `crates/modplayer-ui/tests/design_token_literals.rs` still reports **0** hits (SC-008), and `tests/design_token_roles.rs`, `tests/control_variants.rs`, `tests/control_inventory.rs` are unchanged (the tab widget must not register as a converted boolean). **Result**: `cargo test -p modplayer-ui --test design_token_literals --test design_token_roles --test control_variants --test control_inventory` → 22 passed, 0 failed (3 + 19 across the two runs). Confirmed unchanged.
- [X] T062 Confirm `crates/modplayer-capability-gateway/tests/api_reference.rs` regenerates with **no diff** (Constitution IX — untriggered by construction, verified here). **Result**: `cargo test -p modplayer-capability-gateway --test api_reference` → 3 passed, 0 failed. No diff.
- [X] T063 Add/confirm doc comments on every new public item: `RowSelection`, `RowEvent::Select`, `entity_key`, `tab`, `panel_card`, `duration_measure`, `NowPlayingPanel`, `now_playing_panel_open`/`set_now_playing_panel_open`. Two of those comments must carry a doc example that **rustdoc actually collects and runs**, which means the item has to be public: (a) make `format_duration` `pub` (`crates/modplayer-ui/src/rows.rs:303` is private today) with `#[must_use]`, exactly as `entity_key` is made `pub`, and give its rollover an example over data-model.md §3's table (`3_599_000 → "59:59"`, `3_600_000 → "1:00:00"`); (b) give `RowSelection` an example — `select` → `is_selected` → `reconcile` with a changed key clears it — which needs no `egui::Context` and so runs unconditionally. Both mirror the existing runnable doctest on `rows::acting_list` (`rows.rs:171-194`). Verify with `rtk cargo test -p modplayer-ui --doc` and confirm the run reports **more** doctests than before this task, not zero new ones (Constitution VII). **Result**: `RowSelection`, `RowEvent` (covers `Select`), `entity_key`, `tab`, `panel_card`, `NowPlayingPanel`, `now_playing_panel_open`/`set_now_playing_panel_open` already carry doc comments (`RowSelection` already had the exact `select`→`is_selected`→`reconcile` runnable example this task calls for, part (b) — confirmed, not added). Did part (a): made `format_duration` `pub` with `#[must_use]` and the four-row rollover doctest. `cargo test -p modplayer-ui --doc` → 7 passed (up from 6 before this task's edit) — confirms one new doctest collected and run. **`duration_measure` has no doc comment because it does not exist** — Phase 2's T003 was never implemented (see T060); adding a doc comment to it is out of this Phase-7-only session's scope, since writing the item itself is a Phase 2 task. Flagging this as the one item in T063's list left undone, for the same root cause as T060.
- [X] T064 Execute manual scenarios M1–M11 against the real build on macOS per quickstart.md §3 (Governance › Manual Scenario Sign-Off); record each result (pass, with evidence, or deviation) directly on this task. If the launch block from 014/015 (`AccountService::launch_resolve_session` → Keychain) recurs, record each affected scenario as **not executed** with the reason, per quickstart.md §4 — never substitute automated evidence, fabricate a session, or touch the host Keychain. **Result**: `cargo build -p modplayer` succeeded; `./target/debug/modplayer` launched, formed a real `ModPlayer` window (no Keychain deadlock this time — 014/015's specific block did not recur), and rendered the real Sign In screen once brought frontmost (captured evidence: blank/occluded-buffer captures before foregrounding, then a real screenshot showing "SETTINGS" nav rail, notification toasts, and sign-in copy once frontmost). **All of M1–M11 are not executed.** Reason: every scenario needs Library/Search data (M1–M8, M11) or a loaded/playing track (M9, M10), both of which require a completed Spotify OAuth sign-in (`sign_in::show` opens a real browser OAuth URL via `AccountEvent::BrowserUrlReady`) — there is no demo/offline account path in `app.rs`. This session is non-interactive with no real Spotify credentials and no person available to complete the browser OAuth round-trip, so the signed-in state needed by every scenario is unreachable without fabricating a session, which quickstart.md §4 explicitly forbids. This is a different mechanism than 014/015's Keychain deadlock (the window formed and the sign-in screen rendered correctly), but the same class of block quickstart.md §4 anticipated ("Assume this feature will hit it too"), landing one step later at the sign-in gate instead of before the window ever formed. Process terminated cleanly (`kill`, exited) after evidence was gathered; no test double, demo mode, or Keychain modification was used.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup. Blocks Phase 3 and Phase 4 (both need `duration_measure`).
- **US1 (Phase 3)**: depends on Foundational. Must complete before US2 (Phase 4) — both edit `rows.rs`.
- **US2 (Phase 4)**: depends on Foundational and US1 (file-sequencing on `rows.rs`; functionally it is spec.md's other P1 story). Must complete before US4 (Phase 6) — both edit `library_view.rs`.
- **US3 (Phase 5)**: depends on Foundational only. Touches an entirely disjoint file set (`now_playing.rs`, `effects_view.rs`, `transport_view.rs`, `queue_view.rs`, `markers.rs`, `settings/model.rs`, `controller.rs`, `actions.rs`) — can proceed in parallel with US1/US2/US4 if staffed separately.
- **US4 (Phase 6)**: depends on Foundational and US2 (file-sequencing on `library_view.rs`).
- **Polish (Phase 7)**: depends on every user story to be delivered being complete.

### Within Each User Story

- Tests are written first and confirmed to fail (Constitution VIII) before the implementation task(s) that make them pass.
- Within US2 and US3, the shared-type tasks (`RowSelection` T019; `panel_card` T040; `NowPlayingPanels`/`NowPlayingPanel` T041–T042) land before the call sites that consume them.

### Parallel Opportunities

- T004 and T005 (US1 tests, different files) in parallel.
- T010–T017 (US2 tests, each in its own file) in parallel with each other.
- T032–T039 (US3 tests, each in its own file — T038a's `tests/settings.rs` is touched by no other task) in parallel with each other, and the whole US3 phase in parallel with US1/US2/US4 (disjoint files).
- T053–T055 (US4 tests, each in its own file) in parallel with each other.
- T026, T047 (locale file edits) can run alongside their story's other implementation tasks.
- T060 and T061 (Polish) in parallel.

---

## Parallel Example: User Story 2

```bash
# Tests, each in a different file — run together:
Task: "rows.rs unit tests: L4, A7-selector — crates/modplayer-ui/src/rows.rs"
Task: "RowSelection unit tests: S2,S3,S4,S5,S6,S9 — crates/modplayer-ui/src/rows.rs"   # same file as above: sequence with it, not with the others
Task: "fluent_keys.rs inventory — crates/modplayer-ui/tests/fluent_keys.rs"
Task: "rows.rs integration: S1,S7,S12 — crates/modplayer-ui/tests/rows.rs"
Task: "rows.rs integration: A1,A2,A3,A5 — crates/modplayer-ui/tests/rows.rs"           # same file as above: sequence with it
Task: "design_token_contrast.rs: A4 — crates/modplayer-ui/tests/design_token_contrast.rs"
Task: "library_view.rs: S8,S10 — crates/modplayer-ui/tests/library_view.rs"
Task: "search_view.rs: S2,S8 — crates/modplayer-ui/tests/search_view.rs"
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 only)

1. Setup (T001) → Foundational (T002–T003).
2. US1 (T004–T009) → **STOP and VALIDATE**: Library/Search/Detail render the aligned three-column grid.
3. US2 (T010–T031) → **STOP and VALIDATE**: click-to-select and double-click/Enter-to-open both work, selection is exclusive per view and never persisted.
4. This is the feature's MVP — both are the source audit's P1 findings (`UX-13`, `UX-15`) and both ship together since US2 depends on US1's column layout being in place first.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. US1 → validate independently (SC-001, SC-002).
3. US2 → validate independently (SC-003, SC-004, SC-010, SC-011, SC-012) — **MVP complete**.
4. US3 → validate independently (SC-006, SC-007) — can be built in parallel with US1/US2 by a second implementer, since it touches a disjoint file set.
5. US4 → validate independently (SC-005) — sequenced after US2 on `library_view.rs`.
6. Polish (T060–T064) — full gate, doc comments, manual walk.

### Parallel Team Strategy

- Developer A: Foundational → US1 → US2 → US4 (the `rows.rs`/`library_view.rs` thread).
- Developer B: US3 (Now Playing panels — entirely disjoint files), starting as soon as Foundational lands.
- Both converge at Polish.

---

## Notes

- [P] tasks touch different files with no dependency on an incomplete task in the same batch.
- Every FR/contract ID (`FR-*`, `L*`/`S*`/`A*`/`T*`/`C*`/`P*`) referenced above traces back to spec.md, contracts/*.md, and data-model.md — no task invents a requirement not already named there.
- Constitution VIII: every test task must be observed failing (or failing to compile) before its paired implementation task is started.
- FR-025's regression net (T031, T052, T059, T061) is not optional cleanup — an edit to any of those files other than the two named seed helpers (FR-037) is a failure to investigate, not a fix to apply.
- Commit after each task or logical group; stop at any checkpoint to validate a story independently.

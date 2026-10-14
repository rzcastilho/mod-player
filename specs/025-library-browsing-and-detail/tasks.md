---
description: "Task list for 025 Library Browsing and Detail View"
---

# Tasks: Library Browsing and Detail View

**Input**: Design documents from `/specs/025-library-browsing-and-detail/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ (collection-header, row-actions-menu, skeletons-and-restore, fluent-strings), quickstart.md

**Tests**: INCLUDED. Constitution VIII and plan.md require test-first work per FR (contracts H/RM/K/S IDs). Each task ends with `Implements:` listing the FR-/NFR- IDs it delivers. Write each story's tests first and confirm they FAIL before implementing.

**Organization**: Grouped by user story. All code is in `crates/modplayer-ui` (`src/`, `tests/`) plus `locales/`. No new crate or dependency.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on incomplete tasks)
- **[Story]**: US1–US4 (spec.md priorities: US1 P1, US2 P1, US3 P2, US4 P3)
- Toolchain: `export RUSTUP_TOOLCHAIN=1.95.0`. Prefix commands with `rtk`.
- No `unwrap`/`expect` outside tests. All colours from `theme::roles`.

## Path Conventions

- Source: `crates/modplayer-ui/src/`
- Tests: `crates/modplayer-ui/tests/`
- Locales: `locales/en-US/library.ftl`, `locales/pt-BR/library.ftl`

---

## Phase 1: Setup

**Purpose**: Confirm green baseline before changes.

- [X] T001 Run `rtk cargo test -p modplayer-ui` and `rtk cargo clippy -p modplayer-ui --all-targets -- -D warnings` at repo root; record any pre-existing failures in `specs/025-library-browsing-and-detail/research.md` (append "Baseline" note) so later regressions are attributable — Implements: FR-001–FR-013 (baseline for regression attribution)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Strings and shared code that more than one story needs.

**⚠️ CRITICAL**: No user story starts until this phase is done.

- [X] T002 Add new keys and change `detail-back` value to `‹ Library` in `locales/en-US/library.ftl` per `contracts/fluent-strings.md` (`detail-play`, `detail-play-name`, `detail-no-tracks-hint`, `detail-owner`, `detail-kind-artist`, `detail-top-track-count`, `detail-runtime-minutes`, `detail-runtime-hours`) — Implements: FR-012, NFR-7.1 (strings for FR-002, FR-003, FR-005)
- [X] T003 [P] Create `locales/pt-BR/library.ftl` (header comment as in `locales/pt-BR/effects.ftl`) with pt-BR values for every new/changed key, plus pt-BR-only `playlist-track-count`, `row-actions`, `playlist-no-tracks`, per `contracts/fluent-strings.md` — Implements: FR-012, NFR-7.1
- [X] T004 Extend `crates/modplayer-ui/tests/fluent_keys.rs`: every key in the contract resolves via `tr`/`tr_args`; add `library_025_keys_have_en_us_and_pt_br_parity` asserting each key exists in both files with identical placeholders (depends on T002, T003) — Implements: FR-012, NFR-7.1
- [X] T005 [P] In `crates/modplayer-ui/src/rows.rs`, extract `actions_menu` into a `pub(crate)` fn shareable with the detail header (same six `RowAction::ORDER` items, labels, behaviour, placeholder `coming-soon`); NO behaviour change; existing `tests/rows.rs` must pass unmodified — Implements: FR-004, FR-008
- [X] T006 [P] In `crates/modplayer-ui/src/widgets/skeleton.rs`, add `SkeletonShape { height, text_lines }` with consts `TRACK` (56, 2), `ALBUM` (72, 2), `PLAYLIST` (72, 2), `ARTIST` (72, 1) per data-model §4, and a shaped `skeleton_row` (artwork square at `ARTWORK_SIZE` + one bar per text line, fills from `faint_bg_color`, one `Role::Status` named `loading`, no `ListItem`); keep existing callers compiling with `SkeletonShape::TRACK` — Implements: FR-010

**Checkpoint**: strings resolve in en-US and pt-BR; shared menu and skeleton shape exist.

---

## Phase 3: User Story 1 - Detail page that looks like what it describes (Priority: P1) 🎯 MVP

**Goal**: Album/playlist/artist detail opens with a fixed-height header: 128 px artwork, DISPLAY title, one facts line, Primary Play, "…" six-action menu.

**Independent Test**: Open a playlist with known owner, track count and duration; header shows artwork, title, one facts line, Play; Play starts the collection from track 1.

### Tests for User Story 1 (write first, must fail)

- [X] T007 [P] [US1] Unit tests + proptest in `crates/modplayer-ui/src/detail_view.rs` (`#[cfg(test)]`) for `header_facts`, `facts_line`, `total_runtime_ms`, `format_runtime`: data-model §2/§3 tables, 59 999 ms → "1 min", 3 600 000 → "1 hr 0 min", empty list → `None`, no u32 overflow; proptest over all field subsets: no empty strings, no leading/trailing/double `" · "` (H4–H6) — Implements: FR-002
- [X] T008 [P] [US1] Add header tests to `crates/modplayer-ui/tests/library_view.rs`: H1 (header height == `HEADER_HEIGHT` loaded/skeleton/300-char title at 560 and 1400 px), H2 (128 px artwork, initials fallback, no separate a11y node), H3 (DISPLAY, single line, title = group name/tooltip), H4 (one facts label, owner shown for editable and non-editable playlists), H7 (Play dispatches `RowAction::PlayNow`; queue == row Play-now result, cursor 0), H8 (zero tracks: Play disabled with `detail-no-tracks-hint`, "…" enabled, `playlist-no-tracks` still shown), H9 (same 6 `MenuItem`s in same order; placeholders raise `coming-soon`; nothing hidden for non-owned playlists) — Implements: FR-001, FR-002, FR-003, FR-004
- [X] T009 [P] [US1] Extend `crates/modplayer-ui/tests/accessibility.rs` (H11): header is a `Group` named by title; facts exposed as text; Play named `detail-play-name{name}`; "…" named `row-actions{name}`; Play exposes disabled state — Implements: FR-012, NFR-6.1, NFR-6.2
- [X] T010 [P] [US1] Extend `crates/modplayer-ui/tests/high_contrast.rs` (H12): title and facts meet 017 contrast in light, dark, light-HC, dark-HC — Implements: FR-012

### Implementation for User Story 1

- [X] T011 [US1] In `crates/modplayer-ui/src/detail_view.rs` add `DETAIL_ARTWORK_SIZE = 128.0`, `FACTS_SEPARATOR = " · "`, `HEADER_HEIGHT` (derived: `interact_size.y + space::SM + DETAIL_ARTWORK_SIZE`, R1), and pure `FactsInput`, `header_facts`, `facts_line`, `total_runtime_ms`, `format_runtime` per data-model §2–§3; doc comments with runnable examples (Constitution VII). Makes T007 pass — Implements: FR-001, FR-002
- [X] T012 [US1] In `crates/modplayer-ui/src/rows.rs` make `draw_artwork_url` size-parameterised (row 40 px keeps working, header 128 px) with the same `ArtworkCache` and initials fallback — Implements: FR-001
- [X] T013 [US1] In `detail_view.rs` implement `collection_header(ui, …) -> HeaderEvent { None | Back | Action(RowAction) }`: exact-size rect `(available_width, HEADER_HEIGHT)`; artwork; truncated DISPLAY title (`text_primary`, hover tooltip = full title); one truncated SECONDARY facts label (`text_secondary`); `Variant::Primary` Play via `widgets::controls::button` (disabled + `detail-no-tracks-hint` description/tooltip iff `track_count == 0`); "…" via shared `rows::actions_menu`; `Group` accesskit node named by title. Reserve the top back row (button itself added in US2) — Implements: FR-001, FR-002, FR-003, FR-004, FR-012, NFR-6.1, NFR-6.2
- [X] T014 [US1] Wire into `detail_view::show`: replace the three `draw_*_header` fns for playlist/album/artist; build `FactsInput` from ref + `TrackListState::Cached` (`Some` only when cached); route `HeaderEvent::Action(a)` through `library_view::apply_row_action(controller, &entity, &tracks_or_empty, a)`; retire the `!editable`-only owner line — Implements: FR-001, FR-002, FR-003, FR-004
- [X] T015 [US1] Update the existing test `playlist_detail_shows_owner_label…` in `crates/modplayer-ui/tests/library_view.rs` to assert `detail-owner` (intentional contract change per quickstart) — Implements: FR-002

**Checkpoint**: US1 tests T007–T010 pass; header works for all three kinds; independently demoable.

---

## Phase 4: User Story 2 - Back to the library where I left off (Priority: P1)

**Goal**: Header Back control (text label, top-left, first in tab order) returns to the library tab at its prior scroll position.

**Independent Test**: Scroll a long library list, open an item, Back → same first visible row.

### Tests for User Story 2 (write first)

- [X] T016 [P] [US2] Extend `crates/modplayer-ui/tests/section_memory.rs` with S1–S6 using a shared `SectionMemory` over frames (large fixture): S1 round trip (offset and first visible row equal), S2 active tab preserved (Playlists), S3 content shrank → offset == `max_scroll`, S4 width 1400→960 keeps first visible row, S5 restore applied once (user scroll next frame not overridden), S6 nothing written to disk / sign-out resets — Implements: FR-006
- [X] T017 [P] [US2] Add H10 tests to `crates/modplayer-ui/tests/library_view.rs`: Back is `Variant::Quiet`, label `detail-back`, rect min.x/min.y ≤ every other header child, first focusable (Tab from page start reaches Back before Play), click / Backspace / Alt+Left → `DetailOutcome::Back`, Back usable while header is a skeleton — Implements: FR-005, NFR-6.1

### Implementation for User Story 2

- [X] T018 [US2] In `detail_view.rs` `collection_header`, add the `Variant::Quiet` Back button (`tr("detail-back")`) at the header top-left, first in focus order; return `HeaderEvent::Back`; merge with existing Backspace / Alt+Left into `DetailOutcome::Back`; remove the old standalone `detail-back` button — Implements: FR-005
- [X] T019 [US2] Run T016. If a test exposes a defect (R3: loading/refreshing banner shifting content on the return frame, or `auto_shrink([false,true])` vs the seed), fix at the call site in `crates/modplayer-ui/src/library_view.rs` or `app.rs`, NOT in `section_memory.rs`. If all pass, no production change; note the outcome in `research.md` R3 — Implements: FR-006

**Checkpoint**: US1 + US2 work together (full browse loop).

---

## Phase 5: User Story 3 - Row actions stay with their row, quietly, by keyboard (Priority: P2)

**Goal**: "…" always inside its row at every width, Quiet styling, full keyboard path.

**Independent Test**: At 960 × 640 every row's "…" lies in its row; operate all six actions by keyboard only.

### Tests for User Story 3 (write first)

- [X] T020 [P] [US3] Extend `crates/modplayer-ui/tests/rows.rs`: RM1 (opener Quiet: transparent fill, no outline at rest, always visible), RM2 (opener ⊂ row rect and ⊂ viewport clip for every row kind at widths {400, 480, 560, 960 − nav rail, 1400} and text scale 100 % / max), RM3 (reserved width = measured opener + 2·`item_spacing.x` + `duration_measure`; row height width-independent), RM4 (popup anchored to originating row, also via Shift+F10), RM8 (existing action tests untouched; greyed rows keep six actions) — Implements: FR-007, FR-008, FR-009
- [X] T021 [P] [US3] Add keyboard tests to `crates/modplayer-ui/tests/rows.rs` (RM5–RM7): Shift+F10 / Menu key / secondary click / Enter-Space on opener open the menu; ↓↑ wrap (5→0, 0→5), Home/End, Enter/Space activate, Escape closes without action; focus returns to row (or header "…"); unit test `step()` wrap math — Implements: FR-008, NFR-6.1
- [X] T022 [P] [US3] Extend `crates/modplayer-ui/tests/accessibility.rs` and `tests/interaction_states.rs` / `tests/control_variants.rs` for RM9 and Quiet hover/focus-ring states of the opener (name `row-actions{name}`, role Button; items role MenuItem) — Implements: FR-009, FR-012, NFR-6.1, NFR-6.2

### Implementation for User Story 3

- [X] T023 [US3] In `crates/modplayer-ui/src/rows.rs` switch "…" opener to `widgets::controls::button(ui, Variant::Quiet, "…")`, always visible; replace constant `ACTIONS_RESERVED_WIDTH` with a measured value (button width + `item_spacing.x`, R4) so text column ≥ 0 and the opener never leaves the row rect — Implements: FR-007, FR-009
- [X] T024 [US3] In `crates/modplayer-ui/src/actions.rs` add the Menu/ContextMenu key alongside Shift+F10 in `row_claims` (if egui 0.36 exposes it; otherwise record in `research.md` R7 that Shift+F10 only is used) and add `row_menu_item_claims` so menu keys never trigger global bindings — Implements: FR-008
- [X] T025 [US3] In `rows.rs` implement `MenuNav` + pure `step(focused, key, len)` (Down/Up wrap via `rem_euclid`, Home/End) and the menu keyboard path (pattern from `settings/category_row.rs`); on activate or dismiss `request_focus(return_to)` and clear `MenuNav` from egui temp memory; anchor popup to the originating row's opener — Implements: FR-007, FR-008
- [X] T026 [US3] Confirm the header "…" (T013) uses the same shared menu with `return_to` = header opener id; adjust `collection_header` if needed so RM5–RM7 pass for the header too — Implements: FR-004, FR-008

**Checkpoint**: US3 tests pass; all lists and header menus keyboard-operable.

---

## Phase 6: User Story 4 - Stable loading and meaningful empty states (Priority: P3)

**Goal**: Skeleton rows equal loaded row height/shape; empty states unchanged.

**Independent Test**: Compare skeleton and loaded row heights per tab (0 px diff); each empty state still shows its sentence and one action.

### Tests for User Story 4 (write first)

- [X] T027 [P] [US4] Extend `crates/modplayer-ui/tests/library_view.rs` and unit tests in `widgets/skeleton.rs`: K1 (3 skeletons; 56 for Saved Tracks/Recently Played, 72 for Saved Albums/Followed Artists/Playlists), K2 (skeleton height == loaded `list_row` height per tab and detail track list; first loaded row y == first skeleton y), K3 (artwork square `ARTWORK_SIZE` + `text_lines` bars), K4 (one `Role::Status` "loading", no `ListItem`, not selectable), K5 (header skeleton height == `HEADER_HEIGHT`, 128 px square, title and facts bars, Back live and functional), K6 (empty-state sentences and single primary action unchanged) — Implements: FR-010, FR-011

### Implementation for User Story 4

- [X] T028 [US4] Add `LibraryTab::skeleton_shape()` (SavedTracks/RecentlyPlayed → TRACK, SavedAlbums → ALBUM, FollowedArtists → ARTIST, Playlists → PLAYLIST) and use it in the `library_view.rs` loading branch (fixes today's 56 px for every tab); count stays 3 — Implements: FR-010
- [X] T029 [US4] Use `SkeletonShape::TRACK` for the detail track-list loading skeleton in `detail_view.rs` — Implements: FR-010
- [X] T030 [US4] Implement `header_skeleton(ui) -> bool` in `widgets/skeleton.rs` (rect height == `HEADER_HEIGHT`, 128 px square, title bar, facts bar, live Back button) and use it in `detail_view::show` when the ref is missing — Implements: FR-005, FR-010
- [X] T031 [US4] Verify empty states untouched (K6): no edits to `library-empty*` / `playlist-no-tracks` rendering; existing empty-state tests pass unmodified — Implements: FR-011

**Checkpoint**: all four stories independently functional.

---

## Phase 7: Polish & Cross-Cutting

- [X] T032 Run gates: `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`, `rtk cargo test --workspace`, `cargo deny check`; confirm NFR-10.3 real-time suites untouched and green — Implements: NFR-10.3, FR-001–FR-013 (quality gates)
- [X] T033 [P] Confirm no new/changed hard-coded user-facing strings remain in `detail_view.rs`, `rows.rs`, `widgets/skeleton.rs` (grep for string literals), and that `tests/fluent_keys.rs` passes (FR-012) — Implements: FR-012, NFR-7.1
- [X] T034 Execute manual scenarios M1–M8 from `quickstart.md` via the macOS Quartz recipe (`MODPLAYER_LIBRARY_FIXTURE=large`, `MODPLAYER_ARTWORK_FORCE_FAIL=1`); record pass/deviation and evidence path per scenario in `quickstart.md` (Constitution Governance › Manual Scenario Sign-Off) — Implements: FR-001–FR-012 (manual sign-off)
  - 2026-09-30 results (evidence in `target/manual-walk/`, full table in `quickstart.md` › Manual scenario sign-off): M1 pass · M2 deviation (album/artist headers not reachable live with any account: the Saved Albums/Followed Artists sets come from `/collection/v2/paging`, which returns HTTP 400 so they are always empty, confirmed after the maintainer saved an album and followed 2 artists; covered by automated H1/H4) · M3 pass · M4 pass, except ellipsis not exercised (no long title at the 960 minimum; H3) · M5 pass on the Playlists tab (fixture has no albums) · M6 pass · M7 pass (Shift+F10 only, R7) · M8 pass, except wide-tab and header skeletons not reproducible live (cached library, unpersisted section memory; K1–K6)
- [X] T035 [P] Verify FR-013 scope: `git diff` shows no change to library sorting or offline-pin indicators; `section_memory.rs` unchanged — Implements: FR-013

---

## Dependencies & Execution Order

- **Phase 1 → Phase 2 → stories → Polish.**
- **Phase 2**: T002 → T004; T003 → T004; T005, T006 independent ([P]).
- **US1 (P1)**: needs T002/T003 (strings), T005 (menu), T006 not required. T011 → T013 → T014; T012 → T013; T015 after T014.
- **US2 (P1)**: T018 depends on T013 (header exists). T016/T019 independent of US1 code.
- **US3 (P2)**: T023–T025 in `rows.rs` are sequential (same file); T026 depends on T013 and T025. Depends on T005.
- **US4 (P3)**: needs T006. T030 depends on T013 (`HEADER_HEIGHT`). T028 independent of other stories.
- Stories US3 and US4 can proceed in parallel with each other after Phase 2 (different files, except `detail_view.rs` touches by T029/T030 vs US1/US2 — serialize those).
- **Polish** after all desired stories.

## Parallel Examples

```text
Phase 2:   T003 ‖ T005 ‖ T006  (after/alongside T002)
US1 tests: T007 ‖ T008 ‖ T009 ‖ T010
US3 tests: T020 ‖ T021 ‖ T022
US3 ‖ US4 implementation (rows.rs/actions.rs vs skeleton.rs/library_view.rs)
```

## Implementation Strategy

- **MVP**: Phases 1–3 (US1). Then US2 (completes browse loop, both P1).
- Incremental: US3 (P2), US4 (P3); each ends at a green checkpoint.
- Test-first within each story: run new tests, see them fail, implement, see them pass.

## Notes

- Total: 35 tasks. US1: 9 (T007–T015). US2: 4 (T016–T019). US3: 7 (T020–T026). US4: 5 (T027–T031). Setup 1, Foundational 5, Polish 4.
- Commit after each task or logical group.

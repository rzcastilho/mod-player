---

description: "Task list for 026-search-results-structure"
---

# Tasks: Search Results Structure and Feedback

**Input**: Design documents from `/specs/026-search-results-structure/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ (results-layout, search-feedback, search-status, fluent-strings), quickstart.md

**Tests**: INCLUDED. Constitution VIII + plan require test-first per contract (RL1–RL12, F/C/S/N, R/V/E/D, P1–P4). Write each test task first and confirm it FAILS before its implementation task.

**Organization**: Grouped by user story (US1 P1, US2 P2, US3 P3). `search_view.rs` is edited by all three stories, so its tasks are sequential (no `[P]`); stories are independently testable but should be merged in priority order.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an incomplete task
- **[Implements: …]**: requirement IDs (spec FR-/SC-, constitution NFR-) the task implements or verifies; contract rule IDs (RL/F/C/S/N/R/V/E/D/P) in descriptions trace to these via each contract's **Covers** line
- Toolchain: prefix cargo with `RUSTUP_TOOLCHAIN=1.95.0`

## Path Conventions

Cargo workspace. Core: `crates/modplayer-core/src/`. UI: `crates/modplayer-ui/src/` and `crates/modplayer-ui/tests/`. Connect: `crates/modplayer-audio-source-connect/src/`. Locales: `locales/{en-US,pt-BR}/library.ftl`.

---

## Phase 1: Setup

**Purpose**: Confirm baseline before changing anything

- [X] T001 Run `RUSTUP_TOOLCHAIN=1.95.0 cargo test -p modplayer-core -p modplayer-ui` from the worktree root and record that the baseline is green; note current line refs of `GROUP_VISIBLE_ROWS`, `search-placeholder` and `refreshing` uses in `crates/modplayer-ui/src/search_view.rs`, `crates/modplayer-ui/tests/search_view.rs` (~L361) and `crates/modplayer-ui/tests/accessibility.rs` (~L1395) [Implements: FR-013]

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared strings, shared backoff, shared helpers used by more than one story

**⚠️ CRITICAL**: No user story work starts until this phase is complete

- [X] T002 [P] Add the 8 new keys (`search-field-label`, `search-hint`, `search-clear`, `search-in-flight`, `search-result-count`, `search-group-header`, `search-stale`, `search-rate-limited`) with exact en-US text from contracts/fluent-strings.md to `locales/en-US/library.ftl`; keep `search-placeholder` until T027 removes its consumers [Implements: FR-012a, NFR-7.1]
- [X] T003 [P] Add the same 8 keys (pt-BR values from contracts/fluent-strings.md) plus pt-BR translations of existing keys `search-offline`, `search-no-results`, `search-group-tracks`, `search-group-albums`, `search-group-artists`, `search-group-playlists`, `search-show-more` to `locales/pt-BR/library.ftl` [Implements: FR-012a, NFR-7.1]
- [X] T004 [P] Extend `crates/modplayer-ui/tests/fluent_keys.rs` to exercise the 8 new keys (with `tr_args` for `search-result-count` `count` and `search-group-header` `group`+`count`, including plural one/other) and assert en-US/pt-BR parity for them and the 7 existing keys added in T003 [Implements: FR-012a, NFR-7.1]
- [X] T005 [P] Create `crates/modplayer-core/src/backoff.rs`: move `backoff_delay`, `SYNC_BACKOFF_BASE`, `SYNC_BACKOFF_MAX` (and their unit tests) out of `crates/modplayer-core/src/library/sync.rs` unchanged (`checked_shl`/`saturating_add`), add `mod backoff;` (crate-private) to `crates/modplayer-core/src/lib.rs`, and make `library/sync.rs` `use crate::backoff`. No behaviour change (R10) [Implements: FR-011a, FR-013]
- [X] T006 [P] Change `severity_glyph` and `severity_color` to `pub(crate)` in `crates/modplayer-ui/src/notifications.rs` (used by the status strip in US3) [Implements: FR-011]
- [X] T007 Run `cargo test -p modplayer-core backoff sync` and `cargo test -p modplayer-ui --test fluent_keys`; both must pass (depends on T002–T006) [Implements: FR-011a, FR-012a, NFR-7.1]

**Checkpoint**: Foundation ready

---

## Phase 3: User Story 1 - Scroll search results as one page (Priority: P1) 🎯 MVP

**Goal**: One results `ScrollArea`, stacked single-column groups in fixed order, virtualised rows, pinned header, header counts; nested group scroll areas removed.

**Independent Test**: At 960 × 640 with all four groups loaded, one scroll gesture reaches Playlists; each header stays pinned while its group scrolls; header counts equal rendered rows (contracts/results-layout.md RL1–RL12).

### Tests for User Story 1 (write first, must FAIL)

- [X] T008 [P] [US1] In new `crates/modplayer-ui/src/search_layout.rs` (`#[cfg(test)]`), write proptests P1–P4 (generators per contracts/results-layout.md) for `ResultsLayout::new`, `visible_rows`, `pinned_header` against data-model §3 invariants L1–L4; module stub with `todo!()`-free signatures so tests compile and fail; register `mod search_layout;` in `crates/modplayer-ui/src/lib.rs` [Implements: FR-001, FR-002, FR-003]
- [X] T009 [P] [US1] Extend `crates/modplayer-ui/tests/search_view.rs` with headless tests RL1 (exactly one scroll state; `GROUP_VISIBLE_ROWS` absent), RL2 (field bounds unchanged after 2000 pt wheel scroll), RL3 (Albums `Empty` + Artists `Unsupported` → headers exactly [Tracks, Playlists]), RL4 (400-row Tracks → < 20 `list_row` nodes), RL5 (960 × 640 scroll to max reaches Playlists header), RL6 (pinned Tracks header top == viewport top mid-group), RL7 (one `Role::Header` per shown group; click on pinned header rect emits no `RowEvent::Select`), RL8 (labels "Tracks, 20 results" → "Tracks, 40 results" after Show more; stale 20; `Pending` → "Tracks"), RL9 (Show more after last row, before next header), RL10 (headers share left x at 960 and 1920 widths), RL11 (offset resets on query change, not on Show more) [Implements: FR-001–FR-006, SC-001, SC-002, SC-006]
- [X] T010 [P] [US1] Confirm `crates/modplayer-ui/tests/section_memory.rs` `ViewKey::Search` cases still exist and will be exercised unchanged (RL12); add a case only if missing [Implements: FR-001]

### Implementation for User Story 1

- [X] T011 [US1] Implement `GroupBlock`, `PlacedGroup`, `ResultsLayout::new(blocks, gap)`, `visible_rows`, `pinned_header` in `crates/modplayer-ui/src/search_layout.rs` per data-model §3 and research R2/R3 until T008 passes [Implements: FR-001, FR-002, FR-003]
- [X] T012 [US1] In `crates/modplayer-ui/src/app.rs`, change the `Section::Search` arm to stop wrapping `search_view::show` in the section `ScrollArea` and pass `&mut SectionMemory` (keyed `ViewKey::Search`) into the view [Implements: FR-001]
- [X] T013 [US1] Rework `crates/modplayer-ui/src/search_view.rs`: draw fixed chrome (field row) then exactly one results `ScrollArea` bound to `SectionMemory` `ViewKey::Search`, using `show_viewport` over a `ResultsLayout` built from session groups (shown-group rule V3, order tracks/albums/artists/playlists, `space::LG` gap, 3 skeleton rows for `Pending`, "Show more" footer as last element of the group, viewport ±1 row virtualisation, single column) [Implements: FR-001, FR-002, FR-006]
- [X] T014 [US1] In `crates/modplayer-ui/src/search_view.rs`, draw group headers: `theme::section_label(name)` + count in `text_secondary`, accesskit `Role::Header` label from `search-group-header` (`Pending` → name only); count = `GroupState::items().len()` [Implements: FR-004, NFR-6.1]
- [X] T015 [US1] In `crates/modplayer-ui/src/search_view.rs`, paint the pinned header last at `pinned_header(viewport_top)` (suppress the in-flow copy, block row clicks beneath it) and reset the scroll offset to 0 when the effective query changes (`last_query` detection), not on Show more/stale transitions [Implements: FR-001, FR-003]
- [X] T016 [US1] Delete `GROUP_VISIBLE_ROWS` and all per-group `virtualized_list` calls/nested scroll areas from `crates/modplayer-ui/src/search_view.rs`; offline and no-results messages render inside the single area (or replace it) with no groups [Implements: FR-001, FR-005]
- [X] T017 [US1] Run `cargo test -p modplayer-ui search_layout` and `cargo test -p modplayer-ui --test search_view --test section_memory`; fix until green. Verify `grep -rn GROUP_VISIBLE_ROWS crates/` has no match [Implements: FR-001–FR-006]

**Checkpoint**: US1 fully functional and testable independently (MVP)

---

## Phase 4: User Story 2 - Clear feedback on the query field (Priority: P2)

**Goal**: Field labelled once, trailing clear control, in-flight spinner, settled count line announced once.

**Independent Test**: Type a query → spinner during flight, "<n> results" on settle, × empties field and refocuses it (contracts/search-feedback.md F/C/S/N).

### Tests for User Story 2 (write first, must FAIL)

- [X] T018 [P] [US2] Add core unit tests in `crates/modplayer-core/src/search.rs` for `in_flight()` truth table (idle, debouncing, combined outstanding, loaded, show-more outstanding, offline+armed) and `generation()` bumping on new effective query (S1; retry-waiting/issued rows added in T029) [Implements: FR-009]
- [X] T019 [P] [US2] Extend `crates/modplayer-ui/tests/search_view.rs` with F1 (no "Search" label node; `TextInput` label == `search-field-label`), F2 (hint == `search-hint`), C1 (absent for `""`, present for `" "` and `"abba"`), C2 (button right edge ≥ field right edge, Quiet), C3 (Tab then Enter empties query), C4 (click → `raw_query()==""`, groups `Idle`, field focused next frame), S1/S2/S3 (spinner `ProgressIndicator` "Searching…" present on first frame after typing, gone on reply frame; absent for Show more), N1–N4 (status matrix; "25 results" → "45 results" after Show more; `Live::Polite` only on first settled frame per generation; hidden while strip shows) [Implements: FR-007–FR-010, SC-003, NFR-6.1, NFR-6.2]
- [X] T020 [P] [US2] Update `crates/modplayer-ui/tests/accessibility.rs` search-field case to assert the new accessible name/hint keys instead of `search-placeholder` [Implements: FR-007, NFR-6.1]

### Implementation for User Story 2

- [X] T021 [US2] Add read-only `in_flight()` and `generation()` accessors with doc comments (runnable examples) to `SearchSession` in `crates/modplayer-core/src/search.rs`: `in_flight = debounce_deadline.is_some() || pending_request_id.is_some()` (retry clause added in T035) [Implements: FR-009, FR-010, FR-013]
- [X] T022 [US2] Add `announced_generation: Option<u64>` and `refocus_field: bool` to `SearchViewState` in `crates/modplayer-ui/src/search_view.rs` (defaults `None`/`false`) [Implements: FR-008, FR-010]
- [X] T023 [US2] In `crates/modplayer-ui/src/search_view.rs` remove the visible "Search" label, set the field accessible label to `search-field-label` (no `labelled_by`) and hint text to `search-hint`; keep focus shortcut and Escape behaviour (F3/F4) [Implements: FR-007, NFR-6.1]
- [X] T024 [US2] In `crates/modplayer-ui/src/search_view.rs` add the trailing `widgets::controls::button(ui, Variant::Quiet, "×")` labelled `search-clear`, shown iff `raw_query()` non-empty, in tab order right after the field; activation calls `set_query("", now)` (same path as Escape) and sets `refocus_field`; consume it next frame with `request_focus` on the field id [Implements: FR-008, NFR-6.1, NFR-6.2]
- [X] T025 [US2] In `crates/modplayer-ui/src/search_view.rs` add `egui::Spinner` before the clear control shown iff `!offline && in_flight()`, with accesskit `Role::ProgressIndicator` label `search-in-flight` [Implements: FR-009, NFR-6.1]
- [X] T026 [US2] In `crates/modplayer-ui/src/search_view.rs` add the pure `view_status(&SearchSession) -> SearchStatus` (data-model §5, precedence Offline > Idle > InFlight > NoResults > RateLimited > Settled) and the count line below the field: `search-result-count` with Σ header counts, `Role::Status`, `Live::Polite` only when `announced_generation != Some(generation())` (then record it), hidden while the RateLimited strip shows [Implements: FR-010]
- [X] T027 [US2] Remove `search-placeholder` from `locales/en-US/library.ftl`, `locales/pt-BR/library.ftl`, `crates/modplayer-ui/tests/fluent_keys.rs` and any remaining code use (`grep -rn search-placeholder crates locales` must be empty) [Implements: FR-007, FR-012a, NFR-7.1]
- [X] T028 [US2] Run `cargo test -p modplayer-core search` and `cargo test -p modplayer-ui --test search_view --test accessibility --test fluent_keys`; fix until green [Implements: FR-007–FR-010]

**Checkpoint**: US1 + US2 work independently

---

## Phase 5: User Story 3 - Clear stale and empty states (Priority: P3)

**Goal**: Automatic backed-off retry of rate-limited requests, inline status strip, empty state with truncated query and Clear button, debug seam.

**Independent Test**: Rate-limited Show more keeps rows under the strip and recovers after the retry; empty result names the (truncated) query and one click clears it (contracts/search-status.md R/V/E/D).

### Tests for User Story 3 (write first, must FAIL)

- [X] T029 [P] [US3] Add core unit tests R1–R9 in `crates/modplayer-core/src/search.rs` with injected `Instant`: combined rate-limit schedules retry (R1), show-more rate-limit keeps stale + `resume_offset` (R2), `tick` before/at `due` emits exactly one same-generation `SearchCatalog` (R3), retry success for combined and show-more (R4), repeated rate limit reschedules with `attempt+1` honouring `retry_after_ms` (R5), other error on retry (R6), cancellation by query edit / clear / new generation / `set_offline(true)` and discard of stale replies (R7), offline collapse rules (R8), earlier-query results never shown (R9); extend the `in_flight()` table with retry waiting/issued [Implements: FR-011, FR-011a, FR-013, SC-004]
- [X] T030 [P] [US3] Add `truncate_query` unit + proptest in `crates/modplayer-ui/src/search_layout.rs` (≤ 61 scalars, identity for ≤ 60, prefix-preserving, multi-byte safe) per research R11 [Implements: FR-012]
- [X] T031 [P] [US3] Extend `crates/modplayer-ui/tests/search_view.rs` with V1–V7 (strip visibility, both texts, `surface_raised`/warning glyph/colour, single `Role::Status` node without glyph, no notification raised, stale rows keep counts and no Show more, nothing stale → no groups/spinner/count, old `refreshing` Status node gone) and E1–E3 (200-char query truncated, message width ≤ body measure, Secondary "Clear search" click empties + refocuses, no count/spinner/strip); update the ~L361 `refreshing` assertion here and the ~L1395 one in `crates/modplayer-ui/tests/accessibility.rs` to the new keys [Implements: FR-011, SC-004, NFR-6.1]
- [X] T032 [P] [US3] Add unit test D1 for the debug-only `paged-once` decision fn in `crates/modplayer-audio-source-connect/src/catalog/mod.rs` (first `offset > 0` search → `RateLimited{retry_after_ms: None}`, later calls dispatch; other values rate-limit everything) [Implements: FR-011a]

### Implementation for User Story 3

- [X] T033 [US3] In `crates/modplayer-core/src/search.rs` add `resume_offset: Option<u32>` to `GroupState::RateLimited`, the private `Retry { due, attempt, issued }`, fields `combined_retry` and `show_more_retries: [Option<Retry>; 4]`, and update all existing constructors/matches (V1/V2 invariants) [Implements: FR-011, FR-011a]
- [X] T034 [US3] In `crates/modplayer-core/src/search.rs` change to `apply_reply(&mut self, request_id, result, now: Instant)`; implement the data-model §2 transitions: schedule on combined/show-more `RateLimited` using `crate::backoff::backoff_delay(attempt, retry_after_ms)`, resolve on retry replies (success, reschedule, other error), clear retries on query edit / `reset_to_idle` / new generation, and offline collapse (R8) [Implements: FR-011a]
- [X] T035 [US3] In `crates/modplayer-core/src/search.rs` extend `tick(now)` to emit the due retry (combined: all four kinds @0; show-more: `[kind]` @`resume_offset`), same generation, new request id, mark `issued`; add `retry_scheduled()` accessor; update `in_flight()` to `debounce_deadline.is_some() || (pending_request_id.is_some() && combined_retry.is_none())` [Implements: FR-011a]
- [X] T036 [US3] Update `route_search_result` in `crates/modplayer-core/src/controller.rs` to pass `self.now()` to `apply_reply`; fix any other callers [Implements: FR-011a]
- [X] T037 [US3] In `crates/modplayer-ui/src/search_view.rs` render the status strip above the scroll area when `view_status` is `RateLimited{stale}`: `surface_raised` fill, `radius::SM`, `severity_glyph(Severity::Warning)` in `severity_color(roles, Warning)`, text `text_primary` from `search-stale` / `search-rate-limited`, one `Role::Status` node (label = text only); remove the old `refreshing` Status label from Search only; hide spinner and count line while it shows [Implements: FR-011, NFR-6.1]
- [X] T038 [US3] In `crates/modplayer-ui/src/search_layout.rs` implement `truncate_query` (first 60 Unicode scalars + "…", with doc example); in `crates/modplayer-ui/src/search_view.rs` render the empty state: `search-no-results` with truncated `$query` wrapped within `min(available, body_measure)`, followed by a `Variant::Secondary` `search-clear` button sharing the T024 clear handler; no count/spinner/strip [Implements: FR-012, SC-005, NFR-6.1, NFR-6.2]
- [X] T039 [US3] Add the debug-only (`#[cfg(debug_assertions)]`) `paged-once` decision fn (atomic "first offset>0 seen") to `crates/modplayer-audio-source-connect/src/catalog/mod.rs` and consult it with the command offset in `crates/modplayer-audio-source-connect/src/worker.rs`; release builds compile it out [Implements: FR-011a]
- [X] T040 [US3] Run `cargo test -p modplayer-core search backoff sync`, `cargo test -p modplayer-ui --test search_view --test accessibility --test high_contrast --test design_token_literals`, `cargo test -p modplayer-audio-source-connect force_rate_limited`; fix until green [Implements: FR-011–FR-012a, FR-013]

**Checkpoint**: All user stories independently functional

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T041 [P] Verify no `unwrap`/`expect` outside tests and that new pub items (`SearchSession::in_flight/generation/retry_scheduled`, `ResultsLayout`, `truncate_query`) have doc comments with runnable examples in `crates/modplayer-core/src/search.rs` and `crates/modplayer-ui/src/search_layout.rs` [Implements: FR-013]
- [X] T042 Run full gates from quickstart.md: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`; confirm `grep -rn GROUP_VISIBLE_ROWS crates/` and `grep -rn search-placeholder crates locales` are empty [Implements: FR-001–FR-013, NFR-6.1, NFR-6.2, NFR-7.1]
- [X] T043 Execute manual scenarios M1–M11 from quickstart.md (macOS Quartz recipe; `MODPLAYER_CATALOG_FORCE_429=paged-once` for M7, `=1` for M8) and record pass/deviation + evidence path under each scenario line in this file (Governance › Manual Scenario Sign-Off) [Implements: SC-001–SC-006, FR-001–FR-012a]
  - Run 2026-09-30, debug build, live signed-in account (OAuth re-consent via **Retry** on launch), window 960 × 640 content. Evidence under `target/manual-walk/shots/m/` (gitignored); logs `target/manual-walk/run-*.log`.
  - **Environment facts that shape every scenario**: (a) the live source answers **Tracks only** — `hm://searchview` 404s, the context-resolve fallback reports Albums/Artists/Playlists `Unsupported` (004 research V1), so only the Tracks group is ever shown live; (b) the fallback returns one page of 20, so **Show more exhausts at 20** (`next_offset: None`); (c) synthetic `CGEvent` scroll-wheel events never reach egui on this host (tried line/pixel units, explicit flags, HID/session/pid taps; Library also static), so scrolling was driven by **dragging the results scrollbar**; wheel scrolling stays covered headless by RL2/RL5; (d) keycode-0 Unicode keyboard events are dropped — typing used real keycodes.
  - **M1 Pass (Tracks-only deviation)**: one scrollbar for the results only, field + count line fixed above it, groups single-column, no inner scrollbars; dragged to max reaches "Show more Tracks" after the last row. Playlists header can't be reached live (fact a); RL5 covers it. `m1-top-bar.png`, `m1-bottom-fixed.png`.
  - **M2 Pass after fix**: mid-group "TRACKS 20" pinned at the viewport top, rows slide beneath (`m2-mid-tracks.png`). **Defect found and fixed**: at max scroll the pinned header sat 2 pt above the viewport top (clipped) because it was placed at the clamped `top`, while egui's content runs slightly past `layout.total_height` (`m1-bottom-rest.png`, text rows 174–191 px vs 178–195 px in flow). `draw_results` now pins at egui's real top while it's inside the content; new test `rl6_header_pins_at_viewport_top_at_max_scroll` (failed 47.97 vs 50 before the fix). Re-walked: rows 178–195 px (`m1-bottom-fixed.png`). Albums push-out not observable live (fact a); P3/P4 cover it.
  - **M3 Deviation (source)**: "Show more Tracks" issues the offset-20 request (log `offset=20`); the source exhausts, so the footer disappears, header and count stay 20 (= rendered rows), scroll position kept (clamped to the shorter content), no reset (`m3-after-show-more.png`). 20 → 40 covered by RL8/N2.
  - **M4 Pass**: at the bottom, appending "s" returns the area to the top with "TRACKS"/"ALBUMS"/"ARTISTS" headers without counts over skeleton rows, spinner shown; settles to "20 results" (`m4-edited.png`, `m4-settled.png`).
  - **M5 Pass**: empty — no "Search" label, hint "Tracks, albums, artists, playlists", no × (`m5-empty.png`); typing — spinner + × at the trailing edge (`probe-keys.png`); settled — spinner gone, "20 results" (`m5-settled.png`). VoiceOver spot check not run.
  - **M6 Pass**: Tab from the field lands on × (focus ring, `m6-tabbed.png`); Space empties the field and idles the results (`m6-cleared.png`); typing "love" goes straight into the field (`m6-typing-after.png`).
  - **M7 Pass (source deviation on recovery)**: `paged-once`; Show more → 20 rows kept, strip "⚠ Showing earlier results — search is busy, refreshing shortly" on raised surface, count line and Show more hidden, no toast (`m7-stale.png`). After ~8–10 s backoff the retry goes out (one wire `offset=20` request in `run-m7.log`) and the strip is gone with "20 results" back (`m7-t6.png`); 40 rows impossible live (fact b).
  - **M8 Pass**: `=1`; "⚠ Search is busy — retrying shortly", no groups/spinner/count (`m8-limited.png`). While editing (debounce re-armed by continuous typing) the strip is gone and the spinner shows (`m8-typing-burst.png`); once the edited query is itself force-429'd the strip returns, as expected with `=1` (`m8-after-edit.png`).
  - **M9 Pass after fix**: **Defect found and fixed**: the 80 × "q" search left a blank page — `SearchSession::is_no_results` needs all four groups `Empty`, but live three are `Unsupported`, so the empty state could never show (`m9-empty` first capture, pre-fix). The view now treats `Unsupported` as omitted for the empty state (`search_view::is_no_results`); new test `e1_empty_state_shows_when_other_groups_are_unsupported`. Re-walked: message quotes 60 q's + "…", wraps within the measure, "Clear search" below, no count/spinner/strip (`m9-empty.png`); one click empties and focuses the field — typing "ab" lands in it (`m9-typing-after.png`).
  - **F4 found during M9 and fixed**: Escape in the focused field didn't empty the query (pre-existing on `main`; no test existed despite the contract's "existing test kept") — egui's `TextEdit` gives up focus on the Escape frame, so `has_focus()` was false. Now also accepts `lost_focus()`; new test `f4_escape_in_the_field_empties_the_query`. Re-walked: Esc empties the field (`f4-escape.png`).
  - **M10 Not run (environment)**: turning the host's Wi-Fi off would also cut the agent's own session, and no debug offline seam exists (`MODPLAYER_CONNECT_FORCE_UNAVAILABLE` only reclassifies source errors). Offline precedence and collapse are covered by core R8 and the view's offline tests.
  - **M11 Pass**: High contrast on + `paged-once` stale strip: strip text, glyph and "TRACKS 20" header legible, border/field ring boosted, colours from roles (`m11-hc-stale.png`). Setting restored to off afterwards (`m11-hc-restored.png`).

---

## Dependencies & Execution Order

### Phase Dependencies

- Setup (P1) → Foundational (P2, blocks all stories) → US1 → US2 → US3 → Polish
- US2 and US3 both edit `search_view.rs` and `search.rs`; US3 builds on US2's `view_status`, `in_flight()` and clear handler (T026, T021, T024), so run US1 → US2 → US3 in order. Core work (T029, T033–T036) and connect seam (T032, T039) do not touch the view and can be developed in parallel with US1/US2 UI work by a second developer; merge order still US1 → US2 → US3.

### Task-level

- T008 → T011; T009 → T012–T016; T011 before T013
- T012 before T013 (call-site signature); T013 → T014 → T015 → T016 → T017
- T021 before T025/T026; T024 before T038; T026 before T037
- T033 → T034 → T035 → T036; T034/T035 before T037 live verification
- T005 before T034 (shared backoff); T006 before T037; T002/T003 before any UI string use

### Parallel Opportunities

- Foundational: T002, T003, T004, T005, T006 together
- US1 tests: T008, T009, T010 together
- US2 tests: T018, T019, T020 together
- US3 tests: T029, T030, T031, T032 together (different files)
- Core tasks T033–T036 ‖ UI tasks T037–T038 ‖ connect T039 once T033/T034 signatures are agreed

### Parallel Example: US3 tests

```text
Task: "Core unit tests R1–R9 in crates/modplayer-core/src/search.rs"
Task: "truncate_query proptest in crates/modplayer-ui/src/search_layout.rs"
Task: "V1–V7/E1–E3 view tests in crates/modplayer-ui/tests/search_view.rs"
Task: "D1 decision-fn test in crates/modplayer-audio-source-connect/src/catalog/mod.rs"
```

---

## Implementation Strategy

### MVP First (US1 only)

1. Phase 1 + Phase 2
2. Phase 3 (US1) → run T017 gates → validate manual M1–M4 → demo

### Incremental Delivery

1. Foundation → US1 (one page, pinned headers) → US2 (field feedback) → US3 (stale/empty + retry) → Polish
2. Each story keeps prior stories' tests green

## Notes

- Tests-first: confirm each test task fails before implementing
- Catalog requests, page size, debounce, row actions unchanged (FR-013); only retry state and read-only accessors added to `SearchSession`
- All colours from `theme::roles`; every string via Fluent en-US + pt-BR; no new crates or dependencies
- Commit after each task or logical group

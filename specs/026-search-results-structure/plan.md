# Implementation Plan: Search Results Structure and Feedback

**Branch**: `feature/026-search-results-structure` | **Date**: 2026-09-30 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/026-search-results-structure/spec.md`

## Summary

Restructure the Search view so results read as one page and the query field reports its own state (UI/UX Review §3.4 UX-18/19/20, §3.10; completes 004 FR-015). Three slices:

1. **One results page (US1, FR-001–FR-006)** — `app.rs` stops wrapping `search_view::show` in the section `ScrollArea`; the view draws a fixed chrome (field row, count line, status strip) and then exactly **one** results `ScrollArea` (still `SectionMemory`'s `ViewKey::Search` area, so 020 restore/reset keep working). Inside it, `ScrollArea::show_viewport` renders a pure, precomputed `ResultsLayout` (new `search_layout.rs`): per shown group a header, fixed-height rows (virtualised: viewport ±1 row) and an optional "Show more" footer, stacked single-column in fixed order. A **pinned header** is computed by pure arithmetic and painted last over the rows (pushed out by the next header). Headers show name + rendered-row count (`items().len()`) with accessible name "Tracks, 20 results". `GROUP_VISIBLE_ROWS` and the four nested scroll areas are deleted. Offset resets to 0 on a new effective query.
2. **Field feedback (US2, FR-007–FR-010)** — the visible "Search" label goes; the field gets accessible name `search-field-label` and hint `search-hint`. Trailing Quiet "×" (`search-clear`) while text is present, sharing Escape's reset path and refocusing the field. `egui::Spinner` (`ProgressIndicator`, "Searching…") while `SearchSession::in_flight()` (new read-only accessor: debounce armed or first combined request outstanding). A settled "<n> results" `Role::Status` line, `Live::Polite` only on the first settled frame of each generation.
3. **Stale / empty states (US3, FR-011–FR-012)** — `SearchSession` gains automatic retry of rate-limited requests (combined or Show more) on the same generation, using library sync's `backoff_delay` (moved into a shared `modplayer-core/src/backoff.rs`), cancelled by query edit / clear / offline. Inline status strip (019's Warning glyph + `warning` colour on `surface_raised`, `Role::Status`): "Showing earlier results — search is busy, refreshing shortly" or "Search is busy — retrying shortly". Empty state truncates the quoted query to 60 scalars + "…" and adds a Secondary "Clear search" button. A debug-only `MODPLAYER_CATALOG_FORCE_429=paged-once` value lets the stale → recovered path be driven live.

No engine/audio/plugin changes; no new crate or dependency; nothing persisted. Strings: 8 new Fluent keys (en-US + pt-BR), `search-placeholder` removed ([contracts/fluent-strings.md](./contracts/fluent-strings.md)).

**Recorded assumptions (spec left open; most defensible option chosen)**:
- *Offline while a retry is pending* — FR-011a only says "cancel". Chosen: collapse stale rows back to `Loaded` (Show more usable again) and, for a combined rate limit, re-arm the debounce deadline so the query re-runs on reconnect (reuses 004's offline rule). Rejected: leaving `RateLimited` (would falsely claim "refreshing shortly") or collapsing to `Empty` (would falsely show "No results"). Research R9.
- *Spinner during an issued retry* — the spec says the indicator is not shown "while a rate-limit retry is waiting"; chosen to also hide it while the retried request is outstanding (the strip keeps speaking for it; no flicker between strip and spinner). Research R7.
- *Live rehearsal seam* — the existing force-429 toggle cannot produce stale rows then recovery in one process; chosen a single extra debug-only value `paged-once` rather than leaving US3-AS2 automated-only. Research R12.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned in `rust-toolchain.toml`; run with `RUSTUP_TOOLCHAIN=1.95.0` per constitution Governance recipe)

**Primary Dependencies**: `egui`/`eframe` 0.36 (accesskit) — `ScrollArea::show_viewport`, `Spinner`, `Frame`, accesskit node builder (`Role::Header`/`Status`/`ProgressIndicator`, `Live::Polite`); workspace crates `modplayer-core` (`SearchSession`, `GroupState`, `PlaybackController`, `tr`/`tr_args` Fluent, `Severity`), `modplayer-audio-source` (`SearchKind`, `SearchHit`, `SearchPage`, `CatalogError::RateLimited{retry_after_ms}`, `SourceCommand::SearchCatalog`). No new dependencies.

**Storage**: N/A — scroll offset stays in in-memory `SectionMemory`; retry state is in-memory `SearchSession` state.

**Testing**: `cargo test` — core unit tests with injected `Instant` (`crates/modplayer-core/src/search.rs`, `backoff.rs`); proptest for `ResultsLayout` (P1–P4) and `truncate_query`; headless `egui::Context::run_ui` + accesskit tree assertions driven through `ScriptedHost` in `crates/modplayer-ui/tests/search_view.rs` (extended), `tests/accessibility.rs`, `tests/fluent_keys.rs`, `tests/section_memory.rs`, `tests/design_token_literals.rs`, `tests/high_contrast.rs`; manual scenarios M1–M11 in [quickstart.md](./quickstart.md) executed by the implementing agent.

**Target Platform**: Desktop — macOS, Windows, Linux (eframe/winit); minimum window 960 × 640 (`crates/modplayer/src/main.rs` `with_min_inner_size`).

**Project Type**: Desktop application (Cargo workspace, one crate per component). Touches `modplayer-ui`, `modplayer-core` (search + shared backoff), a debug-only seam in `modplayer-audio-source-connect`, and `locales/`.

**Performance Goals**: 60 fps scrolling with a group of 500+ rows (only viewport ±1 rows laid out; layout is O(groups) per frame, row-range math O(1) per group). Spinner visible on the first frame after an edit (SC-003, ≤ 200 ms — structurally 1 frame).

**Constraints**: Exactly one `ScrollArea` in the view; field row/count/status fixed above it; at most one pinned header; header counts == rendered rows; retry policy identical to library sync (`retry_after_ms` honoured, else 15 s·2^n capped at 4 min); every colour from `theme::roles` (017 inherited); every control keyboard-operable with accessible name (NFR-6.1/6.2); all strings Fluent en-US + pt-BR (NFR-7.1); no `unwrap`/`expect` outside tests; catalog requests, page size, debounce and row actions unchanged (FR-013).

**Scale/Scope**: 4 groups, 3 user stories, 13 FRs. Files: `search_view.rs` (major), new `search_layout.rs`, `app.rs` (call site), `notifications.rs` (visibility of two fns), core `search.rs` (retry + accessors), new core `backoff.rs`, `library/sync.rs` (import moved fn), `controller.rs` (pass `now` to `apply_reply`), connect `catalog/mod.rs` + `worker.rs` (debug seam), 2 locale files, ~6 test files.

All unknowns resolved — see [research.md](./research.md) R1–R13; no open clarification remains.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: **IV (debug seam only), VII, VIII, X** and Governance › Manual Scenario Sign-Off. Others N/A with reason.

- [x] **I. Real-Time Path Is Sacred** — N/A: no engine/audio-thread code; search commands travel the existing `SourceHost::command` path off the audio thread; retry scheduling runs in `PlaybackController::tick` on the UI thread.
- [x] **II. Plugins Are Guests** — N/A: no plugin runtime, gateway or plugin UI contribution touched.
- [x] **III. Host Primitives, Plugin Behaviors** — N/A: no markers/loops/effects/transport primitives change; row actions reuse existing `apply_row_action`.
- [x] **IV. Audio Source Replaceable and Isolated** — Pass: the only connect-crate change is inside its own `catalog` module (debug-only `paged-once` value of an existing toggle, `#[cfg(debug_assertions)]`); no new dependency on the connect crate; all automated UI/core tests use `ScriptedHost` / the synthetic source.
- [x] **V. No Audio Ever Leaves the Engine** — N/A: search metadata only; no sample buffers, cache, or file output.
- [x] **VI. Security and Privacy by Default** — N/A: no credential, network-destination, telemetry or persistence change; retries reuse the existing authenticated catalog path and are bounded by the sync backoff cap (4 min).
- [x] **VII. Rust Quality Gates** — Pass: no new crate/dependency; `forbid(unsafe_code)` unaffected; fmt/clippy `-D warnings`/test/deny in quickstart; backoff arithmetic uses `checked_shl`/`saturating_add` (as sync does today); new pub items (`SearchSession::in_flight/generation/retry_scheduled`, `ResultsLayout`, `truncate_query`) get doc comments with runnable examples.
- [x] **VIII. Test What the NFRs Promise** — Pass: test-first per contract (RL1–RL12, F1–F4, C1–C4, S1–S3, N1–N4, R1–R10, V1–V7, E1–E3, D1); proptest for layout arithmetic (P1–P4) and `truncate_query`; regression tests for the two defects fixed (nested group scroll areas hiding groups; status claiming a refresh that was never scheduled). NFR-10.3 suites untouched.
- [x] **IX. One Plugin API Definition** — N/A: plugin API schema untouched.
- [x] **X. Simplicity, Portability, User's Override** — Pass: no new trait/feature flag; one shared `backoff_delay` instead of a copy; the strip reuses 019's severity glyph/colour fns; one clear handler for Escape/×/empty-state; `SectionMemory` reused (no new `ViewKey`); identical on all OSes; every new control has an accessible name and is keyboard-operable; all strings en-US + pt-BR.
- [x] **Governance › Manual Scenario Sign-Off** — Pass: quickstart M1–M11, executed by the implementing agent via the macOS Quartz recipe (`MODPLAYER_CATALOG_FORCE_429=paged-once` / `=1` for M7/M8).
- [x] **Governance › area-maintainer sign-off (GOV-3.2)** — N/A: engine, Capability Gateway and Plugin Runtime crates not touched.
- [x] **Governance › traceability** — Pass: every contract rule cites spec FR/SC IDs; spec cites 004 FR-015/016/018 and NFR-6.1/6.2/7.1.

**Post-design re-check (after Phase 1)**: unchanged — data model adds pure core state (`Retry`, `resume_offset`) and UI-local value types (`ResultsLayout`, `SearchStatus`); contracts are UI + pure-state contracts; the connect-crate seam is debug-only. **PASS.** No violations → Complexity Tracking empty.

## Project Structure

### Documentation (this feature)

```text
specs/026-search-results-structure/
├── plan.md                      # This file
├── research.md                  # Phase 0: R1–R13 decisions
├── data-model.md                # Phase 1: GroupState/SearchSession retry, ResultsLayout, view state/status
├── quickstart.md                # Phase 1: automated checks + manual scenarios M1–M11
├── contracts/
│   ├── results-layout.md        # RL1–RL12, P1–P4: one scroll area, virtualisation, pinned headers, counts
│   ├── search-feedback.md       # F1–F4, C1–C4, S1–S3, N1–N4: label, clear, spinner, count line
│   ├── search-status.md         # R1–R10, V1–V7, E1–E3, D1: retry machine, status strip, empty state, debug seam
│   └── fluent-strings.md        # new/removed keys, en-US + pt-BR
├── checklists/requirements.md   # (from specify)
└── tasks.md                     # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

```text
crates/modplayer-core/src/
├── backoff.rs                   # NEW: backoff_delay + SYNC_BACKOFF_BASE/MAX moved from library/sync.rs
├── lib.rs                       # mod backoff (crate-private)
├── search.rs                    # Retry state, resume_offset, in_flight/generation/retry_scheduled, apply_reply(now)
├── library/sync.rs              # uses crate::backoff (no behaviour change)
└── controller.rs                # route_search_result passes self.now()

crates/modplayer-ui/src/
├── app.rs                       # Section::Search: pass &mut SectionMemory, no outer ScrollArea
├── search_view.rs               # fixed chrome + single results area, headers/pinned header, clear, spinner, count, strip, empty state
├── search_layout.rs             # NEW: pure ResultsLayout (+ proptests), truncate_query
├── lib.rs                       # mod search_layout
└── notifications.rs             # severity_glyph / severity_color → pub(crate)

crates/modplayer-ui/tests/
├── search_view.rs               # extended: RL*, F*, C*, S*, N*, V*, E*
├── accessibility.rs             # search field / status assertions updated to new keys
├── fluent_keys.rs               # new keys; search-placeholder removed
└── section_memory.rs            # ViewKey::Search cases still pass (RL12)

crates/modplayer-audio-source-connect/src/catalog/mod.rs   # debug-only FORCE_429=paged-once decision fn
crates/modplayer-audio-source-connect/src/worker.rs        # consult it with the command's offset

locales/en-US/library.ftl        # 8 new keys, search-placeholder removed
locales/pt-BR/library.ftl        # pt-BR for new keys + existing search-* keys
```

**Structure Decision**: Existing Cargo workspace, one crate per component (constitution VII). The feature lives in the existing `crates/modplayer-ui` (view + new pure `search_layout` module) and `crates/modplayer-core` (search state + new crate-private `backoff` module shared with `library/sync.rs`), with a debug-only seam in the existing `crates/modplayer-audio-source-connect/src/catalog/` and strings in the existing `locales/en-US` and `locales/pt-BR`. No new crate, no new top-level directory.

## Complexity Tracking

No Constitution Check violations — nothing to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| None | — | — |

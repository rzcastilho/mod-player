# Implementation Plan: Library Browsing and Detail View

**Branch**: `feature/025-library-browsing-and-detail` | **Date**: 2026-09-30 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/025-library-browsing-and-detail/spec.md`

## Summary

Polish the Library → Detail → Back browse loop in `crates/modplayer-ui` (UI/UX Review § 3.3 UX-14/UX-17, § 4.5, § 5.4). Four slices, all UI-only:

1. **Collection header (US1, FR-001–FR-004, FR-012)** — replace `detail_view.rs`'s three `draw_*_header` fns (stacked equal-weight `ui.label`s) with one `collection_header` widget: fixed height, 128 px artwork (`DETAIL_ARTWORK_SIZE`, same `ArtworkCache` + initials fallback as rows via `rows::draw_artwork_url`), `theme::text::DISPLAY` single-line truncated title, one `SECONDARY` facts line built by a pure `header_facts(...) -> Vec<String>` joined with " · " (empty fields dropped), a `Variant::Primary` **Play** button dispatching `RowAction::PlayNow` through the existing `library_view::apply_row_action`, and the rows' "…" six-action menu (extracted from `rows::actions_menu` into a shared `pub(crate)` fn). Running time = Σ `duration_ms` of the `TrackListState::Cached` list, formatted by a pure `format_runtime`.
2. **Back + scroll restore (US2, FR-005/FR-006)** — the `detail-back` button moves into the header's top-left as a `Variant::Quiet` text button, first in tab order; Backspace / Alt+Left stay. Restore reuses `SectionMemory` unchanged: the tab's `ViewKey::Library(LibraryViewKey::Tab(tab))` offset is already recorded every frame and re-applied once on first re-show; research R3 confirms the path and adds the missing regression tests (round trip, clamp after content shrink, width-only resize).
3. **Row actions (US3, FR-007–FR-009)** — "…" opener switches from `ui.button` to `widgets::controls::button(ui, Variant::Quiet, "…")`; `ACTIONS_RESERVED_WIDTH` becomes a measured value (button width + `item_spacing.x`, research R4) so the trailing region can never push the opener past the row rect; containment test extended to the narrowest list width. Menu gains the keyboard path already proven by `settings/category_row.rs` (↑/↓ wrap, Home/End, Enter/Space, Escape, focus return to opener row) plus the Menu/ContextMenu key alongside Shift+F10.
4. **Skeletons (US4, FR-010/FR-011)** — `skeleton_row` gains a shaped variant (artwork square + one bar per text line) and callers pass the height of the kind they replace: the library-wide loading state uses `WIDE_ROW_HEIGHT` for Saved Albums / Followed Artists / Playlists (today 56 for all tabs). Detail header gets a same-height skeleton with a live Back button. Empty states untouched.

No core/engine/audio/plugin changes; no new crate or dependency; no persistence. New strings in `locales/en-US/library.ftl` with pt-BR parity in a new `locales/pt-BR/library.ftl` (pattern set by 024's `pt-BR/effects.ftl`).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned in `rust-toolchain.toml`; run with `RUSTUP_TOOLCHAIN=1.95.0` per constitution Governance recipe)

**Primary Dependencies**: `egui`/`eframe` 0.36.2 (accesskit feature) — `Popup`/`PopupKind::Menu`, `ScrollArea`, accesskit node builder; workspace crates `modplayer-core` (`PlaybackController`, `TrackListState`, `tr`/`tr_args` Fluent), `modplayer-audio-source` (`AlbumRef`, `PlaylistRef`, `ArtistRef`, `TrackRef`). No new dependencies.

**Storage**: N/A — scroll offsets stay in the in-memory `SectionMemory` (never persisted, 020 M6).

**Testing**: `cargo test -p modplayer-ui` — headless `egui::Context::run_ui` + accesskit tree assertions (existing pattern in `tests/library_view.rs`, `tests/rows.rs`, `tests/section_memory.rs`, `tests/accessibility.rs`, `tests/high_contrast.rs`, `tests/fluent_keys.rs`); unit tests for pure fns (`header_facts`, `format_runtime`, menu index wrap); proptest for `format_runtime` / facts joining (no stray separators); manual scenarios M1–M8 in `quickstart.md` executed by the implementing agent.

**Target Platform**: Desktop — macOS, Windows, Linux (eframe/winit); minimum window 960 × 640 (`crates/modplayer/src/main.rs` `with_min_inner_size`).

**Project Type**: Desktop application (Cargo workspace, one crate per component); this feature touches only the `modplayer-ui` crate plus `locales/`.

**Performance Goals**: No regression of 60 fps list scrolling; header work is O(n) running-time sum over an already-loaded track list (≤ a few thousand `u32` adds per frame; memoise not required — research R6). Virtualised lists unchanged.

**Constraints**: Header height constant across skeleton / loaded / long-title states; skeleton row height == loaded row height (0 px diff); "…" rect ⊂ row rect at every width down to the narrowest list area; all colours from `theme::roles` (017 high-contrast needs no special case); every control keyboard-operable with accessible name/role/state (NFR-6.1/6.2); all strings Fluent en-US + pt-BR (NFR-7.1); no `unwrap`/`expect` outside tests.

**Scale/Scope**: 3 detail kinds, 5 library tabs, 6 row actions; ~5 source files changed (`detail_view.rs`, `rows.rs`, `library_view.rs`, `widgets/skeleton.rs`, `actions.rs`), 2 locale files, ~6 test files extended. 8 new Fluent keys, 1 changed value, 3 existing keys gaining pt-BR values ([contracts/fluent-strings.md](./contracts/fluent-strings.md)).

All unknowns resolved — every open point was settled in spec Clarifications or resolved in [research.md](./research.md).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: **VII, VIII, X** (and Governance › Manual Scenario Sign-Off). Others are N/A because the feature is confined to `modplayer-ui` presentation code.

- [x] **I. Real-Time Path Is Sacred** — N/A: no engine/audio-thread code touched; header Play uses the existing `PlaybackController::queue_replace_at` + `play()` command path (same as row Play now), which already crosses into the engine via the sanctioned queues.
- [x] **II. Plugins Are Guests** — N/A: no plugin runtime, gateway or plugin UI contribution is touched.
- [x] **III. Host Primitives, Plugin Behaviors** — N/A: no markers/loops/effects/transport primitives change; header actions reuse existing `RowAction`s.
- [x] **IV. Audio Source Replaceable and Isolated** — N/A for new deps: only existing `modplayer-audio-source` catalog types (`AlbumRef`, `PlaylistRef`, `ArtistRef`, `TrackRef`) are read; no dependency on `audio-source-connect`; all tests use the synthetic/fake host already used by `tests/library_view.rs`.
- [x] **V. No Audio Ever Leaves the Engine** — N/A: no sample buffers, cache decryption or file output; purely UI metadata.
- [x] **VI. Security and Privacy by Default** — N/A: no credential, network, telemetry or persistence change; scroll memory stays in-memory only (020 M6).
- [x] **VII. Rust Quality Gates** — Pass: no new crate/dependency; `#![forbid(unsafe_code)]` unaffected; `fmt`/`clippy -D warnings`/`test`/`deny` required per task list; no `unwrap`/`expect` outside tests (menu index math uses `checked`/`rem_euclid`); new `pub` items (`DETAIL_ARTWORK_SIZE`, `format_runtime`, `header_facts`) carry doc comments with runnable examples.
- [x] **VIII. Test What the NFRs Promise** — Pass: test-first for each FR (contracts H1–H12, RM1–RM9, K1–K6, S1–S6); proptest added for `format_runtime`/facts joining (state-shaped formatting, mirrors 023's pattern); regression tests for the two defects fixed (row "…" containment at narrow width; library skeleton height). NFR-10.3 real-time suites untouched and still run.
- [x] **IX. One Plugin API Definition** — N/A: plugin API schema not touched.
- [x] **X. Simplicity, Portability, User's Override** — Pass: no new trait/feature flag; header reuses `apply_row_action`, `actions_menu`, `SectionMemory`, `widgets::controls::button`, `ArtworkCache` rather than adding parallel paths; keyboard path identical on all OSes (Menu/ContextMenu key is additive to Shift+F10, not platform-specific); every new control has accessible name/role/state (NFR-6.1/6.2); all strings externalised en-US + pt-BR (NFR-7.1).
- [x] **Governance › Manual Scenario Sign-Off** — Pass: `quickstart.md` ends with M1–M8, to be executed by the implementing agent via the macOS Quartz recipe (`MODPLAYER_LIBRARY_FIXTURE=large` for scroll restore; `MODPLAYER_ARTWORK_FORCE_FAIL` for placeholder).
- [x] **Governance › area-maintainer sign-off (GOV-3.2)** — N/A: engine, Capability Gateway and Plugin Runtime crates are not touched.

**Post-design re-check (after Phase 1)**: unchanged — data-model adds only UI-local value types (`HeaderFacts`, `MenuNav`) and one constant; contracts are UI contracts; no principle newly touched. **PASS.**

## Project Structure

### Documentation (this feature)

```text
specs/025-library-browsing-and-detail/
├── plan.md              # This file
├── research.md          # Phase 0: R1–R9 decisions
├── data-model.md        # Phase 1: header facts, runtime format, menu nav, skeleton shapes
├── quickstart.md        # Phase 1: automated checks + manual scenarios M1–M8
├── contracts/
│   ├── collection-header.md   # H1–H12: header layout, facts, Play, "…", Back, a11y
│   ├── row-actions-menu.md    # RM1–RM9: quiet opener, containment, keyboard path
│   ├── skeletons-and-restore.md # K1–K6 skeleton shape/height, S1–S6 scroll restore
│   └── fluent-strings.md      # new/changed keys, en-US + pt-BR
├── checklists/requirements.md # (from specify)
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

```text
crates/modplayer-ui/
├── src/
│   ├── detail_view.rs         # CHANGED: collection_header, header_facts, format_runtime,
│   │                          #   DETAIL_ARTWORK_SIZE, header skeleton; Back moved into header
│   ├── rows.rs                # CHANGED: actions_menu → pub(crate) shared, Quiet opener,
│   │                          #   keyboard nav, measured reserved width; draw_artwork_url sized
│   ├── library_view.rs        # CHANGED: per-tab skeleton height/shape (WIDE_ROW_HEIGHT)
│   ├── widgets/skeleton.rs    # CHANGED: shaped skeleton (artwork + text bars), header skeleton
│   ├── actions.rs             # CHANGED: row_claims += Menu key; row_menu_item_claims
│   ├── section_memory.rs      # UNCHANGED (reused; regression tests only)
│   ├── app.rs                 # UNCHANGED expected (Back path already returns to tab)
│   └── theme/                 # READ-ONLY (text::DISPLAY/SECONDARY, roles, Variant)
└── tests/
    ├── library_view.rs        # EXTENDED: header, facts, Play, skeleton heights, Back order
    ├── rows.rs                # EXTENDED: containment at narrow widths, Quiet paint, keyboard menu
    ├── section_memory.rs      # EXTENDED: library tab round-trip + clamp + resize
    ├── accessibility.rs       # EXTENDED: header group/names, Play disabled description
    ├── high_contrast.rs       # EXTENDED: header legibility in HC appearances
    └── fluent_keys.rs         # EXTENDED: new keys resolve; en-US/pt-BR library parity
locales/
├── en-US/library.ftl          # CHANGED: new detail-* keys; detail-back value
└── pt-BR/library.ftl          # NEW: pt-BR values for this feature's new/changed keys
```

**Structure Decision**: Single existing Cargo workspace; all code lives in the existing `crates/modplayer-ui` crate (`src/` + `tests/`) and the existing `locales/` tree. No new crate, module directory or dependency — every surface extends a file that already owns that concern (detail view, rows, skeleton widget, key claims). `locales/pt-BR/library.ftl` is the only new file, following 024's precedent.

## Complexity Tracking

No constitution violations — table intentionally empty.

Recorded assumptions / rejected alternatives (headless run; spec Clarifications already settled most):

| Decision | Rejected alternative | Why |
|----------|---------------------|-----|
| Reuse `SectionMemory` for back-restore; add tests, no new state (R3) | New `LibraryViewPosition` struct storing first-visible index | Spec + 020 already key offsets per tab; rows are fixed height so pixel offset ≡ index; a second store would duplicate truth |
| Running time summed per frame from `Cached` list (R6) | Cache runtime in `DetailViewState` keyed by target | Sum over ≤ ~10k `u32` is sub-µs; cache adds invalidation surface for no measurable gain |
| Share `rows::actions_menu` for header "…" (R5) | Separate header menu impl | FR-004 demands identical items/order/behaviour; one fn guarantees it |
| Measured `ACTIONS_RESERVED_WIDTH` (button width + spacing) (R4) | Bump constant 40 → 48 | Constant drifts with font/theme scale (014/017); measurement is exact at every scale |
| pt-BR in new `locales/pt-BR/library.ftl` holding only this feature's keys (R8) | Translate the whole library.ftl now | Out of scope; 024 established the per-feature parity-file pattern; pt-BR is not yet a shipped runtime locale |
| Facts separator " · " as a code constant (R2) | Fluent key for separator | Punctuation, identical in en/pt-BR; Fluent keys hold words |
| Search-opened detail out of scope (spec) | Handle Back-to-Search | Search never emits `Open` today (`search_view.rs`) |

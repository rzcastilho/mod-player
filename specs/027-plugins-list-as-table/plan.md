# Implementation Plan: Plugins List as a Real Table

**Branch**: `027-plugins-list-as-table` (git: `feature/027-plugins-list-as-table`) | **Date**: 2026-09-30 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/027-plugins-list-as-table/spec.md`

## Summary

Rebuild the Plugins section (`crates/modplayer-ui/src/plugins_view.rs`) from a stack of unbounded `ui.horizontal` rows into a real seven-column table — Name (+version), Source, Enabled, Health, Permissions, Resource use, Actions — whose header and cells share x-extents computed by one pure layout function (`PluginsColumns::layout`, R1/R2). Column minimums sum (with gaps) to exactly `HOST_CONTENT_FLOOR` (560 pt), so 17 plugins fit at the 960 pt minimum window and at any section width ≥ 560 pt with no overlap and no horizontal scroll; every text cell truncates with a tooltip only when truncated. The header stays fixed above a vertical `ScrollArea` the view now owns (026 Search pattern, R3). Permissions become a count + disclosure expanding one explanation per line inside the row; health words become healthy/degraded/suspended (theme role colours unchanged, word is the carrier); suspended rows show their `SuspendCause` reason and a Restart button calling the existing `plugin_restart`; resource use shows `CPU used % / budget %` and `Mem used MB / budget MB` from the record's `Budgets` (no "64 MB" literal) with "over budget" text; panel controls move into the Actions cell (1 panel inline, ≥ 2 via "Panels (N)" disclosure). Core change is limited to three new `PluginRow` fields (`suspend_cause`, `cpu_budget_pct`, `memory_budget_bytes`). A file-scoped colour-literal test guards FR-009.

## Technical Context

**Language/Version**: Rust 1.95.0 (`rust-toolchain.toml`, workspace `rust-version = "1.95"`)

**Primary Dependencies**: egui/eframe 0.36 (existing), fluent-templates (existing `tr`/`tr_args`), `modplayer-core` (`PluginsView`, `PlaybackController`), `modplayer-plugin-runtime` (`SuspendCause`), `modplayer-capability-gateway` (`Budgets`, `Permission`). No new dependency (R1 rejects `egui_extras`).

**Storage**: N/A — expansion state is session-only UI memory (`PluginsViewState`, R8); panel enable/disable keeps its existing `[plugin_panels]` persistence untouched.

**Testing**: `cargo test` — egui headless `Context::run_ui` + AccessKit node `bounds` harness in `crates/modplayer-ui/tests/plugins_view.rs`; proptest for `PluginsColumns::layout`; `fluent_keys.rs` key table + pt-BR parity; `accessibility.rs` for names/tab order; existing `design_token_literals.rs` scan; core unit tests in `crates/modplayer-core/src/plugins/view.rs`.

**Target Platform**: macOS, Windows, Linux desktop (egui); manual scenarios on macOS per constitution recipe.

**Project Type**: Desktop application (Cargo workspace, crate per component).

**Performance Goals**: Plugins section repaints every 500 ms (existing) and at ≥ 30 Hz UI loop; layout is O(columns) per frame + O(rows) cells — negligible for tens of rows. No per-frame allocation beyond existing string formatting.

**Constraints**: No horizontal scroll; no overlap at section widths ≥ 560 pt; header aligned ±0.5 pt; colour only from theme roles; every control keyboard-operable with accessible name including plugin (and panel) name; all strings externalized (en-US + staged pt-BR); zero runtime/lifecycle/real-time change.

**Scale/Scope**: 17 fixture plugins (the acceptance set, `plugins/fixtures/`), realistically tens of rows. Files: 1 view rewritten, 1 core model file extended, `app.rs` call site, 2 locale files (1 new), 4 test files.

All Technical Context items resolved; no open unknowns remain (see [research.md](./research.md) R1–R13).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: **VII, VIII, X** and Governance › Manual Scenario Sign-Off / traceability. Others N/A with reason.

- [x] **I. Real-Time Path Is Sacred** — N/A: no engine or audio-thread code; the view reads `plugins_view()` on the UI thread; gauges are existing atomics read off-thread; Restart reuses `plugin_restart` (UI thread spawn).
- [x] **II. Plugins Are Guests** — N/A (no behaviour change): budgets, gateway and runtime are only *read* (`record.budgets`, `Lifecycle::Suspended { cause }`); suspension/restart semantics unchanged; budgets become more visible to the user, not relaxed.
- [x] **III. Host Primitives, Plugin Behaviors** — N/A: no markers/loops/effects/transport primitives; panel controls call existing 011 host APIs unchanged.
- [x] **IV. Audio Source Replaceable and Isolated** — N/A: no audio-source crate touched; tests use `ScriptedHost` + `FakeBackend`.
- [x] **V. No Audio Ever Leaves the Engine** — N/A: presentation of plugin metadata only; no sample buffers, cache or file output.
- [x] **VI. Security and Privacy by Default** — N/A: no credentials, network, telemetry or persistence change; permissions are displayed read-only (grant/revoke untouched, FR-011).
- [x] **VII. Rust Quality Gates** — Pass: no new crate or dependency; `forbid(unsafe_code)` unaffected; no `unwrap`/`expect` in library code (layout uses plain `f32` math, zero-window guarded); new pub items (`PluginsColumns`, `PluginsViewState`, `suspension_reason`, new `PluginRow` fields) get doc comments with runnable examples; fmt/clippy `-D warnings`/test/deny listed in quickstart.
- [x] **VIII. Test What the NFRs Promise** — Pass: test-first per contract T1–T20; proptest for layout arithmetic (constitution requires it for layout-style arithmetic, as 026 did); regression test for the overflow defect (17-fixture fit at 960 and 560 pt) and for FR-009 (colour-literal guard). NFR-10.3 suites (isolation, permissions) untouched and stay green.
- [x] **IX. One Plugin API Definition** — N/A: plugin API schema and manifest untouched.
- [x] **X. Simplicity, Portability, User's Override** — Pass: no new trait/feature flag; one pure layout fn instead of a table dependency; reuses `row_frame`, `switch`, `button`, `health_color`, `SectionMemory`, existing `plugin-suspended-cause-*` strings; identical on all OSes; every control keyboard-operable with plugin-named accessible names (R10); strings externalized en-US + staged pt-BR (R5); one-action disable (Enabled switch) and Restart preserved in the row.
- [x] **Governance › Manual Scenario Sign-Off** — Pass: quickstart M1–M10, executed by the implementing agent via the macOS Quartz recipe with `MODPLAYER_PLUGIN_FIXTURES=1`.
- [x] **Governance › area-maintainer sign-off (GOV-3.2)** — N/A: engine, Capability Gateway and Plugin Runtime crates are not modified (only read; R7 rejects adding a gateway helper precisely to avoid this).
- [x] **Governance › traceability** — Pass: contract rules cite FR-001–FR-011 / SC-001–SC-005; source UX-27/28/29 (UI/UX review §3.6) via spec; constitution X ↔ NFR-6.1/6.2/7.1.

**Post-design re-check (after Phase 1)**: unchanged — data model adds three derived core fields and UI-local pure types; contracts are UI + string contracts; no crate boundary, runtime or persistence change. **PASS.** No violations → Complexity Tracking lists none (assumptions recorded below).

## Project Structure

### Documentation (this feature)

```text
specs/027-plugins-list-as-table/
├── plan.md                      # This file
├── research.md                  # Phase 0: R1–R13 decisions
├── data-model.md                # Phase 1: PluginRow fields, PluginsColumns, PluginsViewState
├── quickstart.md                # Phase 1: automated gates + manual M1–M10
├── contracts/
│   ├── plugins-table.md         # Phase 1: T1–T20 UI rules, pure fns, test map
│   └── fluent-strings.md        # Phase 1: changed/new/removed keys, en-US + pt-BR
├── checklists/                  # (from specify/clarify, if present)
└── tasks.md                     # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
crates/modplayer-core/src/plugins/
└── view.rs                      # PluginRow + suspend_cause, cpu_budget_pct, memory_budget_bytes; row() derivation + unit tests

crates/modplayer-ui/src/
├── plugins_view.rs              # rewritten: PluginsColumns, PluginsViewState, header, rows, cells, expansion area, pure fns
├── app.rs                       # Section::Plugins: pass &mut PluginsViewState + &mut SectionMemory; drop outer ScrollArea
└── plugin_panels.rs             # (only if needed) expose SuspendCause → key mapping as pub(crate) for reuse

crates/modplayer-ui/tests/
├── plugins_view.rs              # new table tests (17-fixture fit, alignment, truncation, disclosure, restart, colour-literal guard)
├── accessibility.rs             # update health word assertion; tab order / names
└── fluent_keys.rs               # key table update + pt-BR plugins parity test

locales/
├── en-US/plugins.ftl            # health words, new keys, removed keys
└── pt-BR/plugins.ftl            # NEW: pt-BR values for this feature's keys only
```

**Structure Decision**: Existing Cargo workspace; no new crate or directory beyond `locales/pt-BR/plugins.ftl`. The model change stays in `crates/modplayer-core/src/plugins/view.rs` (the sole owner of `PluginRow`); all layout and interaction live in `crates/modplayer-ui/src/plugins_view.rs`, wired from `crates/modplayer-ui/src/app.rs`; tests in `crates/modplayer-ui/tests/` and core unit tests in `view.rs`. All listed directories exist today.

## Implementation Outline (for /speckit-tasks)

1. **Core** (`view.rs`): add three fields + derivation; unit tests (suspended → `Some(cause)`, active → `None`; DEFAULT budgets → 10.0 / 64 MiB; zero window → 0.0).
2. **Strings**: en-US edits, new pt-BR file, `fluent_keys` table + parity test (tests first).
3. **Pure UI fns**: `PluginsColumns::layout` (+ proptest), `health_word_key`, `suspension_reason`, `cpu_line`/`memory_line` with over-budget flag.
4. **Rendering**: header outside, owned `ScrollArea`, cell placement into column rects, truncation helper, invalid span, expansion area, Actions cell (panels inline/disclosure, Restart), accessible names; `PluginsViewState` in `App`.
5. **Tests**: contract test map (plugins_view.rs, accessibility.rs), colour-literal guard, update existing assertions (`plugins-health-ok` value, removed CPU/Memory columns, `panel_controls_listed`).
6. **Gates + manual** M1–M10.

## Complexity Tracking

No Constitution Check violations — nothing to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| None | — | — |

**Assumptions recorded (headless run — no clarification possible)**:

- **pt-BR**: `locales/pt-BR/plugins.ftl` does not exist and pt-BR is not a runtime locale (`i18n.rs` resolves en-US only). Chosen: create it with this feature's changed/new keys only, plus a parity test (025 precedent). Rejected: skipping pt-BR (contradicts FR-006 "all shipped locales" and Constitution X); translating the whole existing file (out of scope).
- **Budget source**: read from `PluginRecord::budgets` into `PluginRow` (R7). Rejected: UI reading `Budgets::DEFAULT` directly (bypasses the per-plugin seam) and a new gateway helper (would trigger GOV-3.2 sign-off for a one-liner).
- **Table mechanism**: hand-rolled pure column layout (R1). Rejected: `egui_extras::TableBuilder` (new dependency, fixed row heights conflict with in-row expansion) and `egui::Grid` (content-sized columns overflow).
- **Scroll ownership**: the view owns its `ScrollArea` so the header stays fixed (R3). Rejected: keeping `app.rs`'s outer scroll (header scrolls away).
- **Column widths/weights**: MIN `[104, 56, 44, 80, 48, 128, 76]`, gap 4 pt, weights Name 3 / Health 2 / Source 1 / Resource 1 / Actions 1 (R2) — spec left distribution to the plan; Health gets weight for the suspension reason. Values are one-place constants verified by tests.
- **Actions cell narrow case**: buttons wrap within the cell (`horizontal_wrapped`), growing the row height, rather than overflowing or truncating buttons.
- **Removed keys**: `plugins-col-version/-cpu/-memory`, `plugins-cpu`, `plugins-memory`, `plugins-list-separator` are removed only after a workspace grep confirms no other caller.

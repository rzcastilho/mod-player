# Implementation Plan: Effect Chain Rows and Meters

**Branch**: `024-effect-chain-rows-and-meters` | **Date**: 2026-09-29 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/024-effect-chain-rows-and-meters/spec.md`

## Summary

Rework the *presentation* of the Now Playing Effect chain panel (review
finding `UX-25`) without changing any node behavior (FR-014):

1. **Rows** — each node row becomes four zones, left to right: identity
   (grip handle "⠿", position, kind, owner), state (Bypass toggle, "{pct} %
   of budget", auto-bypassed / mode notes moved here), parameters
   (existing controls, units already inside the value box via `.suffix()`),
   actions (`destructive_gap` + Remove). Zones are atomic units separated
   by a vertical separator + spacing; line breaks between zones come from a
   pure `zone_breaks` rule fed by last frame's measured zone widths
   (research R1). The handle gains Grab/Grabbing cursors and a
   node-specific accessible name "Reorder {kind}, position {n}".
2. **Meters** — header figures relabeled as budget figures (changed Fluent
   values, mono, hover explanation); level-pair readouts padded to a fixed
   8-char width; the spectrum gains a left dB gutter (0 / −30 / −60
   reference lines), a bottom tick strip (100 / 1k / 10k labeled, 50 / 200
   / 500 / 2k / 5k minor), and positive/warning/danger bar segmentation
   with a danger cap at 0 dBFS.
3. **Empty state** — zero nodes shows an explanation and one `Primary`
   "Add effect node" button that reveals the existing kind combo + primary
   "Add" and focuses the combo (egui temp memory, resets when the chain
   empties again).

Two files of production code change (`effects_view.rs`,
`widgets/chain_meters.rs`) plus `locales/en-US/effects.ftl` and a new
`locales/pt-BR/effects.ftl` (FR-015, NFR-7.1); tests extend
the existing `modplayer-ui` suites.

## Technical Context

**Language/Version**: Rust, stable toolchain pinned by `rust-toolchain.toml` (run with `RUSTUP_TOOLCHAIN=1.95.0` per constitution recipe), edition as in workspace `Cargo.toml`

**Primary Dependencies**: `egui` / `eframe` 0.36 (feature `accesskit`), `fluent-templates` (via `modplayer-core::tr`/`tr_args`), existing `modplayer-core` (`ChainView`, `NodeRow`, `MeterSnapshot`, `PlaybackController`), `modplayer-effects::catalog`. No new crates.

**Storage**: N/A — no persisted state; per-viewer UI state in egui temp memory only (data-model §3).

**Testing**: `cargo test` — `modplayer-ui` integration tests driving a headless `egui::Context` and asserting on AccessKit output (`tests/effects_view.rs`, `tests/accessibility.rs`, `tests/meter_bands.rs`, `tests/fluent_keys.rs`), plus unit tests for pure helpers in `effects_view.rs` / `widgets/chain_meters.rs`; manual scenarios M1–M10 in [quickstart.md](./quickstart.md).

**Target Platform**: Desktop — macOS, Windows, Linux (egui/eframe); manual sign-off on macOS.

**Project Type**: desktop-app (Cargo workspace, one crate per component; this feature is confined to the `modplayer-ui` crate).

**Performance Goals**: No regression of the UI frame (60 fps); added painting ≤ 192 bar-segment rects + 8 ticks + 3 lines + 6 cached galleys per frame (research R13). Audio thread untouched.

**Constraints**: Presentation-only (FR-014); must fit the 021 single-scroll-region column at the 960 × 640 minimum window (FR-016); spectrum widget minimum outer width 160 px (FR-008); keyboard paths and accessible names must not regress (FR-013); no user-visible literals outside Fluent (FR-015).

**Scale/Scope**: ≤ 16 rows (chain capacity), 6 node kinds, 64 spectrum bands, 2 level pairs; ~11 new Fluent keys + 3 changed values.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: **VII, VIII, X** (and the
Manual Scenario Sign-Off governance rule). Others are explicitly N/A.

- [x] **I. Real-Time Path Is Sacred** — N/A: UI-only; reads the existing `ChainView`/`MeterSnapshot` snapshots already produced off the audio thread; no engine crate change, no new `RtShared` field, no parameter/reorder semantics change (FR-014).
- [x] **II. Plugins Are Guests** — N/A: no plugin runtime, gateway or budget code touched; the "plugin" owner label is only displayed.
- [x] **III. Host Primitives, Plugin Behaviors** — N/A: effect nodes stay host-owned; no DSP or node behavior changes, only their row presentation.
- [x] **IV. Audio Source Replaceable and Isolated** — N/A: no dependency on any `audio-source-*` crate added; tests keep using `ScriptedHost`/synthetic source.
- [x] **V. No Audio Ever Leaves the Engine** — N/A: spectrum/levels display the same aggregated magnitudes already exposed; no sample buffers, files or new export path.
- [x] **VI. Security and Privacy by Default** — N/A: no credential, network, telemetry or signing surface touched.
- [x] **VII. Rust Quality Gates** — Pass: changes stay in `modplayer-ui` (no new crate/dependency), no `unwrap`/`expect` outside tests, `cargo fmt --check` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` / `cargo test --workspace` / `cargo deny check` gates all run in quickstart §1 and tasks T041/T042; new pure helpers carry doc comments.
- [x] **VIII. Test What the NFRs Promise** — Pass: test-first for public behavior — AccessKit-level tests for zones, handle names, focus order, empty state, header labels; unit tests for `zone_breaks`, `format_db`, spectrum axis/segment helpers (quickstart §1 table). No real-time crate touched, so no benchmark/soak requirement.
- [x] **IX. One Plugin API Definition** — N/A: plugin API schema and versioning untouched.
- [x] **X. Simplicity, Portability, User's Override** — Pass: no new trait/abstraction/feature flag/crate; reuses `button`/`switch`/`destructive_gap`/`band_color`/`mono_text`; identical behaviour on all platforms (egui only); every control keeps keyboard operation and an accessible name, the handle's name becomes more specific (NFR-6.1/6.2); all strings externalized. Localization (NFR-7.1, FR-015): every new/changed key ships in both `locales/en-US/effects.ftl` and a new `locales/pt-BR/effects.ftl`; `static_loader!` in `modplayer-core::i18n` embeds every `locales/*` directory with `fallback_language: "en-US"`, so the pt-BR file is compiled in, and `fluent_keys.rs` checks en-US ↔ pt-BR key parity for this feature's keys. Runtime locale selection stays with the app-wide i18n work (unchanged here).
- [x] **Governance › Manual Scenario Sign-Off** — Pass: quickstart.md ends with M1–M10, to be executed by the implementing agent on macOS with evidence recorded in tasks.md.

**Post-design re-check (after Phase 1)**: unchanged — data-model confirms no engine/core data changes; contracts confine the surface to `effects_view.rs`, `widgets/chain_meters.rs` and the en-US/pt-BR `effects.ftl` files. All gates pass.

## Project Structure

### Documentation (this feature)

```text
specs/024-effect-chain-rows-and-meters/
├── plan.md              # This file
├── research.md          # Phase 0 output (R1–R13)
├── data-model.md        # Phase 1 output (inputs, zones, temp memory, pure helpers)
├── quickstart.md        # Phase 1 output (gates, test map, manual scenarios M1–M10)
├── contracts/
│   ├── ui-effect-chain-rows.md   # rows, handle, focus order, empty state
│   ├── ui-chain-meters.md        # header figures, level pairs, spectrum axes/bands
│   └── fluent-strings.md         # changed + new en-US keys
├── checklists/          # from specify/clarify
└── tasks.md             # Phase 2 output (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
crates/modplayer-ui/
├── src/
│   ├── effects_view.rs            # rows → four zones, zone_breaks, handle affordance/name,
│   │                              # header labels + hint, empty state (show, show_row,
│   │                              # show_handle, show_header, show_add_row, new show_empty_state)
│   └── widgets/
│       └── chain_meters.rs        # fixed-width format_db, spectrum gutter/ticks/reference
│                                  # lines, spectrum_segments banding, axis helpers
└── tests/
    ├── effects_view.rs            # zones, units, handle names, focus order, empty state,
    │                              # 960 px fit; update "Add"/"Reorder" fixtures
    ├── accessibility.rs           # handle-name prefix lookup, header labels
    ├── meter_bands.rs             # spectrum band convention
    └── fluent_keys.rs             # new/changed keys in the right arg lists

locales/en-US/
└── effects.ftl                    # 3 changed values, 11 new keys
locales/pt-BR/
└── effects.ftl                    # NEW: pt-BR values for the same 14 keys (FR-015, NFR-7.1)
```

Read-only references (not modified): `crates/modplayer-core/src/effects/view.rs`
(`ChainView`, `NodeRow`, `MeterSnapshot`), `crates/modplayer-ui/src/widgets/controls.rs`
(`button`, `switch`, `destructive_gap`), `crates/modplayer-ui/src/theme/controls.rs`
(`Variant`, `Band`, `band_color`, `mark_color`), `crates/modplayer-ui/src/widgets/peak_meter.rs`
(`SCALE_MIN_DB`/`SCALE_MAX_DB`).

**Structure Decision**: Single existing Cargo workspace; the feature is
confined to the `crates/modplayer-ui` crate (`src/effects_view.rs`,
`src/widgets/chain_meters.rs`, and its `tests/` suites) plus
`locales/en-US/effects.ftl` and `locales/pt-BR/effects.ftl` (the only new
directory, required by FR-015/NFR-7.1). No new modules or crates — the
panel's rows, header and meters already live in exactly these two files,
and splitting them would add structure without a second consumer
(Principle X).

## Phase 0 — Research (complete)

See [research.md](./research.md). Resolved: zone layout inside
`horizontal_wrapped` (R1), separation style (R2), grip glyph + cursors
(R3), handle naming vs. existing test lookups (R4), budget labels via
value-only key changes (R5), fixed-width readouts (R6), spectrum geometry
at 160 px (R7), spectrum banding with 0 dBFS danger cap (R8), empty-state
reveal/focus/one-primary (R9), truncation (R10), focus order (R11),
localization scope (R12), performance (R13). No open unknowns
remain.

## Phase 1 — Design & Contracts (complete)

- [data-model.md](./data-model.md) — existing inputs mapped to zones; new
  temp-memory keys; empty-state transitions; pure helpers
  (`zone_breaks`, `format_db`, `freq_to_frac`, `db_to_frac`,
  `tick_label_spans`, `spectrum_segments`).
- [contracts/ui-effect-chain-rows.md](./contracts/ui-effect-chain-rows.md),
  [contracts/ui-chain-meters.md](./contracts/ui-chain-meters.md),
  [contracts/fluent-strings.md](./contracts/fluent-strings.md).
- [quickstart.md](./quickstart.md) — automated gates, requirement→test
  map, manual scenarios M1–M10.

Agent-context update: no `update-agent-context` script is installed in
`.specify/scripts/bash/`; no new technology is introduced, so nothing
needs recording.

## Implementation Notes for /speckit-tasks

- Suggested order: (1) Fluent keys + `fluent_keys.rs`; (2) US1 rows/handle
  (tests first: zones, handle name, focus order; then fix existing
  `Reorder` exact-name lookups); (3) US2 header/readouts/spectrum (pure
  helper unit tests first); (4) US3 empty state (update fixtures that
  click "Add" on an empty chain — research R9); (5) 960 px fit test;
  (6) manual scenarios.
- US1/US2/US3 are independently shippable; US2's `chain_meters.rs` work
  does not touch `effects_view.rs` beyond `show_header`.
- If `"⠿"` does not render in egui's bundled font, use `"⋮⋮"` (research R3).

## Complexity Tracking

No constitution violations. Recorded assumptions/deviations from the spec's literal wording:

| Deviation / assumption | Why | Rejected alternative |
|---|---|---|
| pt-BR ships as `locales/pt-BR/effects.ftl` holding only this feature's 14 new/changed keys (FR-015, NFR-7.1) | Other pt-BR files do not exist yet; Fluent falls back to en-US (`fallback_language`) for keys outside this feature. Parity for this feature's keys is enforced by `fluent_keys.rs` (T004). | Ship en-US only (violates Constitution X / FR-015); translate every existing `.ftl` (out of scope). |
| Spectrum "≥ 0 dBFS danger" is a 3 px cap, not a segment above the 0 dB line | Bar scale tops out at 0 dBFS (`spectrum_bar_height` clamps); FR-014 forbids rescaling. | Extend scale to +6 dBFS (changes displayed semantics). |
| Budget labels change Fluent **values**, not key ids | Keeps ~10 existing `tr_args` test call-sites valid. | New key ids (test churn, no user benefit). |
| Header/row figures are wholly `mono`, not digits-only | Keeps translated sentence in one Fluent message; precedent `plugins_view.rs`. | `LayoutJob` splitting the message around the number. |
| Zone wrapping uses last frame's measured widths | egui cannot pre-measure atomic child layouts; first frame falls back to today's params-on-new-line. | `Grid` (no wrap) / one zone per line (wastes space). |

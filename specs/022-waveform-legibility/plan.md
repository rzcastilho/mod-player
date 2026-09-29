# Implementation Plan: Waveform Legibility and Scrub Feedback

**Branch**: `022-waveform-legibility` | **Date**: 2026-09-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/022-waveform-legibility/spec.md`

## Summary

Make the Now Playing waveform show *where you are*, *where the loop is* and *where you are about to seek* (`UX-23`). Four changes, all inside existing crates:

1. **Two-tone, played/unplayed fill** — a new 8-bit per-bucket RMS value is computed in the same pass as the existing min/max peaks (`DecodedStore::fold_peaks`, analysis thread only), folded up the ×8 ladder, and stored in a new `RMS8` section of the `.mpwf` cache; `ANALYZER_VERSION` 1 → 2 forces recomputation of old entries through 005's existing invalidation path. The UI paints outer peak + inner ±RMS band, with four pairwise-distinct theme tokens (played/unplayed × peak/average) and a positional played/unplayed split at the displayed playhead (research R1–R7).
2. **Playhead ≥ 3:1 everywhere** — a 2 px core (`text_primary`) over a 4 px casing (`surface_base`); because one is near-black and the other near-white, one of them clears ≥ 3.9:1 against *any* backdrop. The contrast suite enumerates every backdrop FR-003 names in all four appearances (R8–R9).
3. **Every loop region shaded, by its own `armed` flag** — `markers::paint_overlay` shades all complete regions: armed-active fill / armed-inactive hatch (006, unchanged) for the armed region, drawn last; a new idle treatment (1 px outline + 10 % fill ≤ ½ of 25 %) for the rest. `current_region()` no longer drives shading (R10).
4. **Hover scrub indicator** — a stateless per-frame 1 px line + `m:ss.mmm` mono label on the hovered view, painted between overlays and playhead, suppressed during seek/marker drags, never emitting a seek or touching accessibility (R11–R13).

Heights, seek behaviour, marker/region semantics and accessibility contracts are untouched (FR-010, FR-015).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable; `rust-toolchain.toml`, workspace `rust-version = "1.95"`)

**Primary Dependencies**: `egui`/`eframe` 0.36 (AccessKit enabled) for painting and input; existing workspace crates `modplayer-audio-source` (`DecodedStore`, `PeakBucket`), `modplayer-core` (Analysis Service, `.mpwf` cache, markers model), `modplayer-ui` (waveform widgets, theme tokens). No new crates or dependencies.

**Storage**: Existing per-track `.mpwf` waveform cache under `<data_local_dir>/ModPlayer/analysis/` — new `RMS8` section, `ANALYZER_VERSION` bumped to 2, `FORMAT_VERSION` unchanged (research R3). No marker/region persistence changes.

**Testing**: `cargo test --workspace` — unit tests + integration tests in `crates/*/tests/` (shape capture via `egui::Context` + `ClippedShape`, as in 005/006/014/017), `proptest` for cache truncation and hover-label clamping, WCAG contrast suite `crates/modplayer-ui/tests/design_token_contrast.rs`, literal scan `design_token_literals.rs`; manual scenarios M1–M9 in [quickstart.md](./quickstart.md).

**Target Platform**: Desktop macOS, Windows, Linux (egui/wgpu); manual runs on macOS.

**Project Type**: Desktop application — single Cargo workspace, crate-per-component.

**Performance Goals**: Hover line/label tracks the pointer within one rendered frame (SC-003; egui repaints on pointer move); paint adds O(width) rects per view (≤ 2 rects per column instead of 1) — no measurable frame-time change at 60 fps. RMS adds one multiply-add per sample to the existing analysis pass on the below-normal-priority thread.

**Constraints**: Zero changes to the real-time audio callback (Constitution I); complete 10-min cache entry ≤ 1 MB (≈ 709 KB @ 44.1 kHz, ≈ 771 KB @ 48 kHz — R4); no colour literal outside `crates/modplayer-ui/src/theme/**`; legible at the minimum heights overview 64 px / detail 120 px (018/021).

**Scale/Scope**: 3 crates touched (`modplayer-audio-source`, `modplayer-core`, `modplayer-ui`); ~8 source files modified, 2 new modules (`theme/waveform.rs`, `waveform/hover.rs`); ~6 test files extended. Tracks up to 10 min typical; any number of loop regions per track (006).

No open unknowns remain — every Technical Context question is resolved in [research.md](./research.md) R1–R15.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: **I, III, IV, VII, VIII, X**. Not touched: II, V, VI, IX (reasons below).

- [x] **I. Real-Time Path Is Sacred** — RMS is computed in `DecodedStore::fold_peaks`, called only by the Analysis Service's below-normal-priority thread (005 FR-003); the engine crate and audio callback are not modified; no new queue/atomic traffic (FR-014). No engine-crate PR note required since `modplayer-engine` is untouched.
- [x] **II. Plugins Are Guests** — N/A: no plugin API, gateway or runtime change; plugin overlay layers (011) keep their existing slot in the `overlays` hook, and the playhead still paints above them.
- [x] **III. Host Primitives, Plugin Behaviors** — loop-region shading reads host-owned `TrackMarkers`/`LoopRegion::armed` and the engine `loop_state`; semantics unchanged (FR-015). RMS analysis is host Rust, not script.
- [x] **IV. Audio Source Replaceable and Isolated** — the change to `modplayer-audio-source` is an additive field on the source-agnostic `PeakBucket`/`DecodedStore` (not the Connect receiver crate); all tests run on the synthetic source / in-memory stores; no new dependency on `audio-source-connect`.
- [x] **V. No Audio Ever Leaves the Engine** — N/A for new risk: the cache stores only 8-bit per-bucket min/max/RMS summaries (≥ 128 frames per bucket), which cannot reconstruct audio — the same class of derived data 005 already persists; no sample buffer is exposed.
- [x] **VI. Security and Privacy by Default** — N/A: no credentials, network, telemetry or signed artefacts touched; cache file permissions/atomic write path unchanged (005 R7).
- [x] **VII. Rust Quality Gates** — no new crate, no `unsafe`, no `unwrap`/`expect` outside tests; cache decode keeps returning `thiserror` `CacheError`; new public items (`WaveformRoles`, `waveform_roles`, `HoverIndicator`, `hover_indicator`, `loop_shade`, `format_mmss_millis`) carry doc comments with doctests; fmt/clippy `-D warnings`/test/deny gates in quickstart §1.
- [x] **VIII. Test What the NFRs Promise** — tests written first per R15: contrast floor (SC-001), token distinctness (SC-005), RMS vectors + refold + cache v2 + size (SC-005/FR-013), loop-shade table (SC-002), hover/suppression/no-seek (SC-003/SC-004), played split (SC-006); proptests for cache truncation (serialization) and label clamping; manual M1–M9 executed by the implementing agent.
- [x] **IX. One Plugin API Definition** — N/A: plugin API schema and versions untouched; `WaveformPaint`/`paint_overlay` are internal host UI types.
- [x] **X. Simplicity, Portability, User's Override** — no trait, no feature flag, no new crate; identical behaviour on all platforms (pure egui paint); hover previews an already keyboard-operable seek (005 FR-013) so no new binding is needed; accessible names/values unchanged (FR-016); no new user-facing strings (hover text is numeric, so no Fluent key needed for FR-017).

**Post-design re-check (after Phase 1)**: All gates still pass. The design added one internal struct (`WaveformRoles`) and one pure module (`waveform/hover.rs`) — no abstraction layers, no violations. Complexity Tracking below records design choices and rejected alternatives only.

## Project Structure

### Documentation (this feature)

```text
specs/022-waveform-legibility/
├── plan.md                              # This file
├── research.md                          # Phase 0: R1–R15
├── data-model.md                        # Phase 1: PeakBucket.rms, .mpwf v2, WaveformRoles, LoopShade, HoverIndicator
├── quickstart.md                        # Phase 1: automated gates + manual M1–M9
├── contracts/
│   ├── ui-waveform-legibility.md        # WL1–WL6: layer order, fill, playhead, loop shading, hover, unchanged surfaces
│   └── analysis-rms.md                  # AR1–AR6: fold_peaks RMS, refold, analyzer version, RMS8 section, size
└── tasks.md                             # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
crates/
├── modplayer-audio-source/
│   ├── src/decoded.rs            # PeakBucket { min, max, rms }; fold_peaks computes rms (AR1)
│   └── tests/decoded.rs          # RMS test vectors + proptest rms ≤ peak
├── modplayer-core/
│   ├── src/analysis/mod.rs       # ANALYZER_VERSION = 2
│   ├── src/analysis/peaks.rs     # weighted quadratic-mean RMS refold (AR2) + unit tests
│   ├── src/analysis/cache.rs     # RMS8 section encode/decode, V2–V4 validation (AR4)
│   └── tests/analysis.rs         # cache v2 round-trip, v1 rejection, size budget (AR3–AR5)
└── modplayer-ui/
    ├── src/theme/mod.rs          # re-export waveform tokens
    ├── src/theme/waveform.rs     # NEW: WaveformRoles × 4 appearances, waveform_roles(), paint constants
    ├── src/theme/markers.rs      # LOOP_* shading constants (moved from markers.rs literals + idle alpha)
    ├── src/waveform/paint.rs     # ColumnPaint.rms, two-tone + played/unplayed columns, cased playhead
    ├── src/waveform/hover.rs     # NEW: HoverIndicator, hover_indicator(), paint_hover()
    ├── src/waveform/mod.rs       # layer order WL1; hover between overlays and playhead; format_mmss_millis
    ├── src/markers.rs            # paint_overlay: every region, loop_shade(armed, loop_state), armed last
    ├── src/now_playing.rs        # set WaveformPaint.hover_suppressed from drag/marker_drag
    └── tests/
        ├── design_token_contrast.rs   # W1, W2
        ├── waveform.rs                # WL2, WL3, WL5
        ├── markers.rs                 # WL4
        └── design_token_literals.rs   # must stay at 0 hits (no edit expected)
```

**Structure Decision**: Existing single Cargo workspace; all work lands in the three existing crates `crates/modplayer-audio-source`, `crates/modplayer-core` and `crates/modplayer-ui`; `PeakBucket { min, max }` struct literals in existing tests (`crates/modplayer-audio-source/tests/decoded.rs`, `crates/modplayer-core/src/analysis/{cache,peaks}.rs`, `crates/modplayer-core/tests/analysis.rs`, `crates/modplayer-ui/src/waveform/paint.rs`) gain `rms` — `crates/modplayer-audio-source-synthetic` uses `PeakBucket::default()` and needs no edit. Analysis data stays with its owners (`DecodedStore` → Analysis Service → cache), tokens stay in `crates/modplayer-ui/src/theme/`, and paint/hover stay in `crates/modplayer-ui/src/waveform/` and `crates/modplayer-ui/src/markers.rs`, matching 005/006/014/017 placement. No crate added (Principle X).

## Phase 0 — Research (complete)

See [research.md](./research.md). Key decisions: RMS in `fold_peaks` single pass (R1); quadratic-mean refold (R2); new `RMS8` section + analyzer bump, no format bump (R3); size ≤ 771 KB worst case (R4); `WaveformRoles` token table (R5); two-tone column paint (R6); positional split at displayed playhead (R7); cased playhead by construction ≥ 3.9:1 (R8); finite backdrop set (R9); shading by own `armed` flag, idle α 0.10 (R10); stateless hover (R11); shared `m:ss.mmm` formatter + mono pill label (R12); hover line = `text_secondary` 1 px (R13); heights untouched (R14); test placement (R15).

## Phase 1 — Design & Contracts (complete)

- [data-model.md](./data-model.md) — `PeakBucket.rms`, ladder fold rule, `.mpwf` v2 layout and validation, `WaveformRoles` values (contrast-checked at plan time: worst case 4.15:1), shading constants, `ColumnPaint`/`WaveformPaint` extensions, playhead backdrop set, `LoopShade` table, `HoverIndicator`.
- [contracts/ui-waveform-legibility.md](./contracts/ui-waveform-legibility.md) — WL1–WL6.
- [contracts/analysis-rms.md](./contracts/analysis-rms.md) — AR1–AR6.
- [quickstart.md](./quickstart.md) — gates and manual scenarios M1–M9.
- Agent context: `.specify/scripts/bash/` has no agent-context update script (only `check-prerequisites.sh`, `common.sh`, `create-new-feature.sh`, `setup-plan.sh`, `setup-tasks.sh`); nothing to run.

## Implementation Order (input for /speckit-tasks)

1. **Foundation (blocks US4, partially US1)**: `PeakBucket.rms` + `fold_peaks` (AR1) → refold (AR2) → `ANALYZER_VERSION = 2` + `RMS8` cache (AR3–AR5); fix `PeakBucket` literals across crates. `theme/waveform.rs` tokens + contrast/distinctness tests (W1/W2) failing first.
2. **US1 (P1)** — cased playhead (WL3); contrast suite green.
3. **US2 (P1)** — `loop_shade`, every-region shading, armed-last, constants moved to theme (WL4).
4. **US3 (P2)** — `hover.rs`, layering (WL1/WL5), `hover_suppressed` wiring, shared formatter.
5. **US4 (P3)** — two-tone + played/unplayed columns (WL2).
6. **Polish** — literal scan, full regression suite (SC-007), manual M1–M9.

US1–US3 are independent of the RMS foundation (only US4 needs it), so US1/US2/US3 can proceed in parallel with step 1's analysis half.

## Complexity Tracking

No constitution violations. Design choices with rejected alternatives, recorded because the spec left them to plan time:

| Decision | Why | Simpler / Other Alternative Rejected Because |
|---|---|---|
| New `RMS8` cache section instead of widening `WAVE` | Uses the cache's designed unknown-section extension point; only `ANALYZER_VERSION` bumps | Widening `WAVE` needs a second (`FORMAT_VERSION`) bump for one change |
| Separate `WaveformRoles` struct instead of new `Roles` fields | Keeps 014's ten-role contract/tests intact while still per-appearance tokens | Adding 9 fields to `Roles` ripples through every `Roles` literal and 014 T9 |
| Playhead core + casing (two strokes) | Only construction that guarantees ≥ 3:1 against both light and mid-luminance backdrops in every appearance | Single-colour stroke: no colour clears 3:1 against both `#FFFFFF` and mid-tone fills (e.g. today's 2.2:1 over dark-theme accent bars) |
| `armed ∧ loop_state == 0` → armed-inactive (not idle) | Avoids a one-frame flicker to idle between UI arm and engine ack | Mapping to idle is "truthful" to the engine but visibly flickers |
| Idle fill α = 0.10 | Meets FR-005 (≤ 0.125) with headroom; visible at 64 px overview | 0.125 exactly sits on the boundary; outline-only (006's disarmed look) rejected by spec |

**Assumptions recorded** (spec left open, no escalation needed): exact token hex values (data-model §3) and idle alpha chosen here per spec Assumption 6; 48 kHz treated as the worst supported analysis rate for the size budget (R4); plugin-drawn overlay colours are outside the enumerated contrast set but covered by the by-construction casing argument (R8/R9).

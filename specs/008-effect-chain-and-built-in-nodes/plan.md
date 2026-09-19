# Implementation Plan: Effect Chain and Built-In Effect Nodes

**Branch**: `feature/008-effect-chain-and-built-in-nodes` | **Date**: 2026-09-18 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/008-effect-chain-and-built-in-nodes/spec.md`

## Summary

Let a musician change how a song sounds while it plays — slow it down
without changing pitch, transpose it without changing speed, shape its
tone — through an **ordered chain of up to 16 host-owned effect nodes**
sitting between the decoder (after loop/seek evaluation) and the output
limiter, processed at the source rate. Six built-in node types (pitch
shift with formant preservation and quality mode, time stretch with
quality mode, gain, 8-band equalizer, high-/low-pass filter, stereo
tools) expose clamped, 20 ms-ramped parameters; every add, remove,
reorder, bypass and discrete switch lands at a buffer boundary under a
fixed 5 ms equal-power crossfade. An **Effect Chain panel** in Now
Playing (`E` / header toggle) lists nodes in processing order with
type, owner, bypass, drag handle (pointer and `↑`/`↓`), per-node and
whole-chain CPU figures, pre-/post-chain peak/RMS meters and a 64-band
spectrum; `+`/`-` step the first time-stretch node by ±10 points. The
engine measures its render time, raises an **overload event** (90 % on
3 callbacks or any underrun), counts it, names the costliest node and —
only for a non-host owner, i.e. never in this slice — auto-bypasses it.

Technical approach (details in [research.md](research.md)): a new
`modplayer-effects` crate (the constitution's "effects" component, R1)
holds the shared parameter catalog (ranges, defaults, clamp, FR-008
rule), the allocation-free DSP kernels and `ChainRt` — a **16-slot
preallocated pool** (R2) that activates node state in place so `Command`
stays `Copy`/≤ 16 bytes and nothing is allocated on the audio thread. A
**two-pass pull plan over two ping-pong buses** (R3) lets rate-changing
nodes sit anywhere in the chain. Pitch shift and time stretch are one
`StretchStage` kernel (WSOLA + fractional resampler, LPC formant
correction, two "voices" over one ring for click-free mode/engage
seams) whose product parameters *are* FR-007's combined stage (R4).
Continuous parameters ramp per sample with 32-frame biquad coefficient
sub-blocks (R5); every discrete transition crossfades two live states,
reorders in two 5 ms phases (R6). The engine subtracts the chain's lead
from the published position and publishes the advance rate for the
playhead (R7); the controller keeps the streaming `Player` within
500 ms and gates end-of-track on the engine position under a non-unity
tempo (R8). Meters and the spectrum (hand-rolled 1 024-point FFT) are
computed on the RT and published as atomics — no samples leave the
engine (R9); cost timing, overload detection and non-host auto-bypass
run on the RT (R10). The controller's `ChainModel` is the shadow state,
replayed on every stream rebuild (R11). The UI reuses the Queue-panel
pattern, egui's built-in drag-and-drop and 007's focus claims (R14); the
action catalog gains `nav.toggle_effect_chain` and enables the two tempo
steps (R15). Criterion benches (effects crate) and the 60 s / 24 h soak
(engine) discharge 001's deferred Constitution VIII obligations (R16).

Assumptions taken headlessly (each with its rejected alternative in
Complexity Tracking): the new crate; the slot pool; the pull plan;
WSOLA/LPC rather than a phase vocoder or a crate; two-phase reorder;
`Q_MAX = 8`; engine-gated end-of-track; release-only SC-003 assertion.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–007

**Primary Dependencies**: existing eframe/egui 0.36 (+accesskit; its built-in `dnd_drag_source`/`dnd_drop_zone` for reorder), cpal 0.18, rtrb 0.4, fluent-templates 0.15, thiserror 2, assert_no_alloc 1.1 (engine dev), proptest 1 (dev). **One new dev-only dependency**: `criterion 0.7` (`default-features = false`, `cargo_bench_support`) in `modplayer-effects` (research R16). **No new runtime dependency**; all DSP (WSOLA, biquads, LPC, FFT) is hand-written in the new crate (research R1, R4, R9)

**Storage**: N/A — the chain is session-scoped and never written to disk (FR-002, FR-016); no settings, secure-store or track-state change. All chain/meter data lives in controller shadow state and `RtShared` atomics

**Testing**: `cargo test --workspace` with `FakeBackend` (001), `SyntheticSource`/`ScriptedHost` (003/006), offscreen `egui::Context` with injected events (004–007); `assert_no_alloc` on every render path; proptest for clamping, ramp/crossfade arithmetic, pull-plan conservation and `rewind_in_loop`; click-free tests on 006's first-difference metric; a 200-trial p95 latency test; criterion benches (`cargo bench -p modplayer-effects`); `soak.rs` 60 s in CI and 24 h / release SC-003 under `#[ignore = "manual"]`; manual scenarios M1–M14 in [quickstart.md](quickstart.md); CI gates unchanged (fmt, clippy `-D warnings` incl. benches, test, deny, licence headers) on ubuntu / macos / windows

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical DSP, bindings and behaviour; no platform-specific code (all `f32` math, `Instant::now` already on the render path)

**Project Type**: Desktop application — Cargo workspace grows from 10 to **11 crates** (`crates/modplayer-effects`)

**Performance Goals**: reference chain (pitch shift + time stretch + 8-band EQ) < 50 % of one core at 128 frames / 44.1 kHz, i.e. < 1.45 ms per render (SC-003, NFR-1.8); ratio change audible ≤ 20 ms p95 at the performance preset (SC-001, NFR-1.1: 2.9 ms buffer + 10 ms synthesis hop); every chain edit click-free (≤ 1.5 × baseline first difference, SC-002); spectrum FFT ≈ 40 µs every ≥ 256 frames; per-node timing overhead ≈ 1 µs/render; panel draws 16 rows × ≤ 34 controls within a 16 ms frame; position published ≥ 60 Hz at any tempo via `PositionClock` extrapolation

**Constraints**: Constitution I — no allocation, lock, I/O, log or `dyn` under `render` (slot pool, preallocated buses/rings/FFT scratch; `assert_no_alloc` proof); parameter and chain changes only at buffer boundaries; Constitution V — meters/spectrum published as numbers only, no sample sink anywhere (SC-009 API check); `#![forbid(unsafe_code)]` in the new crate; no `unwrap`/`expect` outside tests; every new control keyboard-operable with an accessible name/role/state; strings externalised (en-US); `Command` stays `Copy` ≤ 16 bytes; chain capacity 16; ≈ 2 MiB preallocated per `Processor`; no plugin nodes, presets, persistence or key display (FR-016)

**Scale/Scope**: new crate ≈ 3 200 LOC (catalog 350, smoothing/crossfade/biquad 300, gain/eq/filter/stereo 500, stretch + LPC 900, spectrum 150, ChainRt/slot/cost 700, benches 200 + ≈ 60 unit/prop tests); engine delta ≈ 500 LOC (commands/events/shared, render refactor, position, overload) + ≈ 900 LOC of tests; core ≈ 700 LOC (`effects` model/view 450, controller façade/tick rules 250) + ≈ 500 test LOC; UI ≈ 900 LOC (`effects_view` 650, `chain_meters` 150, action/claims wiring 100) + ≈ 700 test LOC; 1 new `.ftl` (≈ 60 keys) + 1 key in `controls.ftl`; 16 nodes × 6 kinds × ≤ 32 params

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | **Yes** | ✅ PASS | All DSP runs inside `Processor::render` on the callback thread; the only new state on that path is the preallocated `ChainRt` (16-slot pool, two buses, spectrum scratch — research R2, R3, R9), activated/mutated only through the existing `Command` SPSC queue drained at the buffer boundary (contracts/engine-effect-chain.md §2), publishing through `RtShared` atomics and the existing `Event` queue. No allocation, lock, I/O, log, `dyn` call or plugin call anywhere under `render`; `tests/realtime.rs::render_with_full_chain_never_allocates` proves it with 16 nodes under `assert_no_alloc`. Parameter ramps, chain edits and crossfades start only at boundaries (FR-005/FR-010). The latency (SC-001) and loop-seam (006, unchanged `fill_from_source`) tests stay green. Every engine/effects PR carries a real-time safety note and engine-maintainer sign-off. |
| II. Plugins Are Guests | No | N/A | No plugin runtime exists (FR-016). `NodeOwner::Plugin(PluginId)` is only a data-model value used by engine tests so 009 can activate the non-host auto-bypass without a breaking change (research R10). |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | Effect nodes are host primitives in host Rust (`modplayer-effects`, FR-6/FR-6.3); every DSP algorithm (WSOLA, LPC, biquads, stereo, FFT) is implemented and benchmarked in the host; no scripting tier exists or is exposed. |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | The `AudioSource` trait and every source crate are untouched; the chain consumes the frames `fill_from_source` already produced. Every automated test runs on `SyntheticSource`/`ScriptedHost` + `FakeBackend`; `single_dependent.rs` stays green. The Player-follow rule (research R8) uses only the existing `SourceCommand::Seek` and `SourceEvent::EndOfTrack`. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | **Yes** | ✅ PASS | The chain, meters and spectrum are computed on the RT; the UI receives only `f32` gauges (peak/RMS/64 bands/costs) through `RtShared` (research R9, FR-011, FR-013). No public item of `modplayer-effects` or the engine returns or writes sample slices beyond `Processor::render`'s device buffer; `decoded_store_boundary.rs::effects_crate_exposes_no_sample_sink` enforces it (SC-009). No debug flag, test helper or bench writes audio anywhere (benches synthesise their input in memory). |
| VI. Security and Privacy by Default | No | N/A | No credential, network, telemetry or persisted data is touched; overload counters and costs are local gauges (NFR §8 "local"). |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | One new crate for a constitution-named component (`modplayer-effects`), `#![forbid(unsafe_code)]`, `deny(clippy::unwrap_used, expect_used)`, `thiserror` errors (`ChainError`), doc examples on public items (`catalog::clamp`, `ChainModel::set_param`, `StretchStage`), MSRV unchanged; one dev-only dependency (`criterion`, MIT OR Apache-2.0, no default features) verified by `cargo deny check`; benches compile under the existing `clippy --all-targets` gate; CI matrix unchanged. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Test-first for public behaviour with the named tests in contracts §12 / quickstart; control-to-audio latency (SC-001) and click-free seams (SC-002, every transition) automated; proptests for parameter clamping (all ranges + Nyquist), ramp/crossfade arithmetic, pull-plan conservation and `rewind_in_loop`; **criterion benchmarks and the 24 h soak deferred by 001 are delivered here** (`benches/{nodes,reference_chain}.rs`, `soak.rs` 60 s in CI / 24 h manual — research R16, FR-018); overload/auto-bypass engine tests; manual scenarios M1–M14 executed by the implementing agent (Governance › Manual Scenario Sign-Off). |
| IX. One Plugin API Definition | No | N/A | No plugin API in this slice; the catalog's `NodeKind`/`ParamId` are host types 009 may later expose through its schema. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No trait with a single implementor (nodes are an enum, not a trait; `ChainRt` is concrete); no feature flag; the new crate is justified by two consumers (core and engine) and the constitution's component list (research R1); no new runtime dependency (egui's DnD, hand-written DSP); behaviour and keys identical on all three platforms (NFR-9.2); every control keyboard-operable and accessibly named (NFR-6.1/6.2, FR-015); strings externalised (NFR-7.1). User override N/A (no plugins), but a user can always bypass or remove any node in one action and un-bypass an auto-bypassed one. |
| Governance: engine/gateway/runtime sign-off | **Yes** | ✅ PASS | `crates/modplayer-engine/` and the new `crates/modplayer-effects/` (real-time code) require the engine area maintainer's sign-off and a real-time safety note on every PR. Requirement ids (FR-, SC-, NFR-, DM-, AR-, C-) are referenced throughout spec, plan, contracts and test names. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no `unsafe`, no feature flag, no trait, no runtime dependency; the one new crate is a constitution-named component with two consumers; the real-time path gains only preallocated, boundary-applied state proven allocation-free; the raw-sample boundary is unchanged and newly enforced by an API-surface test; every headless assumption (Complexity Tracking) sits outside the non-negotiable principles.

## Project Structure

### Documentation (this feature)

```text
specs/008-effect-chain-and-built-in-nodes/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R17 with evidence from the engine/core/ui sources
├── data-model.md        # Phase 1: catalog, core ChainModel/ChainView/MeterSnapshot, RT ChainRt/NodeSlot/StretchStage, RtShared delta, UI state, notifications, catalog delta, Command/Event delta
├── quickstart.md        # Phase 1: automated gates (named tests), release measurements, manual scenarios M1–M14
├── contracts/
│   ├── engine-effect-chain.md   # render order, Command/Event delta, pull plan, transitions, kernels, combined stage, metering, overload, position, RT safety, boundary, tests
│   ├── effects-service.md       # ChainModel API + rules G1–G9, controller façade rules C1–C10, notifications, action catalog delta, errors
│   └── ui-effect-chain.md       # panel placement/layout, parameter controls, reorder, actions, Fluent keys, repaint, tests
├── checklists/requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

Existing layout (001–007) is kept; `+` marks new files, `~` modified files.

```text
Cargo.toml                                   ~ workspace.dependencies += criterion (dev, default-features = false)
deny.toml                                    ~ comment only if `cargo deny check` surfaces nothing new (expected)
locales/en-US/
├── effects.ftl                              + panel, node, parameter, mode, notification strings (≈ 60 keys)
└── controls.ftl                             ~ + action-nav-toggle-effect-chain
crates/
├── modplayer-effects/                       + NEW CRATE (Constitution VII "effects"; research R1)
│   ├── Cargo.toml                           + no runtime deps; dev: proptest, criterion
│   ├── src/lib.rs                           + forbid(unsafe_code); pub mod catalog, consts, smooth, crossfade, biquad, nodes, spectrum, rt
│   ├── src/catalog.rs                       + NodeKind, NodeOwner, PluginId, ParamId, ParamDef, params(), clamp(), QualityMode, mode rule
│   ├── src/consts.rs                        + MAX_NODES, ramp/crossfade ms, spectrum, cost window, overload, Q_MAX, stretch presets
│   ├── src/smooth.rs                        + Smoothed (20 ms linear ramp)
│   ├── src/crossfade.rs                     + Crossfade (5 ms equal-power state, reuses loop_math curve shape)
│   ├── src/biquad.rs                        + RBJ coefficients (peak/shelves/HP/LP), Biquad state, shadow switch
│   ├── src/nodes/{mod,gain,eq,filter,stereo,stretch,lpc}.rs  + kernels (research R4, R5, R12, R13)
│   ├── src/spectrum.rs                      + 1024-pt radix-2 real FFT, Hann, 64 log bands
│   ├── src/rt/{mod,chain,slot,cost}.rs      + ChainRt (plan/process/latency/advance_rate/reset), NodeSlot, CostRing
│   ├── benches/{nodes,reference_chain}.rs   + criterion (research R16)
│   └── tests/{catalog.rs, nodes.rs, stretch.rs, chain_rt.rs}  + unit/prop tests (contracts §12)
├── modplayer-engine/
│   ├── Cargo.toml                           ~ + modplayer-effects
│   ├── src/command.rs                       ~ + ChainInsert/ChainRemove/ChainMove/ChainSetBypass/ChainSetParam (≤ 16 B const-assert kept)
│   ├── src/event.rs                         ~ + Overload, AutoBypassed
│   ├── src/shared.rs                        ~ + advance_rate in anchor, node/chain cost, render pct, overload_count, over_budget, 8 meter atomics, spectrum[64] + generation
│   ├── src/processor.rs                     ~ fill_from_source extracted; plan → fill → pre-meter → chain → gain/tone → post-meter/spectrum → limiter; cost/overload/auto-bypass; lead-compensated position; reset_history on Seek/Stop
│   ├── src/loop_math.rs                     ~ + rewind_in_loop (+ proptest)
│   ├── src/position_clock.rs                ~ extrapolate at source_rate × advance_rate
│   └── tests/{effects_pipeline.rs +, effects_transitions.rs +, effects_latency.rs +, effects_position.rs +, effects_overload.rs +, effects_meters.rs +, soak.rs +, realtime.rs ~, boundary.rs ~}
├── modplayer-core/
│   ├── Cargo.toml                           ~ + modplayer-effects
│   ├── src/lib.rs                           ~ pub mod effects; re-exports
│   ├── src/effects/{mod,model,view}.rs      + ChainModel, NodeModel, ChainError, ChainView/NodeRow, MeterSnapshot
│   ├── src/controller.rs                    ~ chain field + façade, replay on open_stream_on, event mirror (Overload/AutoBypassed), over-budget warning lifecycle, tempo_step, engine-gated end-of-track (research R8; the Player re-seek was dropped 2026-09-19)
│   ├── src/notifications.rs                 ~ + 3 keys
│   ├── src/actions/catalog.rs               ~ ToggleEffectChain (45), tempo steps enabled
│   └── tests/{controller_effects.rs +, actions.rs ~, controller_streaming.rs ~ (unity end-of-track tests re-run)}
├── modplayer-ui/
│   ├── Cargo.toml                           ~ + modplayer-effects (catalog types for controls)
│   ├── src/lib.rs                           ~ pub mod effects_view
│   ├── src/effects_view.rs                  + panel: header figures, meters, rows (dnd, handle claims, bypass, cost, notes, params per kind, remove), add row + inline refusal, toggle helper
│   ├── src/widgets/chain_meters.rs          + level_pair (peak+RMS bars, hold/integration) and spectrum widgets
│   ├── src/widgets/mod.rs                   ~ pub mod chain_meters
│   ├── src/now_playing.rs                   ~ "Effects" toggle beside "Queue"; show panel when open
│   ├── src/actions.rs                       ~ invoke: ToggleEffectChain, TempoStepUp/Down; EFFECT_HANDLE_CLAIMS
│   └── tests/{effects_view.rs +, actions.rs ~, accessibility.rs ~, fluent_keys.rs ~, now_playing.rs ~}
└── modplayer/
    └── tests/{decoded_store_boundary.rs ~ (+ effects_crate_exposes_no_sample_sink), single_dependent.rs (unchanged)}
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
(one crate per architectural component, Constitution VII) and add
**one crate, `crates/modplayer-effects`** — the "effects" component the
constitution names, with two genuine consumers (research R1): the
engine (`crates/modplayer-engine`, real-time orchestration in
`processor.rs`) and core (`crates/modplayer-core/src/effects/`, the
controller's shadow model) must clamp with the same function so the UI
and the RT agree bit-for-bit (SC-005). The UI (`crates/modplayer-ui/src/
effects_view.rs`, `widgets/chain_meters.rs`) sits beside the Queue and
Markers panels it mirrors. The dependency graph becomes
`modplayer → {ui, core, audio-io, account, secure-store, audio-source-connect}`;
`ui → {core, audio-io, engine, effects, account, secure-store, audio-source}`;
`core → {engine, effects, audio-io, audio-source, audio-source-synthetic}`;
`engine → {effects, audio-source, audio-source-synthetic}`;
`effects → {}` (std only). The source, receiver and audio-io crates are
not modified. Locale files stay under `locales/en-US/`.

## Design notes that tasks must respect

1. **Nothing allocates under `render`** (R2, R3, R9): every buffer —
   16 slots' stretch rings/voice tails/LPC scratch, both buses, the
   spectrum ring and FFT scratch, cost rings — is allocated in
   `Processor::new`; `ChainInsert` re-initialises in place.
2. **Plan before fill** (R3): `ChainRt::plan(needed)` decides how many
   source frames this render pulls; `fill_from_source` is 006's loop
   body with only its destination changed; the chain must output exactly
   `needed` frames every render.
3. **One stage for pitch and tempo** (R4): `stretch = p / r`, `step = p`;
   adjacency (bypass ignored) selects the combined stage; the second
   member idles and gets half the cost.
4. **Unity is pass-through** (R4, SC-013): a stretch stage at exactly
   unity copies nothing and adds no delay; engage/disengage is a 5 ms
   two-voice seam.
5. **Clamp once, on both sides** (R5): `catalog::clamp` is the only
   clamping function; the model returns its result to the UI and the RT
   re-applies it defensively.
6. **Every transition is a crossfade of two live states** (R6): dry/wet
   mix for add/remove/bypass, two-phase for reorder, shadow biquad or
   dual configuration for discrete switches, second voice for the
   stretch stage; length `ceil(0.005 × source_rate)`, equal power.
7. **Position lead and advance rate** (R7): `lead = carry_len +
   latency_frames`; `rewind_in_loop` when a loop is active; the anchor
   carries `advance_rate` and `PositionClock` multiplies by it.
8. **History resets on `Seek`/`Stop` only** (FR-001a): never on a loop
   wrap; rebuild starts empty.
9. **Overload on the RT** (R10): render % window, 3 × 90 % or 1 × 100 %,
   counter + flag + event; bypass only a non-host costliest slot, at most
   once per window; the warning is keyed and dismissed by key when the
   flag clears.
10. **Replay on rebuild** (R11): `open_stream_on` pushes
    `chain.replay()` after the loop re-push; source-rate re-clamp first.
11. **Player follow under tempo** (R8): no re-seek (dropped 2026-09-19 — the receiver is consumption-paced);
    engine-gated end-of-track; byte-for-byte unchanged at unity.
12. **Tempo steps target the first time-stretch node** (FR-017), bypass
    ignored, through `chain_set_param` so FR-008 runs; missing node →
    one keyed Info, coalesced by visibility.
13. **Panel = Queue-panel conventions** (R14): temp-memory open flag,
    header toggle + `E`, rows keyed by `NodeId`, handle claims `↑`/`↓`,
    `DragValue` `TextLike` claims, inline refusal at capacity, every
    string in `effects.ftl`.
14. **Catalog is 45 actions** (R15): `E` added, tempo steps enabled;
    007's tests re-pointed, defaults still conflict-free.
15. **Benches and soak are deliverables** (R16): benches compile in CI;
    the 60 s soak runs in CI; SC-003 and the 24 h soak are release-only
    manual runs whose figures are recorded in quickstart.md.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. The headless assumptions and plan-level
decisions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| New crate `modplayer-effects` (R1) | Constitution VII names "effects" as a component; the catalog has two consumers (core, engine) that must clamp identically; DSP is benchmarkable without a `Processor`. | A module inside the engine — the constitution's component list and Constitution VIII's benches argue for the crate; a module in core — forbidden (samples off the RT crate). |
| 16-slot preallocated pool instead of `Box` handover (R2) | Keeps `Command` `Copy`/≤ 16 B, no return queue, no drop-on-RT hazard; ≈ 2 MiB bounded memory. | `rtrb<Box<dyn Node>>` + garbage queue — two more queues and a bug class only tests can catch. |
| Two-pass pull plan over ping-pong buses (R3) | Rate-changing nodes anywhere in the chain (spec allows non-adjacent and duplicate stretch nodes) without recursion or trait objects. | Fixed stretch position outside the chain — violates FR-007/reorder; trait-object pull graph — `dyn` on the RT. |
| WSOLA + LPC formant, hand-written (R4) | Low latency (SC-001's 20 ms), allocation-free, no RT dependency, one kernel = FR-007's combined stage. | Phase vocoder — latency and CPU; `rubato`/`signalsmith` crates — allocation behaviour outside our control on the RT path. |
| Two-phase reorder (dry-out, move, wet-in), ≤ 10 ms total (R6) | A node has one DSP state and cannot be at two positions; both phases are click-free. | Rendering the chain twice in both orders — duplicated state and CPU; hard swap — clicks (FR-005). |
| Stretch stage pass-through at exact unity (R4) | SC-013 sample-exact at defaults; zero cost and zero latency for an idle node. | Always-engaged WSOLA at unity — a constant 25–50 ms delay and CPU for nothing. |
| `Q_MAX = 8` for resonance 1.0 (R12) | A stable biquad never self-oscillates; +18 dB is the conventional "controllable resonance" cap the −1 dBFS limiter can still hold. | `Q_MAX = 10` — sustained limiter clipping on full-scale sweeps; `4` — too tame. |
| Engine-gated end-of-track under tempo (R8; the 500 ms Player re-seek was dropped 2026-09-19) | At 0.5× the Player would end the track halfway; at 2× the engine would wrap before the Player ends. | Ignoring Player events and detecting end on the RT — needs a receiver-crate change outside this slice. |
| Spectrum via hand-rolled 1 024-pt FFT on the RT (R9) | Constitution V forbids shipping samples to an off-thread FFT; 60 LOC, ≈ 40 µs. | `rustfft` — a dependency on the RT path; 64 biquads — 50× the cost at large buffers. |
| SC-003 asserted only in release under `#[ignore = "manual"]`, plus a criterion bench (R16) | CI runs debug builds; debug timing is meaningless and would flake. | Asserting in CI — flaky/wrong; skipping the assertion — leaves SC-003 unverified. |
| 24 h soak manual, 60 s in CI (R16, as 001 planned) | Hosted runners cannot run 24 h; the 60 s run catches gross leaks every merge. | 24 h in CI — infeasible. |
| `criterion` dev-dependency with `default-features = false` (R16) | Constitution VIII names criterion; disabling plotters/rayon avoids new licence surface. | `divan` — not what the constitution names. |

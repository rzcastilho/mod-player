# Research: Effect Chain and Built-In Effect Nodes

**Feature**: 008-effect-chain-and-built-in-nodes | **Date**: 2026-09-18 | **Spec**: [spec.md](spec.md)

Every unknown in plan.md's Technical Context is resolved here. Format per
decision: Decision / Rationale / Alternatives considered. The spec's two
clarification passes already fixed every *product-level* number (ranges,
defaults, 20 ms ramp, 5 ms crossfade, 90 %/3-callback overload rule,
~1 s cost window, 64 spectrum bands, `E`, `+`/`-` steps); this file fixes
the *engineering* choices the spec left to the plan: where the DSP lives,
how a node's state reaches the real-time thread without allocating, how a
rate-changing node pulls input, which algorithms implement pitch/time,
how every transition stays click-free, how position/latency are
published, how the streaming `Player` follows a non-unity tempo, and how
the constitution's benchmark/soak obligations are finally met. Headless
session: every decision was taken without a human; the ones that could
plausibly go another way are also listed in plan.md's Complexity
Tracking.

Code read for this research (all in this worktree):
`crates/modplayer-engine/src/{processor,command,event,shared,resample,limiter,loop_math,position_clock,output_stage,types,lib}.rs`,
`crates/modplayer-engine/tests/{realtime,loop_seam}.rs`,
`crates/modplayer-audio-source/src/lib.rs`,
`crates/modplayer-audio-source-connect/src/{rt,decode_ahead,events}.rs`,
`crates/modplayer-audio-source-synthetic/src/lib.rs`,
`crates/modplayer-core/src/{controller,notifications,actions/{catalog,mod}}.rs`,
`crates/modplayer-ui/src/{now_playing,actions,widgets/peak_meter}.rs`,
`Cargo.toml`, `deny.toml`, `.github/workflows/ci.yml`,
`specs/{001,006,007}-*/{plan,research,quickstart}.md`,
`.specify/memory/constitution.md`.

---

## R1. Where the DSP lives: a new `modplayer-effects` crate, orchestrated by the engine

**Decision**: Add `crates/modplayer-effects` — the "effects" architectural
component Constitution VII names explicitly — holding (a) the **parameter
catalog** (`NodeKind`, `ParamId`, per-parameter range/default/clamp,
the FR-008 auto-switch rule) and (b) the **DSP kernels** (`Gain`, `Eq8`,
`Filter`, `StereoTools`, the `StretchStage` WSOLA+resampler, LPC formant
correction, biquad math, the 1024-point spectrum FFT) and (c) the
allocation-free **`ChainRt`** (slot pool, ordering, pull planning,
per-node crossfades, cost timing). `modplayer-engine` depends on it and
owns the *integration*: new `Command`/`Event` variants, `RtShared`
atomics, the render-order change, latency-compensated position, the
overload rule and auto-bypass. `modplayer-core` depends on it for the
catalog only (clamping, defaults, auto-switch, UI ranges), so the UI and
the RT clamp with the same function.

**Rationale**: Constitution VII lists `effects` as its own crate;
Constitution X's "why is an existing crate insufficient" test is met
twice over — the catalog has two genuine consumers (core and engine)
that must agree bit-for-bit on clamping (SC-005), and the DSP kernels are
testable and benchmarkable (Constitution VIII: criterion) without a
`Processor`, a source or a ring buffer. Keeping `ChainRt` in the effects
crate keeps every pure audio rule (pull planning, crossfade arithmetic,
cost attribution) proptest-able on plain slices; the engine only adds the
queue/atomic plumbing it already owns. `#![forbid(unsafe_code)]`,
`deny(clippy::unwrap_used, expect_used)` as in the engine.

**Alternatives considered**: (a) everything inside `modplayer-engine`
(`src/effects/`) — rejected: the constitution names the crate, the catalog
would force core to depend on the engine for pure numbers (it already
does, but the benches and proptests would then have to live in the
engine crate, mixing RT plumbing with DSP), and a 2 500-LOC module would
dwarf the rest of the engine; (b) DSP in `modplayer-core` — rejected
outright (Constitution I: nothing off the RT crate touches samples;
core is the non-RT service layer); (c) one crate per node — rejected
(Constitution X: no second consumer for any of them).

## R2. Node state reaches the RT by a preallocated slot pool, not a `Box` handover

**Decision**: `ChainRt` owns `MAX_NODES = 16` preallocated `NodeSlot`s,
each holding one `NodeDsp` enum whose heavy variant (`StretchStage`)
already has its ring, voice tails and LPC scratch allocated at
`Processor::new` (on the controller thread), sized from the source rate.
`Command::ChainInsert { slot, position, kind, owner }` *activates* a free
slot — re-initialising its state in place, reusing the existing buffers —
and `ChainRemove { slot }` frees it after its 5 ms fade-out. Nothing is
allocated or dropped on the audio thread, ever; the existing
`assert_no_alloc` harness (`tests/realtime.rs`) proves it. The
controller's `ChainModel` allocates node ids (`NodeId(u32)`, monotonic,
never reused) and maps them to slot indices; the RT only ever sees slots.
Memory: ≈ 130 KiB per slot at 44.1 kHz (ring `2·MAX_FRAMES + 2·frame_max
+ search`, two voice tails, LPC scratch) → ≈ 2 MiB per `Processor`,
allocated once per stream (re)build.

**Rationale**: The spec's Assumption ("DSP state allocated off the RT and
handed over by identifier/pointer swap at a buffer boundary") is met by
the slot identifier; `Command` stays `Copy`, ≤ 16 bytes (every new
variant is ≤ 8 bytes); there is no return queue for freed state and no
"never drop a `Box` on the RT" discipline for reviewers to police. 2 MiB
is negligible for a desktop app and is bounded by construction.

**Alternatives considered**: (a) `rtrb::RingBuffer<Box<dyn Node>>` handover
plus a garbage-return queue — rejected: two more SPSC pairs, a fat
pointer (16 bytes, non-`Copy`) that cannot ride the existing `Command`
queue, and a class of bug (dropping on the RT) that only a test can
catch; (b) lazily sized per-kind slots — rejected: any slot may become a
stretch stage later, and re-sizing means allocating on the RT; (c)
`MAX_NODES = 32` — rejected: doubles memory for no requirement (FR-002:
"at least 16"; the constant is `pub` so a later slice can raise it).

## R3. Rate-changing nodes: a two-pass pull plan over two ping-pong buses

**Decision**: A time-stretch/pitch stage consumes a different number of
frames than it produces, so `ChainRt::process` is planned back-to-front
then executed front-to-back, once per render:

1. **Plan** (pass 1, back to front): starting from the `needed` output
   frames the output stage asked for, each node reports `input_for(out)`:
   in-place nodes return `out`; a stretch stage returns
   `max(0, lookahead(out) − ring_occupancy)` where `lookahead(out) =
   ceil(out × consume_ratio) + frame_len + search_radius`. The result is
   `in_frames[0]` — how many source frames this render must pull.
2. **Fill**: the existing loop-aware source filler (006's `render` body,
   extracted into `fill_from_source`) writes exactly `in_frames[0]` frames
   into bus A. It is unchanged in behaviour: loops, seams and seeks are
   still evaluated on the source stream *before* the chain (FR-001).
3. **Execute** (pass 2, front to back): in-place nodes process bus A (or
   B) in place; a stretch stage appends its input from one bus to its
   ring and writes `out` frames to the other bus, swapping the roles.
   The last bus is the `fresh` slice master gain / test tone / limiter
   already operate on.

Bus capacity `CHAIN_BUS_FRAMES = 4·MAX_FRAMES + 2·FRAME_MAX` (≈ 24 k
frames at 44.1 kHz). A pull request beyond it is clamped; the starved
stage repeats its last grain rather than emitting silence — reachable
only with ≥ 3 stacked 2.0× stretch nodes at the 4 096-frame defensive
cap, documented as the degenerate bound.

**Rationale**: Deterministic, allocation-free, no recursion or borrow
juggling over a `Vec` of nodes, and it supports *any* number and order
of stretch stages (the spec allows two time-stretch nodes and a
non-adjacent pitch/stretch pair). The loop machinery stays exactly where
006 proved it, feeding the chain instead of the mix buffer.

**Alternatives considered**: (a) pull-model trait objects (`next.pull(n)`)
— rejected: trait objects on the RT contradict 001's "no `dyn` on the
real-time path" and the recursion needs `&mut` to two nodes at once; (b)
fixed 1:1 chain with the stretch stage outside it (before or after the
chain) — rejected: violates FR-007's "adjacent nodes combine, non-
adjacent process in chain order" and the reorder story; (c) push model
with per-node output FIFOs — rejected: every node would need its own
ring (16× the memory of R2) and latency would grow per node.

## R4. Pitch shift and time stretch: one `StretchStage` (WSOLA + resampler), combined by parameter products

**Decision**: Both node kinds are instances of one kernel,
`StretchStage { stretch, step }`, where WSOLA time-stretches by
`stretch = output_len / input_len` and a fractional-delay resampler then
reads the stretched stream at `step` input frames per output frame:

| Node(s) | pitch factor `p = 2^(semitones/12)` | tempo ratio `r` | `stretch` | `step` | consume ratio |
|---|---|---|---|---|---|
| time stretch alone | 1 | r | `1 / r` | 1 | `r` |
| pitch shift alone | p | 1 | `p` | `p` | 1 |
| adjacent pair (FR-007) | p | r | `p / r` | `p` | `r` |

Pitch alone: stretch by `p` then read at `p` → duration unchanged, pitch
× p. Tempo alone: stretch by `1/r`, read 1:1 → pitch unchanged, plays at
`r` × speed. The adjacent pair is *literally* one stage with the product
parameters ("a single resampler stage", FR-007); the cost is timed once
and attributed half to each node (FR-012a). A bypassed member contributes
`p = 1` / `r = 1` (spec: "applies only the active member's parameters").

**WSOLA parameters** (rate-independent, in milliseconds, converted once
at construction):

| Mode | frame | synthesis hop | search radius | resampler | delay `D` |
|---|---|---|---|---|---|
| performance | 20 ms | 10 ms | 5 ms | linear | ≈ 25 ms |
| quality | 40 ms | 20 ms | 10 ms | cubic Hermite | ≈ 50 ms |

Hann windows (COLA at 50 % overlap), normalised cross-correlation search
over `±search_radius` around the natural analysis position, per channel
stereo with a *shared* offset (mid-channel correlation) so the image
doesn't wander. At exactly unity (`stretch = 1`, `step = 1`) the stage is
**pass-through** (no delay, no cost) — the "engaged" WSOLA/resampler path
runs only while a parameter is off unity or ramping (SC-013's "within the
resampler's pass-through tolerance" becomes sample-exact).

**Formant preservation** (pitch shift only, `formant = on`): order-16 LPC
per channel over 1 024-frame Hann windows at a 512-frame hop
(autocorrelation + Levinson–Durbin, fixed arrays): the input is whitened
by the inverse filter, the residual is pitch-shifted, and the *original*
envelope is re-applied by the all-pole synthesis filter — the classic
LPC formant-preserving shifter. Cost ≈ 2 × 16 MACs/sample + one
Levinson per hop, well inside budget.

**Mode auto-switch** is a pure function in the catalog crate
(`mode_after_change(kind, value, mode, auto_flag) -> (mode, auto_flag,
note)`), evaluated by the core model on every semitone/ratio change
(FR-008) and mirrored to the RT as an ordinary discrete parameter.

**SC-001 latency**: a ratio change alters the consume ratio from the next
output frame and grain selection from the next synthesis hop, so at the
performance preset (128 frames ≈ 2.9 ms) the tempo change is audible
within `2.9 ms + 10 ms` ≈ 13 ms (< 20 ms). SC-001's 1.0 → 0.6 stays
inside the stage-use range, i.e. in performance mode; quality mode's
20 ms hop is the accepted NFR-1.14/1.15 trade-off.

**Rationale**: WSOLA is the standard time-domain, allocation-free,
low-latency tempo/pitch algorithm for polyphonic music at moderate
factors (the spec's 0.25–2.0 / ±12 st); it needs no FFT per hop, its
latency is one frame, and it degrades gracefully. A single kernel with
product parameters is exactly what FR-007 asks for and halves the DSP
surface to test. LPC envelope correction is the cheapest formant method
that works on polyphonic material (no pitch detection).

**Alternatives considered**: (a) phase vocoder (STFT) with phase locking —
rejected: 2–4× the CPU, ≥ 2 048-frame latency (fails SC-001's 20 ms at
any hop that avoids phasiness), and an FFT dependency on the RT path;
(b) `rubato` / `signalsmith-stretch` crates — rejected: a new runtime
dependency on the real-time path with allocation behaviour we don't
control, and Constitution III wants host DSP we can benchmark and
reason about; (c) PSOLA for formants — rejected: needs pitch tracking,
monophonic only; (d) two independent stages when adjacent — rejected by
FR-007 (double cost, double latency).

## R5. Parameter smoothing: per-sample 20 ms linear ramps, 32-frame sub-blocks for filter coefficients

**Decision**: Every continuous parameter is a `Smoothed { current,
target, step, remaining }` advanced per frame; a new target restarts the
ramp from `current` over `ceil(0.020 × source_rate)` frames (FR-010).
Gains, width, balance and the stretch stage's `stretch`/`step` are read
per frame. Biquad coefficients (EQ bands, filter) are recomputed at
32-frame sub-block boundaries from the ramp's current value — 27
coefficient updates across a 20 ms ramp at 44.1 kHz, each a ≤ 0.04 dB /
≤ 0.4 % frequency step, inaudible — so a ramp never costs a `sin`/`cos`
per sample. Clamping happens in the catalog function *before* the ramp
target is set, on both sides (core returns the clamped value to the UI,
SC-005; the RT re-clamps as defence in depth exactly as `Limiter::
set_ceiling` does).

**Rationale**: FR-010 fixes the ramp shape and length; per-sample ramps
are the zipper-free baseline, and sub-block coefficient updates are the
conventional way to keep a parametric EQ's cost flat under automation.

**Alternatives considered**: (a) per-sample coefficient recompute —
rejected: 8 bands × trig per sample ≈ 10× the EQ's steady-state cost
during ramps; (b) one recompute per render — rejected: at the Safe
preset (1 024 frames ≈ 23 ms) a 20 ms ramp becomes a single step (zipper).

## R6. Discrete switches and chain edits: a 5 ms equal-power crossfade between two live states

**Decision**: Every FR-005 transition is a crossfade of length
`ceil(0.005 × source_rate)` frames using 006's `crossfade_gains` curve,
between two states that both produce audio during the fade:

| Transition | old state | new state | mechanism |
|---|---|---|---|
| add / un-bypass | dry (node input) | wet (node output) | node-level dry/wet mix ramps 0 → 1 |
| remove / bypass | wet | dry | mix ramps 1 → 0; `remove` frees the slot when the ramp ends |
| reorder | wet at old position | wet at new position | **two phases**: fade to dry at the old position (5 ms), relocate at the next boundary, fade to wet at the new position (5 ms) |
| mute, mono sum, phase invert, channel swap (stereo/gain) | output with old flags | output with new flags | the node computes both configurations for the fade's frames (cheap nodes) and mixes |
| EQ band type, filter mode | old-coefficient biquad | new-coefficient biquad | each band/filter carries a shadow biquad state; the fade mixes the two, then the shadow is retired |
| stretch engage / disengage / quality mode / formant | voice A | voice B | the stage holds two "voices" (analysis cursor, synthesis phase, overlap tail, LPC state) reading the same ring; the new voice starts at the same material position and the fade mixes them |

A discrete change arriving mid-fade restarts the fade from the current
mix value. A stretch stage moving *from* or *to* pass-through changes
its delay `D`; the fade across that is a seam — the same click-free jump
006's loop seam performs (the material skips `D` ≈ 25–50 ms once, under
an equal-power fade).

**Rationale**: "Crossfade old signal → new signal" is only meaningful if
both signals exist for 5 ms; for in-place nodes that means dry/wet or
dual-state processing (cheap), and for the stretch stage two voices over
one ring (bounded, preallocated). The two-phase reorder is the only
scheme that keeps a single DSP state per node: the node cannot be at two
positions at once. Total transition ≤ 10 ms, both phases click-free, the
node's *effect* is absent for ≈ 10 ms — inaudible and within the spec's
"swap at a buffer boundary with no click" (the swap *starts* at the
boundary).

**Alternatives considered**: (a) render the whole chain twice (old and
new order) and crossfade the outputs — rejected: every node's state
would have to be duplicated (2× memory, 2× CPU for 5 ms, and stretch
rings cannot be cloned on the RT); (b) hard swap at the boundary —
rejected by FR-005; (c) tie the length to 006's per-region loop crossfade
— rejected by the spec's Clarifications (its valid `0` would click).

## R7. Position and latency: subtract the chain's lead, publish the advance rate

**Decision**: `ChainRt::latency_frames()` = Σ over engaged stretch stages
of buffered-but-not-yet-output input frames (≈ `D` per engaged stage);
`ChainRt::advance_rate()` = Π over engaged *time-stretch* stages of their
current tempo ratio (pitch-only stages consume 1:1). `Processor::render`
publishes `position = rewind(raw_source_position, lead)` where `lead =
carry_len + latency_frames`; `loop_math::rewind_in_loop(raw, lead, a, b)`
generalises 006's "republish through `B`" rule with modular arithmetic so
a lead longer than a tiny loop still lands inside `[a, b)` (proptested).
`RtShared::write_anchor` gains `advance_rate` (f32 bits) and
`PositionClock::now` extrapolates at `source_rate × advance_rate`
(FR-001a). Nodes reset their history on `Command::Seek` and `Stop`
(cue jumps are seeks) and never on a loop wrap.

**Rationale**: The chain's input runs ahead of what is audible by its
buffered lookahead; without the subtraction the playhead would lead by
25–50 ms whenever a stage is engaged. The advance rate makes 005's
extrapolating playhead correct at any tempo with no UI change beyond
reading one more field. The lead changes by `D` exactly when a stage
engages/disengages — a one-off ≤ 50 ms playhead correction, below one
UI frame's worth of motion at 60 fps and accepted.

**Alternatives considered**: (a) leave the lead uncompensated — rejected
(FR-001a's "± one buffer" test would still pass on deltas but the seek/
marker alignment would be visibly early); (b) ramp the lead across the
seam — rejected as complexity for an invisible effect.

## R8. The streaming `Player` under a non-unity tempo: drift-bounded re-seek and engine-gated end-of-track

**Decision**: The Connect `Player` (003) keeps decoding at real time and
the RT reads the `DecodedStore` (005) at whatever rate the chain
consumes, so audio is unaffected; what drifts is the Player's own
position (Connect remote display) and its `EndOfTrack`. The controller
therefore, while `advance_rate ≠ 1` and playing:

1. ~~re-seeks the Player to the engine position (006's coalesced
   `SourceCommand::Seek` path, ≤ 4/s) whenever the predicted drift
   `|elapsed_since_last_reseek × (1 − rate)|` reaches **500 ms**~~ —
   **withdrawn 2026-09-19** (see the addendum below): the receiver's
   RT pops the sample ring in lockstep in both of its feeds, so the
   Player is paced by the engine's consumption and never drifts;
   nothing is sent;
2. treats `SourceEvent::EndOfTrack` as **engine-gated**: it is mirrored
   into the transport reducer only once the engine position is within
   `drift_bound = 500 ms + one buffer` of `len_frames` (else held in
   `end_of_track_pending` and mirrored on the tick that crosses the
   threshold); symmetrically, if the engine crosses `len_frames − one
   buffer` first (rate > 1), the controller mirrors `EndOfTrack` itself
   and swallows the Player's own for that track (cleared on the next
   `TrackStarted`).

At unity this code path is inert and 003/006 behaviour is byte-for-byte
unchanged (asserted by the existing controller tests).

**Rationale**: FR-001a requires markers/loops/seeks in source frames and
a position that advances at `ratio ×` real time; the only cross-cutting
consumer of *Player* time is end-of-track, which at 0.5× would otherwise
cut the last half of every track. 006 already established the re-seek
precedent for loops.

**Alternatives considered**: (a) pause the Player and drive it purely by
seeks — rejected: librespot's Player is the decode source of the store's
frontier; (b) ignore Player `EndOfTrack` entirely and detect end on the
RT (`Event::TrackEnded`) — rejected: the RT source wraps modulo `len`
today (003 contract) and changing that is a receiver-crate change
outside this slice; the engine-gated mirror achieves the same without
touching `AudioSource`.

Known limitation (recorded, not fixed here): within the first seconds of
a track, before the decode-ahead store covers the position, the RT reads
the real-time ring; a ratio > 1 there starves the ring exactly as a seek
ahead of the frontier does today (005 R2's rescue transition applies).

**Addendum, 2026-09-19 manual walk (M2 deviation — open)**: on the real
Connect source the premise "audio is unaffected" does not hold. With a
solo time-stretch node at 55 % the playhead advanced at ≈ 0.46 × wall
time, at 45 % at ≈ 0.33 × (unity baseline 1.0 ×; inside an armed loop,
where the RT is on the `Store` feed, the rate was exact). Each drift
re-seek (rule 1) targets the *lead-subtracted* published position, and
on the `Ring` feed the seek's `Reposition` marker rewinds the RT cursor
by roughly the buffered lead / ring depth — about 100 ms per re-seek,
once per ≈ 1 s at these ratios, which is the whole shortfall. The
deeper flaw is the premise: this receiver is a *pull-through* local
player whose reported position cannot drift from the engine's
(backpressure ties it to consumption), so the predicted-drift model
(`elapsed × |1 − rate|`) manufactures drift that is not there and every
"correction" is a real rewind. Candidate fixes, both outside this
slice's engine/core scope because the receiver's `Reposition` handling
is where the rewind happens: (a) seek to the raw source position
(`published + chain latency`) — removes the lead rewind but still
replays whatever decode-ahead the ring held; (b) drop rule 1 for a
pull-through source (keep rule 2's engine-gated end-of-track, which is
what actually needed the tempo awareness). **(b) applied the same day**:
`rt.rs::fill` pops the ring in lockstep on the `Store` feed too, so the
Player can never run ahead of consumption by more than the ring depth
in *either* feed — rule 1 had nothing real to correct. `follow_tempo_
drift` and `last_tempo_reseek_at` are gone; `player_is_never_reseeked_
for_tempo_drift` pins the absence. Re-verified on hardware: see the
T101 M2 re-run note in tasks.md.

## R9. Metering and spectrum on the RT, published as atomics — no samples leave the engine

**Decision**: Pre-chain peak/RMS (L/R) are computed over bus A right after
the source fill; post-chain peak/RMS (L/R) over `fresh` after master gain
and test tone, immediately before the limiter (spec render order). The
spectrum is a **1 024-point real FFT** (hand-rolled radix-2, precomputed
twiddles, ≈ 60 LOC in `modplayer-effects::spectrum`) over a Hann-windowed
mono (`(L+R)/2`) ring of the last 1 024 post-chain frames, folded into 64
log-spaced bands 20 Hz–20 kHz, recomputed whenever ≥ 256 new frames have
accumulated (≤ 172 Hz at 44.1 kHz; ≈ 40 µs per FFT). Everything is
published as `f32` bits in `RtShared` (`[AtomicU32; 64]` bands, 8 meter
atomics, `spectrum_generation`) — the UI reads numbers, never samples
(Constitution V, FR-013). Meter ballistics (peak hold, 300 ms RMS
integration) are UI-side.

**Rationale**: Publishing samples for an off-thread FFT is exactly what
Constitution V forbids; an FFT of this size is < 2 % of a 2.9 ms
callback and only runs every other render at the performance preset.
Per-render meters are cheap and the UI already smooths the 001 peak
meter.

**Alternatives considered**: (a) 64 band-pass biquads — rejected: 128
biquads × N frames, ≈ 5 M flops at 4 096 frames vs a fixed 100 k for the
FFT; (b) `rustfft`/`realfft` — rejected: a dependency on the RT path for
60 lines of code; (c) 128 bands — rejected: no UI need, doubles the
atomics; 64 is the spec's default.

## R10. Cost measurement, overload detection and auto-bypass live on the RT

**Decision**: `Instant::now()` (already used at the end of every render)
brackets each node's `process` and the whole render. A per-slot ring of
`N = ceil(1 s / callback_period)` (cap 1 024) callback costs, as a
percentage of the callback period `out_frames / device_rate`, yields the
rolling mean published per slot (`RtShared::node_cost[slot]`) and the
chain total (FR-012a); a combined stage's time is split half/half. The
render-time percentage feeds the FR-012 state machine on the RT:
`consecutive_over_90 ≥ 3` or any `≥ 100` raises an overload event —
`overload_count += 1` (atomic), `over_budget = true` (atomic),
`Event::Overload { costliest_slot, render_pct }` — and, if the costliest
slot's owner is non-host, not bypassed, and no auto-bypass happened in
the current window, sets that slot bypassed (5 ms fade), flags it
`auto_bypassed`, and pushes `Event::AutoBypassed { slot }`. `over_budget`
clears once `N` consecutive callbacks stayed under 90 %. The controller
mirrors: keyed `Warning` `effect-chain-over-budget` (args: node type,
owner) raised on the event and re-used while `over_budget` is true,
dismissed by key when it turns false; the model's `auto_bypassed` flag
set from the event; the UI label "auto-bypassed (over budget)".

**Rationale**: The decision must land at the next buffer boundary and
needs per-slot costs and owners the RT already has; doing it on the RT
makes the engine test in FR-018 ("exactly the most expensive non-host
node is bypassed, never a host node") a pure `Processor` test. In this
slice every owner is `Host`, so the branch is exercised only by tests
that insert a node with `NodeOwner::Plugin(_)` (the data model's
non-host value) — no engine change is needed when 009 lands.

**Alternatives considered**: (a) controller-side decision from published
costs — rejected: a UI-thread round trip, a tick late, and the engine
test would need a controller; (b) `std::time::Instant` replaced by a
cycle counter — rejected: `Instant` is already on the render path and is
vDSO/`mach_absolute_time`, non-blocking.

## R11. Chain state across stream rebuilds: the controller re-pushes it, DSP history restarts

**Decision**: `open_stream_on` (which builds a fresh `Processor` on device
change, preset change or sample-rate change) re-pushes the whole chain
after its existing loop re-push: for each node in order `ChainInsert`,
then every parameter (`ChainSetParam`, clamped at the *new* source rate —
FR-014's coefficient recompute falls out of this for free), then
`ChainSetBypass` where set. Node ids and slot indices are unchanged
(the slot pool is per-`Processor`; the controller's slot map is reused).
DSP history (rings, biquad states) starts empty — a rebuild is already
an audible discontinuity.

**Rationale**: Mirrors 006's `stream_rebuild_repushes_armed_region`
exactly; the chain is controller shadow state (FR-002: global,
session-scoped), and the RT copy is derived from it.

**Alternatives considered**: moving `ChainRt` into `RtShared`-like
survivor state — rejected: it holds `Vec`s and DSP state that must be
rebuilt per source rate anyway.

**Addendum, 2026-09-19 manual walk (M14)**: the re-push works — after a
Balanced ↔ Performance change all 16 nodes, their order and every
parameter/bypass/mode state were intact — but audio never resumed after
either rebuild (position frozen, meters at −60 dB, "Buffering…" forever
on Pause/Play). This is not the chain: `ConnectSource::attach` builds a
fresh sample/marker ring pair per stream and parks the producers as
"pending" until `SourceCommand::Initialize`, which the controller sends
once per session (`set_playback_permitted`) and `handle_initialize`
ignores while a worker already exists — so every re-attach after
registration (preset change, device change, device fallback) leaves the
new RT reading rings nobody writes. Present since 003 (`36b4b44`), so
006's `stream_rebuild_repushes_armed_region` and this slice's
`stream_rebuild_replays_chain` were always green on `ScriptedHost` only.
**Fixed the same day** in the receiver crate: `swap.rs::RingSwap`, a
three-slot mutex hand-off the host parks the new worker-side ring ends
in whenever a worker already runs; the `RingSink` adopts the sample
producer at the top of its next `write` (crediting the abandoned ring's
unread frames to the consumed clock so write-stamped markers stay due
on time) and the command loop adopts the marker producer and retirement
consumer at the top of its next iteration. `attach()` also seeds the
new marker ring with a `MarkerKind::Reattach { store }` (due at frame
0) so the re-attached RT holds the playing track's `DecodedStore`
immediately — sample-exact loop seams keep working across the rebuild
— without touching `cursor` or `track_seq`. Re-verified on hardware
(T101 M14 re-run note in tasks.md).

## R12. Filter resonance cap: `Q_MAX = 8` (≈ +18 dB), Butterworth at 0

**Decision**: normalised resonance `res ∈ [0, 1]` maps to biquad `Q =
Q_BUTTER + res × (Q_MAX − Q_BUTTER)` with `Q_BUTTER = 1/√2`, `Q_MAX = 8`
(peak gain ≈ +18 dB at cutoff). Cutoff/EQ frequency clamp to
`min(20 kHz, 0.45 × source_rate)` (spec), coefficients from the RBJ
cookbook (HP/LP, peaking, low-shelf, high-shelf).

**Rationale**: A stable 2nd-order biquad never self-oscillates at finite
Q; the spec's cap is about the *peak* the −1 dBFS brickwall limiter can
still bring under the ceiling without gross clipping — +18 dB on a
full-scale narrow band is the conventional "resonant but controllable"
top of a filter-resonance knob. `Q_MAX` is a `pub const` in the catalog
so the value is auditable.

**Alternatives considered**: `Q_MAX = 10` (+20 dB) — rejected as the
first value where a full-scale sweep produces sustained limiter clipping
in a quick model; `Q_MAX = 4` — rejected as too tame for the "resonance"
control users expect.

## R13. Stereo tools order and math

**Decision**: per frame: width via mid/side (`M = (L+R)/2`, `S = (L−R)/2
× width`), balance as far-channel attenuation (`gL = min(1, 1 − bal)`,
`gR = min(1, 1 + bal)`), mono sum (`R = −R` first if phase invert; then
`L = R = (L+R)/2`), channel swap last. Width and balance ramp (20 ms);
the four flags crossfade (R6). Phase invert is only applied when mono sum
is on (spec) and the UI disables it otherwise.

**Rationale**: the spec fixes the order (mono sum before swap) and the
invert semantics; mid/side is the standard width control and is exactly
identity at `width = 1`, satisfying SC-013 sample-exactly.

**Alternatives considered**: constant-power balance — rejected: a −3 dB
centre would change the sound at the default (SC-013).

## R14. UI: an `effects_view` panel beside the Queue panel, egui drag-and-drop for reorder, 007 claims for keys

**Decision**: `crates/modplayer-ui/src/effects_view.rs` draws the panel
below the transport row when `effect_chain_panel_open_id()` (egui temp
memory, same pattern as the Queue panel) is true; the header gains an
"Effects" `selectable_label` and `toggle_effect_chain_panel(ctx)` serves
both it and `HostAction::ToggleEffectChain` (`E`). Rows use egui's
`dnd_drag_source`/`dnd_drop_zone` (built into egui 0.36) for pointer
reorder; the drag handle is a focusable button registering
`EFFECT_HANDLE_CLAIMS = {ArrowUp, ArrowDown}` with 007's `FocusClaims` so
`↑`/`↓` move the node and never move focus; `DragValue`s register
`TextLike` claims as 007 did for the marker panel. Per-node widgets:
kind + owner label, bypass `toggle_value`, CPU `%` label, "auto-bypassed
(over budget)" / "quality mode auto-switched" notes, parameter controls
per kind (sliders/`DragValue` with the catalog's ranges and units,
`ComboBox` for discrete). The panel header shows the whole-chain CPU
figure, the pre/post peak+RMS meters (a `widgets/chain_meters.rs` widget
reusing `peak_meter`'s dB scale) and the 64-band spectrum. "Add node…" is
a `ComboBox` of the six kinds; a refused add (chain full) shows an inline
reason as 006's marker limit does. Every string in a new
`locales/en-US/effects.ftl`.

**Rationale**: matches the Queue/Markers panel conventions (spec
Clarifications), needs no new dependency (egui ships DnD), and 007's
claims registry is the sanctioned way to give a widget its own keys
without stealing registered actions (FR-019 precedence; the waveform's
`+`/`-` zoom claim already resolves the tempo-step overlap).

**Alternatives considered**: a dedicated nav-rail section — rejected by
the spec; a hand-rolled drag list — rejected (egui's DnD is accessible
and already tested upstream).

## R15. Action catalog delta: 45 actions, tempo steps enabled

**Decision**: `HostAction::ToggleEffectChain` is added (`host.nav.
toggle_effect_chain`, category Navigation, scope `NowPlaying`, no repeat,
enabled, default `["E"]`); `TempoStepUp`/`TempoStepDown` flip to
`enabled_by_default: true` with their bindings unchanged. `CATALOG`
becomes `[ActionDef; 45]`; 007's count/enabled tests and its
`no_continuous_actions_in_this_slice` are re-pointed; the defaults-
conflict-free test still passes (`E` is unbound elsewhere — verified
against 001–007's key tables). `invoke` maps `TempoStepUp/Down` to
`controller.tempo_step(±1)` and `ToggleEffectChain` to
`effects_view::toggle_effect_chain_panel(ctx)`.

**Rationale**: FR-017 verbatim; 007 designed the catalog for exactly this
additive change (its research R8 left the two arms in `invoke` for it).

**Alternatives considered**: none material.

## R16. Benchmarks and soak: `criterion` as a dev-dependency of the effects crate; the soak in the engine

**Decision**: `criterion = { version = "0.7", default-features = false,
features = ["cargo_bench_support"] }` (MIT OR Apache-2.0; plotters and
rayon off so no new licence surface — `cargo deny check` re-run at
T001) as a dev-dependency of `modplayer-effects` only, with
`benches/nodes.rs` (one group per node kind, 128/256/1 024-frame blocks
at 44.1 kHz, both stretch modes, formant on/off) and
`benches/reference_chain.rs` (SC-003's pitch + stretch + 8-band EQ at
128 frames, reporting `ns/render` against the 2.9 ms period → the 50 %
figure). Benches compile under CI's `cargo clippy --all-targets`; they
run manually (`cargo bench -p modplayer-effects`) and their figures are
recorded in quickstart.md. The soak (`modplayer-engine tests/soak.rs`)
renders the reference chain through a real `Processor` with continuous
parameter drags, reorders and bypasses: 60 s in CI (asserting RSS /
allocation-counter stability via `assert_no_alloc` plus a coarse
`peak_rss` sample every 10 s), 24 h under `#[ignore = "manual"]`
(`MODPLAYER_SOAK_HOURS=24`). An engine test `reference_chain_under_
half_core` asserts the 50 % budget and is `#[ignore = "manual"]` because
CI runs debug builds (`cargo test --release -p modplayer-engine --
--ignored reference_chain_under_half_core` on reference hardware).

**Rationale**: Constitution VIII requires criterion and the 24 h soak
for real-time crates and 001 deferred both "until an effect chain
exists"; this is that slice (FR-018). Debug-build timing in CI would be
meaningless, hence the release-only manual assertion plus the release
bench.

**Alternatives considered**: `divan` — rejected (criterion is what the
constitution names); running the soak for 24 h in CI — rejected as in
001 (infeasible on hosted runners).

## R17. Tests strategy summary (what pins each requirement)

- **Catalog / clamping** (`modplayer-effects`): proptests that
  `clamp(kind, param, x, rate)` is idempotent, within range, monotone,
  and equals the requested value inside range for every `ParamId`
  including the rate-derived Nyquist clamp; auto-switch rule table.
- **DSP kernels**: identity at defaults (sample-exact for gain/EQ/filter/
  stereo; pass-through for the stretch stage at unity), frequency-response
  spot checks (EQ +6 dB at 1 kHz, HP −3 dB at cutoff, LP attenuation),
  stereo tools truth table, ramp arithmetic proptests (reaches target in
  exactly `ramp_frames`, monotone), WSOLA tempo (click train interval ×
  1/r), pitch (sine frequency × p, duration unchanged), formant envelope
  peak retention.
- **ChainRt**: pull-plan proptest (Σ produced == requested, ring never
  overflows, 16 nodes any order), crossfade proptests (equal power, ends
  exactly at 0/1), cost attribution half/half.
- **Engine** (`modplayer-engine/tests/effects_*.rs`): every transition
  click-free using 006's first-difference metric (`≤ 1.5 × baseline`);
  SC-001 latency on a synthetic click train; position advance at 0.5
  with a loop (SC-011); seek resets history, wrap does not; overload →
  event/counter/non-host-only bypass; `realtime.rs` extended with a
  16-node chain under `assert_no_alloc`; empty-chain output identical to
  001's pipeline.
- **Core** (`tests/controller_effects.rs`): add/remove/move/bypass/param
  command sequences, clamped return values, auto-switch flags, tempo
  steps (+ coalesced notification), rebuild re-push, Player re-seek and
  engine-gated end-of-track, over-budget notification lifecycle.
- **UI** (`tests/effects_view.rs`): panel toggle by `E` and header, rows
  in order with labels, add/remove/bypass/reorder by pointer and `↑`/`↓`,
  clamped display, notes, inline refusal at capacity, disabled phase
  invert, `+`/`-` vs waveform focus; `accessibility.rs`/`fluent_keys.rs`
  extended.
- **Boundary**: `modplayer/tests/decoded_store_boundary.rs` extended: no
  public item in `modplayer-effects`/engine returns or writes `&[f32]`
  audio outside the crates (SC-009 API-surface check).

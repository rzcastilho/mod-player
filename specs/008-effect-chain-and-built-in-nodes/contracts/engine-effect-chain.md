# Contract: Engine effect chain (extends 001 contracts/engine-commands.md and 006 contracts/engine-loop.md)

**Crates**: `modplayer-effects` (new: `catalog.rs`, `consts.rs`,
`smooth.rs`, `crossfade.rs`, `biquad.rs`, `nodes/{gain,eq,filter,
stereo,stretch,lpc}.rs`, `spectrum.rs`, `rt/{chain,slot,cost}.rs`),
`modplayer-engine` (`command.rs`, `event.rs`, `shared.rs`,
`processor.rs`, `loop_math.rs`, `position_clock.rs`). Implements FR-001,
FR-001a, FR-002 (RT half), FR-005, FR-006, FR-007, FR-008 (RT half),
FR-009, FR-010, FR-011, FR-012, FR-012a, FR-013, FR-014, FR-016 (data
model), FR-018; research R1–R12. Any PR touching
`crates/modplayer-engine/` or `crates/modplayer-effects/` carries a
**real-time safety note** and the engine maintainer's sign-off
(Constitution I / Governance).

## 1. Render order (FR-001)

```
drain_commands
→ plan (ChainRt::plan(needed) → in_frames)
→ fill_from_source(bus_a, in_frames)         # 006's loop/seam/seek logic, unchanged, extracted
→ [pre-chain meter over bus_a]
→ ChainRt::process(bus_a → … → fresh, needed) # nodes in order; stretch stages swap buses
→ master gain → + test tone
→ [post-chain meter + spectrum over fresh]
→ limiter → peak (001) → output stage → carry → clock/anchor
```

Rules:

1. `needed` is unchanged: `output_stage.required_source_frames(out_frames)`.
   The chain always yields exactly `needed` frames.
2. The chain runs at the source rate; the output stage alone converts
   to the device rate (FR-001).
3. An empty chain (or every node bypassed after its fade) is bit-exact
   with 001's pipeline: `bus_a` *is* `fresh` (no copy), proven by
   `empty_chain_is_bit_exact_with_001_pipeline`.
4. `fill_from_source` keeps every 006 rule (segment classification, seam
   capture, `wrap_loop`, `last_wrap` bookkeeping, paused silence). Its
   only change is the destination slice and frame count.

## 2. `Command` delta (all `Copy`, ≤ 16 bytes; drained in full at the boundary)

| Command | Effect on the RT | Rejected (no-op) when |
|---|---|---|
| `ChainInsert { slot, position, kind, owner }` | activates `slot` with `kind`/`owner` at defaults, `mix` fading 0 → 1 over 5 ms, inserts at `min(position, len)` in `order` | slot already active, `len == MAX_NODES`, slot ≥ MAX_NODES |
| `ChainRemove { slot }` | `removing = true`, `mix` fades → 0; the slot leaves `order` and deactivates when the fade ends | slot inactive |
| `ChainMove { slot, position }` | phase 1: `mix` fades → 0 with `pending_move = Some(position)`; when it reaches 0 the slot is relocated in `order` and `mix` fades → 1 (research R6). A move to its current index is a no-op. | slot inactive |
| `ChainSetBypass { slot, bypassed }` | `mix` target = `!bypassed`; `auto_bypassed = false` on un-bypass | slot inactive |
| `ChainSetParam { slot, param, value }` | re-clamps with `catalog::clamp(kind, param, value, source_rate)`; continuous → new ramp target (20 ms from current); discrete → 5 ms crossfade into the new configuration (§4) | slot inactive, unknown param for the kind |
| `Seek(_)`, `Stop` (existing) | additionally `ChainRt::reset_history()` — every stretch ring and voice, biquad states and LPC states cleared; `mix` states and ramps untouched (FR-001a) | — |

Loop wraps (`wrap_loop`) never touch the chain (FR-001a).

Ordering guarantee: commands are applied in queue order at the next
boundary, so `ChainInsert` followed by `ChainSetParam`s (the model's
`replay`) is atomic from the listener's point of view.

## 3. Pull plan and buses (research R3)

```rust
impl ChainRt {
    /// Pass 1 (back to front). Returns the source frames to pull. Never allocates.
    pub fn plan(&mut self, needed: usize) -> usize;
    /// Pass 2. `input` holds `plan()`'s frames; writes exactly `needed` frames into the returned bus.
    pub fn process(&mut self, needed: usize, now: impl Fn() -> Instant) -> &mut [f32];
    pub fn latency_frames(&self) -> u64;   // Σ engaged stretch stages' buffered lead
    pub fn advance_rate(&self) -> f32;     // Π engaged time-stretch ratios; 1.0 otherwise
}
```

| Node | `input_for(out)` | processes |
|---|---|---|
| Gain / Eq / Filter / Stereo | `out` | in place |
| Stretch (engaged) | `max(0, ceil(out × consume) + frame + search − occupancy)` | ring append, then `out` frames to the other bus |
| Stretch (pass-through) | `out` | in place (copy-free) |
| second member of a combined pair | `out` (its stage idles) | nothing; the first member's stage runs with `stretch = p / r`, `step = p` |

Bounds: a plan is clamped to `CHAIN_BUS_FRAMES`; a stage that receives
fewer frames than planned repeats its last grain (documented degenerate
case, ≥ 3 stacked 2.0× stages at 4 096 frames). `Σ plan[0] ==` frames
actually filled (proptest `plan_conserves_frames`).

## 4. Transitions (FR-005, research R6)

| Kind | Length | Curve | Where |
|---|---|---|---|
| continuous parameter | `ceil(0.020 × source_rate)` frames, linear, restart-from-current | `Smoothed::set_target` | per node |
| add / remove / bypass / reorder phase | `ceil(0.005 × source_rate)` frames | `loop_math::crossfade_gains` (equal power) | `NodeSlot::mix` |
| discrete parameter | 5 ms, equal power | dual configuration / shadow biquad / second voice | per node |

Guarantee: at every transition the output's max first-difference stays
≤ `1.5 × baseline` on 006's click metric (`tests/effects_transitions.rs`).
When `transport != Playing` the same fades run over silence, so the
chain is in its final state before audio resumes (spec edge case).

## 5. Node kernels (`modplayer_effects::nodes`)

Each kernel: `fn process(&mut self, buf: &mut [f32], frames: usize, params: &[Smoothed])`
(in place, stereo interleaved) except `StretchStage::process(&mut self,
input: &[f32], in_frames, out: &mut [f32], out_frames)`. All are
`#[inline]`-friendly loops, allocation-free, `f32`.

| Kernel | Math | Identity at defaults |
|---|---|---|
| `GainDsp` | `y = x × 10^(dB/20)`, mute = 5 ms fade to 0 | 0 dB, sample-exact |
| `Eq8Dsp` | 8 × RBJ biquads (peak / low-shelf / high-shelf) per channel; coefficients at 32-frame sub-blocks; shadow biquad per band during a type switch | gain 0 dB ⇒ `b = [1,0,0]`, `a = [1,0,0]`, sample-exact |
| `FilterDsp` | RBJ HP or LP, `Q = Q_BUTTER + res × (Q_MAX − Q_BUTTER)`; mode switch via shadow | HP at 20 Hz, `res = 0`: |H| ≥ −0.01 dB above 40 Hz — *not* sample-exact; SC-013's "sample-exact for filter" is met by the **explicit pass-through short-circuit** the kernel applies while `mode == HP && cutoff == 20 && res == 0` (the defaults), documented as such |
| `StereoDsp` | research R13 order: width (M/S) → balance → mono sum (+invert) → swap | width 1, balance 0, flags off ⇒ sample-exact |
| `StretchStage` | research R4: WSOLA + fractional-delay resampler, two voices, LPC formant | unity ⇒ pass-through, sample-exact |

## 6. Combined stage (FR-007)

Evaluated on every `order` mutation and every bypass change: for each
adjacent `(PitchShift, TimeStretch)` or `(TimeStretch, PitchShift)` pair in
`order` (positions `i`, `i+1`, ignoring bypass state), `combined[i+1] =
true`. The first member's `StretchStage` runs with `p` from the
pitch-shift node (1 if bypassed), `r` from the time-stretch node (1 if
bypassed), `mode = Quality` if either is in quality mode, `formant` from
the pitch node. Its measured time is credited `0.5` to each slot's
`CostRing`. Un-pairing (reorder/remove) hands the second member its own
stage back through the ordinary 5 ms engage seam.

## 7. Metering (FR-011, research R9)

| Value | Computed over | Published |
|---|---|---|
| pre peak/RMS L/R | `bus_a[..in_frames]` after `fill_from_source` (0 when `in_frames == 0`) | every render |
| post peak/RMS L/R | `fresh` after master gain + tone, before limiter | every render |
| spectrum (64 bands, linear magnitude 0–1, log-spaced 20 Hz–20 kHz) | last 1 024 post-chain mono frames, Hann, radix-2 FFT | when ≥ 256 new frames since last; `spectrum_generation += 1` |
| `advance_rate` | `ChainRt::advance_rate()` | with the anchor (seqlock) |

RMS is per render (`sqrt(mean(x²))`); ballistics are the UI's.

## 8. Cost, overload, auto-bypass (FR-012, FR-012a, research R10)

```
period_ns   = out_frames × 1e9 / device_rate
N           = min(1024, ceil(device_rate / out_frames))          # ≈ 1 s of callbacks
node_pct    = rolling mean over N of (node_ns / period_ns × 100)  # per slot; combined stage split ½/½
chain_pct   = Σ active node_pct
render_pct  = (render_end − render_start) / period_ns × 100      # chain + host, this callback

overload event when render_pct >= 100 || (render_pct >= 90 for 3 consecutive callbacks)
on event:  overload_count += 1; over_budget = true; push Event::Overload { costliest_slot, render_pct }
           if slots[costliest].owner != Host && !bypassed && no auto-bypass in the last N callbacks:
               bypassed = auto_bypassed = true (5 ms fade); push Event::AutoBypassed { slot }
clear:     over_budget = false after N consecutive callbacks with render_pct < 90
```

`costliest_slot` ranks **all** active slots (host included — the warning
names it); only the *bypass* is restricted to non-host. A slot's cost
reads 0 until its ring holds ≥ 1 sample and after a render-free window
(no track / stopped ⇒ no callbacks ⇒ the controller shows 0 when
`transport != Playing`, since the RT cannot observe "no callback").

## 9. Position (FR-001a, research R7)

```
lead      = carry_len + chain.latency_frames()
published = if loop active && raw ≥ a { loop_math::rewind_in_loop(raw, lead, a, b) } else { raw.saturating_sub(lead) }
anchor    = (published, now, playing, consumed, advance_rate)
```

`rewind_in_loop(raw, lead, a, b) = a + ((raw − a) + k·(b − a) − lead) mod (b − a)`
for the smallest `k` making the sum non-negative — reduces to 006's
`b − (lead − (raw − a))` when `lead − (raw − a) < b − a`. `PositionClock::
now` extrapolates `elapsed × source_rate × advance_rate`, still capped at
two buffers.

## 10. Real-time safety (Constitution I)

- No allocation, lock, I/O, log or `dyn` call anywhere under `render`;
  `tests/realtime.rs::render_with_full_chain_never_allocates` drives 16
  nodes of every kind through insert/param/move/bypass/remove and seek
  each render under `assert_no_alloc`.
- `Instant::now()` is the only syscall-shaped call (already on the path
  since 001).
- `Processor::new` allocates every buffer (buses, 16 slots' stretch
  buffers, spectrum ring/scratch) on the caller's thread.

## 11. Audio boundary (FR-013, Constitution V)

No public item of `modplayer-effects` or `modplayer-engine` exposes a
sample slice beyond `Processor::render(&mut [f32])` (the device
callback) and the crate-internal kernels; `ChainRt` is `pub` for engine
tests but takes and returns slices the *engine* owns. `modplayer/tests/
decoded_store_boundary.rs` gains `effects_crate_exposes_no_sample_sink`
(scans the public API of both crates for `Write`/`File`/`TcpStream`/
`&[f32]`-returning items) — SC-009.

## 12. Tests pinning this contract

| Test (crate · file) | Pins |
|---|---|
| `effects · catalog.rs` proptests `clamp_is_idempotent_in_range_and_monotone`, `nyquist_clamp_tracks_rate`, `defaults_are_in_range` | FR-006, FR-010, SC-005 |
| `effects · mode_rule.rs` `auto_switch_table` | FR-008 |
| `effects · smooth.rs` `ramp_reaches_target_in_exactly_ramp_frames`, `retarget_restarts_from_current` | FR-010 |
| `effects · crossfade.rs` `equal_power_and_exact_endpoints` | FR-005 |
| `effects · nodes/*` `gain_eq_filter_stereo_identity_at_defaults` (sample-exact), `eq_peak_plus_6db_at_1khz`, `filter_hp_minus_3db_at_cutoff`, `resonance_caps_at_q_max`, `stereo_truth_table`, `mono_sum_invert_cancels_centre` | FR-006, FR-009, SC-013, US3 |
| `effects · nodes/stretch.rs` `unity_is_pass_through`, `tempo_half_doubles_click_interval`, `pitch_up_7_keeps_duration`, `combined_stage_products`, `formant_on_keeps_envelope_peaks`, `quality_mode_uses_cubic` | US1 AS1–AS6, FR-007 |
| `effects · rt/chain.rs` `plan_conserves_frames` (proptest, 16 nodes any order), `combined_cost_split_half`, `starved_stage_repeats_not_silence` | FR-007, FR-012a, R3 |
| `engine · tests/effects_transitions.rs` `every_edit_and_switch_is_click_free` (table over add/remove/move/bypass × every kind, every discrete param) | FR-005, SC-002, SC-007 |
| `engine · tests/effects_latency.rs` `ratio_change_audible_within_20ms_p95` (128 frames, 44.1 kHz, click train, 200 trials) | SC-001, NFR-1.1 |
| `engine · tests/effects_position.rs` `position_advances_at_ratio_with_loop`, `seek_resets_history_wrap_does_not`, `lead_is_subtracted` | FR-001a, SC-011 |
| `engine · tests/effects_overload.rs` `overload_counts_and_names_costliest`, `non_host_costliest_is_bypassed_exactly_once_per_window`, `host_costliest_is_never_bypassed`, `over_budget_clears_after_clean_window` | FR-012, SC-004 |
| `engine · tests/effects_meters.rs` `pre_ignores_nodes_and_master_post_reflects_both`, `spectrum_peaks_in_expected_band` | FR-011, SC-008 |
| `engine · tests/realtime.rs` `render_with_full_chain_never_allocates` | Constitution I |
| `engine · tests/boundary.rs` `chain_commands_apply_at_next_boundary` | FR-005 |
| `engine · tests/effects_pipeline.rs` `empty_chain_is_bit_exact_with_001_pipeline`, `rate_change_rebuild_recomputes_coefficients` | FR-001, FR-014 |
| `engine · src/loop_math.rs` proptest `rewind_in_loop_lands_in_region` | R7 |
| `engine · tests/soak.rs` `reference_chain_soak_60s` (CI), `reference_chain_soak_24h` (`#[ignore = "manual"]`), `reference_chain_under_half_core` (`#[ignore = "manual"]`, release) | FR-018, NFR-2.7, SC-003 |
| `effects · benches/{nodes,reference_chain}.rs` | Constitution VIII |

// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ChainRt`: the real-time copy of the effect chain, owned by
//! `Processor` (data-model.md §3.2, contracts/engine-effect-chain.md).
//! Every buffer is preallocated at construction; `render`-path methods
//! (`plan`, `process`, the `apply_*` command handlers) never allocate.
//!
//! The two-pass pull plan (research R3, contracts/engine-effect-chain.md
//! §3): `plan(needed)` walks the processing order back-to-front, asking
//! each node how many input frames it needs to produce however many
//! output frames the node after it demanded (in-place kernels: `out`
//! unchanged; an engaged `Stretch` node: `StretchStage::input_for`).
//! `plan[0]` — what falls out the front — is how many frames the caller
//! must fill into `input_buffer_mut` before calling `process`, which
//! then walks the same order front-to-back, ping-ponging between
//! `bus_a`/`bus_b` at every node whose frame count actually changes.

use std::time::Instant;

use crate::catalog::{self, NodeKind, NodeOwner, ParamId, QualityMode};
use crate::consts::{
    CHAIN_BUS_FRAMES, COST_RING_CAPACITY, MAX_NODES, OVERLOAD_PCT, OVERLOAD_STREAK, PARAM_RAMP_MS,
    SWITCH_CROSSFADE_MS,
};
use crate::nodes::stretch::{StretchParams, params_from_ratio, params_from_semitones};
use crate::rt::slot::{MAX_PARAMS, NodeDsp, NodeSlot};

/// The real-time effect chain: a 16-slot pool plus the processing order
/// (data-model.md §3.2).
pub struct ChainRt {
    slots: [NodeSlot; MAX_NODES],
    /// Slot indices in processing order; only `order[..order_len]` is
    /// meaningful.
    order: [u8; MAX_NODES],
    order_len: usize,
    /// Ping-pong buses for a rate-changing kernel's pull plan (research
    /// R3): the input is filled into whichever bus `input_buffer_mut`
    /// names; `process` swaps buses at every frame-count-changing node.
    bus_a: Vec<f32>,
    bus_b: Vec<f32>,
    /// Preallocated scratch holding a slot's pre-kernel ("dry") samples
    /// while its `mix` is mid-crossfade, so wet/dry blending never
    /// allocates on the render path.
    mix_scratch: Vec<f32>,
    /// Per-render frame counts, back-to-front by processing order
    /// (research R3): `plan[i]` is how many frames flow *into*
    /// `order[i]`; `plan[order_len]` is the render's fixed `needed`
    /// output count.
    plan: [u32; MAX_NODES + 1],
    source_rate: u32,
    /// FR-012/research R10: consecutive callbacks with `render_pct >= 90`
    /// (an overload event fires at 3) — reset on any callback below 90.
    over_90_streak: u32,
    /// Consecutive callbacks with `render_pct < 90` while `over_budget`
    /// — clears the flag once this reaches `COST_RING_CAPACITY` (≈ 1 s of
    /// callbacks, reusing that same "about one second" constant per
    /// contracts/engine-effect-chain.md §8's `N`).
    under_90_streak: u32,
    /// `true` from the first `Overload` event in the current excursion
    /// until a clean window clears it — FR-012's non-host auto-bypass
    /// fires at most once per excursion, not once per render.
    over_budget: bool,
    auto_bypassed_this_window: bool,
    overload_count: u32,
}

/// [`ChainRt::record_render_pct`]'s result for the caller (`Processor`)
/// to turn into `Event`s and `RtShared` updates (contracts/engine-
/// effect-chain.md §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverloadOutcome {
    /// This render's whole-callback percentage crossed the overload
    /// threshold — the caller must push `Event::Overload`.
    pub overload: bool,
    /// The costliest active slot, host included (only meaningful when
    /// `overload` is set and at least one slot is active).
    pub costliest_slot: u8,
    /// This render's `render_pct`, rounded, for `Event::Overload`.
    pub render_pct: u16,
    /// `Some(slot)` when `costliest_slot` was just auto-bypassed (always
    /// a non-host slot) — the caller must push `Event::AutoBypassed`.
    pub auto_bypassed_slot: Option<u8>,
}

impl ChainRt {
    /// A chain with every slot free, sized for `source_rate` (used to
    /// convert `PARAM_RAMP_MS`/`SWITCH_CROSSFADE_MS` to frame counts).
    #[must_use]
    pub fn new(source_rate: u32) -> Self {
        Self {
            slots: std::array::from_fn(|_| NodeSlot::inactive()),
            order: [0; MAX_NODES],
            order_len: 0,
            bus_a: vec![0.0; CHAIN_BUS_FRAMES * 2],
            bus_b: vec![0.0; CHAIN_BUS_FRAMES * 2],
            mix_scratch: vec![0.0; CHAIN_BUS_FRAMES * 2],
            plan: [0; MAX_NODES + 1],
            source_rate: source_rate.max(1),
            over_90_streak: 0,
            under_90_streak: 0,
            over_budget: false,
            auto_bypassed_this_window: false,
            overload_count: 0,
        }
    }

    /// Re-target ramp/crossfade timing to a new source rate (a fresh
    /// `Processor`/`ChainRt` is built per stream anyway — this exists for
    /// completeness and for tests that probe timing directly).
    pub fn set_source_rate(&mut self, source_rate: u32) {
        self.source_rate = source_rate.max(1);
    }

    #[must_use]
    pub const fn source_rate(&self) -> u32 {
        self.source_rate
    }

    fn ramp_frames(&self) -> u32 {
        ((PARAM_RAMP_MS / 1000.0) * self.source_rate as f32).ceil() as u32
    }

    fn crossfade_frames(&self) -> u32 {
        ((SWITCH_CROSSFADE_MS / 1000.0) * self.source_rate as f32).ceil() as u32
    }

    /// The slot indices currently in processing order.
    #[must_use]
    pub fn order(&self) -> &[u8] {
        &self.order[..self.order_len]
    }

    /// Read a slot by its RT slot index (`0..MAX_NODES`).
    #[must_use]
    pub fn slot(&self, index: usize) -> &NodeSlot {
        &self.slots[index]
    }

    // -- Pull plan (research R3) --------------------------------------

    /// This render's effective `StretchParams` for the node at
    /// `order[order_i]`: solo (its own catalog params) or, when it is
    /// the *first* member of an adjacent combined pitch/stretch pair
    /// (FR-007), the product of both members' params (a bypassed member
    /// contributes identity). The *second* member of a pair (`
    /// combined_second`) is never asked — its own kernel idles.
    fn stretch_params_for(&self, order_i: usize) -> StretchParams {
        let slot_idx = self.order[order_i] as usize;
        let partner_idx = if order_i + 1 < self.order_len
            && self.slots[self.order[order_i + 1] as usize].combined_second
        {
            Some(self.order[order_i + 1] as usize)
        } else {
            None
        };
        if let Some(partner_idx) = partner_idx {
            let mut semitones = 0.0f32;
            let mut formant = false;
            let mut pitch_quality = QualityMode::Performance;
            let mut ratio = 1.0f32;
            let mut stretch_quality = QualityMode::Performance;
            for &idx in &[slot_idx, partner_idx] {
                let s = &self.slots[idx];
                if s.bypassed {
                    continue;
                }
                match s.kind {
                    NodeKind::PitchShift => {
                        semitones = s.params[0].current;
                        formant = s.params[1].current >= 0.5;
                        pitch_quality = quality_from(s.params[2].current);
                    }
                    NodeKind::TimeStretch => {
                        ratio = s.params[0].current;
                        stretch_quality = quality_from(s.params[1].current);
                    }
                    _ => {}
                }
            }
            StretchParams::combined(semitones, ratio, formant, pitch_quality, stretch_quality)
        } else {
            let s = &self.slots[slot_idx];
            match s.kind {
                NodeKind::PitchShift => params_from_semitones(
                    s.params[0].current,
                    s.params[1].current >= 0.5,
                    s.params[2].current,
                ),
                NodeKind::TimeStretch => {
                    params_from_ratio(s.params[0].current, s.params[1].current)
                }
                _ => StretchParams {
                    stretch: 1.0,
                    step: 1.0,
                    formant: false,
                    quality: QualityMode::Performance,
                },
            }
        }
    }

    /// How many input frames the node at `order[order_i]` needs to
    /// produce `out` output frames this render (contracts/engine-
    /// effect-chain.md §3). A bypassed-and-settled node, the idling
    /// second member of a combined pair, and every in-place kernel all
    /// need exactly `out`.
    fn input_for_order(&self, order_i: usize, out: usize) -> usize {
        let slot_idx = self.order[order_i] as usize;
        let slot = &self.slots[slot_idx];
        let NodeDsp::Stretch(stage) = &slot.dsp else {
            return out;
        };
        if slot.combined_second || (slot.bypassed && slot.mix.is_settled()) {
            return out;
        }
        let params = self.stretch_params_for(order_i);
        stage.input_for(out, &params)
    }

    /// Pass 1 (back to front): how many source frames this render needs,
    /// and every intermediate node's own input requirement, into `plan`.
    /// Never allocates.
    pub fn plan(&mut self, needed: usize) -> usize {
        let needed = needed.min(CHAIN_BUS_FRAMES);
        self.plan[self.order_len] = needed as u32;
        let mut demand = needed;
        for i in (0..self.order_len).rev() {
            let in_frames = self.input_for_order(i, demand).min(CHAIN_BUS_FRAMES);
            self.plan[i] = in_frames as u32;
            demand = in_frames;
        }
        if self.order_len == 0 {
            self.plan[0] = needed as u32;
        }
        self.plan[0] as usize
    }

    /// The buffer `plan`'s result must be filled into before `process`
    /// (research R3's bus A) — sized to exactly `frames` (`plan(needed)`'s
    /// return value).
    pub fn input_buffer_mut(&mut self, frames: usize) -> &mut [f32] {
        let frames = frames.min(CHAIN_BUS_FRAMES);
        &mut self.bus_a[..frames * 2]
    }

    /// Pass 2 (front to back): run every active node over the bus
    /// `input_buffer_mut` was filled into, ping-ponging at every node
    /// whose frame count changes, and return exactly `needed` output
    /// frames (contracts/engine-effect-chain.md §3). `plan` must have
    /// been called (with this same `needed`) immediately before.
    pub fn process(&mut self, needed: usize, now: impl Fn() -> Instant) -> &mut [f32] {
        let needed = needed.min(CHAIN_BUS_FRAMES);
        self.resolve_one_pending_move();

        // FR-012a: this render's own nominal period, at the source rate —
        // the effects crate has no device-rate dependency (research R1),
        // so per-node costs are a percentage of *this* render's source-
        // frame duration rather than the true device callback period; the
        // caller (`Processor`, which knows the device rate) computes the
        // whole-render `render_pct` that actually drives the overload
        // state machine (`record_render_pct`) from real wall time
        // separately.
        let period_ns =
            f64::from(self.plan[self.order_len]) * 1e9 / f64::from(self.source_rate.max(1));

        let mut current_is_a = true;
        let mut current_len = self.plan[0] as usize;
        let mut write = 0usize;

        for read in 0..self.order_len {
            let slot_idx = self.order[read] as usize;
            let out_frames = self.plan[read + 1] as usize;
            let in_frames = current_len;

            let params = self.stretch_params_for(read);
            let combined_carrier = read + 1 < self.order_len && {
                let next = self.order[read + 1] as usize;
                self.slots[next].combined_second
            };
            let is_combined_second = self.slots[slot_idx].combined_second;

            let (src, dst) = if current_is_a {
                let (a, b) = (&mut self.bus_a, &mut self.bus_b);
                (a, b)
            } else {
                let (b, a) = (&mut self.bus_b, &mut self.bus_a);
                (b, a)
            };

            let t0 = now();
            let keep = process_one_slot(
                &mut self.slots[slot_idx],
                src,
                in_frames,
                dst,
                out_frames,
                &mut self.mix_scratch,
                &params,
                combined_carrier,
                self.source_rate,
            );
            // Debug/test-only cost inflation (`NodeSlot::burn_ns`,
            // contracts/engine-effect-chain.md §12): a plain busy-wait —
            // no allocation, lock, I/O or log, so it stays real-time
            // safe even at `0` (one comparison) on every other path.
            let burn_ns = self.slots[slot_idx].burn_ns;
            if burn_ns > 0 {
                let spin_start = now();
                while now().duration_since(spin_start).as_nanos() < u128::from(burn_ns) {}
            }
            // FR-012a: per-node cost as a percentage of `period_ns`. A
            // combined pair's measured time is the *carrier's* real WSOLA
            // work — it splits half/half with its idling second member
            // (contracts/engine-effect-chain.md §6); the second member's
            // own trivial passthrough call is never separately timed (it
            // was already credited by the carrier the iteration before).
            if period_ns > 0.0 && !is_combined_second {
                let elapsed_ns = now().duration_since(t0).as_nanos() as f64;
                let pct = (elapsed_ns / period_ns * 100.0) as f32;
                if combined_carrier {
                    let half = pct * 0.5;
                    self.slots[slot_idx].cost.push(half);
                    let partner = self.order[read + 1] as usize;
                    self.slots[partner].cost.push(half);
                } else {
                    self.slots[slot_idx].cost.push(pct);
                }
            }

            // Every node's *result* lands in `dst` (the bus opposite
            // `src`), whether or not its frame count changed — an
            // in-place kernel still gets copied src -> dst inside
            // `process_one_slot` rather than mutated through a shared
            // reference (the pull plan's ping-pong is per-*node*, not
            // conditional on the frame count actually differing).
            current_is_a = !current_is_a;
            current_len = out_frames;

            if keep {
                self.order[write] = slot_idx as u8;
                write += 1;
            } else {
                self.slots[slot_idx].deactivate();
            }
        }
        self.order_len = write;
        self.recompute_combined();

        let final_buf = if current_is_a {
            &mut self.bus_a
        } else {
            &mut self.bus_b
        };
        &mut final_buf[..needed.min(current_len) * 2]
    }

    // -- Commands (contracts/engine-effect-chain.md §2) ----------------

    /// `ChainInsert`: activates `slot` at `kind`/`owner`'s catalog
    /// defaults, fading in over the 5 ms crossfade, inserted at
    /// `min(position, len)`. A no-op when `slot` is out of range,
    /// already active, or the chain is full.
    pub fn apply_insert(&mut self, slot: u8, position: u8, kind: NodeKind, owner: NodeOwner) {
        let slot_idx = slot as usize;
        if slot_idx >= MAX_NODES || self.slots[slot_idx].active || self.order_len >= MAX_NODES {
            return;
        }
        let crossfade_frames = self.crossfade_frames();
        self.slots[slot_idx].activate(kind, owner, crossfade_frames, self.source_rate);
        let pos = (position as usize).min(self.order_len);
        for i in (pos..self.order_len).rev() {
            self.order[i + 1] = self.order[i];
        }
        self.order[pos] = slot;
        self.order_len += 1;
        self.recompute_combined();
    }

    /// `ChainRemove`: fades `slot` to dry over the 5 ms crossfade; it
    /// leaves `order` and deactivates once that fade completes (in
    /// `process`). A no-op for an inactive slot.
    pub fn apply_remove(&mut self, slot: u8) {
        let slot_idx = slot as usize;
        if slot_idx >= MAX_NODES || !self.slots[slot_idx].active {
            return;
        }
        let crossfade_frames = self.crossfade_frames();
        self.slots[slot_idx].removing = true;
        self.slots[slot_idx].mix.set_target(false, crossfade_frames);
    }

    /// `ChainMove` (research R6, two-phase reorder): phase 1 fades `slot`
    /// to dry with `pending_move` set; `process` executes the actual
    /// reposition and fades back in once that fade settles. A no-op for
    /// an inactive slot or a move to its current index.
    pub fn apply_move(&mut self, slot: u8, position: u8) {
        let slot_idx = slot as usize;
        if slot_idx >= MAX_NODES || !self.slots[slot_idx].active {
            return;
        }
        let Some(cur) = self.order[..self.order_len]
            .iter()
            .position(|&s| s as usize == slot_idx)
        else {
            return;
        };
        let target = (position as usize).min(self.order_len.saturating_sub(1));
        if target == cur {
            return;
        }
        let crossfade_frames = self.crossfade_frames();
        self.slots[slot_idx].pending_move = Some(target as u8);
        self.slots[slot_idx].mix.set_target(false, crossfade_frames);
    }

    /// `ChainSetBypass`: mix target follows `!bypassed`; un-bypassing
    /// clears `auto_bypassed`. A no-op for an inactive slot.
    pub fn apply_set_bypass(&mut self, slot: u8, bypassed: bool) {
        let slot_idx = slot as usize;
        if slot_idx >= MAX_NODES || !self.slots[slot_idx].active {
            return;
        }
        let crossfade_frames = self.crossfade_frames();
        let s = &mut self.slots[slot_idx];
        s.bypassed = bypassed;
        if !bypassed {
            s.auto_bypassed = false;
        }
        s.mix.set_target(!bypassed, crossfade_frames);
        self.recompute_combined();
    }

    /// `ChainSetParam`: re-clamps with `catalog::clamp` and retargets the
    /// parameter's ramp (continuous, 20 ms) or crossfades it (discrete —
    /// `GainDsp`'s own mute fade, or a `Stretch` node's formant/mode
    /// two-voice switch, research R6). A no-op for an inactive slot or a
    /// param the kind lacks.
    pub fn apply_set_param(&mut self, slot: u8, param: ParamId, value: f32) {
        let slot_idx = slot as usize;
        if slot_idx >= MAX_NODES || !self.slots[slot_idx].active {
            return;
        }
        let kind = self.slots[slot_idx].kind;
        if !catalog::params(kind).iter().any(|p| p.id == param) {
            return;
        }
        let idx = param.0 as usize;
        if idx >= MAX_PARAMS {
            return;
        }
        let clamped = catalog::clamp(kind, param, value, self.source_rate);
        let continuous = catalog::is_continuous(kind, param);
        let ramp = self.ramp_frames();
        let crossfade_frames = self.crossfade_frames();
        let s = &mut self.slots[slot_idx];
        if continuous {
            s.params[idx].set_target(clamped, ramp);
        } else {
            s.params[idx].set_target(clamped, 0);
            match (kind, idx, &mut s.dsp) {
                (NodeKind::Gain, 1, NodeDsp::Gain(gain)) => {
                    gain.set_muted(clamped >= 0.5, crossfade_frames);
                }
                (NodeKind::PitchShift, 1 | 2, NodeDsp::Stretch(stage)) => {
                    let formant = s.params[1].current >= 0.5;
                    let quality = quality_from(s.params[2].current);
                    let (cur_formant, cur_quality) = stage.active_config();
                    if formant != cur_formant || quality != cur_quality {
                        stage.switch_voice(
                            &mut s.stretch_buffers,
                            formant,
                            quality,
                            crossfade_frames,
                        );
                    }
                }
                (NodeKind::TimeStretch, 1, NodeDsp::Stretch(stage)) => {
                    let quality = quality_from(s.params[1].current);
                    let (cur_formant, cur_quality) = stage.active_config();
                    if quality != cur_quality {
                        stage.switch_voice(
                            &mut s.stretch_buffers,
                            cur_formant,
                            quality,
                            crossfade_frames,
                        );
                    }
                }
                (NodeKind::Equalizer, i, NodeDsp::Eq(eq)) if i >= 16 && (i - 16) % 4 == 3 => {
                    let band = (i - 16) / 4;
                    eq.switch_band_type(band, clamped as u8, crossfade_frames);
                }
                (NodeKind::Filter, 0, NodeDsp::Filter(filter)) => {
                    filter.switch_mode(clamped as u8, crossfade_frames);
                }
                (NodeKind::StereoTools, 2..=4, NodeDsp::Stereo(stereo)) => {
                    let mono_sum = s.params[2].current >= 0.5;
                    let phase_invert = s.params[3].current >= 0.5;
                    let channel_swap = s.params[4].current >= 0.5;
                    stereo.set_flags_target(mono_sum, phase_invert, channel_swap, crossfade_frames);
                }
                _ => {}
            }
        }
    }

    /// Σ every engaged stretch carrier's buffered lead, in source frames
    /// (research R7). A combined pair's second member never contributes
    /// (its own kernel idles).
    #[must_use]
    pub fn latency_frames(&self) -> u64 {
        let mut total = 0u64;
        for i in 0..self.order_len {
            let slot_idx = self.order[i] as usize;
            let slot = &self.slots[slot_idx];
            if slot.combined_second {
                continue;
            }
            if let NodeDsp::Stretch(stage) = &slot.dsp {
                let params = self.stretch_params_for(i);
                total += stage.latency_frames(&params);
            }
        }
        total
    }

    /// Π every engaged carrier's time-stretch consume ratio (research
    /// R7) — `1.0` for a pitch-only stage, the tempo ratio for a
    /// time-stretch (solo or combined) one.
    #[must_use]
    pub fn advance_rate(&self) -> f32 {
        let mut rate = 1.0f32;
        for i in 0..self.order_len {
            let slot_idx = self.order[i] as usize;
            let slot = &self.slots[slot_idx];
            if slot.combined_second || slot.bypassed {
                continue;
            }
            if matches!(slot.dsp, NodeDsp::Stretch(_)) {
                rate *= self.stretch_params_for(i).consume_ratio();
            }
        }
        rate
    }

    /// Clears every stretch ring/voice, biquad and LPC state on
    /// `Seek`/`Stop` (FR-001a).
    pub fn reset_history(&mut self) {
        for slot in &mut self.slots {
            if let NodeDsp::Stretch(stage) = &mut slot.dsp {
                stage.reset_history();
            }
        }
    }

    /// Debug/test-only hook (contracts/engine-effect-chain.md §12,
    /// `effects_overload.rs`; not wired to any env var in the binary —
    /// quickstart.md M13 builds a genuinely heavy chain instead): inflate
    /// `slot`'s measured per-render cost by `ns` nanoseconds. A no-op for
    /// an out-of-range or inactive slot.
    pub fn set_burn_ns(&mut self, slot: u8, ns: u64) {
        if let Some(s) = self.slots.get_mut(slot as usize) {
            s.burn_ns = ns;
        }
    }

    /// The last `record_render_pct` count of overload events (FR-012,
    /// NFR-8.2's UI counter).
    #[must_use]
    pub const fn overload_count(&self) -> u32 {
        self.overload_count
    }

    /// Whether the chain is currently in an overload excursion (FR-012):
    /// set on the first `Overload` event, cleared after a clean window.
    #[must_use]
    pub const fn over_budget(&self) -> bool {
        self.over_budget
    }

    /// The active slot (host included) with the highest rolling-mean cost
    /// (contracts/engine-effect-chain.md §8: "ranks all active slots");
    /// `None` for an empty chain.
    fn costliest_active_slot(&self) -> Option<usize> {
        self.order[..self.order_len]
            .iter()
            .map(|&s| s as usize)
            .max_by(|&a, &b| {
                self.slots[a]
                    .cost
                    .mean()
                    .partial_cmp(&self.slots[b].cost.mean())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// FR-012/FR-012a, research R10: the whole-render overload state
    /// machine (contracts/engine-effect-chain.md §8). `render_pct` is
    /// this render's whole-callback percentage of the *device* period
    /// (chain + host) — computed by the caller (`Processor`, the only
    /// place that knows the device rate) from real wall-clock time, and
    /// handed in here once per render, after `process()`. Never
    /// allocates.
    pub fn record_render_pct(&mut self, render_pct: f32) -> OverloadOutcome {
        if render_pct >= OVERLOAD_PCT {
            self.over_90_streak += 1;
            self.under_90_streak = 0;
        } else {
            self.over_90_streak = 0;
            self.under_90_streak = self.under_90_streak.saturating_add(1);
        }

        let mut outcome = OverloadOutcome {
            overload: false,
            costliest_slot: 0,
            render_pct: render_pct.round().clamp(0.0, f32::from(u16::MAX)) as u16,
            auto_bypassed_slot: None,
        };

        if render_pct >= 100.0 || self.over_90_streak >= OVERLOAD_STREAK {
            outcome.overload = true;
            self.overload_count += 1;
            self.over_budget = true;
            if let Some(slot_idx) = self.costliest_active_slot() {
                outcome.costliest_slot = slot_idx as u8;
                let bypassable = !self.auto_bypassed_this_window
                    && !self.slots[slot_idx].owner.is_host()
                    && !self.slots[slot_idx].bypassed;
                if bypassable {
                    let crossfade_frames = self.crossfade_frames();
                    self.slots[slot_idx].bypassed = true;
                    self.slots[slot_idx].auto_bypassed = true;
                    self.slots[slot_idx].mix.set_target(false, crossfade_frames);
                    self.auto_bypassed_this_window = true;
                    self.recompute_combined();
                    outcome.auto_bypassed_slot = Some(slot_idx as u8);
                }
            }
        } else if self.over_budget && self.under_90_streak >= COST_RING_CAPACITY as u32 {
            self.over_budget = false;
            self.auto_bypassed_this_window = false;
        }
        outcome
    }

    /// FR-007: recompute which slots are the *second* member of an
    /// adjacent (ignoring bypass) pitch/stretch pair — run on every
    /// `order` mutation and bypass change (contracts/engine-effect-
    /// chain.md §6).
    fn recompute_combined(&mut self) {
        for i in 0..self.order_len {
            self.slots[self.order[i] as usize].combined_second = false;
        }
        let mut i = 0usize;
        while i + 1 < self.order_len {
            let a = self.order[i] as usize;
            let b = self.order[i + 1] as usize;
            let pair = matches!(
                (self.slots[a].kind, self.slots[b].kind),
                (NodeKind::PitchShift, NodeKind::TimeStretch)
                    | (NodeKind::TimeStretch, NodeKind::PitchShift)
            );
            if pair {
                self.slots[b].combined_second = true;
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    /// Executes at most one settled reorder-phase-1 fade per call
    /// (research R6): the slot leaves its old position and starts fading
    /// back in at the new one. Simultaneous completions (rare — a
    /// crossfade spans many renders) resolve one per render rather than
    /// risking a stale index after reordering `order` mid-scan.
    fn resolve_one_pending_move(&mut self) {
        for i in 0..self.order_len {
            let slot_idx = self.order[i] as usize;
            let slot = &self.slots[slot_idx];
            if slot.pending_move.is_none() || !slot.mix.is_settled() {
                continue;
            }
            let target = slot
                .pending_move
                .unwrap_or(0)
                .min(self.order_len.saturating_sub(1) as u8) as usize;
            self.slots[slot_idx].pending_move = None;
            self.move_slot_to(slot_idx, target);
            let crossfade_frames = self.crossfade_frames();
            self.slots[slot_idx].mix.set_target(true, crossfade_frames);
            self.recompute_combined();
            return;
        }
    }

    fn move_slot_to(&mut self, slot_idx: usize, target: usize) {
        let Some(cur) = self.order[..self.order_len]
            .iter()
            .position(|&s| s as usize == slot_idx)
        else {
            return;
        };
        let target = target.min(self.order_len.saturating_sub(1));
        if target == cur {
            return;
        }
        let val = self.order[cur];
        if target > cur {
            for i in cur..target {
                self.order[i] = self.order[i + 1];
            }
        } else {
            for i in (target + 1..=cur).rev() {
                self.order[i] = self.order[i - 1];
            }
        }
        self.order[target] = val;
    }
}

fn quality_from(mode_value: f32) -> QualityMode {
    if mode_value >= 0.5 {
        QualityMode::Quality
    } else {
        QualityMode::Performance
    }
}

/// Runs one active slot's kernel over `src`'s first `in_frames` frames,
/// writing `out_frames` frames into `dst` (equal to `in_frames` for
/// every in-place kernel), blending dry/wet through `mix_scratch` while
/// `slot.mix` is unsettled. A free function (not a method) so its
/// disjoint borrows of `slot` and the buses/`mix_scratch` — different
/// fields of `ChainRt` — can coexist. `combined_carrier`: this slot is
/// the first member of an adjacent combined pair this render, so its own
/// `bypassed` flag must not short-circuit processing (the pair's active
/// member may need it to keep running). Returns whether `slot` stays
/// active (`false` once a `removing` fade settles).
#[allow(clippy::too_many_arguments)]
fn process_one_slot(
    slot: &mut NodeSlot,
    src: &mut [f32],
    in_frames: usize,
    dst: &mut [f32],
    out_frames: usize,
    mix_scratch: &mut [f32],
    params: &StretchParams,
    combined_carrier: bool,
    source_rate: u32,
) -> bool {
    if !slot.active {
        return false;
    }
    let was_mixing = !slot.mix.is_settled();
    if slot.bypassed && !was_mixing && !combined_carrier {
        // Fully dry and settled: copy through unchanged (in-place kernels
        // never change frame count while bypassed; a bypassed-and-
        // settled Stretch's `input_for`/`plan` already returned `out`).
        let n = in_frames.min(out_frames);
        dst[..n * 2].copy_from_slice(&src[..n * 2]);
        return true;
    }

    let dry_src: &[f32] = if was_mixing {
        mix_scratch[..in_frames * 2].copy_from_slice(&src[..in_frames * 2]);
        &mix_scratch[..in_frames * 2]
    } else {
        src
    };

    if matches!(slot.dsp, NodeDsp::Stretch(_)) {
        advance_stretch_params(slot, out_frames);
    }
    let combined_second = slot.combined_second;

    match &mut slot.dsp {
        NodeDsp::Gain(gain) => {
            let n = in_frames.min(out_frames);
            dst[..n * 2].copy_from_slice(&src[..n * 2]);
            let mut level = slot.params[0];
            gain.process(&mut dst[..n * 2], n, &mut level);
            slot.params[0] = level;
        }
        NodeDsp::Eq(eq) => {
            let n = in_frames.min(out_frames);
            dst[..n * 2].copy_from_slice(&src[..n * 2]);
            eq.process(&mut dst[..n * 2], n, &mut slot.params[..], source_rate);
        }
        NodeDsp::Filter(filter) => {
            let n = in_frames.min(out_frames);
            dst[..n * 2].copy_from_slice(&src[..n * 2]);
            filter.process(&mut dst[..n * 2], n, &mut slot.params[..], source_rate);
        }
        NodeDsp::Stereo(stereo) => {
            let n = in_frames.min(out_frames);
            dst[..n * 2].copy_from_slice(&src[..n * 2]);
            stereo.process(&mut dst[..n * 2], n, &mut slot.params[..]);
        }
        NodeDsp::Stretch(stage) => {
            if combined_second {
                // Idle: the carrier already ran with the combined
                // product parameters; this node's own kernel produces no
                // extra delay or cost (contracts/engine-effect-chain.md
                // §6). Straight passthrough of whatever landed on `src`.
                let n = in_frames.min(out_frames);
                dst[..n * 2].copy_from_slice(&src[..n * 2]);
            } else {
                let buffers = &mut *slot.stretch_buffers;
                if !params.is_unity() {
                    stage.append(buffers, src, in_frames);
                }
                stage.process(buffers, src, dst, out_frames, params);
            }
        }
    }

    if was_mixing {
        for f in 0..out_frames.min(dry_src.len() / 2) {
            let (dry, wet) = slot.mix.gains();
            dst[f * 2] = dry_src[f * 2] * dry + dst[f * 2] * wet;
            dst[f * 2 + 1] = dry_src[f * 2 + 1] * dry + dst[f * 2 + 1] * wet;
            slot.mix.advance();
        }
    }
    !(slot.removing && slot.mix.is_settled())
}

/// Advance a `Stretch` node's own continuous catalog params (semitones/
/// ratio and, indirectly via their shared `Smoothed`, nothing else — mode
/// and formant are discrete) by `frames` (FR-010's 20 ms ramp) — called
/// once per render rather than per sample (a coarser but allocation-free
/// approximation appropriate to the stage's own per-render parameter
/// snapshot).
fn advance_stretch_params(slot: &mut NodeSlot, frames: usize) {
    let indices: &[usize] = match slot.kind {
        NodeKind::PitchShift => &[0],
        NodeKind::TimeStretch => &[0],
        _ => &[],
    };
    for &i in indices {
        for _ in 0..frames {
            let _ = slot.params[i].advance();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(chain: &mut ChainRt, frames: usize, renders: usize) {
        for _ in 0..renders {
            let planned = chain.plan(frames);
            let input = chain.input_buffer_mut(planned);
            input.fill(0.5);
            let _ = chain.process(frames, Instant::now);
        }
    }

    #[test]
    fn empty_chain_process_leaves_buffer_untouched() {
        let mut chain = ChainRt::new(44_100);
        let planned = chain.plan(2);
        assert_eq!(planned, 2);
        let input = chain.input_buffer_mut(planned);
        input.copy_from_slice(&[0.11f32, -0.22, 0.33, -0.44]);
        let out = chain.process(2, Instant::now);
        assert_eq!(out, &[0.11f32, -0.22, 0.33, -0.44]);
    }

    #[test]
    fn insert_remove_settles_and_frees_the_slot() {
        let mut chain = ChainRt::new(44_100);
        chain.apply_insert(0, 0, NodeKind::Gain, NodeOwner::Host);
        assert_eq!(chain.order(), &[0]);
        // Drain the 5 ms insert fade-in.
        drain(&mut chain, 256, 50);
        assert!(chain.slot(0).mix.is_settled());

        chain.apply_remove(0);
        drain(&mut chain, 256, 50);
        assert!(
            chain.order().is_empty(),
            "slot must leave order once removed"
        );
        assert!(
            !chain.slot(0).active,
            "slot must free itself once its fade settles"
        );
    }

    #[test]
    fn move_reorders_two_nodes() {
        let mut chain = ChainRt::new(44_100);
        chain.apply_insert(0, 0, NodeKind::Gain, NodeOwner::Host);
        chain.apply_insert(1, 1, NodeKind::Gain, NodeOwner::Host);
        drain(&mut chain, 256, 50);
        assert_eq!(chain.order(), &[0, 1]);

        chain.apply_move(0, 1);
        drain(&mut chain, 256, 50);
        assert_eq!(chain.order(), &[1, 0]);
    }

    #[test]
    fn set_param_clamps_and_ramps() {
        let mut chain = ChainRt::new(44_100);
        chain.apply_insert(0, 0, NodeKind::Gain, NodeOwner::Host);
        chain.apply_set_param(0, ParamId(0), 999.0);
        // Clamped to Gain's max (12 dB) as the new ramp target.
        assert_eq!(chain.slot(0).params[0].target, 12.0);
    }

    #[test]
    fn set_param_on_unknown_param_is_noop() {
        let mut chain = ChainRt::new(44_100);
        chain.apply_insert(0, 0, NodeKind::Gain, NodeOwner::Host);
        chain.apply_set_param(0, ParamId(9), 1.0);
        assert_eq!(chain.slot(0).params[9].target, 0.0);
    }

    #[test]
    fn adjacent_pitch_and_stretch_are_flagged_combined() {
        let mut chain = ChainRt::new(44_100);
        chain.apply_insert(0, 0, NodeKind::PitchShift, NodeOwner::Host);
        chain.apply_insert(1, 1, NodeKind::TimeStretch, NodeOwner::Host);
        assert!(chain.slot(1).combined_second);
        assert!(!chain.slot(0).combined_second);
    }

    // `plan_conserves_frames` (contracts/engine-effect-chain.md §12) lives
    // in `tests/chain_rt.rs`.
}

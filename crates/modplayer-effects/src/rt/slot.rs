// SPDX-License-Identifier: MIT OR Apache-2.0

//! `NodeSlot`: one entry of `ChainRt`'s 16-slot preallocated pool
//! (research R2, data-model.md §3.1). A slot's `dsp` is activated in
//! place by `ChainInsert` and never reallocated for the life of the
//! `Processor` — only its *contents* change as a node comes and goes.

use crate::catalog::{NodeKind, NodeOwner};
use crate::crossfade::Crossfade;
use crate::nodes::stretch::StretchBuffers;
use crate::nodes::{Eq8Dsp, FilterDsp, GainDsp, StereoDsp, StretchStage};
use crate::rt::cost::CostRing;
use crate::smooth::Smoothed;

/// The largest parameter count any node kind has (the equalizer's 8
/// bands × 4, data-model.md §1.3) — `NodeSlot::params` is sized to this
/// so every kind's parameters fit without the array itself depending on
/// `kind`.
pub const MAX_PARAMS: usize = 48;

/// The activated DSP state for a slot: one variant per `NodeKind`'s
/// kernel (data-model.md §3.1). Kept small and `Copy` — `Stretch`'s big
/// buffers (ring, intermediate, tails) live in the sibling
/// `NodeSlot::stretch_buffers` field instead (research R2's "buffers
/// preallocated at construction, lent to the variant on activation").
// `StretchStage` (~1.6 KiB) is deliberately kept `Copy`, plain control
// state for a `Stretch` node — its genuinely big buffers already live
// outside this enum, in `NodeSlot::stretch_buffers` (see the doc comment
// above). 16 slots x ~1.6 KiB is a trivial, bounded, one-time cost; the
// alternative (boxing this variant) would need a real-time-unsafe
// allocation on every `ChainInsert`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Copy)]
pub enum NodeDsp {
    Gain(GainDsp),
    Eq(Eq8Dsp),
    Filter(FilterDsp),
    Stereo(StereoDsp),
    Stretch(StretchStage),
}

impl NodeDsp {
    /// The DSP state to activate for `kind` on `ChainInsert`, at
    /// `source_rate` (needed by `Stretch`'s WSOLA timing).
    #[must_use]
    pub const fn for_kind(kind: NodeKind, source_rate: u32) -> Self {
        match kind {
            NodeKind::PitchShift | NodeKind::TimeStretch => {
                NodeDsp::Stretch(StretchStage::new(source_rate))
            }
            NodeKind::Gain => NodeDsp::Gain(GainDsp::new()),
            NodeKind::Equalizer => NodeDsp::Eq(Eq8Dsp::new()),
            NodeKind::Filter => NodeDsp::Filter(FilterDsp::new()),
            NodeKind::StereoTools => NodeDsp::Stereo(StereoDsp::new()),
        }
    }
}

/// One slot of `ChainRt`'s fixed pool (data-model.md §3.1). Not `Copy`
/// (its `stretch_buffers` box isn't) — every mutation goes through
/// `&mut self` methods or in-place field assignment, never a
/// whole-struct copy, which matches the real-time-safety requirement
/// that nothing on this type's hot paths reallocates.
#[derive(Debug)]
pub struct NodeSlot {
    /// `false` for a free slot — skipped by `plan`/`process` and by
    /// `order`.
    pub active: bool,
    pub kind: NodeKind,
    pub owner: NodeOwner,
    pub dsp: NodeDsp,
    /// Indexed by `ParamId.0` (as `usize`); only the entries `catalog::
    /// params(kind)` actually defines are ever read or written.
    pub params: [Smoothed; MAX_PARAMS],
    /// Dry/wet mix for add/remove/bypass and reorder's two phases
    /// (research R6).
    pub mix: Crossfade,
    pub bypassed: bool,
    /// Set only by the overload state machine (`Event::AutoBypassed`,
    /// Phase 6); cleared by a user un-bypass.
    pub auto_bypassed: bool,
    /// Reorder phase 2's target position, set once phase 1's dry-out
    /// fade completes (research R6).
    pub pending_move: Option<u8>,
    /// `true` while fading out for removal; the slot frees itself once
    /// `mix` reaches fully dry.
    pub removing: bool,
    pub cost: CostRing,
    /// FR-007: `true` this render iff this slot is the *second* member
    /// of an adjacent pitch/stretch pair (its own `Stretch` kernel
    /// idles; the first member's runs with the combined product
    /// parameters) — recomputed every render by `ChainRt` (T034).
    pub combined_second: bool,
    /// Debug/test-only cost-inflation hook (contracts/engine-effect-
    /// chain.md §12, `effects_overload.rs`): `ChainRt::process` busy-waits
    /// this many extra nanoseconds after this slot's real kernel work,
    /// letting an overload test drive `render_pct` past threshold
    /// deterministically without a genuinely heavy DSP load. `0` (the
    /// default, and every non-test path) costs one `u64` comparison per
    /// node per render — no allocation, lock, I/O or log, so it stays
    /// real-time safe even compiled into a release binary (quickstart.md
    /// M13's heavy-chain walk drives the same measurement; tests reuse the
    /// field through `ChainRt::set_burn_ns`).
    pub burn_ns: u64,
    /// A `Stretch` node's big, always-preallocated scratch (ring,
    /// intermediate stream, overlap tails) — boxed so every slot in the
    /// 16-slot pool doesn't pay ~250 KiB regardless of its active kind;
    /// allocated once at construction/activation time (off the render
    /// path — only `ChainInsert`, drained at a buffer boundary before
    /// any sample is processed, ever (re)allocates it).
    pub stretch_buffers: Box<StretchBuffers>,
}

impl NodeSlot {
    /// A free slot, ready to be activated by `ChainInsert`. Allocates its
    /// `stretch_buffers` box once (off the render path — this only ever
    /// runs from `ChainRt::new`, at `Processor::new` construction time);
    /// every later free/reactivate cycle reuses that same allocation
    /// (see `deactivate`, `activate`), never reallocating it.
    #[must_use]
    pub fn inactive() -> Self {
        Self {
            active: false,
            kind: NodeKind::Gain,
            owner: NodeOwner::Host,
            dsp: NodeDsp::Gain(GainDsp::new()),
            params: [Smoothed::new(0.0); MAX_PARAMS],
            mix: Crossfade::settled(false),
            bypassed: false,
            auto_bypassed: false,
            pending_move: None,
            removing: false,
            cost: CostRing::new(),
            combined_second: false,
            burn_ns: 0,
            stretch_buffers: Box::new(StretchBuffers::new()),
        }
    }

    /// Activate this slot for a freshly inserted `kind`/`owner` node, at
    /// every parameter's catalog default, fading in over
    /// `crossfade_frames` (research R6). Reactivates the cost ring from
    /// empty. `source_rate` seeds a `Stretch` kernel's WSOLA timing.
    /// Real-time safe: never allocates — `stretch_buffers`'s existing
    /// heap allocation is only *cleared in place* (`fill(0.0)`), never
    /// replaced.
    pub fn activate(
        &mut self,
        kind: NodeKind,
        owner: NodeOwner,
        crossfade_frames: u32,
        source_rate: u32,
    ) {
        self.active = true;
        self.kind = kind;
        self.owner = owner;
        self.dsp = NodeDsp::for_kind(kind, source_rate);
        if matches!(kind, NodeKind::PitchShift | NodeKind::TimeStretch) {
            self.stretch_buffers.ring.fill(0.0);
            for intermediate in &mut self.stretch_buffers.intermediates {
                intermediate.fill(0.0);
            }
            for tail in &mut self.stretch_buffers.tails {
                tail.fill(0.0);
            }
        }
        for (i, slot) in self.params.iter_mut().enumerate() {
            let default = crate::catalog::params(kind)
                .iter()
                .find(|p| p.id.0 as usize == i)
                .map_or(0.0, |p| p.default);
            *slot = Smoothed::new(default);
        }
        self.mix = Crossfade::settled(false);
        self.mix.set_target(true, crossfade_frames);
        self.bypassed = false;
        self.auto_bypassed = false;
        self.pending_move = None;
        self.removing = false;
        self.combined_second = false;
        self.burn_ns = 0;
        self.cost.clear();
    }

    /// Free this slot (`ChainRt::process`'s "slot leaves `order` and its
    /// `mix` fade has settled" path). Real-time safe: resets every small
    /// field in place and leaves `stretch_buffers`'s allocation
    /// untouched (its contents are irrelevant while `!active`) rather
    /// than replacing the whole `NodeSlot`, which would otherwise
    /// allocate a fresh box on the render thread.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.kind = NodeKind::Gain;
        self.owner = NodeOwner::Host;
        self.dsp = NodeDsp::Gain(GainDsp::new());
        self.params = [Smoothed::new(0.0); MAX_PARAMS];
        self.mix = Crossfade::settled(false);
        self.bypassed = false;
        self.auto_bypassed = false;
        self.pending_move = None;
        self.removing = false;
        self.combined_second = false;
        self.burn_ns = 0;
        self.cost.clear();
    }
}

impl Default for NodeSlot {
    fn default() -> Self {
        Self::inactive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activate_seeds_catalog_defaults() {
        let mut slot = NodeSlot::inactive();
        slot.activate(NodeKind::Gain, NodeOwner::Host, 220, 44_100);
        assert!(slot.active);
        // level_db default is 0.0
        assert_eq!(slot.params[0].current, 0.0);
        // mute default is 0.0 (off)
        assert_eq!(slot.params[1].current, 0.0);
        assert!(!slot.mix.is_settled());
    }
}

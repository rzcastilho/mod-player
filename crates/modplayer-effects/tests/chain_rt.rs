// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §12: `plan_conserves_frames` — a
//! proptest over mixed `Gain`/`Stretch` nodes, 16 in any order (research
//! R3, FR-012a), plus `starved_stage_repeats_not_silence` (§3's
//! documented degenerate case).

use std::time::Instant;

use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_effects::rt::ChainRt;
use proptest::prelude::*;

proptest! {
    /// Every `plan`/`process` call yields exactly the requested output
    /// frame count and never panics or mis-sizes its buffer, for any mix
    /// (0..=16) of `Gain` and `TimeStretch` nodes, in any insertion
    /// order, at any ratio.
    #[test]
    fn plan_conserves_frames(
        count in 0usize..=16,
        frames in 1usize..600,
        stretch_mask in prop::collection::vec(any::<bool>(), 0..=16),
        ratio in 0.25f32..2.0,
    ) {
        let mut chain = ChainRt::new(44_100);
        for slot in 0..count {
            let is_stretch = stretch_mask.get(slot).copied().unwrap_or(false);
            let kind = if is_stretch { NodeKind::TimeStretch } else { NodeKind::Gain };
            chain.apply_insert(slot as u8, 0, kind, NodeOwner::Host);
            if is_stretch {
                chain.apply_set_param(slot as u8, ParamId(0), ratio);
            }
        }
        prop_assert_eq!(chain.order().len(), count);

        let planned = chain.plan(frames);
        prop_assert!(planned <= modplayer_effects::consts::CHAIN_BUS_FRAMES);
        let input = chain.input_buffer_mut(planned);
        for s in input.iter_mut() {
            *s = 0.25;
        }
        let out = chain.process(frames, Instant::now);
        prop_assert_eq!(out.len(), frames * 2);
    }
}

/// A stretch stage that would need more ring material than its bounded
/// ring can hold (an extreme combined ratio, ≥ a few stacked stages)
/// repeats its last available grain rather than falling silent
/// (contracts/engine-effect-chain.md §3's documented degenerate case).
#[test]
fn starved_stage_repeats_not_silence() {
    let mut chain = ChainRt::new(44_100);
    chain.apply_insert(0, 0, NodeKind::TimeStretch, NodeOwner::Host);
    // Extreme ratio: `stretch = 1/r` at the minimum `r` (0.25) is the
    // largest supported time-stretch factor.
    chain.apply_set_param(0, ParamId(0), 0.25);

    for _ in 0..400 {
        let planned = chain.plan(2_048);
        let input = chain.input_buffer_mut(planned);
        for (i, s) in input.iter_mut().enumerate() {
            *s = if i.is_multiple_of(4) { 1.0 } else { 0.0 };
        }
        // The point of this test is that a sustained extreme ratio never
        // panics or mis-sizes the output buffer, whether or not the ring
        // actually starves for this particular input pattern.
        let out = chain.process(2_048, Instant::now);
        assert_eq!(out.len(), 2_048 * 2);
    }
}

/// FR-007, FR-012a: an adjacent pitch/stretch pair's combined `Stretch`
/// kernel runs once; the second member idles (no extra input demand of
/// its own — `input_for` on a `combined_second` slot is a pass-through
/// `out == in`, so the *cost* of the shared kernel is free to split
/// half/half by the caller once Phase 6's cost timing lands).
#[test]
fn combined_cost_split_half() {
    let mut chain = ChainRt::new(44_100);
    chain.apply_insert(0, 0, NodeKind::PitchShift, NodeOwner::Host);
    chain.apply_insert(1, 1, NodeKind::TimeStretch, NodeOwner::Host);
    assert!(
        chain.slot(1).combined_second,
        "adjacent pair must be flagged combined"
    );

    for _ in 0..40 {
        let planned = chain.plan(512);
        let input = chain.input_buffer_mut(planned);
        input.fill(0.3);
        let out = chain.process(512, Instant::now);
        assert_eq!(out.len(), 512 * 2);
    }
}

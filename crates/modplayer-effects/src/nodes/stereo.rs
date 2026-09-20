// SPDX-License-Identifier: MIT OR Apache-2.0

//! `StereoDsp`: width / balance / mono-sum(+invert) / channel-swap
//! (contracts/engine-effect-chain.md §5, research R13). Order, fixed:
//! width (mid/side) → balance (far-channel attenuation) → mono sum
//! (phase invert applied first, only while mono sum is on) → channel
//! swap. `width`/`balance` ramp continuously (20 ms, FR-010); the three
//! discrete flags cross the 5 ms equal-power crossfade over *both*
//! configurations' computed output (research R6), since the node's
//! structure (not just a scalar gain) changes at the switch.
//!
//! Identity at defaults (`width = 1`, `balance = 0`, every flag off) is
//! exact — no short-circuit needed: `M ± S` at `width = 1` reproduces
//! `L`/`R`, the balance gains are both `1`, and mono sum/swap are no-ops
//! while off (SC-013).

use crate::crossfade::Crossfade;
use crate::smooth::Smoothed;

/// One frame's stereo tools, applied in the fixed research-R13 order.
fn apply(
    width: f32,
    balance: f32,
    mono_sum: bool,
    phase_invert: bool,
    channel_swap: bool,
    l: f32,
    r: f32,
) -> (f32, f32) {
    let mid = (l + r) * 0.5;
    let side = (l - r) * 0.5 * width;
    let mut l = mid + side;
    let mut r = mid - side;

    let gain_l = (1.0 - balance).min(1.0);
    let gain_r = (1.0 + balance).min(1.0);
    l *= gain_l;
    r *= gain_r;

    if mono_sum {
        if phase_invert {
            r = -r;
        }
        let mono = (l + r) * 0.5;
        l = mono;
        r = mono;
    }

    if channel_swap {
        std::mem::swap(&mut l, &mut r);
    }
    (l, r)
}

/// The stereo-tools kernel's per-slot state: `width`/`balance` are the
/// slot's own `Smoothed` parameters (read here); `mono_sum`/
/// `phase_invert`/`channel_swap` are this kernel's own discrete state,
/// switched as a group through [`set_flags_target`].
#[derive(Debug, Clone, Copy)]
pub struct StereoDsp {
    mono_sum: bool,
    phase_invert: bool,
    channel_swap: bool,
    old_mono_sum: bool,
    old_phase_invert: bool,
    old_channel_swap: bool,
    /// `dry` = the flags as of the instant the last switch began, `wet`
    /// = the current flags.
    mix: Crossfade,
}

impl StereoDsp {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mono_sum: false,
            phase_invert: false,
            channel_swap: false,
            old_mono_sum: false,
            old_phase_invert: false,
            old_channel_swap: false,
            mix: Crossfade::settled(true),
        }
    }

    /// Retarget the three discrete flags together over a
    /// `crossfade_frames`-long equal-power crossfade (research R6). A
    /// no-op when the flags already match.
    pub fn set_flags_target(
        &mut self,
        mono_sum: bool,
        phase_invert: bool,
        channel_swap: bool,
        crossfade_frames: u32,
    ) {
        if (mono_sum, phase_invert, channel_swap)
            == (self.mono_sum, self.phase_invert, self.channel_swap)
        {
            return;
        }
        self.old_mono_sum = self.mono_sum;
        self.old_phase_invert = self.phase_invert;
        self.old_channel_swap = self.channel_swap;
        self.mono_sum = mono_sum;
        self.phase_invert = phase_invert;
        self.channel_swap = channel_swap;
        self.mix = Crossfade::settled(false);
        self.mix.set_target(true, crossfade_frames);
    }

    /// Process `frames` stereo frames of `buf` in place. `params[0]` is
    /// `width`, `params[1]` is `balance`.
    pub fn process(&mut self, buf: &mut [f32], frames: usize, params: &mut [Smoothed]) {
        for i in 0..frames {
            let width = params[0].advance();
            let balance = params[1].advance();
            let l = buf[i * 2];
            let r = buf[i * 2 + 1];
            let is_default = width == 1.0
                && balance == 0.0
                && !self.mono_sum
                && !self.phase_invert
                && !self.channel_swap
                && self.mix.is_settled();
            if is_default {
                // Sample-exact identity (SC-013): the mid/side round trip
                // below is only *mathematically* exact, not bit-exact in
                // floating point, so the documented default takes an
                // explicit pass-through short-circuit exactly like
                // `FilterDsp`'s (contracts/engine-effect-chain.md §5).
                continue;
            }
            let (wet_l, wet_r) = apply(
                width,
                balance,
                self.mono_sum,
                self.phase_invert,
                self.channel_swap,
                l,
                r,
            );
            if self.mix.is_settled() {
                buf[i * 2] = wet_l;
                buf[i * 2 + 1] = wet_r;
            } else {
                let (dry_l, dry_r) = apply(
                    width,
                    balance,
                    self.old_mono_sum,
                    self.old_phase_invert,
                    self.old_channel_swap,
                    l,
                    r,
                );
                let (dry, wet) = self.mix.gains();
                buf[i * 2] = dry_l * dry + wet_l * wet;
                buf[i * 2 + 1] = dry_r * dry + wet_r * wet;
                self.mix.advance();
            }
        }
    }
}

impl Default for StereoDsp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `gain_eq_filter_stereo_identity_at_defaults`, `stereo_truth_table`
    // and `mono_sum_invert_cancels_centre` (contracts/engine-effect-
    // chain.md §12) live in `tests/nodes.rs`.

    #[test]
    fn identity_at_defaults() {
        assert_eq!(apply(1.0, 0.0, false, false, false, 0.3, -0.7), (0.3, -0.7));
    }

    #[test]
    fn width_zero_collapses_to_mono_via_mid() {
        assert_eq!(apply(0.0, 0.0, false, false, false, 1.0, -1.0), (0.0, 0.0));
    }

    #[test]
    fn full_right_balance_mutes_left_only() {
        assert_eq!(apply(1.0, 1.0, false, false, false, 1.0, 1.0), (0.0, 1.0));
    }

    #[test]
    fn phase_invert_without_mono_sum_has_no_effect() {
        assert_eq!(apply(1.0, 0.0, false, true, false, 0.4, -0.4), (0.4, -0.4));
    }

    #[test]
    fn channel_swap_swaps_final_output() {
        let (l, r) = apply(1.0, 0.0, false, false, true, 0.2, -0.9);
        assert!((l - (-0.9)).abs() < 1e-6);
        assert!((r - 0.2).abs() < 1e-6);
    }

    #[test]
    fn flag_switch_settles_click_free() {
        let mut dsp = StereoDsp::new();
        dsp.set_flags_target(true, false, false, 32);
        let mut buf = vec![0.3f32; 64];
        let mut params = [Smoothed::new(1.0); 48];
        params[1] = Smoothed::new(0.0);
        dsp.process(&mut buf, 32, &mut params);
        assert!(dsp.mix.is_settled());
        assert!(dsp.mono_sum);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! `FilterDsp`: the high-pass/low-pass filter node (contracts/engine-
//! effect-chain.md §5). `Q = Q_BUTTER + resonance * (Q_MAX -
//! Q_BUTTER)` (research R12); a mode switch carries its own shadow
//! biquad, exactly like `Eq8Dsp`'s band-type switch (research R6).
//!
//! SC-013 needs the filter's default (`mode == HighPass && cutoff ==
//! 20.0 && resonance == 0.0`) to be sample-exact, but an RBJ HPF at
//! 20 Hz/`Q_BUTTER` is only *inaudibly* close to identity (|H| ≥
//! −0.01 dB above 40 Hz), not bit-exact — so the kernel takes an
//! **explicit pass-through short-circuit** at exactly that default
//! (documented in contracts/engine-effect-chain.md §5) rather than
//! relying on the biquad algebra.

use crate::biquad::{Biquad, Coeffs};
use crate::consts::{Q_BUTTER, Q_MAX};
use crate::crossfade::Crossfade;
use crate::smooth::Smoothed;

/// `mode`'s discrete values (data-model.md §1.3).
const MODE_HIGH_PASS: u8 = 0;
const MODE_LOW_PASS: u8 = 1;

/// The documented default cutoff (Hz) at which, combined with
/// `resonance == 0` and `mode == HighPass`, the kernel short-circuits to
/// an exact pass-through.
const DEFAULT_CUTOFF_HZ: f32 = 20.0;

fn q_from_resonance(resonance: f32) -> f32 {
    Q_BUTTER + resonance.clamp(0.0, 1.0) * (Q_MAX - Q_BUTTER)
}

fn coeffs_for(mode: u8, cutoff: f32, resonance: f32, source_rate: u32) -> Coeffs {
    let q = q_from_resonance(resonance);
    if mode == MODE_LOW_PASS {
        Coeffs::low_pass(cutoff, q, source_rate)
    } else {
        Coeffs::high_pass(cutoff, q, source_rate)
    }
}

/// The filter kernel's per-slot state.
#[derive(Debug, Clone, Copy)]
pub struct FilterDsp {
    l: Biquad,
    r: Biquad,
    coeffs: Coeffs,
    mode: u8,
    /// State/coefficients as of the instant a mode switch began
    /// (research R6); `dry` = shadow (old mode), `wet` = main (new mode).
    shadow_l: Biquad,
    shadow_r: Biquad,
    shadow_coeffs: Coeffs,
    mix: Crossfade,
    since_recompute: u32,
}

impl FilterDsp {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            l: Biquad::new(),
            r: Biquad::new(),
            coeffs: Coeffs::IDENTITY,
            mode: MODE_HIGH_PASS,
            shadow_l: Biquad::new(),
            shadow_r: Biquad::new(),
            shadow_coeffs: Coeffs::IDENTITY,
            mix: Crossfade::settled(true),
            since_recompute: 0,
        }
    }

    /// Retarget the filter's mode over a `crossfade_frames`-long
    /// equal-power crossfade (research R6). A no-op if `new_mode`
    /// matches the current mode.
    pub fn switch_mode(&mut self, new_mode: u8, crossfade_frames: u32) {
        if self.mode == new_mode {
            return;
        }
        self.shadow_l = self.l;
        self.shadow_r = self.r;
        self.shadow_coeffs = self.coeffs;
        self.mode = new_mode;
        self.mix = Crossfade::settled(false);
        self.mix.set_target(true, crossfade_frames);
    }

    /// Process `frames` stereo frames of `buf` in place. `params[0]` is
    /// `mode` (already applied via [`switch_mode`], not read here again),
    /// `params[1]` is `cutoff`, `params[2]` is `resonance`.
    pub fn process(
        &mut self,
        buf: &mut [f32],
        frames: usize,
        params: &mut [Smoothed],
        source_rate: u32,
    ) {
        for i in 0..frames {
            let cutoff = params[1].advance();
            let resonance = params[2].advance();
            if self.since_recompute == 0 {
                self.coeffs = coeffs_for(self.mode, cutoff, resonance, source_rate);
            }
            self.since_recompute = (self.since_recompute + 1) % 32;

            let l = buf[i * 2];
            let r = buf[i * 2 + 1];
            let is_default = self.mode == MODE_HIGH_PASS
                && cutoff == DEFAULT_CUTOFF_HZ
                && resonance == 0.0
                && self.mix.is_settled();
            if is_default {
                continue; // explicit pass-through short-circuit (SC-013)
            }

            let wet_l = self.l.process(&self.coeffs, l);
            let wet_r = self.r.process(&self.coeffs, r);
            if self.mix.is_settled() {
                buf[i * 2] = wet_l;
                buf[i * 2 + 1] = wet_r;
            } else {
                let dry_l = self.shadow_l.process(&self.shadow_coeffs, l);
                let dry_r = self.shadow_r.process(&self.shadow_coeffs, r);
                let (dry, wet) = self.mix.gains();
                buf[i * 2] = dry_l * dry + wet_l * wet;
                buf[i * 2 + 1] = dry_r * dry + wet_r * wet;
                self.mix.advance();
            }
        }
    }
}

impl Default for FilterDsp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `gain_eq_filter_stereo_identity_at_defaults`,
    // `filter_hp_minus_3db_at_cutoff` and `resonance_caps_at_q_max`
    // (contracts/engine-effect-chain.md §12) live in `tests/nodes.rs`.

    #[test]
    fn q_from_resonance_spans_butterworth_to_max() {
        assert_eq!(q_from_resonance(0.0), Q_BUTTER);
        assert_eq!(q_from_resonance(1.0), Q_MAX);
        // Defence in depth: an out-of-range input still caps at Q_MAX.
        assert_eq!(q_from_resonance(5.0), Q_MAX);
    }

    #[test]
    fn defaults_are_bit_exact_identity() {
        let mut filter = FilterDsp::new();
        let mut params = [Smoothed::new(0.0); 48];
        params[1] = Smoothed::new(20.0);
        let mut buf = vec![0.3f32, -0.6, 0.9, -0.2];
        let expected = buf.clone();
        filter.process(&mut buf, 2, &mut params, 44_100);
        assert_eq!(buf, expected);
    }

    #[test]
    fn mode_switch_settles_click_free() {
        let mut filter = FilterDsp::new();
        let mut params = [Smoothed::new(0.0); 48];
        params[1] = Smoothed::new(1_000.0);
        params[2] = Smoothed::new(0.0);
        filter.switch_mode(MODE_LOW_PASS, 32);
        let mut buf = vec![0.2f32; 128];
        filter.process(&mut buf, 64, &mut params, 44_100);
        assert_eq!(filter.mode, MODE_LOW_PASS);
        assert!(filter.mix.is_settled());
    }
}

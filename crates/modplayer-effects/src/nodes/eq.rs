// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Eq8Dsp`: the 8-band parametric equalizer (contracts/engine-effect-
//! chain.md §5). Every band is always present (FR-006); each band's
//! `type[b]` (peak/low-shelf/high-shelf) is an RBJ biquad recomputed at
//! 32-frame sub-block boundaries (research R5) from that band's ramped
//! `freq`/`gain`/`q`. A band-type change carries its own shadow biquad
//! (research R6): the shadow keeps running the *old* type from the same
//! filter memory while the main biquad runs the *new* type, and a 5 ms
//! equal-power crossfade blends the two so the switch never clicks.

use crate::biquad::{Biquad, Coeffs};
use crate::catalog::ParamId;
use crate::crossfade::Crossfade;
use crate::smooth::Smoothed;

const BANDS: usize = 8;
/// Biquad coefficients are recomputed every this many frames (research
/// R5): cheap enough to track a 20 ms ramp smoothly, far below the cost
/// of a per-sample `sin`/`cos` recompute.
const RECOMPUTE_INTERVAL: u32 = 32;

/// `type[b]`'s discrete values (data-model.md §1.3).
const TYPE_PEAK: u8 = 0;
const TYPE_LOW_SHELF: u8 = 1;
const TYPE_HIGH_SHELF: u8 = 2;

fn coeffs_for(band_type: u8, freq: f32, gain_db: f32, q: f32, source_rate: u32) -> Coeffs {
    match band_type {
        TYPE_LOW_SHELF => Coeffs::low_shelf(freq, gain_db, q, source_rate),
        TYPE_HIGH_SHELF => Coeffs::high_shelf(freq, gain_db, q, source_rate),
        // `TYPE_PEAK` and any other value both fall back to peaking —
        // `TYPE_PEAK` is `0`, so listing it here would be a redundant
        // wildcard arm (clippy::wildcard_in_or_patterns).
        _ => Coeffs::peaking(freq, gain_db, q, source_rate),
    }
}

#[derive(Debug, Clone, Copy)]
struct Band {
    l: Biquad,
    r: Biquad,
    coeffs: Coeffs,
    band_type: u8,
    /// The band's state and coefficients the instant a type switch began
    /// (research R6) — kept running with the *old* type until the
    /// crossfade settles.
    shadow_l: Biquad,
    shadow_r: Biquad,
    shadow_coeffs: Coeffs,
    /// `dry` = shadow (old type), `wet` = main (new type).
    mix: Crossfade,
}

impl Band {
    const fn new() -> Self {
        Self {
            l: Biquad::new(),
            r: Biquad::new(),
            coeffs: Coeffs::IDENTITY,
            band_type: TYPE_PEAK,
            shadow_l: Biquad::new(),
            shadow_r: Biquad::new(),
            shadow_coeffs: Coeffs::IDENTITY,
            mix: Crossfade::settled(true),
        }
    }
}

/// The equalizer kernel's per-slot state: 8 always-present bands.
#[derive(Debug, Clone, Copy)]
pub struct Eq8Dsp {
    bands: [Band; BANDS],
    since_recompute: u32,
}

impl Eq8Dsp {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bands: [Band::new(); BANDS],
            since_recompute: 0,
        }
    }

    /// Retarget band `band`'s type over a `crossfade_frames`-long
    /// equal-power crossfade (research R6). A no-op if `band` is out of
    /// range or `new_type` matches the band's current type.
    pub fn switch_band_type(&mut self, band: usize, new_type: u8, crossfade_frames: u32) {
        let Some(b) = self.bands.get_mut(band) else {
            return;
        };
        if b.band_type == new_type {
            return;
        }
        b.shadow_l = b.l;
        b.shadow_r = b.r;
        b.shadow_coeffs = b.coeffs;
        b.band_type = new_type;
        b.mix = Crossfade::settled(false);
        b.mix.set_target(true, crossfade_frames);
    }

    fn recompute(&mut self, params: &[Smoothed], source_rate: u32) {
        for (bi, b) in self.bands.iter_mut().enumerate() {
            let freq = params[ParamId::eq_band(bi as u8, 0).0 as usize].current;
            let gain = params[ParamId::eq_band(bi as u8, 1).0 as usize].current;
            let q = params[ParamId::eq_band(bi as u8, 2).0 as usize].current;
            b.coeffs = coeffs_for(b.band_type, freq, gain, q, source_rate);
        }
    }

    /// Process `frames` stereo frames of `buf` in place. `params` is the
    /// slot's full parameter array (indexed by `ParamId`, data-model.md
    /// §1.3's `16 + 4*band + n` scheme); every band's `freq`/`gain`/`q`
    /// ramp advances every sample, and coefficients are recomputed every
    /// [`RECOMPUTE_INTERVAL`] frames from the ramps' current values.
    pub fn process(
        &mut self,
        buf: &mut [f32],
        frames: usize,
        params: &mut [Smoothed],
        source_rate: u32,
    ) {
        for i in 0..frames {
            for bi in 0..BANDS as u8 {
                let _ = params[ParamId::eq_band(bi, 0).0 as usize].advance();
                let _ = params[ParamId::eq_band(bi, 1).0 as usize].advance();
                let _ = params[ParamId::eq_band(bi, 2).0 as usize].advance();
            }
            if self.since_recompute == 0 {
                self.recompute(params, source_rate);
            }
            self.since_recompute = (self.since_recompute + 1) % RECOMPUTE_INTERVAL;

            let mut l = buf[i * 2];
            let mut r = buf[i * 2 + 1];
            for b in &mut self.bands {
                let wet_l = b.l.process(&b.coeffs, l);
                let wet_r = b.r.process(&b.coeffs, r);
                if b.mix.is_settled() {
                    l = wet_l;
                    r = wet_r;
                } else {
                    let dry_l = b.shadow_l.process(&b.shadow_coeffs, l);
                    let dry_r = b.shadow_r.process(&b.shadow_coeffs, r);
                    let (dry, wet) = b.mix.gains();
                    l = dry_l * dry + wet_l * wet;
                    r = dry_r * dry + wet_r * wet;
                    b.mix.advance();
                }
            }
            buf[i * 2] = l;
            buf[i * 2 + 1] = r;
        }
    }
}

impl Default for Eq8Dsp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `gain_eq_filter_stereo_identity_at_defaults` and
    // `eq_peak_plus_6db_at_1khz` (contracts/engine-effect-chain.md §12)
    // live in `tests/nodes.rs`.

    fn default_params() -> [Smoothed; 48] {
        let mut params = [Smoothed::new(0.0); 48];
        for b in 0..8u8 {
            let freq = 63.0 * (1u32 << b) as f32;
            params[ParamId::eq_band(b, 0).0 as usize] = Smoothed::new(freq);
            params[ParamId::eq_band(b, 1).0 as usize] = Smoothed::new(0.0);
            params[ParamId::eq_band(b, 2).0 as usize] = Smoothed::new(1.0);
        }
        params
    }

    #[test]
    fn defaults_are_bit_exact_identity() {
        let mut eq = Eq8Dsp::new();
        let mut params = default_params();
        let mut buf = vec![0.3f32, -0.6, 0.9, -0.2, 0.0, 1.0];
        let expected = buf.clone();
        eq.process(&mut buf, 3, &mut params, 44_100);
        assert_eq!(buf, expected);
    }

    #[test]
    fn band_type_switch_settles_without_a_discontinuity_step() {
        let mut eq = Eq8Dsp::new();
        let mut params = default_params();
        params[ParamId::eq_band(0, 1).0 as usize] = Smoothed::new(6.0);
        eq.switch_band_type(0, TYPE_LOW_SHELF, 32);
        let mut buf = vec![0.2f32; 128];
        eq.process(&mut buf, 64, &mut params, 44_100);
        assert_eq!(eq.bands[0].band_type, TYPE_LOW_SHELF);
        assert!(eq.bands[0].mix.is_settled());
    }
}

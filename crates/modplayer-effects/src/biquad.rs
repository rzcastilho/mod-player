// SPDX-License-Identifier: MIT OR Apache-2.0

//! RBJ "Audio EQ Cookbook" biquad coefficients (peak / low-shelf /
//! high-shelf / high-pass / low-pass, research R12) and a transposed
//! direct-form-II biquad state (`Biquad`) — the shared building block
//! behind `nodes::eq::Eq8Dsp` and `nodes::filter::FilterDsp`
//! (contracts/engine-effect-chain.md §5).
//!
//! `Coeffs::peaking`/`low_shelf`/`high_shelf` short-circuit to
//! [`Coeffs::IDENTITY`] at `gain_db == 0.0` (sample-exact, SC-013) rather
//! than relying on the cookbook algebra to land on exactly `[1,0,0]` —
//! trig rounding would not otherwise guarantee that.

use std::f32::consts::TAU;

/// A normalised biquad transfer function (`a0` folded to `1`):
/// `y = b0*x + b1*x1 + b2*x2 - a1*y1 - a2*y2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Coeffs {
    /// The pass-through transfer function (`b0 = 1`, everything else
    /// `0`) — sample-exact identity.
    pub const IDENTITY: Coeffs = Coeffs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    fn normalize(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        let inv = 1.0 / a0;
        Coeffs {
            b0: b0 * inv,
            b1: b1 * inv,
            b2: b2 * inv,
            a1: a1 * inv,
            a2: a2 * inv,
        }
    }

    /// `w0` (normalised angular frequency), `cos(w0)` and `alpha =
    /// sin(w0) / (2*Q)`; `freq` is clamped just below Nyquist defensively
    /// (research R5's "defence in depth" — the catalog already clamps
    /// every caller's value).
    fn w0_cos_alpha(freq: f32, q: f32, source_rate: u32) -> (f32, f32) {
        let rate = source_rate.max(1) as f32;
        let freq = freq.clamp(1.0, 0.499 * rate);
        let w0 = TAU * freq / rate;
        let alpha = w0.sin() / (2.0 * q.max(1e-4));
        (w0.cos(), alpha)
    }

    /// RBJ peaking EQ: boosts/cuts a band around `freq` by `gain_db`,
    /// `Q` wide. Sample-exact identity at `gain_db == 0.0`.
    #[must_use]
    pub fn peaking(freq: f32, gain_db: f32, q: f32, source_rate: u32) -> Self {
        if gain_db == 0.0 {
            return Self::IDENTITY;
        }
        let (cosw0, alpha) = Self::w0_cos_alpha(freq, q, source_rate);
        let a = 10f32.powf(gain_db / 40.0);
        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cosw0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cosw0;
        let a2 = 1.0 - alpha / a;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    /// RBJ low shelf: below `freq`, gain shifts by `gain_db`. Sample-exact
    /// identity at `gain_db == 0.0`.
    #[must_use]
    pub fn low_shelf(freq: f32, gain_db: f32, q: f32, source_rate: u32) -> Self {
        if gain_db == 0.0 {
            return Self::IDENTITY;
        }
        let (cosw0, alpha) = Self::w0_cos_alpha(freq, q, source_rate);
        let a = 10f32.powf(gain_db / 40.0);
        let sqrt_a = a.sqrt();
        let b0 = a * ((a + 1.0) - (a - 1.0) * cosw0 + 2.0 * sqrt_a * alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cosw0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cosw0 - 2.0 * sqrt_a * alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cosw0 + 2.0 * sqrt_a * alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cosw0);
        let a2 = (a + 1.0) + (a - 1.0) * cosw0 - 2.0 * sqrt_a * alpha;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    /// RBJ high shelf: above `freq`, gain shifts by `gain_db`. Sample-exact
    /// identity at `gain_db == 0.0`.
    #[must_use]
    pub fn high_shelf(freq: f32, gain_db: f32, q: f32, source_rate: u32) -> Self {
        if gain_db == 0.0 {
            return Self::IDENTITY;
        }
        let (cosw0, alpha) = Self::w0_cos_alpha(freq, q, source_rate);
        let a = 10f32.powf(gain_db / 40.0);
        let sqrt_a = a.sqrt();
        let b0 = a * ((a + 1.0) + (a - 1.0) * cosw0 + 2.0 * sqrt_a * alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cosw0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cosw0 - 2.0 * sqrt_a * alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cosw0 + 2.0 * sqrt_a * alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cosw0);
        let a2 = (a + 1.0) - (a - 1.0) * cosw0 - 2.0 * sqrt_a * alpha;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    /// RBJ high-pass, `Q` (research R12: `Q_BUTTER` at `resonance == 0`
    /// gives the maximally-flat Butterworth −3 dB-at-cutoff response).
    #[must_use]
    pub fn high_pass(freq: f32, q: f32, source_rate: u32) -> Self {
        let (cosw0, alpha) = Self::w0_cos_alpha(freq, q, source_rate);
        let b0 = (1.0 + cosw0) / 2.0;
        let b1 = -(1.0 + cosw0);
        let b2 = (1.0 + cosw0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cosw0;
        let a2 = 1.0 - alpha;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    /// RBJ low-pass, `Q` (research R12).
    #[must_use]
    pub fn low_pass(freq: f32, q: f32, source_rate: u32) -> Self {
        let (cosw0, alpha) = Self::w0_cos_alpha(freq, q, source_rate);
        let b0 = (1.0 - cosw0) / 2.0;
        let b1 = 1.0 - cosw0;
        let b2 = (1.0 - cosw0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cosw0;
        let a2 = 1.0 - alpha;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }
}

/// One channel's transposed-direct-form-II biquad state (two floats of
/// memory) — cheap enough that `Eq8Dsp` keeps `2 × 8` (L/R × bands) plus
/// as many again for the shadow used during a type switch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    w1: f32,
    w2: f32,
}

impl Biquad {
    #[must_use]
    pub const fn new() -> Self {
        Self { w1: 0.0, w2: 0.0 }
    }

    /// Process one sample through `coeffs`, advancing this channel's
    /// state.
    pub fn process(&mut self, coeffs: &Coeffs, x: f32) -> f32 {
        let w = x - coeffs.a1 * self.w1 - coeffs.a2 * self.w2;
        let y = coeffs.b0 * w + coeffs.b1 * self.w1 + coeffs.b2 * self.w2;
        self.w2 = self.w1;
        self.w1 = w;
        y
    }

    /// Clear this channel's memory (used on `Command::Seek`/`Stop`,
    /// FR-001a).
    pub fn reset(&mut self) {
        self.w1 = 0.0;
        self.w2 = 0.0;
    }
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaking_at_zero_gain_is_identity() {
        assert_eq!(Coeffs::peaking(1_000.0, 0.0, 1.0, 44_100), Coeffs::IDENTITY);
    }

    #[test]
    fn shelves_at_zero_gain_are_identity() {
        assert_eq!(Coeffs::low_shelf(200.0, 0.0, 1.0, 44_100), Coeffs::IDENTITY);
        assert_eq!(
            Coeffs::high_shelf(8_000.0, 0.0, 1.0, 44_100),
            Coeffs::IDENTITY
        );
    }

    #[test]
    fn identity_biquad_passes_samples_unchanged() {
        let mut b = Biquad::new();
        for x in [0.1f32, -0.5, 0.9, -1.0, 0.0] {
            assert_eq!(b.process(&Coeffs::IDENTITY, x), x);
        }
    }

    #[test]
    fn reset_clears_memory() {
        let mut b = Biquad::new();
        let c = Coeffs::low_pass(1_000.0, 0.707, 44_100);
        b.process(&c, 1.0);
        assert_ne!(b, Biquad::new());
        b.reset();
        assert_eq!(b, Biquad::new());
    }
}

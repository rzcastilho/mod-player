// SPDX-License-Identifier: MIT OR Apache-2.0

//! `LpcState`: order-16 linear-predictive-coding envelope estimation and
//! the whiten/resynthesize filter pair the `StretchStage`'s formant
//! preservation uses (research R4, contracts/engine-effect-chain.md §5).
//! Fixed-size arrays throughout — allocation-free, one instance per
//! channel per voice.
//!
//! The classic LPC formant-preserving shift: `envelope(window)` computes
//! an order-16 all-pole model of a (Hann-windowed) block of samples via
//! autocorrelation + Levinson-Durbin; `whiten` runs the inverse filter to
//! flatten a signal's spectral envelope (leaving the excitation/residual);
//! `resynthesize` runs the all-pole synthesis filter to re-impose a
//! (possibly different) envelope onto a residual. Filter memory
//! (`history`) persists across calls so streaming a signal in blocks
//! produces a continuous filter response, not a click at every block
//! boundary.

use crate::consts::LPC_ORDER;

/// White-noise correction applied to `r[0]` before Levinson-Durbin
/// (`r[0] *= 1 + WHITE_NOISE_CORRECTION`): the standard −40 dB noise floor
/// that keeps the normal equations conditioned on strongly periodic
/// material, where an f32 order-16 solve otherwise places poles *on* the
/// unit circle (speech/audio codecs use the same 1e-4 figure).
const WHITE_NOISE_CORRECTION: f32 = 1e-4;

/// Bandwidth-expansion factor applied to the solved predictor
/// (`a[i] *= BANDWIDTH_EXPANSION^(i+1)`): pulls every pole radially inward
/// by 2 %, so the all-pole synthesis filter stays stable even when it is
/// driven by a residual whitened with a *different* block's envelope (the
/// formant-correction pairing in `stretch.rs::apply_formant`). 0.98 is the
/// classic 60 Hz-at-44.1 kHz expansion — inaudible as envelope smoothing.
const BANDWIDTH_EXPANSION: f32 = 0.98;

/// Absolute ceiling on a synthesis-filter output sample (+24 dBFS): a
/// well-behaved signal never gets near it, and it guarantees the filter
/// memory can never reach `inf` (whose `inf - inf` step bound is `NaN`).
const SYNTH_OUTPUT_CEILING: f32 = 16.0;

/// One channel's LPC filter memory and current envelope. `#[derive(Copy)]`
/// so it is cheap to embed twice per voice (stereo) without indirection.
#[derive(Debug, Clone, Copy)]
pub struct LpcState {
    /// Predictor coefficients `a[1..=LPC_ORDER]` such that the model
    /// predicts `x[n] ~= sum_{i=1}^{order} a[i] * x[n-i]`.
    coeffs: [f32; LPC_ORDER],
    /// Inverse-filter (whitening) memory: the last `LPC_ORDER` *input*
    /// samples seen by `whiten`.
    whiten_history: [f32; LPC_ORDER],
    /// Synthesis-filter memory: the last `LPC_ORDER` *output* samples
    /// produced by `resynthesize`.
    synth_history: [f32; LPC_ORDER],
}

impl LpcState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            coeffs: [0.0; LPC_ORDER],
            whiten_history: [0.0; LPC_ORDER],
            synth_history: [0.0; LPC_ORDER],
        }
    }

    /// Reset every filter memory and the stored envelope to silence
    /// (called on `Command::Seek`/`Stop`, FR-001a).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// The most recently computed envelope coefficients.
    #[must_use]
    pub const fn coeffs(&self) -> [f32; LPC_ORDER] {
        self.coeffs
    }

    /// Analyse `window` (mono, any length >= `LPC_ORDER + 1`; shorter
    /// windows leave the previous envelope unchanged) via autocorrelation
    /// and Levinson-Durbin, and store the resulting order-`LPC_ORDER`
    /// all-pole envelope. Does not touch filter history.
    pub fn analyze(&mut self, window: &[f32]) {
        if window.len() <= LPC_ORDER {
            return;
        }
        let mut autocorr = autocorrelate(window, LPC_ORDER);
        // A silent (or near-silent) window has an ill-conditioned normal
        // equation; leave the previous envelope in place rather than
        // divide by (near) zero. So does a window carrying a non-finite
        // sample (its autocorrelation is `NaN`/`inf` throughout).
        if !autocorr.iter().all(|r| r.is_finite()) || autocorr[0] <= f32::EPSILON {
            return;
        }
        autocorr[0] *= 1.0 + WHITE_NOISE_CORRECTION;
        let mut coeffs = levinson_durbin(&autocorr);
        let mut gamma = BANDWIDTH_EXPANSION;
        for coeff in &mut coeffs {
            *coeff *= gamma;
            gamma *= BANDWIDTH_EXPANSION;
        }
        self.coeffs = coeffs;
    }

    /// Run this filter's own stored envelope's inverse (whitening) filter
    /// over `samples` in place, updating `whiten_history` as it goes:
    /// `residual[n] = x[n] - sum_i coeffs[i] * x[n-i]`.
    pub fn whiten(&mut self, samples: &mut [f32]) {
        whiten_with(&self.coeffs, &mut self.whiten_history, samples);
    }

    /// Run the all-pole synthesis filter for `target_coeffs` (typically a
    /// *different*, earlier-captured envelope than this state's own —
    /// "the original envelope re-applied", research R4) over `residual`
    /// in place, updating `synth_history`:
    /// `y[n] = residual[n] + sum_i target_coeffs[i] * y[n-i]`.
    pub fn resynthesize(&mut self, target_coeffs: &[f32; LPC_ORDER], residual: &mut [f32]) {
        resynthesize_with(target_coeffs, &mut self.synth_history, residual);
    }
}

impl Default for LpcState {
    fn default() -> Self {
        Self::new()
    }
}

/// `window`'s biased autocorrelation `r[0..=order]` (`r[0]` is the
/// signal's energy).
fn autocorrelate(window: &[f32], order: usize) -> [f32; LPC_ORDER + 1] {
    let mut r = [0.0f32; LPC_ORDER + 1];
    let n = window.len();
    for (lag, slot) in r.iter_mut().enumerate().take(order + 1) {
        let mut sum = 0.0f64;
        for i in lag..n {
            sum += f64::from(window[i]) * f64::from(window[i - lag]);
        }
        *slot = sum as f32;
    }
    r
}

/// The Levinson-Durbin recursion: `order`-th order LPC coefficients
/// `a[1..=order]` from autocorrelation `r[0..=order]`. Fixed arrays, no
/// allocation. Falls back to all-zero coefficients (pass-through, no
/// prediction) on numerical breakdown (a reflection coefficient outside
/// `(-1, 1)`, which a well-formed autocorrelation never produces but a
/// pathological input in principle could).
fn levinson_durbin(r: &[f32; LPC_ORDER + 1]) -> [f32; LPC_ORDER] {
    let mut a = [0.0f32; LPC_ORDER];
    let mut error = r[0];
    if error <= f32::EPSILON {
        return a;
    }
    for i in 0..LPC_ORDER {
        let mut acc = r[i + 1];
        for j in 0..i {
            acc -= a[j] * r[i - j];
        }
        let k = acc / error;
        if !k.is_finite() || k.abs() >= 1.0 {
            // Numerical breakdown: keep whatever order was stable so far.
            break;
        }
        let mut new_a = a;
        new_a[i] = k;
        for j in 0..i {
            new_a[j] = a[j] - k * a[i - 1 - j];
        }
        a = new_a;
        error *= 1.0 - k * k;
        if error <= f32::EPSILON {
            break;
        }
    }
    a
}

fn whiten_with(coeffs: &[f32; LPC_ORDER], history: &mut [f32; LPC_ORDER], samples: &mut [f32]) {
    for sample in samples.iter_mut() {
        // A non-finite input would poison the inverse filter's memory (and
        // then every later residual); treat it as silence instead.
        let x = if sample.is_finite() { *sample } else { 0.0 };
        let mut predicted = 0.0f32;
        for (i, c) in coeffs.iter().enumerate() {
            predicted += c * history[i];
        }
        let residual = x - predicted;
        // Shift history: history[0] is the most recent sample.
        for i in (1..LPC_ORDER).rev() {
            history[i] = history[i - 1];
        }
        history[0] = x;
        *sample = residual;
    }
}

fn resynthesize_with(
    coeffs: &[f32; LPC_ORDER],
    history: &mut [f32; LPC_ORDER],
    residual: &mut [f32],
) {
    for sample in residual.iter_mut() {
        let mut predicted = 0.0f32;
        for (i, c) in coeffs.iter().enumerate() {
            predicted += c * history[i];
        }
        let mut y = *sample + predicted;
        // Real-time safety net: `target_coeffs` (the caller's periodically
        // refreshed "original" envelope) and this filter's own history can
        // be an ill-matched pair for the material actually flowing through
        // it (research R4's formant correction whitens with *one* block's
        // envelope and resynthesizes with *another*), so a resonant
        // all-pole feedback loop can otherwise grow sample over sample.
        // Clamping each step against the filter's own last output directly
        // bounds the one thing a "click" is (contracts/engine-effect-
        // chain.md §4's first-difference metric) without touching a
        // well-behaved signal, whose natural per-sample step is far
        // smaller than its own amplitude at any audible frequency.
        //
        // The step bound alone is not a magnitude bound (it still allows
        // doubling every sample), so it is backed by an absolute ceiling
        // and a non-finite reset: the 2026-09-19 manual walk (M3) aborted
        // the whole process on the audio thread when a runaway filter
        // reached `inf`, making the step bound `inf - inf = NaN` and
        // `f32::clamp` panic on its `NaN` limits.
        let last = history[0];
        let max_step = last.abs().max(1.0) * 2.0;
        y = y.clamp(last - max_step, last + max_step);
        if y.is_finite() {
            y = y.clamp(-SYNTH_OUTPUT_CEILING, SYNTH_OUTPUT_CEILING);
        } else {
            // A non-finite input sample or prediction: drop this sample
            // and forget the (now meaningless) memory so the filter
            // restarts from silence rather than propagating `NaN`.
            *history = [0.0; LPC_ORDER];
            y = 0.0;
        }
        for i in (1..LPC_ORDER).rev() {
            history[i] = history[i - 1];
        }
        history[0] = y;
        *sample = y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(freq: f32, rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| (TAU * freq * i as f32 / rate).sin())
            .collect()
    }

    #[test]
    fn whiten_then_resynthesize_with_same_envelope_is_near_identity() {
        // A resonant (formant-like) signal: sine + damped harmonic.
        let rate = 44_100.0;
        let window: Vec<f32> = sine(300.0, rate, 1024)
            .iter()
            .zip(sine(900.0, rate, 1024).iter())
            .map(|(a, b)| a + 0.5 * b)
            .collect();

        let mut analysis = LpcState::new();
        analysis.analyze(&window);
        let coeffs = analysis.coeffs();

        let mut whiten_state = LpcState::new();
        let mut residual = window.clone();
        whiten_state.whiten(&mut residual);

        let mut synth_state = LpcState::new();
        let mut resynth = residual.clone();
        synth_state.resynthesize(&coeffs, &mut resynth);

        // Round-tripping through whiten (with the *analysed* envelope, via
        // a fresh whiten pass driven by the same coefficients) then
        // resynthesize with the same envelope should approximately
        // reconstruct the original signal once the filter memory has
        // settled (skip the first `LPC_ORDER` samples).
        let mut whiten_with_analysis = window.clone();
        whiten_with(&coeffs, &mut [0.0; LPC_ORDER], &mut whiten_with_analysis);
        let mut round_trip = whiten_with_analysis.clone();
        resynthesize_with(&coeffs, &mut [0.0; LPC_ORDER], &mut round_trip);

        let skip = LPC_ORDER * 2;
        let max_err = window[skip..]
            .iter()
            .zip(round_trip[skip..].iter())
            .fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(max_err < 0.05, "round-trip error too large: {max_err}");
    }

    #[test]
    fn silent_window_leaves_envelope_unchanged() {
        let mut state = LpcState::new();
        state.analyze(&[0.0f32; 2048]);
        assert_eq!(state.coeffs(), [0.0; LPC_ORDER]);
    }

    #[test]
    fn short_window_is_a_noop() {
        let mut state = LpcState::new();
        state.analyze(&[1.0, 2.0, 3.0]);
        assert_eq!(state.coeffs(), [0.0; LPC_ORDER]);
    }

    /// Regression for the 2026-09-19 manual walk (M3): an unstable
    /// synthesis target (poles at radius 1.1) drove the filter memory to
    /// `inf`, whose step bound `inf - inf` is `NaN`, and `f32::clamp`
    /// panicked on the audio thread — aborting the process. The filter
    /// must now stay finite and bounded (and never panic) no matter what
    /// coefficients it is handed.
    #[test]
    fn unstable_target_envelope_never_panics_and_stays_bounded() {
        let mut coeffs = [0.0f32; LPC_ORDER];
        coeffs[0] = 2.2;
        coeffs[1] = -1.21; // y[n] = 2.2 y[n-1] - 1.21 y[n-2]: |poles| = 1.1
        let mut signal: Vec<f32> = sine(440.0, 44_100.0, 44_100);
        signal[0] = 1.0;

        let mut history = [0.0f32; LPC_ORDER];
        resynthesize_with(&coeffs, &mut history, &mut signal);

        assert!(signal.iter().all(|s| s.is_finite()), "non-finite output");
        let peak = signal.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            peak <= SYNTH_OUTPUT_CEILING,
            "output escaped the ceiling: {peak}"
        );
        assert!(history.iter().all(|h| h.is_finite()));
    }

    /// A `NaN`/`inf` input sample is dropped (both filters) and the
    /// synthesis memory restarts from silence — later finite samples come
    /// out finite.
    #[test]
    fn non_finite_input_is_dropped_not_propagated() {
        let coeffs = {
            let mut state = LpcState::new();
            state.analyze(&sine(300.0, 44_100.0, 1024));
            state.coeffs()
        };
        let mut block = sine(300.0, 44_100.0, 256);
        block[10] = f32::NAN;
        block[20] = f32::INFINITY;

        let mut whiten_state = LpcState::new();
        whiten_state.whiten(&mut block);
        assert!(block.iter().all(|s| s.is_finite()), "whiten leaked NaN");

        let mut synth_state = LpcState::new();
        synth_state.resynthesize(&coeffs, &mut block);
        assert!(block.iter().all(|s| s.is_finite()), "resynth leaked NaN");

        let mut direct = sine(300.0, 44_100.0, 256);
        direct[5] = f32::NAN;
        let mut history = [0.5f32; LPC_ORDER];
        resynthesize_with(&coeffs, &mut history, &mut direct);
        assert!(direct.iter().all(|s| s.is_finite()));
        assert!(history.iter().all(|h| h.is_finite()));
    }

    /// The real formant-correction pairing: whiten with one block's
    /// envelope, resynthesize with a slightly different block's (a
    /// near-pure tone, both models placing a pole near the unit circle).
    /// With white-noise correction + bandwidth expansion the mismatch must
    /// not compound into a runaway over ten seconds of audio.
    #[test]
    fn mismatched_envelopes_on_a_pure_tone_stay_bounded() {
        let rate = 44_100.0;
        let mut orig = LpcState::new();
        orig.analyze(&sine(440.0, rate, 1024));
        let orig_coeffs = orig.coeffs();

        let mut flatten = LpcState::new();
        let mut peak = 0.0f32;
        for block_index in 0..(10 * 44_100 / 512) {
            let start = block_index * 512;
            let mut block: Vec<f32> = (start..start + 512)
                .map(|i| (TAU * 445.0 * i as f32 / rate).sin() * 0.5)
                .collect();
            flatten.analyze(&block);
            flatten.whiten(&mut block);
            flatten.resynthesize(&orig_coeffs, &mut block);
            assert!(block.iter().all(|s| s.is_finite()));
            peak = peak.max(block.iter().fold(0.0f32, |m, s| m.max(s.abs())));
        }
        assert!(peak < 4.0, "mismatched envelopes ran away: peak {peak}");
    }

    /// `analyze` on a window carrying a non-finite sample keeps the
    /// previous envelope (its autocorrelation is meaningless).
    #[test]
    fn non_finite_window_leaves_envelope_unchanged() {
        let mut state = LpcState::new();
        state.analyze(&sine(300.0, 44_100.0, 1024));
        let before = state.coeffs();
        let mut poisoned = sine(300.0, 44_100.0, 1024);
        poisoned[100] = f32::NAN;
        state.analyze(&poisoned);
        assert_eq!(state.coeffs(), before);
        assert!(before.iter().all(|c| c.is_finite()));
    }
}

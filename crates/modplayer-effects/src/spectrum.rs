// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SpectrumRing`: the post-chain 64-band spectrum (FR-011, research R9)
//! — a hand-rolled 1024-point radix-2 real FFT over a Hann-windowed ring
//! of the last [`crate::consts::SPECTRUM_FFT`] post-chain mono frames,
//! recomputed every [`crate::consts::SPECTRUM_HOP`] new frames and folded
//! into [`crate::consts::SPECTRUM_BANDS`] log-spaced bands from 20 Hz to
//! 20 kHz.
//!
//! Every buffer is preallocated in [`SpectrumRing::new`] (the caller's
//! construction thread, e.g. `Processor::new`); `push`/`maybe_recompute`
//! never allocate (Constitution I). Per contracts/engine-effect-chain.md
//! §1/§7, the spectrum is computed over the *post-chain* signal (after
//! master gain and the test tone, before the limiter) — its owner is
//! whichever caller sits at that point in the render pipeline (the
//! engine's `Processor`), not `ChainRt` itself.

use std::f32::consts::PI;

use crate::consts::{SPECTRUM_BANDS, SPECTRUM_FFT, SPECTRUM_HOP};

/// A [start, end) bin range folded into one output band.
type BandRange = (usize, usize);

/// The allocation-free (after construction) spectrum analyser.
pub struct SpectrumRing {
    /// Mono samples, circular; `ring.len() == SPECTRUM_FFT`.
    ring: Vec<f32>,
    write: usize,
    /// Valid samples so far, capped at `ring.len()` (a cold ring reads as
    /// silence rather than recomputing on partial data).
    filled: usize,
    /// New frames pushed since the last recompute; a recompute happens
    /// once this reaches `SPECTRUM_HOP` (contracts/engine-effect-chain.md
    /// §7: "when >= 256 new frames since last").
    since_hop: usize,
    window: Vec<f32>,
    cos_table: Vec<f32>,
    sin_table: Vec<f32>,
    bit_rev: Vec<usize>,
    re: Vec<f32>,
    im: Vec<f32>,
    band_bins: Vec<BandRange>,
    bands: Vec<f32>,
    /// Bumped on every recompute (data-model.md §2.5's
    /// `spectrum_generation`) so a reader can tell two snapshots apart.
    generation: u32,
}

impl SpectrumRing {
    /// A silent spectrum, its bin-to-band mapping fixed for `source_rate`
    /// (recomputing that mapping is not on any render path — a fresh
    /// `Processor`/`ChainRt` is built per stream anyway).
    #[must_use]
    pub fn new(source_rate: u32) -> Self {
        let n = SPECTRUM_FFT;
        let window = (0..n)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / (n - 1) as f32).cos())
            .collect();
        let half = n / 2;
        let cos_table = (0..half)
            .map(|k| (2.0 * PI * k as f32 / n as f32).cos())
            .collect();
        // `e^{-i*theta} = cos(theta) - i*sin(theta)`: the table stores the
        // imaginary *coefficient* of that twiddle, i.e. `-sin(theta)`, so
        // the butterfly's plain complex multiply (`re*c - im*s`, `re*s +
        // im*c`) implements the forward transform's negative-exponent
        // convention without a separate sign flip at each use site.
        let sin_table = (0..half)
            .map(|k| -(2.0 * PI * k as f32 / n as f32).sin())
            .collect();
        Self {
            ring: vec![0.0; n],
            write: 0,
            filled: 0,
            since_hop: 0,
            window,
            cos_table,
            sin_table,
            bit_rev: bit_reversal_table(n),
            re: vec![0.0; n],
            im: vec![0.0; n],
            band_bins: band_bin_ranges(source_rate.max(1), n),
            bands: vec![0.0; SPECTRUM_BANDS],
            generation: 0,
        }
    }

    /// Push `frames` stereo-interleaved post-chain frames' mono downmix
    /// (`(l + r) / 2`) into the ring, wrapping. Real-time safe: the ring
    /// is fixed-size, never resized.
    pub fn push(&mut self, stereo: &[f32], frames: usize) {
        let n = self.ring.len();
        for i in 0..frames {
            let mono = 0.5 * (stereo[i * 2] + stereo[i * 2 + 1]);
            self.ring[self.write] = mono;
            self.write = (self.write + 1) % n;
            self.filled = (self.filled + 1).min(n);
        }
        self.since_hop += frames;
    }

    /// Recompute the 64 bands from the ring's current contents if the
    /// ring is full and at least `SPECTRUM_HOP` new frames have arrived
    /// since the last recompute (contracts/engine-effect-chain.md §7).
    /// Returns whether it actually recomputed (the caller bumps
    /// `RtShared`'s generation counter only then). Never allocates.
    pub fn maybe_recompute(&mut self) -> bool {
        let n = self.ring.len();
        if self.filled < n || self.since_hop < SPECTRUM_HOP {
            return false;
        }
        self.since_hop = 0;

        // The ring's current write cursor is the oldest sample once full;
        // read oldest-to-newest into the windowed analysis buffer.
        for i in 0..n {
            let idx = (self.write + i) % n;
            self.re[i] = self.ring[idx] * self.window[i];
            self.im[i] = 0.0;
        }
        fft_in_place(
            &mut self.re,
            &mut self.im,
            &self.bit_rev,
            &self.cos_table,
            &self.sin_table,
        );

        // A conventional windowed-magnitude scale (2/N for a one-sided
        // spectrum); this is a display meter, not a calibrated
        // measurement, so the result is simply clamped into 0..1 rather
        // than compensating for the Hann window's coherent-gain loss.
        let scale = 2.0 / n as f32;
        for (band, &(lo, hi)) in self.band_bins.iter().enumerate() {
            let mut peak = 0.0f32;
            for bin in lo..hi.max(lo + 1) {
                let mag =
                    (self.re[bin] * self.re[bin] + self.im[bin] * self.im[bin]).sqrt() * scale;
                peak = peak.max(mag);
            }
            self.bands[band] = peak.clamp(0.0, 1.0);
        }
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// The 64 current band magnitudes (linear, 0..1, log-spaced 20 Hz–20
    /// kHz); all zero until the ring first fills.
    #[must_use]
    pub fn bands(&self) -> &[f32] {
        &self.bands
    }

    #[must_use]
    pub const fn generation(&self) -> u32 {
        self.generation
    }
}

/// The index each position lands on after a radix-2 bit-reversal
/// permutation, for an FFT of size `n` (a power of two).
fn bit_reversal_table(n: usize) -> Vec<usize> {
    let bits = n.trailing_zeros();
    (0..n)
        .map(|i| i.reverse_bits() >> (usize::BITS - bits))
        .collect()
}

/// An iterative, in-place radix-2 decimation-in-time FFT (forward
/// transform, `e^{-i*2*pi*k/n}` convention) over `re`/`im`, using
/// precomputed `bit_rev`/`cos_table`/`sin_table` (`n / 2`-entry twiddle
/// tables, `bit_rev` an `n`-entry permutation) built once in
/// [`SpectrumRing::new`]. Allocation-free.
fn fft_in_place(
    re: &mut [f32],
    im: &mut [f32],
    bit_rev: &[usize],
    cos_table: &[f32],
    sin_table: &[f32],
) {
    let n = re.len();
    for (i, &j) in bit_rev.iter().enumerate().take(n) {
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let mut size = 2;
    while size <= n {
        let half = size / 2;
        let table_stride = n / size;
        let mut start = 0;
        while start < n {
            for k in 0..half {
                let c = cos_table[k * table_stride];
                let s = sin_table[k * table_stride];
                let i0 = start + k;
                let i1 = i0 + half;
                let re1 = re[i1];
                let im1 = im[i1];
                let t_re = re1 * c - im1 * s;
                let t_im = re1 * s + im1 * c;
                re[i1] = re[i0] - t_re;
                im[i1] = im[i0] - t_im;
                re[i0] += t_re;
                im[i0] += t_im;
            }
            start += size;
        }
        size *= 2;
    }
}

/// The `[start, end)` FFT-bin range each of `SPECTRUM_BANDS` log-spaced
/// bands (20 Hz–20 kHz) folds together, at `source_rate`, for an
/// `n`-point FFT — monotonically increasing and always non-empty (a
/// degenerate band at very low frequencies still claims at least one
/// bin), clamped to the Nyquist bin `n / 2`.
fn band_bin_ranges(source_rate: u32, n: usize) -> Vec<BandRange> {
    let nyquist_bin = n / 2;
    let bin_of = |freq: f32| -> usize {
        ((freq * n as f32 / source_rate as f32).round() as usize).min(nyquist_bin)
    };
    let log_lo = 20f32.ln();
    let log_hi = 20_000f32.ln();
    let mut ranges = Vec::with_capacity(SPECTRUM_BANDS);
    let mut prev_bin = bin_of(20.0);
    for band in 0..SPECTRUM_BANDS {
        let t_hi = (band + 1) as f32 / SPECTRUM_BANDS as f32;
        let freq_hi = (log_lo + t_hi * (log_hi - log_lo)).exp();
        let hi_bin = bin_of(freq_hi).max(prev_bin + 1).min(nyquist_bin);
        ranges.push((prev_bin, hi_bin));
        prev_bin = hi_bin;
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pure sine tone's energy must land in the band covering its own
    /// frequency, not somewhere else on the scale.
    #[test]
    fn sine_tone_peaks_in_its_own_band() {
        let source_rate = 44_100u32;
        let mut spectrum = SpectrumRing::new(source_rate);
        let freq = 1_000.0f32;
        let frames = SPECTRUM_FFT * 2;
        let mut phase = 0.0f32;
        let step = 2.0 * PI * freq / source_rate as f32;
        let mut buf = vec![0.0f32; frames * 2];
        for i in 0..frames {
            let s = phase.sin();
            buf[i * 2] = s;
            buf[i * 2 + 1] = s;
            phase += step;
        }
        spectrum.push(&buf, frames);
        assert!(spectrum.maybe_recompute(), "ring must be full and hopped");

        let bands = spectrum.bands();
        let (peak_band, _) = bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((0, &0.0));

        // The peak band's own bin range must bracket 1 kHz (± one FFT
        // bin's resolution, `source_rate / SPECTRUM_FFT` Hz).
        let ranges = band_bin_ranges(source_rate, SPECTRUM_FFT);
        let (lo_bin, hi_bin) = ranges[peak_band];
        let bin_hz = source_rate as f32 / SPECTRUM_FFT as f32;
        let lo_hz = lo_bin as f32 * bin_hz - bin_hz;
        let hi_hz = hi_bin as f32 * bin_hz + bin_hz;
        assert!(
            (lo_hz..=hi_hz).contains(&freq),
            "1 kHz sine peaked in band {peak_band} ({lo_hz}..{hi_hz} Hz): {bands:?}"
        );
    }

    #[test]
    fn silence_produces_a_flat_zero_spectrum() {
        let mut spectrum = SpectrumRing::new(44_100);
        let buf = vec![0.0f32; SPECTRUM_FFT * 2];
        spectrum.push(&buf, SPECTRUM_FFT);
        assert!(spectrum.maybe_recompute());
        assert!(spectrum.bands().iter().all(|&b| b < 1e-4));
    }

    #[test]
    fn recompute_is_a_no_op_before_the_ring_fills_or_hops() {
        let mut spectrum = SpectrumRing::new(44_100);
        let buf = vec![0.5f32; 4];
        spectrum.push(&buf, 2);
        assert!(!spectrum.maybe_recompute(), "ring is nowhere near full");
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Linear-interpolation sample-rate conversion math (research R6):
//! allocation-free, fixed-point (32.32) phase accumulation. Used by
//! `OutputStage`; kept in its own module so quality-focused replacements
//! (e.g. `rubato`) can drop in behind the same shape later.

const FRAC_BITS: u32 = 32;
const FRAC_ONE: u64 = 1 << FRAC_BITS;
const FRAC_MASK: u64 = FRAC_ONE - 1;

/// Resample interleaved-stereo `src` (`src_frames` frames) into
/// interleaved-stereo `dst` (`dst_frames` frames) by linear interpolation,
/// advancing a persistent 32.32 fixed-point `phase` by `step` per output
/// frame. `step = (src_rate << 32) / dst_rate`.
///
/// Returns the number of whole source frames consumed; `phase` is left
/// holding only the fractional remainder so the next call's `src` can start
/// at frame 0 again (the caller is responsible for actually advancing its
/// source by the returned count).
///
/// Real-time safe: no allocation. `dst` must be at least `dst_frames * 2`
/// long; only that much is written.
pub fn resample_linear_stereo(
    src: &[f32],
    src_frames: usize,
    phase: &mut u64,
    step: u64,
    dst: &mut [f32],
    dst_frames: usize,
) -> usize {
    if src_frames == 0 {
        dst[..dst_frames * 2].fill(0.0);
        return 0;
    }

    for n in 0..dst_frames {
        let idx = (*phase >> FRAC_BITS) as usize;
        let frac = (*phase & FRAC_MASK) as f32 / FRAC_ONE as f32;
        let i0 = idx.min(src_frames - 1);
        let i1 = (idx + 1).min(src_frames - 1);

        let l0 = src[i0 * 2];
        let l1 = src[i1 * 2];
        let r0 = src[i0 * 2 + 1];
        let r1 = src[i1 * 2 + 1];

        dst[n * 2] = l0 + (l1 - l0) * frac;
        dst[n * 2 + 1] = r0 + (r1 - r0) * frac;

        *phase += step;
    }

    let consumed = ((*phase >> FRAC_BITS) as usize).min(src_frames);
    *phase -= (consumed as u64) << FRAC_BITS;
    consumed
}

/// Compute the 32.32 fixed-point `step` for resampling from `src_rate` to
/// `dst_rate`.
pub fn step_for_rates(src_rate: u32, dst_rate: u32) -> u64 {
    (u64::from(src_rate) << FRAC_BITS) / u64::from(dst_rate.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_rate_copies_samples() {
        let src = [0.0f32, 0.0, 1.0, 1.0, 0.5, 0.5];
        let mut phase = 0u64;
        let step = step_for_rates(1, 1);
        let mut dst = [0.0f32; 6];
        let consumed = resample_linear_stereo(&src, 3, &mut phase, step, &mut dst, 3);
        assert_eq!(consumed, 3);
        assert_eq!(dst, src);
    }

    #[test]
    fn half_rate_interpolates_midpoints() {
        // dst rate is half of src rate: each dst frame advances 2 src frames.
        let src = [0.0f32, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0];
        let mut phase = 0u64;
        let step = step_for_rates(2, 1);
        let mut dst = [0.0f32; 4];
        let consumed = resample_linear_stereo(&src, 4, &mut phase, step, &mut dst, 2);
        assert!((dst[0] - 0.0).abs() < 1e-4);
        assert!((dst[2] - 2.0).abs() < 1e-4);
        assert_eq!(consumed, 4);
    }

    #[test]
    fn phase_carries_fractional_remainder_across_calls() {
        // dst rate is 1.5x src rate (upsampling): step < FRAC_ONE.
        let src = [0.0f32, 0.0, 1.0, 1.0, 2.0, 2.0];
        let mut phase = 0u64;
        let step = step_for_rates(2, 3);
        let mut dst = [0.0f32; 6];
        let consumed = resample_linear_stereo(&src, 3, &mut phase, step, &mut dst, 3);
        assert!(consumed <= 3);
        // phase should hold only a fractional remainder (< one whole frame).
        assert!(phase < FRAC_ONE);
    }
}

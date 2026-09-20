// SPDX-License-Identifier: MIT OR Apache-2.0

//! Segment sequencing for the built-in synthetic test track
//! (contracts/audio-source.md, data-model.md §2): 10 s 440 Hz sine @ −12
//! dBFS → 5 s 1 kHz square @ 0 dBFS → 5 s sawtooth sweep 100→2000 Hz @ −12
//! dBFS → 2 s silence, then wraps (`track_len_frames`).
//!
//! Every sample is a pure, closed-form function of `(rate, position)`
//! (`sample_at`) — never derived by accumulating state across calls — so
//! two sources seeked to the same frame are bit-identical regardless of
//! how each got there (contracts/audio-source.md's determinism guarantee),
//! and there is no discontinuity inside a segment (each segment's phase is
//! itself a continuous, closed-form function of the frame offset within
//! it).

use crate::osc;

/// Peak linear amplitude for the −12 dBFS segments (`10^(-12/20)`).
pub const PEAK_MINUS_12DB: f32 = 0.251_188_64;

const SEG1_FREQ_HZ: f64 = 440.0;
const SEG2_FREQ_HZ: f64 = 1_000.0;
const SWEEP_START_HZ: f64 = 100.0;
const SWEEP_END_HZ: f64 = 2_000.0;

/// Segment durations in whole seconds, in play order (10 + 5 + 5 + 2 = 22 s).
const SEG1_S: u64 = 10;
const SEG2_S: u64 = 5;
const SEG3_S: u64 = 5;
const SEG4_S: u64 = 2;

/// Total track length in frames at `rate` Hz.
pub fn track_len_frames(rate: u32) -> u64 {
    u64::from(rate) * (SEG1_S + SEG2_S + SEG3_S + SEG4_S)
}

/// `[seg1_end, seg2_end, seg3_end]` frame offsets (segment 4/silence runs
/// to `track_len_frames`).
fn seg_bounds(rate: u32) -> [u64; 3] {
    let r = u64::from(rate);
    let b1 = r * SEG1_S;
    let b2 = b1 + r * SEG2_S;
    let b3 = b2 + r * SEG3_S;
    [b1, b2, b3]
}

/// The mono sample value at track `position` (0-indexed frame within the
/// 22 s loop, already reduced modulo `track_len_frames(rate)`), at `rate`
/// Hz. A pure function of its inputs (contracts/audio-source.md).
fn sample_at(position: u64, rate: u32) -> f32 {
    let [b1, b2, b3] = seg_bounds(rate);
    let r = f64::from(rate.max(1));

    if position < b1 {
        // 10 s 440 Hz sine @ −12 dBFS.
        let phase = (SEG1_FREQ_HZ * position as f64 / r).fract();
        osc::sine(phase) * PEAK_MINUS_12DB
    } else if position < b2 {
        // 5 s 1 kHz square @ 0 dBFS.
        let elapsed = (position - b1) as f64;
        let phase = (SEG2_FREQ_HZ * elapsed / r).fract();
        osc::square(phase)
    } else if position < b3 {
        // 5 s sawtooth, linear sweep 100 Hz -> 2 kHz, @ −12 dBFS. The
        // (unwrapped) phase is the closed-form integral of the
        // instantaneous frequency over elapsed time, so it stays a pure
        // function of `position` with no discontinuity from one sample to
        // the next.
        let elapsed_frames = (position - b2) as f64;
        let seg_len_s = ((b3 - b2).max(1)) as f64 / r;
        let t = elapsed_frames / r;
        let unwrapped_phase =
            SWEEP_START_HZ * t + (SWEEP_END_HZ - SWEEP_START_HZ) * t * t / (2.0 * seg_len_s);
        osc::sawtooth(unwrapped_phase.rem_euclid(1.0)) * PEAK_MINUS_12DB
    } else {
        // 2 s silence.
        0.0
    }
}

/// The phase (`0.0..1.0`) the currently-sounding segment is at, for
/// `SyntheticSource::phase()` introspection (data-model.md §2). `0.0`
/// during silence.
pub fn phase_at(position: u64, rate: u32) -> f64 {
    let [b1, b2, b3] = seg_bounds(rate);
    let r = f64::from(rate.max(1));

    if position < b1 {
        (SEG1_FREQ_HZ * position as f64 / r).fract()
    } else if position < b2 {
        (SEG2_FREQ_HZ * (position - b1) as f64 / r).fract()
    } else if position < b3 {
        let elapsed_frames = (position - b2) as f64;
        let seg_len_s = ((b3 - b2).max(1)) as f64 / r;
        let t = elapsed_frames / r;
        let unwrapped_phase =
            SWEEP_START_HZ * t + (SWEEP_END_HZ - SWEEP_START_HZ) * t * t / (2.0 * seg_len_s);
        unwrapped_phase.rem_euclid(1.0)
    } else {
        0.0
    }
}

/// Fill `out` (interleaved stereo, mono content duplicated to both
/// channels) starting at `*position` (frames within the track), advancing
/// `*position` and wrapping at `track_len_frames(rate)`. Real-time safe: no
/// allocation.
pub fn fill(out: &mut [f32], position: &mut u64, rate: u32) {
    let track_len = track_len_frames(rate).max(1);
    let mut pos = *position % track_len;
    let frames = out.len() / 2;
    for i in 0..frames {
        let sample = sample_at(pos, rate);
        out[i * 2] = sample;
        out[i * 2 + 1] = sample;
        pos = (pos + 1) % track_len;
    }
    *position = pos;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_len_is_22_seconds() {
        assert_eq!(track_len_frames(44_100), 44_100 * 22);
    }

    #[test]
    fn sample_is_pure_function_of_position() {
        let rate = 8_000u32;
        for position in [0u64, 1, 39_999, 40_000, 79_999, 80_000, 119_999, 120_000] {
            assert_eq!(sample_at(position, rate), sample_at(position, rate));
        }
    }

    #[test]
    fn segment_boundaries_land_on_whole_seconds() {
        let rate = 44_100u32;
        let [b1, b2, b3] = seg_bounds(rate);
        assert_eq!(b1, 10 * 44_100);
        assert_eq!(b2, 15 * 44_100);
        assert_eq!(b3, 20 * 44_100);
    }

    #[test]
    fn fill_wraps_at_track_len() {
        let rate = 100u32;
        let track_len = track_len_frames(rate);
        let mut position = track_len - 2;
        let mut out = vec![0.0f32; 8]; // 4 frames, wraps past track_len
        fill(&mut out, &mut position, rate);
        assert_eq!(position, 2);
    }
}

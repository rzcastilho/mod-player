// SPDX-License-Identifier: MIT OR Apache-2.0

//! The first-launch/device-check test tone (contracts/audio-source.md,
//! data-model.md §2): 440 Hz sine, 1.0 s, 10 ms linear fade-in/fade-out,
//! −20 dBFS peak (linear 0.1). `render_add` sums into the caller's buffer —
//! it never overwrites — so it composes with whatever the engine already
//! placed there (US1 T045 wires this in after master gain, before the
//! limiter).

use crate::osc::{Oscillator, sine};

/// Frequency of the test tone.
pub const FREQ_HZ: f64 = 440.0;
/// Total duration of the test tone.
pub const DURATION_S: f64 = 1.0;
/// Duration of each fade (in and out).
pub const FADE_S: f64 = 0.01;
/// Peak linear amplitude (−20 dBFS).
pub const PEAK: f32 = 0.1;

/// Linear fade-in/fade-out envelope for a tone of `total_frames`, fading
/// over `fade_frames` at each end. Pure function of frame index so it is
/// independently testable for monotonicity (contracts/audio-source.md).
fn envelope(elapsed_frames: u64, total_frames: u64, fade_frames: u64) -> f32 {
    let fade_frames = fade_frames.max(1);
    if elapsed_frames < fade_frames {
        elapsed_frames as f32 / fade_frames as f32
    } else if total_frames.saturating_sub(elapsed_frames) <= fade_frames {
        total_frames.saturating_sub(elapsed_frames) as f32 / fade_frames as f32
    } else {
        1.0
    }
}

/// The test tone: 440 Hz sine, 1.0 s, 10 ms linear fades, −20 dBFS peak.
#[derive(Debug, Clone)]
pub struct TestTone {
    rate: u32,
    elapsed_frames: u64,
    osc: Oscillator,
}

impl TestTone {
    /// Construct a fresh tone at `rate` Hz, starting from its fade-in.
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate.max(1),
            elapsed_frames: 0,
            osc: Oscillator::new(),
        }
    }

    fn total_frames(&self) -> u64 {
        (f64::from(self.rate) * DURATION_S).round() as u64
    }

    fn fade_frames(&self) -> u64 {
        (f64::from(self.rate) * FADE_S).round().max(1.0) as u64
    }

    /// `true` once the tone has played its full 1.0 s duration.
    pub fn is_finished(&self) -> bool {
        self.elapsed_frames >= self.total_frames()
    }

    /// Sum this tone's contribution into `out` (interleaved stereo,
    /// `out.len() / 2` frames), advancing it by that many frames. Returns
    /// `true` while the tone is still playing after this call (i.e. more
    /// frames remain), `false` once it has completed.
    ///
    /// Real-time safe: no allocation.
    pub fn render_add(&mut self, out: &mut [f32]) -> bool {
        let total = self.total_frames();
        let fade = self.fade_frames();
        let rate = f64::from(self.rate);
        let frames = out.len() / 2;

        for i in 0..frames {
            if self.elapsed_frames >= total {
                break;
            }
            let phase = self.osc.advance(FREQ_HZ, rate);
            let env = envelope(self.elapsed_frames, total, fade);
            let sample = sine(phase) * PEAK * env;
            out[i * 2] += sample;
            out[i * 2 + 1] += sample;
            self.elapsed_frames += 1;
        }

        !self.is_finished()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_ramps_monotonically_in_first_and_last_ten_ms() {
        let rate = 44_100u32;
        let total = (f64::from(rate) * DURATION_S).round() as u64;
        let fade = (f64::from(rate) * FADE_S).round() as u64;

        // First 10 ms: strictly non-decreasing, 0.0 -> ~1.0.
        let mut previous = -1.0f32;
        for frame in 0..fade {
            let value = envelope(frame, total, fade);
            assert!(
                value >= previous,
                "envelope must not decrease during fade-in"
            );
            previous = value;
        }
        assert!((envelope(0, total, fade)).abs() < 1e-9);

        // Last 10 ms: strictly non-increasing, ~1.0 -> ~0.0.
        let mut previous = 2.0f32;
        for frame in (total - fade)..total {
            let value = envelope(frame, total, fade);
            assert!(
                value <= previous,
                "envelope must not increase during fade-out"
            );
            previous = value;
        }
    }

    #[test]
    fn peak_amplitude_is_point_one() {
        let mut tone = TestTone::new(44_100);
        let mut out = vec![0.0f32; (tone.total_frames() as usize) * 2];
        tone.render_add(&mut out);
        let peak = out.iter().fold(0.0f32, |max, &s| max.max(s.abs()));
        assert!((peak - 0.1).abs() < 1e-6, "peak was {peak}");
    }

    #[test]
    fn is_finished_after_one_second_times_rate_frames() {
        let rate = 8_000u32;
        let mut tone = TestTone::new(rate);
        let mut out = vec![0.0f32; (rate as usize - 1) * 2];
        let still_playing = tone.render_add(&mut out);
        assert!(still_playing);
        assert!(!tone.is_finished());

        let mut last = vec![0.0f32; 2];
        let still_playing = tone.render_add(&mut last);
        assert!(!still_playing);
        assert!(tone.is_finished());
    }

    #[test]
    fn render_add_sums_rather_than_overwrites() {
        let mut tone = TestTone::new(44_100);
        let mut out = vec![0.3f32; 8];
        tone.render_add(&mut out);
        // Every sample must differ from the pre-existing 0.3 by exactly the
        // tone's own contribution, i.e. never simply replaced with 0.
        assert!(out.iter().all(|&s| s != 0.0));
    }

    #[test]
    fn render_add_stops_adding_once_finished_mid_buffer() {
        let rate = 4u32; // total_frames = 4, fade_frames = 1 (rounds up from 0.04)
        let mut tone = TestTone::new(rate);
        let mut out = vec![0.0f32; 12]; // 6 frames, tone lasts only 4
        let still_playing = tone.render_add(&mut out);
        assert!(!still_playing);
        assert!(tone.is_finished());
        // Frames 4 and 5 (indices 8..12) must be untouched (still 0.0).
        assert_eq!(&out[8..12], &[0.0, 0.0, 0.0, 0.0]);
    }
}

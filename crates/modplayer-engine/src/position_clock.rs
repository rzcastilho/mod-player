// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PositionClock`: derives a >= 60 Hz position estimate from `RtShared`'s
//! position anchor (engine-delta.md §3) without touching the audio thread.
//! At the Safe buffer preset (1024 frames) the render rate is only about
//! 43 Hz, below FR-006's 60 Hz, so per-render publication alone cannot
//! satisfy it; this extrapolates between renders using `Instant::now()`
//! (a non-blocking vDSO/`mach_absolute_time` read, called from the UI
//! thread here, not from `render`).

use std::time::{Duration, Instant};

use crate::shared::RtShared;

/// Reads `shared`'s position anchor and extrapolates it to `Instant::now()`
/// at `source_rate` frames/second while playing; returns the frozen anchor
/// otherwise (paused/stopped/buffering — no drift).
pub struct PositionClock;

impl PositionClock {
    pub fn now(shared: &RtShared, source_rate: u32) -> Duration {
        let anchor = shared.read_anchor();
        let position_frames = if anchor.playing {
            let elapsed = Instant::now().saturating_duration_since(anchor.instant);
            let extra_frames = (elapsed.as_secs_f64() * f64::from(source_rate.max(1))) as u64;
            // Capped at `position + one_buffer_duration * 2` (engine-delta.md
            // §3) so a stalled anchor (e.g. the audio thread wedged) cannot
            // make position run away unboundedly.
            let cap = u64::from(anchor.buffer_frames) * 2;
            anchor.position_frames + extra_frames.min(cap)
        } else {
            anchor.position_frames
        };
        frames_to_duration(position_frames, source_rate)
    }
}

fn frames_to_duration(frames: u64, rate: u32) -> Duration {
    Duration::from_secs_f64(frames as f64 / f64::from(rate.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn frozen_while_not_playing() {
        let shared = RtShared::new();
        shared.write_anchor(44_100, Instant::now(), false, 256);
        let d1 = PositionClock::now(&shared, 44_100);
        thread::sleep(Duration::from_millis(20));
        let d2 = PositionClock::now(&shared, 44_100);
        assert_eq!(d1, d2, "position must not drift while not playing");
        assert_eq!(d1, Duration::from_secs(1));
    }

    #[test]
    fn extrapolates_while_playing() {
        let shared = RtShared::new();
        shared.write_anchor(0, Instant::now(), true, 1024);
        thread::sleep(Duration::from_millis(20));
        let elapsed = PositionClock::now(&shared, 44_100);
        assert!(
            elapsed > Duration::ZERO,
            "position must advance while playing"
        );
    }

    #[test]
    fn extrapolation_is_capped_at_two_buffers() {
        let shared = RtShared::new();
        // A tiny buffer (1 frame) with a long-stale anchor: extrapolation
        // must not run away past the cap.
        shared.write_anchor(0, Instant::now() - Duration::from_secs(10), true, 1);
        let position = PositionClock::now(&shared, 44_100);
        let cap = frames_to_duration(2, 44_100);
        assert!(
            position <= cap,
            "extrapolation must be capped at two buffers"
        );
    }
}

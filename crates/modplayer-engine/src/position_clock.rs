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
            // 008, research R7: the playhead advances at `source_rate *
            // advance_rate` while an engaged time-stretch stage changes
            // tempo (`1.0` at unity, unchanged from 001-006's behaviour).
            let rate = f64::from(source_rate.max(1)) * f64::from(anchor.advance_rate);
            let extra_frames = (elapsed.as_secs_f64() * rate) as u64;
            // Capped at `position + one_buffer_duration * 2` (engine-delta.md
            // §3) so a stalled anchor (e.g. the audio thread wedged) cannot
            // make position run away unboundedly.
            let cap = u64::from(anchor.buffer_frames) * 2;
            anchor.position_frames + extra_frames.min(cap)
        } else {
            anchor.position_frames
        };
        // A render that runs late publishes an anchor whose true position
        // is behind what was already extrapolated (by at most the cap
        // above); hold the last report across that rather than stepping
        // the playhead backwards. Anything further back is a real seek or
        // track change and passes straight through.
        let hold_within = u64::from(anchor.buffer_frames) * 2;
        let position_frames = shared.monotonic_position(position_frames, hold_within);
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
        shared.write_anchor(44_100, Instant::now(), false, 256, 1.0);
        let d1 = PositionClock::now(&shared, 44_100);
        thread::sleep(Duration::from_millis(20));
        let d2 = PositionClock::now(&shared, 44_100);
        assert_eq!(d1, d2, "position must not drift while not playing");
        assert_eq!(d1, Duration::from_secs(1));
    }

    #[test]
    fn extrapolates_while_playing() {
        let shared = RtShared::new();
        shared.write_anchor(0, Instant::now(), true, 1024, 1.0);
        thread::sleep(Duration::from_millis(20));
        let elapsed = PositionClock::now(&shared, 44_100);
        assert!(
            elapsed > Duration::ZERO,
            "position must advance while playing"
        );
    }

    /// A late anchor whose true position is behind the extrapolated
    /// report must not step the clock backwards; a real seek back must.
    #[test]
    fn late_anchor_never_steps_backwards_but_a_seek_does() {
        let shared = RtShared::new();
        // Anchor at 10_000 frames; after well over two buffers' worth of
        // wall time the extrapolation sits at the cap, 10_000 + 512.
        shared.write_anchor(10_000, Instant::now(), true, 256, 1.0);
        thread::sleep(Duration::from_millis(30));
        let ahead = PositionClock::now(&shared, 44_100);
        assert!(
            ahead > frames_to_duration(10_500, 44_100),
            "sanity: extrapolation must have reached the cap: {ahead:?}"
        );
        // The render thread now publishes the real position: only one
        // buffer actually rendered.
        shared.write_anchor(10_256, Instant::now(), true, 256, 1.0);
        let after = PositionClock::now(&shared, 44_100);
        assert!(
            after >= ahead,
            "late anchor stepped back: {ahead:?} -> {after:?}"
        );

        // A seek back well beyond the cap is a real move and passes.
        shared.write_anchor(1_000, Instant::now(), true, 256, 1.0);
        let seeked = PositionClock::now(&shared, 44_100);
        assert!(
            seeked < after,
            "a seek must move the clock back: {seeked:?}"
        );
        assert!(seeked < Duration::from_millis(40));
    }

    #[test]
    fn extrapolation_is_capped_at_two_buffers() {
        let shared = RtShared::new();
        // A tiny buffer (1 frame) with a long-stale anchor: extrapolation
        // must not run away past the cap.
        shared.write_anchor(0, Instant::now() - Duration::from_secs(10), true, 1, 1.0);
        let position = PositionClock::now(&shared, 44_100);
        let cap = frames_to_duration(2, 44_100);
        assert!(
            position <= cap,
            "extrapolation must be capped at two buffers"
        );
    }
}

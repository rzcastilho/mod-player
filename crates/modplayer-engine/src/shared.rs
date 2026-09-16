// SPDX-License-Identifier: MIT OR Apache-2.0

//! `RtShared`: the atomics shared between a running `Processor` and the
//! controller/UI, wrapped in an `Arc` created once per app run and never
//! replaced — this is what lets the audio clock survive stream rebuilds
//! without a lock (research R3, contracts/engine-commands.md).
//!
//! The position anchor (engine-delta.md §3) is a seqlock: `Processor`
//! writes `(position, instant, playing, buffer_frames)` once per render,
//! bracketed by two `anchor_generation` increments; `PositionClock`
//! (`position_clock.rs`) retries its read whenever it observes an odd or
//! changing generation, so it never sees a torn write.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Atomics shared across a `Processor`'s lifetime and every rebuild of it.
#[derive(Debug)]
pub struct RtShared {
    /// Monotonic audio clock in source-rate frames consumed; never reset,
    /// never decreases (FR-006, FR-025c).
    clock_frames: AtomicU64,
    /// Track position within the looping source; advances only while
    /// `Playing`.
    position_frames: AtomicU64,
    /// `f32` bits of the max `|sample|` at the limiter output for the last
    /// rendered buffer.
    peak_bits: AtomicU32,
    /// Frames actually delivered per callback, as observed by the output
    /// backend adapter; 0 until the first callback.
    negotiated_frames: AtomicU32,
    /// Process-start reference instant that `anchor_instant_nanos` is
    /// measured from (engine-delta.md §3); captured once, immutable.
    epoch: Instant,
    anchor_position_frames: AtomicU64,
    anchor_instant_nanos: AtomicU64,
    /// Seqlock generation: even when quiescent; a writer increments it
    /// before and after publishing the anchor pair.
    anchor_generation: AtomicU64,
    anchor_playing: AtomicBool,
    /// Source frames the render that produced the current anchor actually
    /// covered; bounds `PositionClock`'s extrapolation ("capped at
    /// position + one_buffer_duration × 2", engine-delta.md §3).
    anchor_buffer_frames: AtomicU32,
}

impl Default for RtShared {
    fn default() -> Self {
        Self::new()
    }
}

/// A torn-free snapshot of the position anchor (engine-delta.md §3).
#[derive(Debug, Clone, Copy)]
pub struct AnchorSnapshot {
    pub position_frames: u64,
    pub instant: Instant,
    pub playing: bool,
    pub buffer_frames: u32,
}

impl RtShared {
    pub fn new() -> Self {
        Self {
            clock_frames: AtomicU64::new(0),
            position_frames: AtomicU64::new(0),
            peak_bits: AtomicU32::new(0),
            negotiated_frames: AtomicU32::new(0),
            epoch: Instant::now(),
            anchor_position_frames: AtomicU64::new(0),
            anchor_instant_nanos: AtomicU64::new(0),
            anchor_generation: AtomicU64::new(0),
            anchor_playing: AtomicBool::new(false),
            anchor_buffer_frames: AtomicU32::new(0),
        }
    }

    /// Publish the position anchor for the render just completed
    /// (engine-delta.md §3); called once per render, on the audio thread.
    pub fn write_anchor(
        &self,
        position_frames: u64,
        now: Instant,
        playing: bool,
        buffer_frames: u32,
    ) {
        self.anchor_generation.fetch_add(1, Ordering::AcqRel);
        self.anchor_position_frames
            .store(position_frames, Ordering::Relaxed);
        self.anchor_instant_nanos.store(
            now.saturating_duration_since(self.epoch).as_nanos() as u64,
            Ordering::Relaxed,
        );
        self.anchor_playing.store(playing, Ordering::Relaxed);
        self.anchor_buffer_frames
            .store(buffer_frames, Ordering::Relaxed);
        self.anchor_generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Read the position anchor with a seqlock retry loop. Called from the
    /// UI thread; the spin only ever contends with a single writer's brief
    /// store sequence.
    pub fn read_anchor(&self) -> AnchorSnapshot {
        loop {
            let g1 = self.anchor_generation.load(Ordering::Acquire);
            if !g1.is_multiple_of(2) {
                continue;
            }
            let position_frames = self.anchor_position_frames.load(Ordering::Relaxed);
            let instant_nanos = self.anchor_instant_nanos.load(Ordering::Relaxed);
            let playing = self.anchor_playing.load(Ordering::Relaxed);
            let buffer_frames = self.anchor_buffer_frames.load(Ordering::Relaxed);
            let g2 = self.anchor_generation.load(Ordering::Acquire);
            if g1 == g2 {
                return AnchorSnapshot {
                    position_frames,
                    instant: self.epoch + Duration::from_nanos(instant_nanos),
                    playing,
                    buffer_frames,
                };
            }
        }
    }

    /// Current clock value (`Acquire`).
    pub fn clock_frames(&self) -> u64 {
        self.clock_frames.load(Ordering::Acquire)
    }

    /// Advance the clock by `delta` frames (`Release`). Called once per
    /// buffer, at the end of `render`.
    pub fn advance_clock(&self, delta: u64) {
        self.clock_frames.fetch_add(delta, Ordering::Release);
    }

    /// Current track position (`Acquire`).
    pub fn position_frames(&self) -> u64 {
        self.position_frames.load(Ordering::Acquire)
    }

    /// Publish a new track position (`Release`).
    pub fn set_position_frames(&self, position: u64) {
        self.position_frames.store(position, Ordering::Release);
    }

    /// Max `|sample|` at the limiter output for the last buffer.
    pub fn peak(&self) -> f32 {
        f32::from_bits(self.peak_bits.load(Ordering::Relaxed))
    }

    /// Publish the peak for the buffer just rendered.
    pub fn set_peak(&self, peak: f32) {
        self.peak_bits.store(peak.to_bits(), Ordering::Relaxed);
    }

    /// Frames per callback, as observed by the output backend adapter.
    pub fn negotiated_frames(&self) -> u32 {
        self.negotiated_frames.load(Ordering::Relaxed)
    }

    /// Publish the observed callback frame count (the output backend
    /// adapter's job — see contracts/output-backend.md).
    pub fn set_negotiated_frames(&self, frames: u32) {
        self.negotiated_frames.store(frames, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_advances_and_never_resets() {
        let shared = RtShared::new();
        assert_eq!(shared.clock_frames(), 0);
        shared.advance_clock(128);
        shared.advance_clock(256);
        assert_eq!(shared.clock_frames(), 384);
    }

    #[test]
    fn peak_round_trips_through_bits() {
        let shared = RtShared::new();
        shared.set_peak(0.75);
        assert!((shared.peak() - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn anchor_round_trips_without_tearing() {
        let shared = RtShared::new();
        let before = shared.read_anchor();
        assert_eq!(before.position_frames, 0);
        assert!(!before.playing);

        let now = Instant::now();
        shared.write_anchor(1_000, now, true, 256);
        let snapshot = shared.read_anchor();
        assert_eq!(snapshot.position_frames, 1_000);
        assert!(snapshot.playing);
        assert_eq!(snapshot.buffer_frames, 256);
        assert!(snapshot.instant >= now);
    }
}

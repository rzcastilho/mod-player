// SPDX-License-Identifier: MIT OR Apache-2.0

//! `RtShared`: the atomics shared between a running `Processor` and the
//! controller/UI, wrapped in an `Arc` created once per app run and never
//! replaced — this is what lets the audio clock survive stream rebuilds
//! without a lock (research R3, contracts/engine-commands.md).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Atomics shared across a `Processor`'s lifetime and every rebuild of it.
#[derive(Debug, Default)]
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
}

impl RtShared {
    pub fn new() -> Self {
        Self::default()
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
}

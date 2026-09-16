// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SourceHost`: the off-real-time half of the additive `AudioSource`
//! extension (contracts/audio-source-host.md §1). Owned by the
//! `PlaybackController` on the UI thread; never touched from the audio
//! callback. `AudioSource` itself (the real-time half) is unchanged.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::AudioSource;
use crate::types::{BufferStatus, SourceCommand, SourceEvent, SourceHealth};

/// Off-real-time handle to an audio source.
pub trait SourceHost: Send + 'static {
    /// The real-time half handed to a `Processor`. One fresh value per
    /// stream (re)build; `position_frames` restores the read position.
    type Rt: AudioSource;

    /// Build (or rebuild) the real-time half. Called on every stream open;
    /// the previous `Rt` is dropped with the previous stream. Must not
    /// block on network; may allocate (off the real-time path).
    fn attach(&mut self, position_frames: u64) -> Self::Rt;

    /// Non-blocking. Commands are applied asynchronously; their effects
    /// arrive as events.
    fn command(&mut self, cmd: SourceCommand);

    /// Drain every event raised since the last call (FIFO). Non-blocking.
    fn poll(&mut self) -> Vec<SourceEvent>;

    /// Current buffer status (read from shared atomics; cheap).
    fn buffer_status(&self) -> BufferStatus;

    /// Current health (mirrors the last `SourceEvent::Health`).
    fn health(&self) -> SourceHealth;

    /// Shared real-time atomics for the position/underrun readouts.
    fn rt_shared(&self) -> Arc<SourceRtShared>;
}

/// Atomics shared between a `SourceHost`'s real-time half and the
/// controller (contracts/audio-source-host.md §1). The synthetic host
/// reports `underrun = false`, `ring_fill_frames = u32::MAX` (always
/// ready) and never touches the others.
#[derive(Debug, Default)]
pub struct SourceRtShared {
    /// Set by `fill` on a short read; cleared by the controller after
    /// observing it.
    underrun: AtomicBool,
    track_seq: AtomicU32,
    /// `0` = unknown.
    track_len_frames: AtomicU64,
    ring_fill_frames: AtomicU32,
    consumed_frames: AtomicU64,
}

impl SourceRtShared {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn underrun(&self) -> bool {
        self.underrun.load(Ordering::Acquire)
    }

    pub fn set_underrun(&self, value: bool) {
        self.underrun.store(value, Ordering::Release);
    }

    pub fn track_seq(&self) -> u32 {
        self.track_seq.load(Ordering::Acquire)
    }

    pub fn set_track_seq(&self, value: u32) {
        self.track_seq.store(value, Ordering::Release);
    }

    pub fn track_len_frames(&self) -> u64 {
        self.track_len_frames.load(Ordering::Acquire)
    }

    pub fn set_track_len_frames(&self, value: u64) {
        self.track_len_frames.store(value, Ordering::Release);
    }

    pub fn ring_fill_frames(&self) -> u32 {
        self.ring_fill_frames.load(Ordering::Acquire)
    }

    pub fn set_ring_fill_frames(&self, value: u32) {
        self.ring_fill_frames.store(value, Ordering::Release);
    }

    pub fn consumed_frames(&self) -> u64 {
        self.consumed_frames.load(Ordering::Acquire)
    }

    pub fn set_consumed_frames(&self, value: u64) {
        self.consumed_frames.store(value, Ordering::Release);
    }

    pub fn add_consumed_frames(&self, delta: u64) {
        self.consumed_frames.fetch_add(delta, Ordering::AcqRel);
    }
}

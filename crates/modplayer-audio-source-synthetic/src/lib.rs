// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! The built-in deterministic synthetic test track and test tone
//! (`SyntheticSource`, `TestTone`), the only `AudioSource` implementation in
//! this slice (Constitution Principle IV).
//!
//! `track.rs` sequences the four looping segments (US2 T059); `tone.rs` is
//! the first-launch/device-check test tone (US1). Both build on the
//! oscillator primitives in `osc.rs`.
//!
//! `host.rs`/`scripted.rs` (003-streaming-playback-and-queue) add the
//! `SourceHost` implementors that prove the seam is additive:
//! `SyntheticHost` (this crate's own source, production default) and
//! `ScriptedHost` (a public test double used across crates, as 002 did
//! with `FakeAuthorizationService`).

pub mod host;
pub mod osc;
pub mod scripted;
pub mod tone;
pub mod track;

use modplayer_audio_source::AudioSource;

pub use host::SyntheticHost;
pub use scripted::{ScriptedHost, ScriptedHostHandle, ScriptedRt};
pub use tone::TestTone;

/// The built-in deterministic synthetic test track (contracts/audio-source.md):
/// 10 s 440 Hz sine @ −12 dBFS → 5 s 1 kHz square @ 0 dBFS → 5 s sawtooth
/// sweep 100→2000 Hz @ −12 dBFS → 2 s silence, then loops.
#[derive(Debug, Clone)]
pub struct SyntheticSource {
    sample_rate: u32,
    position: u64,
    track_len: u64,
    /// The phase (`0.0..1.0`) of the segment currently sounding at
    /// `position`, a pure function of `(sample_rate, position)`
    /// (data-model.md §2, `track::phase_at`).
    phase: f64,
}

impl SyntheticSource {
    /// Construct a synthetic source at `sample_rate` Hz, positioned at the
    /// start of the track.
    pub fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1);
        Self {
            sample_rate,
            position: 0,
            track_len: track::track_len_frames(sample_rate),
            phase: 0.0,
        }
    }

    /// The oscillator phase, continuous across `fill` calls (data-model.md §2).
    pub fn phase(&self) -> f64 {
        self.phase
    }
}

impl Default for SyntheticSource {
    fn default() -> Self {
        Self::new(44_100)
    }
}

impl AudioSource for SyntheticSource {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn len_frames(&self) -> Option<u64> {
        Some(self.track_len)
    }

    fn position(&self) -> u64 {
        self.position
    }

    fn seek(&mut self, frame: u64) {
        self.position = if self.track_len == 0 {
            0
        } else {
            frame % self.track_len
        };
        self.phase = track::phase_at(self.position, self.sample_rate);
    }

    fn fill(&mut self, out: &mut [f32]) {
        track::fill(out, &mut self.position, self.sample_rate);
        self.phase = track::phase_at(self.position, self.sample_rate);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_is_44_1_khz_and_starts_at_zero() {
        let s = SyntheticSource::default();
        assert_eq!(s.sample_rate(), 44_100);
        assert_eq!(s.position(), 0);
        assert_eq!(s.len_frames(), Some(44_100 * 22));
    }

    #[test]
    fn fill_advances_and_wraps_position() {
        let mut s = SyntheticSource::new(10);
        let len = s.len_frames().unwrap_or_default();
        let mut buf = vec![0.0f32; (len as usize - 2) * 2];
        s.fill(&mut buf);
        assert_eq!(s.position(), len - 2);
        let mut buf2 = vec![0.0f32; 8]; // 4 frames, wraps past track_len
        s.fill(&mut buf2);
        assert_eq!(s.position(), 2);
    }

    #[test]
    fn seek_wraps_modulo_len() {
        let mut s = SyntheticSource::new(10);
        let len = s.len_frames().unwrap_or_default();
        s.seek(len + 3);
        assert_eq!(s.position(), 3);
    }
}

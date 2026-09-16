// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ConnectRtSource`: the real-time half (`impl AudioSource`) that the
//! audio callback pulls from (contracts/audio-source-host.md §4,
//! contracts/connect-source.md §7). Popped from the `rtrb` ring `RingSink`
//! fills; markers pushed by the worker/event-mapper re-anchor the local
//! track-position baseline at exact write-side frame boundaries (research
//! R4). `fill`/`seek` never allocate, lock, block, or perform I/O.

use std::sync::Arc;

use modplayer_audio_source::{AudioSource, SourceRtShared};
use rtrb::Consumer;

use crate::program::{Marker, MarkerKind};

/// Spotify's own decode/output rate.
pub const SAMPLE_RATE: u32 = 44_100;

pub struct ConnectRtSource {
    samples: Consumer<f32>,
    markers: Consumer<Marker>,
    shared: Arc<SourceRtShared>,
    /// Frames delivered since the current track's last `TrackStart`/
    /// `Reposition` marker. Plain field, not an atomic: only ever touched
    /// from the audio callback thread that owns this value.
    track_position_frames: u64,
    track_seq: u32,
}

impl ConnectRtSource {
    pub(crate) fn new(
        samples: Consumer<f32>,
        markers: Consumer<Marker>,
        shared: Arc<SourceRtShared>,
        position_frames: u64,
    ) -> Self {
        let track_seq = shared.track_seq();
        Self {
            samples,
            markers,
            shared,
            track_position_frames: position_frames,
            track_seq,
        }
    }

    /// Apply every marker whose `at_written_frame` has already been
    /// reached by the frames consumed so far. Called once per `fill`, at
    /// buffer granularity (contracts/audio-source-host.md §4) — a marker
    /// that lands mid-buffer takes effect at the start of the *next*
    /// buffer rather than splitting this one, trading a few milliseconds
    /// of positional precision for a Consumer-only, allocation-free
    /// implementation.
    fn apply_due_markers(&mut self) {
        let consumed = self.shared.consumed_frames();
        loop {
            match self.markers.peek() {
                Ok(marker) if marker.at_written_frame <= consumed => {}
                _ => break,
            }
            let Ok(marker) = self.markers.pop() else {
                break;
            };
            match marker.kind {
                MarkerKind::TrackStart => {
                    self.track_position_frames = 0;
                    self.track_seq = self.track_seq.wrapping_add(1);
                    self.shared.set_track_seq(self.track_seq);
                }
                MarkerKind::Reposition => {
                    self.track_position_frames = marker.position_frames;
                }
                MarkerKind::TrackEnd => {
                    // Bookkeeping only; the host mirrors `EndOfTrack` over
                    // the event channel.
                }
            }
        }
    }
}

impl AudioSource for ConnectRtSource {
    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn len_frames(&self) -> Option<u64> {
        let len = self.shared.track_len_frames();
        (len != 0).then_some(len)
    }

    fn position(&self) -> u64 {
        self.track_position_frames
    }

    fn seek(&mut self, frame: u64) {
        // Discard the readable ring content without allocating (contract
        // §4): stale pre-seek audio must never reach the speakers. The
        // host also sends `SourceCommand::Seek` so the streaming half
        // performs the real seek and refills the ring from the new
        // position.
        while self.samples.pop().is_ok() {}
        self.track_position_frames = frame;
    }

    fn fill(&mut self, out: &mut [f32]) {
        self.apply_due_markers();

        let frames_needed = out.len() / 2;
        let mut delivered = 0usize;
        for i in 0..frames_needed {
            let left = self.samples.pop();
            let right = self.samples.pop();
            match (left, right) {
                (Ok(l), Ok(r)) => {
                    out[i * 2] = l;
                    out[i * 2 + 1] = r;
                    delivered += 1;
                }
                _ => break,
            }
        }

        if delivered < frames_needed {
            for i in delivered..frames_needed {
                out[i * 2] = 0.0;
                out[i * 2 + 1] = 0.0;
            }
            self.shared.set_underrun(true);
        }

        self.track_position_frames += delivered as u64;
        self.shared.add_consumed_frames(delivered as u64);
        self.shared
            .set_ring_fill_frames(self.samples.slots() as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtrb::RingBuffer;

    fn build(position: u64) -> (rtrb::Producer<f32>, rtrb::Producer<Marker>, ConnectRtSource) {
        let (sample_tx, sample_rx) = RingBuffer::<f32>::new(64);
        let (marker_tx, marker_rx) = RingBuffer::<Marker>::new(8);
        let shared = Arc::new(SourceRtShared::new());
        let source = ConnectRtSource::new(sample_rx, marker_rx, shared, position);
        (sample_tx, marker_tx, source)
    }

    #[test]
    fn short_read_fills_silence_and_sets_underrun() {
        let (mut sample_tx, _marker_tx, mut source) = build(0);
        let _ = sample_tx.push(0.5);
        let _ = sample_tx.push(-0.5); // one full frame available

        let mut out = [1.0f32; 8]; // 4 frames requested
        source.fill(&mut out);

        assert_eq!(&out[0..2], &[0.5, -0.5]);
        assert_eq!(&out[2..], &[0.0; 6]);
        assert_eq!(source.position(), 1);
        assert!(source.shared.underrun());
    }

    #[test]
    fn full_read_advances_position_without_underrun() {
        let (mut sample_tx, _marker_tx, mut source) = build(0);
        for _ in 0..4 {
            let _ = sample_tx.push(0.1);
            let _ = sample_tx.push(-0.1);
        }
        let mut out = [0.0f32; 8];
        source.fill(&mut out);
        assert_eq!(source.position(), 4);
        assert!(!source.shared.underrun());
    }

    #[test]
    fn track_start_marker_resets_position_and_bumps_track_seq() {
        let (mut sample_tx, mut marker_tx, mut source) = build(500);
        let starting_seq = source.track_seq;
        let _ = marker_tx.push(Marker::track_start(0));
        for _ in 0..2 {
            let _ = sample_tx.push(0.2);
            let _ = sample_tx.push(-0.2);
        }
        let mut out = [0.0f32; 4];
        source.fill(&mut out);
        assert_eq!(
            source.position(),
            2,
            "position resets then advances by delivered frames"
        );
        assert_eq!(source.track_seq, starting_seq.wrapping_add(1));
    }

    #[test]
    fn seek_discards_readable_ring_content() {
        let (mut sample_tx, _marker_tx, mut source) = build(0);
        for _ in 0..10 {
            let _ = sample_tx.push(0.9);
        }
        source.seek(12_345);
        assert_eq!(source.position(), 12_345);
        // Nothing left to pop: the stale content was discarded.
        let mut out = [1.0f32; 2];
        source.fill(&mut out);
        assert_eq!(out, [0.0, 0.0]);
    }

    #[test]
    fn len_frames_reports_none_until_the_shared_length_is_set() {
        let (_sample_tx, _marker_tx, source) = build(0);
        assert_eq!(source.len_frames(), None);
        source.shared.set_track_len_frames(44_100 * 3);
        assert_eq!(source.len_frames(), Some(44_100 * 3));
    }
}

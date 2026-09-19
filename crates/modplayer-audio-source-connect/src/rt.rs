// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ConnectRtSource`: the real-time half (`impl AudioSource`) that the
//! audio callback pulls from (contracts/audio-source-host.md §4,
//! contracts/connect-source.md §7, contracts/connect-source-delta.md §3).
//! Popped from the `rtrb` ring `RingSink` fills; markers pushed by the
//! worker/event-mapper re-anchor the local track-position baseline at
//! exact write-side frame boundaries (research R4). Since
//! 005-now-playing-waveform it also holds the current track's
//! `DecodedStore` and chooses, at restart points only (`seek`/
//! `TrackStart`) or at a rescue transition (research R2), whether `fill`
//! reads from the store (sample-exact) or the ring (003 behaviour).
//! `fill`/`seek` never allocate, lock, block, or perform I/O — the RT
//! never drops a store `Arc` either: a replaced store is *moved* into the
//! retirement ring, drained on the worker thread (research R1).

use std::sync::Arc;

use modplayer_audio_source::{AudioSource, DecodedStore, SourceRtShared};
use rtrb::{Consumer, Producer, PushError};

use crate::program::{Marker, MarkerKind};

/// Spotify's own decode/output rate.
pub const SAMPLE_RATE: u32 = 44_100;

/// Which source `fill` currently reads sample data from (data-model.md
/// §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Feed {
    /// 003 behaviour: pop the `rtrb` sample ring.
    Ring,
    /// Read the current track's `DecodedStore` at `cursor`, discarding the
    /// same frame count from the ring in lockstep (never blocking).
    Store,
}

pub struct ConnectRtSource {
    samples: Consumer<f32>,
    markers: Consumer<Marker>,
    shared: Arc<SourceRtShared>,
    /// The track position played and reported by `position()` — the only
    /// position value this type has, on either feed (contracts/
    /// connect-source-delta.md §3 rule 9). Plain field, not an atomic:
    /// only ever touched from the audio callback thread that owns this
    /// value.
    cursor: u64,
    track_seq: u32,
    feed: Feed,
    /// The ring's own marker-derived track position; `None` after a flush
    /// until the next `Reposition` (data-model.md §2.2).
    ring_pos: Option<u64>,
    /// The current track's store — the only store `Arc` the RT holds.
    /// `pub(crate)` so `tests/rt_feed.rs` (an external integration test
    /// that `#[path]`-includes this module, like `tests/markers.rs`) can
    /// set up a store directly rather than only through a `TrackStart`
    /// marker.
    pub(crate) store: Option<Arc<DecodedStore>>,
    /// Retirement-ring producer: every store `TrackStart` replaces is
    /// moved here, never dropped by the RT itself (research R1).
    retired: Producer<Arc<DecodedStore>>,
    /// Defensive slot for the unreachable `Full` case on `retired.push`;
    /// re-pushed at the top of every `fill` (contracts/connect-source-
    /// delta.md §3 rule 5b). `pub(crate)`: see `store`'s doc above.
    pub(crate) parked: Option<Arc<DecodedStore>>,
}

impl ConnectRtSource {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        samples: Consumer<f32>,
        markers: Consumer<Marker>,
        shared: Arc<SourceRtShared>,
        position_frames: u64,
        retired: Producer<Arc<DecodedStore>>,
    ) -> Self {
        let track_seq = shared.track_seq();
        Self {
            samples,
            markers,
            shared,
            cursor: position_frames,
            track_seq,
            feed: Feed::Ring,
            ring_pos: None,
            store: None,
            retired,
            parked: None,
        }
    }

    /// Move `store` into the retirement ring rather than dropping it here
    /// (research R1). `Full` is unreachable in practice (`RETIRED_CAPACITY
    /// = MARKER_CAPACITY + 1` bounds outstanding retirements below
    /// capacity — see `lib.rs`), but is still handled without a drop:
    /// parked, retried at the top of every `fill`.
    fn retire(&mut self, store: Arc<DecodedStore>) {
        match self.retired.push(store) {
            Ok(()) => {}
            Err(PushError::Full(rejected)) => self.parked = Some(rejected),
        }
    }

    /// Retry a `parked` retirement, if any (rule 5b).
    fn retry_parked(&mut self) {
        if let Some(store) = self.parked.take() {
            self.retire(store);
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
                MarkerKind::TrackStart { store } => {
                    let old = self.store.replace(store);
                    if let Some(old) = old {
                        self.retire(old);
                    }
                    self.track_seq = self.track_seq.wrapping_add(1);
                    self.shared.set_track_seq(self.track_seq);
                    self.cursor = 0;
                    self.feed = Feed::Ring;
                    self.ring_pos = None;
                }
                MarkerKind::Reposition => {
                    self.ring_pos = Some(marker.position_frames);
                    if self.feed == Feed::Ring {
                        self.cursor = marker.position_frames;
                    }
                    // Rule 4: never moves `cursor` on the `Store` feed.
                }
                MarkerKind::TrackEnd => {
                    // Bookkeeping only; the host mirrors `EndOfTrack` over
                    // the event channel.
                }
                MarkerKind::Reattach { store } => {
                    // A re-attached RT starts with no store; adopt the
                    // playing track's without touching cursor/track_seq.
                    // Anything it somehow already held goes to the
                    // retirement ring like every other replaced store.
                    if let Some(old) = self.store.replace(store) {
                        self.retire(old);
                    }
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
        self.cursor
    }

    fn seek(&mut self, frame: u64) {
        // Discard the readable ring content without allocating (contract
        // §4): stale pre-seek audio must never reach the speakers. The
        // host also sends `SourceCommand::Seek` so the streaming half
        // performs the real seek and refills the ring from the new
        // position.
        while self.samples.pop().is_ok() {}
        self.cursor = frame;
        self.ring_pos = None;
        // A restart point (research R2): sample-exact from the store when
        // it already covers the target, otherwise 003's ring behaviour
        // (contracts/connect-source-delta.md §3 rules 1-2).
        self.feed = if self.store.as_ref().is_some_and(|store| store.covers(frame)) {
            Feed::Store
        } else {
            Feed::Ring
        };
    }

    fn fill(&mut self, out: &mut [f32]) {
        self.retry_parked();
        self.apply_due_markers();

        let frames_needed = out.len() / 2;
        let mut delivered = 0usize;
        let mut one_frame = [0.0f32; 2];

        while delivered < frames_needed {
            match self.feed {
                Feed::Store => {
                    let covers = self
                        .store
                        .as_ref()
                        .is_some_and(|store| store.covers(self.cursor));
                    if !covers {
                        // Store exhaustion (rule 7): fall back to the ring
                        // at its own marker-derived baseline, or keep
                        // `cursor` if that baseline is unknown.
                        self.feed = Feed::Ring;
                        self.cursor = self.ring_pos.unwrap_or(self.cursor);
                        continue;
                    }
                    let Some(store) = self.store.as_ref() else {
                        self.feed = Feed::Ring;
                        continue;
                    };
                    let copied = store.read_frames(self.cursor, &mut one_frame);
                    if copied == 0 {
                        // Defensive: `covers` was true a moment ago but the
                        // read yielded nothing — never happens with the
                        // single-writer discipline, but never blocks/spins
                        // either way (rule 3 "never waits").
                        self.feed = Feed::Ring;
                        self.cursor = self.ring_pos.unwrap_or(self.cursor);
                        continue;
                    }
                    out[delivered * 2] = one_frame[0];
                    out[delivered * 2 + 1] = one_frame[1];
                    // Pop-and-drop the ring in lockstep (rule 3): never
                    // blocks — an empty ring here is simply skipped, the
                    // `Player` stays paced by its own sink regardless.
                    let _ = self.samples.pop();
                    let _ = self.samples.pop();
                    self.cursor += 1;
                    delivered += 1;
                }
                Feed::Ring => match (self.samples.pop(), self.samples.pop()) {
                    (Ok(l), Ok(r)) => {
                        out[delivered * 2] = l;
                        out[delivered * 2 + 1] = r;
                        self.cursor += 1;
                        delivered += 1;
                    }
                    _ => {
                        // Underrun: rescue from the store if it already
                        // covers this exact frame (rule 6) — delivered in
                        // this same `fill`, not the next one.
                        if self
                            .store
                            .as_ref()
                            .is_some_and(|store| store.covers(self.cursor))
                        {
                            self.feed = Feed::Store;
                            continue;
                        }
                        break;
                    }
                },
            }
        }

        if delivered < frames_needed {
            for i in delivered..frames_needed {
                out[i * 2] = 0.0;
                out[i * 2 + 1] = 0.0;
            }
            self.shared.set_underrun(true);
        }

        self.shared.add_consumed_frames(delivered as u64);
        self.shared
            .set_ring_fill_frames(self.samples.slots() as u32);
    }

    /// The current track's store — the same `Arc` this RT already holds
    /// for its own `Feed::Store` reads (006, contracts/engine-loop.md §1):
    /// never cloned or dropped here, just borrowed for the engine's loop
    /// seam.
    fn decoded_store(&self) -> Option<&Arc<DecodedStore>> {
        self.store.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtrb::RingBuffer;

    /// `RETIRED_CAPACITY` mirrors `lib.rs`'s `MARKER_CAPACITY + 1`; kept
    /// local so this module's tests don't need `lib.rs`.
    const RETIRED_CAPACITY: usize = 65;

    #[allow(clippy::type_complexity)]
    fn build(
        position: u64,
    ) -> (
        rtrb::Producer<f32>,
        rtrb::Producer<Marker>,
        rtrb::Consumer<Arc<DecodedStore>>,
        ConnectRtSource,
    ) {
        let (sample_tx, sample_rx) = RingBuffer::<f32>::new(64);
        let (marker_tx, marker_rx) = RingBuffer::<Marker>::new(8);
        let (retired_tx, retired_rx) = RingBuffer::<Arc<DecodedStore>>::new(RETIRED_CAPACITY);
        let shared = Arc::new(SourceRtShared::new());
        let source = ConnectRtSource::new(sample_rx, marker_rx, shared, position, retired_tx);
        (sample_tx, marker_tx, retired_rx, source)
    }

    fn filled_store(frames: u64) -> Arc<DecodedStore> {
        let store = DecodedStore::new(SAMPLE_RATE, frames);
        let interleaved: Vec<f32> = (0..frames)
            .flat_map(|i| [0.1 + i as f32, -0.1 - i as f32])
            .collect();
        store.write_frames(0, &interleaved);
        store.set_complete(frames);
        store
    }

    #[test]
    fn short_read_fills_silence_and_sets_underrun() {
        let (mut sample_tx, _marker_tx, _retired_rx, mut source) = build(0);
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
        let (mut sample_tx, _marker_tx, _retired_rx, mut source) = build(0);
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
        let (mut sample_tx, mut marker_tx, _retired_rx, mut source) = build(500);
        let starting_seq = source.track_seq;
        let store = filled_store(4);
        let _ = marker_tx.push(Marker::track_start(0, store));
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

    /// `swap.rs`: a re-attached RT adopts the playing track's store from
    /// the `Reattach` marker `attach()` seeds its ring with — at the
    /// cursor it was built with, same `track_seq`, no retirement — and
    /// can then serve a sample-exact read from that store.
    #[test]
    fn reattach_marker_adopts_store_without_moving_cursor_or_track_seq() {
        let (_sample_tx, mut marker_tx, mut retired_rx, mut source) = build(1_000);
        source.shared.set_track_seq(7);
        source.track_seq = 7;
        let store = filled_store(2_048);
        assert!(marker_tx.push(Marker::reattach(Arc::clone(&store))).is_ok());

        let mut out = [0.0f32; 4];
        source.fill(&mut out);

        assert!(
            source
                .store
                .as_ref()
                .is_some_and(|held| Arc::ptr_eq(held, &store)),
            "the store must be adopted"
        );
        assert_eq!(source.track_seq, 7, "not a track change");
        assert_eq!(source.shared.track_seq(), 7);
        assert!(retired_rx.pop().is_err(), "nothing to retire on a fresh RT");
        // Ring empty, store covers the cursor: the rescue transition reads
        // frames 1 000 and 1 001 sample-exactly from the store.
        assert_eq!(source.position(), 1_002);
        assert_eq!(out, [1_000.1, -1_000.1, 1_001.1, -1_001.1]);
    }

    #[test]
    fn seek_discards_readable_ring_content() {
        let (mut sample_tx, _marker_tx, _retired_rx, mut source) = build(0);
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
        let (_sample_tx, _marker_tx, _retired_rx, source) = build(0);
        assert_eq!(source.len_frames(), None);
        source.shared.set_track_len_frames(44_100 * 3);
        assert_eq!(source.len_frames(), Some(44_100 * 3));
    }
}

// 005-now-playing-waveform's store/ring feed-rule tests (contracts/
// connect-source-delta.md §3) live in `tests/rt_feed.rs` (an external
// integration test, like `tests/markers.rs`), not here — see that file's
// header comment for why (its `#[path]` inclusion needs a couple of these
// fields at `pub(crate)` rather than fully private).

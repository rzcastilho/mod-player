// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Marker` (real-time boundary events pushed from the worker/sink side to
//! `ConnectRtSource`) and `ProgramMap` (the worker's memory of the last
//! `LoadProgram`, used to resolve which effective-order index a
//! `TrackChanged` event corresponds to) — contracts/connect-source.md §1,
//! §3, data-model.md §5.

use std::sync::Arc;

use modplayer_audio_source::{DecodedStore, TrackId};

/// What a [`Marker`] tells `ConnectRtSource::fill` to do once the RT
/// consumer's cumulative consumed-frame count reaches `at_written_frame`
/// (research R4; contracts/audio-source-host.md §4).
#[derive(Debug, Clone)]
pub enum MarkerKind {
    /// A new track's first frame: reset the local track-position baseline
    /// to 0, bump `track_seq`, and swap in the new track's decoded store
    /// (005-now-playing-waveform, contracts/connect-source-delta.md §2,
    /// data-model.md §2.2). The replaced store is *moved* into the
    /// retirement ring by `ConnectRtSource` as this marker applies — the
    /// RT half never drops a store `Arc` itself (research R1).
    TrackStart { store: Arc<DecodedStore> },
    /// The current track ended in the ring (bookkeeping only; the host
    /// mirrors `SourceEvent::EndOfTrack` separately over the event
    /// channel — this marker does not by itself change RT state).
    TrackEnd,
    /// A discontinuous position update (post-seek `Playing`/`Seeked`):
    /// set the ring's position baseline to `position_frames` (and the
    /// local track-position baseline too, when on the `Ring` feed —
    /// contracts/connect-source-delta.md §3 rule 4: ignored on `Store`).
    Reposition,
}

impl PartialEq for MarkerKind {
    /// `TrackStart`'s store compares by `Arc::ptr_eq` (identity) —
    /// `DecodedStore` is not itself content-comparable (Constitution V,
    /// contracts/decoded-store.md §1), matching `SourceEvent`'s own
    /// `PartialEq`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::TrackStart { store: a }, Self::TrackStart { store: b }) => Arc::ptr_eq(a, b),
            (Self::TrackEnd, Self::TrackEnd) | (Self::Reposition, Self::Reposition) => true,
            _ => false,
        }
    }
}

/// One boundary event, stamped against the sink's write-side frame count
/// so the RT side applies it in exact temporal order relative to the
/// samples it is popping (never against wall-clock time). No longer
/// `Copy` (005-now-playing-waveform): `TrackStart` carries an
/// `Arc<DecodedStore>`, moved through the `rtrb` marker ring.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub kind: MarkerKind,
    /// The sink's `written_frames` value at (or after) which this marker
    /// takes effect.
    pub at_written_frame: u64,
    /// Only meaningful for [`MarkerKind::Reposition`].
    pub position_frames: u64,
}

impl Marker {
    /// `store` is the new track's `DecodedStore`, already created by the
    /// decode-ahead spawn path before this marker is pushed (contracts/
    /// connect-source-delta.md §1's ordering: store → marker →
    /// `TrackStarted`/`BecameActive` → `SourceEvent::DecodedStore`).
    pub fn track_start(at_written_frame: u64, store: Arc<DecodedStore>) -> Self {
        Self {
            kind: MarkerKind::TrackStart { store },
            at_written_frame,
            position_frames: 0,
        }
    }

    pub fn track_end(at_written_frame: u64) -> Self {
        Self {
            kind: MarkerKind::TrackEnd,
            at_written_frame,
            position_frames: 0,
        }
    }

    pub fn reposition(at_written_frame: u64, position_frames: u64) -> Self {
        Self {
            kind: MarkerKind::Reposition,
            at_written_frame,
            position_frames,
        }
    }
}

/// The worker's memory of the last `SourceCommand::LoadProgram`
/// (contracts/connect-source.md §1-2): lets `TrackChanged` resolve which
/// effective-order index the service just started, preferring the index
/// the host expects next so duplicate track ids in the order resolve
/// correctly.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramMap {
    pub generation: u64,
    pub order: Vec<TrackId>,
    /// The slot every `LoadProgram` (re)starts at (`Program::cursor_index`
    /// → `PlayingTrack::Index`): the first `TrackChanged` after a load is
    /// expected there.
    pub cursor_index: u32,
}

impl ProgramMap {
    pub fn new(generation: u64, order: Vec<TrackId>, cursor_index: u32) -> Self {
        Self {
            generation,
            order,
            cursor_index,
        }
    }

    /// The slot the next `TrackChanged` is expected to start:
    /// `last_index + 1` once something from this program has played,
    /// otherwise the load's own `cursor_index`.
    pub fn expected_next(&self, last_index: Option<u32>) -> u32 {
        last_index.map_or(self.cursor_index, |i| i.wrapping_add(1))
    }

    /// Resolve a started track to its slot: by uri first
    /// ([`index_of`](Self::index_of)); failing that, a uri absent from the
    /// order while the expected slot exists is taken to be *that* slot —
    /// the service played a **relinked alternative** (`Track.alternatives`)
    /// in the original's place, which is what the official client does
    /// silently. The 2026-09-17 manual walk (004 quickstart M4) hit this
    /// on a saved track whose id had been retired: the alternative's uri
    /// matched nothing, the start was reported as a foreign reveal, and
    /// the host collapsed its 395-track context to that one item. A
    /// genuinely foreign start (a remote client loading another context
    /// onto this device) is indistinguishable here and is accepted as the
    /// rarer case; `None` only when nothing is expected at all.
    pub fn resolve(&self, uri: &str, last_index: Option<u32>) -> Option<u32> {
        let expected = self.expected_next(last_index);
        self.index_of(uri, Some(expected))
            .or_else(|| ((expected as usize) < self.order.len()).then_some(expected))
    }

    /// Resolve `uri` to an index in `order`, preferring `expected_next`
    /// when it names a matching entry (duplicate-track disambiguation,
    /// contracts/connect-source.md §7).
    pub fn index_of(&self, uri: &str, expected_next: Option<u32>) -> Option<u32> {
        if let Some(expected) = expected_next
            && self.order.get(expected as usize).map(TrackId::as_str) == Some(uri)
        {
            return Some(expected);
        }
        self.order
            .iter()
            .position(|track| track.as_str() == uri)
            .map(|index| index as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> TrackId {
        TrackId::new(id).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn index_of_prefers_expected_next_for_duplicates() {
        let map = ProgramMap::new(
            1,
            vec![
                track("spotify:track:a"),
                track("spotify:track:a"),
                track("spotify:track:b"),
            ],
            0,
        );
        assert_eq!(map.index_of("spotify:track:a", Some(1)), Some(1));
        assert_eq!(map.index_of("spotify:track:a", Some(0)), Some(0));
        assert_eq!(map.index_of("spotify:track:a", None), Some(0));
    }

    #[test]
    fn index_of_falls_back_to_first_match_when_expectation_is_wrong() {
        let map = ProgramMap::new(
            1,
            vec![track("spotify:track:a"), track("spotify:track:b")],
            0,
        );
        assert_eq!(map.index_of("spotify:track:b", Some(5)), Some(1));
    }

    #[test]
    fn index_of_none_when_absent() {
        let map = ProgramMap::new(1, vec![track("spotify:track:a")], 0);
        assert_eq!(map.index_of("spotify:track:z", None), None);
    }

    #[test]
    fn resolve_expects_the_cursor_slot_first_then_the_following_one() {
        let map = ProgramMap::new(
            1,
            vec![
                track("spotify:track:a"),
                track("spotify:track:b"),
                track("spotify:track:c"),
            ],
            1,
        );
        assert_eq!(map.expected_next(None), 1);
        assert_eq!(map.expected_next(Some(1)), 2);
        assert_eq!(map.resolve("spotify:track:b", None), Some(1));
        assert_eq!(map.resolve("spotify:track:c", Some(1)), Some(2));
        // A known uri wins over the expectation.
        assert_eq!(map.resolve("spotify:track:a", Some(1)), Some(0));
    }

    #[test]
    fn resolve_treats_an_unknown_uri_as_a_relinked_alternative_in_the_expected_slot() {
        let map = ProgramMap::new(
            1,
            vec![track("spotify:track:a"), track("spotify:track:b")],
            0,
        );
        // First start after the load: the alternative stands in for slot 0.
        assert_eq!(map.resolve("spotify:track:alt-of-a", None), Some(0));
        // Next start: slot 1.
        assert_eq!(map.resolve("spotify:track:alt-of-b", Some(0)), Some(1));
        // Nothing expected past the end of the order.
        assert_eq!(map.resolve("spotify:track:alt", Some(1)), None);
    }
}

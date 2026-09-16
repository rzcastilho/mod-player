// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Marker` (real-time boundary events pushed from the worker/sink side to
//! `ConnectRtSource`) and `ProgramMap` (the worker's memory of the last
//! `LoadProgram`, used to resolve which effective-order index a
//! `TrackChanged` event corresponds to) — contracts/connect-source.md §1,
//! §3, data-model.md §5.

use modplayer_audio_source::TrackId;

/// What a [`Marker`] tells `ConnectRtSource::fill` to do once the RT
/// consumer's cumulative consumed-frame count reaches `at_written_frame`
/// (research R4; contracts/audio-source-host.md §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// A new track's first frame: reset the local track-position baseline
    /// to 0 and bump `track_seq`.
    TrackStart,
    /// The current track ended in the ring (bookkeeping only; the host
    /// mirrors `SourceEvent::EndOfTrack` separately over the event
    /// channel — this marker does not by itself change RT state).
    TrackEnd,
    /// A discontinuous position update (post-seek `Playing`/`Seeked`):
    /// set the local track-position baseline to `position_frames`.
    Reposition,
}

/// One boundary event, stamped against the sink's write-side frame count
/// so the RT side applies it in exact temporal order relative to the
/// samples it is popping (never against wall-clock time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    pub kind: MarkerKind,
    /// The sink's `written_frames` value at (or after) which this marker
    /// takes effect.
    pub at_written_frame: u64,
    /// Only meaningful for [`MarkerKind::Reposition`].
    pub position_frames: u64,
}

impl Marker {
    pub fn track_start(at_written_frame: u64) -> Self {
        Self {
            kind: MarkerKind::TrackStart,
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
}

impl ProgramMap {
    pub fn new(generation: u64, order: Vec<TrackId>) -> Self {
        Self { generation, order }
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
        );
        assert_eq!(map.index_of("spotify:track:a", Some(1)), Some(1));
        assert_eq!(map.index_of("spotify:track:a", Some(0)), Some(0));
        assert_eq!(map.index_of("spotify:track:a", None), Some(0));
    }

    #[test]
    fn index_of_falls_back_to_first_match_when_expectation_is_wrong() {
        let map = ProgramMap::new(1, vec![track("spotify:track:a"), track("spotify:track:b")]);
        assert_eq!(map.index_of("spotify:track:b", Some(5)), Some(1));
    }

    #[test]
    fn index_of_none_when_absent() {
        let map = ProgramMap::new(1, vec![track("spotify:track:a")]);
        assert_eq!(map.index_of("spotify:track:z", None), None);
    }
}

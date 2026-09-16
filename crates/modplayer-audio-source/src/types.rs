// SPDX-License-Identifier: MIT OR Apache-2.0

//! Value types for the additive `SourceHost` seam (data-model.md §1,
//! contracts/audio-source-host.md §2-3). This crate stays dependency-free
//! (Constitution IV): every type below is defined from `std` alone, even
//! where a richer sibling exists elsewhere (e.g. `modplayer-engine`'s
//! `VolumePercent`) — the trait crate cannot depend on its own consumers.
//!
//! `Program` (data-model.md §2.3) is the host -> source handoff sent as
//! `SourceCommand::LoadProgram`'s payload. Although data-model.md's table
//! of contents lists it next to `Queue` (which computes it), it is defined
//! here rather than in `modplayer-core::queue` because `SourceCommand` must
//! be nameable in this dependency-free crate; `Queue::program()` (core)
//! builds and returns this same type.

use std::fmt;
use std::time::{Duration, Instant};

/// Newtype over the service's stable track identifier in URI form
/// (`spotify:track:<base62>`). Invariant: non-empty, ASCII, <= 64 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrackId(String);

/// `TrackId::new` rejected its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackIdError {
    /// Empty, non-ASCII, or longer than 64 bytes.
    Invalid,
}

impl fmt::Display for TrackIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid track id: must be non-empty ASCII, <= 64 bytes")
    }
}

impl std::error::Error for TrackIdError {}

impl TrackId {
    /// Construct a `TrackId` from a URI, validating the invariant.
    pub fn new(uri: impl Into<String>) -> Result<Self, TrackIdError> {
        let uri = uri.into();
        if uri.is_empty() || uri.len() > 64 || !uri.is_ascii() {
            return Err(TrackIdError::Invalid);
        }
        Ok(Self(uri))
    }

    /// The URI as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether the service will currently serve a track (data-model.md §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Available,
    Unavailable,
}

/// Repeat mode (FR-013/014). Shared by `modplayer-core::queue::Queue` and
/// `SourceCommand::ReportState`/`RemoteCommand::Repeat` — defined here so
/// both sides of the seam name the same type without a dependency cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    Off,
    One,
    All,
}

/// Playback intent mirrored to the source (data-model.md §3.1's `Intent`,
/// structurally identical to `modplayer_engine::Transport`). Defined here
/// — not in `modplayer-engine` (which this crate cannot depend on without a
/// cycle) — so `SourceCommand::ReportState` can carry it; `modplayer-core`'s
/// `transport::Intent` is a re-export of this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Intent {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// Master/outbound volume as a 0-100 percentage. A minimal mirror of
/// `modplayer_engine::VolumePercent` (this crate cannot depend on the
/// engine crate — Constitution IV); `SourceCommand::SetVolume` and
/// `RemoteCommand::Volume` carry this type, converted at the controller
/// boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VolumePercent(u8);

impl VolumePercent {
    /// Construct from a `u8`, clamping to 100.
    pub fn new(pct: u8) -> Self {
        Self(pct.min(100))
    }

    /// The percentage, `0..=100`.
    pub fn value(self) -> u8 {
        self.0
    }
}

/// Track metadata (data-model.md §1.2, DM-2 narrowed to this slice).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRef {
    pub id: TrackId,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub artwork_url: Option<String>,
    pub duration_ms: u32,
    pub availability: Availability,
}

impl TrackRef {
    /// Construct a `TrackRef`, applying the title fallback (data-model.md
    /// §1.2: "falls back to 'Unknown title' when the service omits it").
    pub fn new(
        id: TrackId,
        title: impl Into<String>,
        artists: Vec<String>,
        album: Option<String>,
        artwork_url: Option<String>,
        duration_ms: u32,
        availability: Availability,
    ) -> Self {
        let title = title.into();
        let title = if title.trim().is_empty() {
            "Unknown title".to_string()
        } else {
            title
        };
        Self {
            id,
            title,
            artists,
            album,
            artwork_url,
            duration_ms,
            availability,
        }
    }
}

/// Source health (data-model.md §1.3). `Transient -> Unavailable` only on
/// an unrecoverable classification; duration alone never escalates it.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SourceHealth {
    #[default]
    Ok,
    Transient {
        since: Instant,
        next_retry_in: Duration,
    },
    Unavailable {
        client_update_required: bool,
    },
}

/// Current buffer status (data-model.md §1.4), read from shared atomics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BufferStatus {
    pub ring_fill_frames: u32,
    pub ready: bool,
    pub current_prefetched: bool,
    pub next: Option<TrackId>,
}

/// The host -> source handoff (data-model.md §2.3): the full effective
/// play order plus where playback is within it. Sent as
/// `SourceCommand::LoadProgram`'s payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub order: Vec<TrackId>,
    pub cursor_index: u32,
    pub position_ms: u32,
    pub start_playing: bool,
    pub repeat_all: bool,
    pub repeat_one: bool,
    pub generation: u64,
}

/// The context handed back on a transfer-in with a known upcoming context
/// (data-model.md §6).
#[derive(Debug, Clone, PartialEq)]
pub struct TransferContext {
    pub current: TrackRef,
    pub position_ms: u32,
    pub playing: bool,
    pub shuffle: Option<bool>,
    pub repeat: Option<Repeat>,
}

/// A command already applied by the source that the host mirrors into its
/// own transport/queue state (contracts/audio-source-host.md §3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RemoteCommand {
    Play,
    Pause,
    SkipNext,
    SkipPrev,
    Seek(u32),
    Volume(VolumePercent),
    Shuffle(bool),
    Repeat(Repeat),
}

/// A command from the `PlaybackController` to a `SourceHost` implementor
/// (contracts/audio-source-host.md §2). Plain data, `Clone + Debug`, no
/// secrets — carried over `std::sync::mpsc`.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceCommand {
    Initialize {
        device_name: String,
        device_id: String,
    },
    Deregister,
    Shutdown,
    Retry,
    SetDeviceName(String),
    LoadProgram(Program),
    Play,
    Pause,
    Stop,
    /// Seek within the current track, in milliseconds.
    Seek(u32),
    SkipNext,
    SkipPrev,
    SetVolume(VolumePercent),
    RequestTransferHere,
    ReportState {
        intent: Intent,
        position_ms: u32,
        repeat: Repeat,
    },
    /// List up to `limit` playable tracks from the signed-in account, via
    /// the source's own session (spec Amendment 2026-09-16: the public Web
    /// API 429s a Keymaster token, so "Play from account" sources tracks
    /// through the session's metadata client instead). The reply is a
    /// `SourceEvent::AccountTracks` carrying the same `request_id`.
    ListAccountTracks {
        request_id: u64,
        limit: u8,
    },
}

/// Why a `SourceCommand::ListAccountTracks` could not be fulfilled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountReadError {
    /// The source has no session, the session read failed, or the account
    /// exposes no playable tracks to seed from.
    Unavailable,
}

/// An event from a `SourceHost` implementor to the `PlaybackController`
/// (contracts/audio-source-host.md §3). Plain data, `Clone + Debug`, no
/// secrets.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceEvent {
    Registered {
        device_name: String,
    },
    Deregistered,
    Health(SourceHealth),
    TrackStarted {
        track: TrackRef,
        /// `(generation, index)`; `None` when the source itself chose the
        /// track (source-driven or a remote queue addition).
        program: Option<(u64, u32)>,
        position_ms: u32,
        playing: bool,
    },
    Loading {
        position_ms: u32,
    },
    Playing {
        position_ms: u32,
    },
    Paused {
        position_ms: u32,
    },
    Stopped,
    Seeked {
        position_ms: u32,
    },
    EndOfTrack,
    Unavailable {
        track: TrackId,
    },
    RemoteCommand(RemoteCommand),
    BecameActive {
        context: Option<TransferContext>,
    },
    BecameInactive,
    TierRejected,
    /// Reply to `SourceCommand::ListAccountTracks` with the same
    /// `request_id`. `Ok(vec)` (possibly empty) on a successful read,
    /// `Err` when the session could not supply tracks.
    AccountTracks {
        request_id: u64,
        result: Result<Vec<TrackRef>, AccountReadError>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_id_rejects_empty_and_too_long() {
        assert!(TrackId::new("").is_err());
        assert!(TrackId::new("x".repeat(65)).is_err());
        assert!(TrackId::new("spotify:track:abc123").is_ok());
    }

    #[test]
    fn track_id_rejects_non_ascii() {
        assert!(TrackId::new("spotify:track:caf\u{e9}").is_err());
    }

    #[test]
    fn track_ref_falls_back_to_unknown_title() {
        let id = TrackId::new("spotify:track:abc").unwrap_or_else(|_| unreachable!());
        let track = TrackRef::new(id, "   ", vec![], None, None, 1000, Availability::Available);
        assert_eq!(track.title, "Unknown title");
    }

    #[test]
    fn volume_percent_clamps() {
        assert_eq!(VolumePercent::new(150).value(), 100);
        assert_eq!(VolumePercent::new(50).value(), 50);
    }

    #[test]
    fn repeat_defaults_to_off() {
        assert_eq!(Repeat::default(), Repeat::Off);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! The `AudioSource` trait: the single sanctioned seam through which sample
//! data enters the ModPlayer real-time engine (Constitution Principle IV).
//!
//! See `specs/001-walking-skeleton/contracts/audio-source.md`.
//!
//! `host` and `types` (003-streaming-playback-and-queue) add the additive
//! off-real-time half of the seam (`SourceHost`, `SourceCommand`,
//! `SourceEvent`, ...) without changing `AudioSource` itself
//! (contracts/audio-source-host.md).
//!
//! `catalog` (004-search-and-library-browse) adds catalog/library
//! identities and request/reply payloads (data-model.md §1) carried by the
//! additive `SourceCommand`/`SourceEvent` variants in `types`
//! (contracts/catalog-source.md).
//!
//! `decoded_store` (006-markers-loops-and-cues) is a provided,
//! additive `AudioSource` method giving the engine's loop seam
//! sample-exact random access into the current track's retained decoded
//! store (contracts/engine-loop.md §1).

use std::sync::Arc;

pub mod catalog;
pub mod decoded;
pub mod host;
pub mod types;

pub use catalog::{
    AlbumId, AlbumIdError, AlbumRef, ArtistId, ArtistIdError, ArtistRef, CatalogError, LibraryItem,
    LibraryPage, LibrarySet, PlaylistId, PlaylistIdError, PlaylistRef, ReleaseDate,
    SearchGroupPage, SearchHit, SearchKind, SearchPage, TrackList, TrackListSource,
    pack_request_id, unpack_request_id,
};
pub use decoded::{CHUNK_FRAMES, DecodedStore, MAX_STORE_FRAMES, PeakBucket, StoreState};
pub use host::{SourceHost, SourceRtShared};
pub use types::{
    Availability, BufferStatus, Intent, Program, RemoteCommand, Repeat, SourceCommand, SourceEvent,
    SourceHealth, TrackId, TrackIdError, TrackRef, TrackRefExtras, TransferContext, VolumePercent,
};

/// A producer of interleaved stereo `f32` frames at its own fixed sample rate.
///
/// Real-time contract: `fill` and `seek` are called from the audio callback
/// and MUST NOT allocate, lock, block, log, or perform I/O.
pub trait AudioSource: Send + 'static {
    /// Sample rate of the frames produced by `fill`. Constant for the
    /// lifetime of the value.
    fn sample_rate(&self) -> u32;

    /// Total length in frames of the current material, if finite. `None`
    /// means "unbounded / streaming" (no implementor this slice).
    fn len_frames(&self) -> Option<u64>;

    /// Current read position in frames (`0 <= pos < len` when finite).
    fn position(&self) -> u64;

    /// Move the read position. Wraps modulo `len_frames` when finite.
    fn seek(&mut self, frame: u64);

    /// Write exactly `out.len() / 2` stereo frames into `out` (interleaved
    /// L,R), advancing `position` by that many frames (wrapping at
    /// `len_frames`). Must fill the whole slice; silence is written
    /// explicitly.
    fn fill(&mut self, out: &mut [f32]);

    /// The current track's retained decoded store, if this source has one
    /// (006, contracts/engine-loop.md §1). Real-time contract: returns a
    /// reference to an `Arc` the source already holds — never clones,
    /// never drops. Default `None`: a source with no retained store still
    /// loops with an exact period, only the seam's crossfade degrades to a
    /// hard cut.
    fn decoded_store(&self) -> Option<&Arc<DecodedStore>> {
        None
    }
}

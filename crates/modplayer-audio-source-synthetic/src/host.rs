// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SyntheticHost`: the trivial `SourceHost` implementor over
//! `SyntheticSource` (contracts/audio-source-host.md §5). Proves the
//! `SourceHost` seam is additive — every command besides `Initialize` is a
//! no-op, and `attach` always hands back the same deterministic built-in
//! track, seeked to the requested position.

use std::sync::Arc;

use modplayer_audio_source::{
    AudioSource, BufferStatus, CatalogError, DecodedStore, SourceCommand, SourceEvent,
    SourceHealth, SourceHost, SourceRtShared, TrackId,
};

use crate::SyntheticSource;

/// The URI of the built-in synthetic track for `DecodedStore`/`TrackId`
/// purposes (005-now-playing-waveform, research R14). Not a real Spotify
/// URI — `SyntheticHost` never talks to the service — but a stable,
/// well-formed `TrackId` so a `SourceEvent::DecodedStore` can name it.
fn synthetic_track_id() -> TrackId {
    TrackId::new("modplayer:synthetic:built-in").unwrap_or_else(|_| unreachable!())
}

/// The default `SourceHost` used before any real streaming source is
/// wired up (001's `SyntheticSource`, now behind the `SourceHost` seam).
pub struct SyntheticHost {
    sample_rate: u32,
    events: Vec<SourceEvent>,
    rt_shared: Arc<SourceRtShared>,
    /// The built-in track's retained decoded store (005-now-playing-
    /// waveform, contracts/decoded-store.md §3): built and filled
    /// synchronously to `Complete` at construction from
    /// `crate::track::fill` (the same closed-form content `attach`'s
    /// `SyntheticSource` plays), since this host never actually decodes
    /// anything — its one track is always already "decoded" in full.
    track_id: TrackId,
    store: Arc<DecodedStore>,
}

impl SyntheticHost {
    /// Construct a host producing `sample_rate` Hz synthetic audio.
    pub fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1);
        let len_frames = crate::track::track_len_frames(sample_rate);
        let store = DecodedStore::new(sample_rate, len_frames);
        let mut buf = vec![0.0f32; (len_frames * 2) as usize];
        let mut position = 0u64;
        crate::track::fill(&mut buf, &mut position, sample_rate);
        store.write_frames(0, &buf);
        store.set_complete(len_frames);
        Self {
            sample_rate,
            events: Vec::new(),
            rt_shared: Arc::new(SourceRtShared::new()),
            track_id: synthetic_track_id(),
            store,
        }
    }
}

impl Default for SyntheticHost {
    fn default() -> Self {
        Self::new(44_100)
    }
}

impl SourceHost for SyntheticHost {
    type Rt = SyntheticSource;

    fn attach(&mut self, position_frames: u64) -> Self::Rt {
        // 006-markers-loops-and-cues, contracts/engine-loop.md §1: this
        // host's one track is always already fully decoded (`self.store`,
        // built at construction), so every attach carries it.
        let mut source = SyntheticSource::with_store(self.sample_rate, Arc::clone(&self.store));
        source.seek(position_frames);
        source
    }

    fn command(&mut self, cmd: SourceCommand) {
        // Every command besides `Initialize` is a no-op (contracts/
        // audio-source-host.md §2's "Synthetic/Scripted behaviour" column),
        // except the catalog commands (004-search-and-library-browse),
        // which this host has no catalog to answer from
        // (contracts/catalog-source.md §4: "`SyntheticHost` answers every
        // catalog command with `Err(Unsupported)`").
        match cmd {
            SourceCommand::Initialize { device_name, .. } => {
                self.events.push(SourceEvent::Registered { device_name });
                // Raised right after (the closest analogue of) a
                // `TrackStarted` this always-one-track host has — its
                // built-in track is always "current" (research R14,
                // contracts/decoded-store.md §3).
                self.events.push(SourceEvent::DecodedStore {
                    track: self.track_id.clone(),
                    store: Arc::clone(&self.store),
                });
            }
            SourceCommand::SearchCatalog { request_id, .. } => {
                self.events.push(SourceEvent::SearchResult {
                    request_id,
                    result: Err(CatalogError::Unsupported),
                });
            }
            SourceCommand::FetchLibrary { request_id, .. } => {
                self.events.push(SourceEvent::LibraryPage {
                    request_id,
                    result: Err(CatalogError::Unsupported),
                });
            }
            SourceCommand::FetchTrackList { request_id, .. } => {
                self.events.push(SourceEvent::TrackList {
                    request_id,
                    result: Err(CatalogError::Unsupported),
                });
            }
            SourceCommand::HydrateRefs {
                request_id,
                tracks,
                albums,
                artists,
            } => {
                // `Hydrated` carries no `Result` (contracts/catalog-
                // source.md §2): report every requested id as `missing`,
                // this host's closest equivalent of "unsupported".
                let missing = tracks
                    .iter()
                    .map(|id| id.to_string())
                    .chain(albums.iter().map(|id| id.to_string()))
                    .chain(artists.iter().map(|id| id.to_string()))
                    .collect();
                self.events.push(SourceEvent::Hydrated {
                    request_id,
                    tracks: Vec::new(),
                    albums: Vec::new(),
                    artists: Vec::new(),
                    missing,
                });
            }
            SourceCommand::CancelCatalog { .. } => {}
            _ => {}
        }
    }

    fn poll(&mut self) -> Vec<SourceEvent> {
        std::mem::take(&mut self.events)
    }

    fn buffer_status(&self) -> BufferStatus {
        // "Always ready" (contracts/audio-source-host.md §1).
        BufferStatus {
            ring_fill_frames: u32::MAX,
            ready: true,
            current_prefetched: true,
            next: None,
        }
    }

    fn health(&self) -> SourceHealth {
        SourceHealth::Ok
    }

    fn rt_shared(&self) -> Arc<SourceRtShared> {
        Arc::clone(&self.rt_shared)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn attach_restores_position() {
        let mut host = SyntheticHost::new(44_100);
        let source = host.attach(1_000);
        assert_eq!(source.position(), 1_000);
    }

    #[test]
    fn poll_after_initialize_yields_registered_then_decoded_store() {
        let mut host = SyntheticHost::new(44_100);
        host.command(SourceCommand::Initialize {
            device_name: "ModPlayer on Test".to_string(),
            device_id: "deadbeef".to_string(),
        });
        let events = host.poll();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            SourceEvent::Registered { device_name } if device_name == "ModPlayer on Test"
        ));
        // 005-now-playing-waveform (research R14): the built-in track's
        // fully-filled `DecodedStore`, right after `Registered`.
        match &events[1] {
            SourceEvent::DecodedStore { track, store } => {
                assert_eq!(*track, synthetic_track_id());
                assert_eq!(store.state(), modplayer_audio_source::StoreState::Complete);
            }
            other => panic!("expected DecodedStore, got {other:?}"),
        }
        // Draining again yields nothing further.
        assert!(host.poll().is_empty());
    }

    #[test]
    fn other_commands_are_no_ops() {
        let mut host = SyntheticHost::new(44_100);
        host.command(SourceCommand::Play);
        host.command(SourceCommand::Pause);
        host.command(SourceCommand::Stop);
        assert!(host.poll().is_empty());
    }
}

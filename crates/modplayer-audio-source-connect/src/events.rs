// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PlayerEvent` -> `SourceEvent` mapping and marker emission
//! (contracts/connect-source.md §3). Stateless per call except for the
//! small amount of bookkeeping (`MapperState`) needed to resolve a
//! `TrackChanged`'s program index and to know whether a `VolumeChanged`
//! was our own echo.
//!
//! Transfer-in's full "wait up to 1 s for the following `TrackChanged`
//! before deciding whether `context` is known" behaviour (contracts/
//! connect-source.md §3, T074) is owned by `worker.rs`'s player-event
//! task, which intercepts `PlayerEvent::SessionConnected`/the following
//! `TrackChanged` *before* they ever reach this module's `map` — the
//! `SessionConnected` arm below is unreachable in production for that
//! reason and stands only as this function's own record of the plain
//! (unsolicited-context-free) case `worker.rs` falls back to on a timeout.

use std::sync::Arc;

use librespot_playback::player::PlayerEvent;
use modplayer_audio_source::{
    Availability, DecodedStore, RemoteCommand, SourceEvent, TrackId, TrackRef,
};

use crate::mixer::{HostMixer, to_pct};
use crate::program::{Marker, ProgramMap};

/// Per-worker bookkeeping the mapper needs across calls.
#[derive(Debug, Default)]
pub struct MapperState {
    program: Option<ProgramMap>,
    last_index: Option<u32>,
}

impl MapperState {
    /// Record a new `LoadProgram`. Every load restarts playback at the
    /// program's `cursor_index`, so the "what starts next" expectation is
    /// reset with it (`ProgramMap::expected_next`).
    pub fn set_program(&mut self, program: ProgramMap) {
        self.program = Some(program);
        self.last_index = None;
    }
}

/// Build a `TrackRef` from a librespot `AudioItem` (contracts/connect-
/// source.md §3's `TrackRef::from(audio_item)`).
pub fn track_ref_from_audio_item(item: &librespot_metadata::audio::AudioItem) -> Option<TrackRef> {
    let id = TrackId::new(item.uri.clone()).ok()?;
    let artists = match &item.unique_fields {
        librespot_metadata::audio::UniqueFields::Track { artists, .. } => {
            artists.0.iter().map(|a| a.name.clone()).collect()
        }
        librespot_metadata::audio::UniqueFields::Episode { show_name, .. } => {
            vec![show_name.clone()]
        }
        librespot_metadata::audio::UniqueFields::Local { artists, .. } => {
            artists.clone().into_iter().collect()
        }
    };
    let album = match &item.unique_fields {
        librespot_metadata::audio::UniqueFields::Track { album, .. } => Some(album.clone()),
        librespot_metadata::audio::UniqueFields::Local { album, .. } => album.clone(),
        librespot_metadata::audio::UniqueFields::Episode { .. } => None,
    };
    let artwork_url = item
        .covers
        .iter()
        .max_by_key(|cover| cover.width.max(cover.height))
        .map(|cover| cover.url.clone());
    Some(TrackRef::new(
        id,
        item.name.clone(),
        artists,
        album,
        artwork_url,
        item.duration_ms,
        Availability::Available,
    ))
}

/// Map one `PlayerEvent` to the `SourceEvent` (if any) and RT `Marker` (if
/// any) it implies (contract §3). `written_frames` is the sink's current
/// cumulative write count, used to stamp markers. `store` (005-now-
/// playing-waveform, contracts/connect-source-delta.md §1) is the new
/// track's already-created `DecodedStore` — `Some` only for `TrackChanged`,
/// built by the caller (`worker.rs`) right before this call so the
/// resulting `TrackStart` marker can carry it; ignored for every other
/// event.
pub fn map(
    event: PlayerEvent,
    state: &mut MapperState,
    mixer: &HostMixer,
    written_frames: u64,
    store: Option<Arc<DecodedStore>>,
) -> (Option<SourceEvent>, Option<Marker>) {
    match event {
        PlayerEvent::TrackChanged { audio_item } => {
            let Some(track) = track_ref_from_audio_item(&audio_item) else {
                return (None, None);
            };
            let program = state.program.as_ref().and_then(|map| {
                map.resolve(&track.id.to_string(), state.last_index)
                    .map(|idx| (map.generation, idx))
            });
            if let Some((_, idx)) = program {
                state.last_index = Some(idx);
            }
            let event = SourceEvent::TrackStarted {
                track,
                program,
                position_ms: 0,
                playing: true,
            };
            let marker = store.map(|store| Marker::track_start(written_frames, store));
            (Some(event), marker)
        }
        PlayerEvent::Loading { position_ms, .. } => {
            (Some(SourceEvent::Loading { position_ms }), None)
        }
        PlayerEvent::Playing { position_ms, .. } => (
            Some(SourceEvent::Playing { position_ms }),
            Some(Marker::reposition(
                written_frames,
                ms_to_frames(position_ms),
            )),
        ),
        PlayerEvent::Paused { position_ms, .. } => {
            (Some(SourceEvent::Paused { position_ms }), None)
        }
        PlayerEvent::Stopped { .. } => (Some(SourceEvent::Stopped), None),
        PlayerEvent::Seeked { position_ms, .. } => (
            Some(SourceEvent::Seeked { position_ms }),
            Some(Marker::reposition(
                written_frames,
                ms_to_frames(position_ms),
            )),
        ),
        PlayerEvent::PositionCorrection { position_ms, .. } => (
            Some(SourceEvent::Seeked { position_ms }),
            Some(Marker::reposition(
                written_frames,
                ms_to_frames(position_ms),
            )),
        ),
        PlayerEvent::EndOfTrack { .. } => (
            Some(SourceEvent::EndOfTrack),
            Some(Marker::track_end(written_frames)),
        ),
        PlayerEvent::Unavailable { track_id, .. } => {
            let Ok(id) = TrackId::new(track_id.to_string()) else {
                return (None, None);
            };
            (Some(SourceEvent::Unavailable { track: id }), None)
        }
        PlayerEvent::VolumeChanged { volume } => {
            if mixer.take_echo() {
                (None, None)
            } else {
                (
                    Some(SourceEvent::RemoteCommand(RemoteCommand::Volume(
                        modplayer_audio_source::VolumePercent::new(to_pct(volume)),
                    ))),
                    None,
                )
            }
        }
        PlayerEvent::ShuffleChanged { shuffle } => (
            Some(SourceEvent::RemoteCommand(RemoteCommand::Shuffle(shuffle))),
            None,
        ),
        PlayerEvent::RepeatChanged { context, track } => {
            let repeat = if track {
                modplayer_audio_source::Repeat::One
            } else if context {
                modplayer_audio_source::Repeat::All
            } else {
                modplayer_audio_source::Repeat::Off
            };
            (
                Some(SourceEvent::RemoteCommand(RemoteCommand::Repeat(repeat))),
                None,
            )
        }
        PlayerEvent::SessionConnected { .. } => {
            (Some(SourceEvent::BecameActive { context: None }), None)
        }
        PlayerEvent::SessionDisconnected { .. } => (Some(SourceEvent::BecameInactive), None),
        // Bookkeeping-only events with no host-visible effect this phase.
        PlayerEvent::PlayRequestIdChanged { .. }
        | PlayerEvent::Preloading { .. }
        | PlayerEvent::TimeToPreloadNextTrack { .. }
        | PlayerEvent::PositionChanged { .. }
        | PlayerEvent::SessionClientChanged { .. }
        | PlayerEvent::AutoPlayChanged { .. }
        | PlayerEvent::FilterExplicitContentChanged { .. } => (None, None),
    }
}

/// `librespot`'s own decode/output rate (Spotify streams are always
/// 44.1 kHz stereo) — matches `ConnectRtSource::sample_rate`.
const SAMPLE_RATE: u64 = 44_100;

fn ms_to_frames(position_ms: u32) -> u64 {
    (u64::from(position_ms) * SAMPLE_RATE) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_changed_is_suppressed_when_it_was_our_own_echo() {
        let mixer = HostMixer::new(50);
        mixer.set_from_host(80);
        let mut state = MapperState::default();
        let (event, marker) = map(
            PlayerEvent::VolumeChanged { volume: 60000 },
            &mut state,
            &mixer,
            0,
            None,
        );
        assert_eq!(event, None);
        assert_eq!(marker, None);
    }

    #[test]
    fn volume_changed_is_reported_when_not_an_echo() {
        let mixer = HostMixer::new(50);
        let mut state = MapperState::default();
        let (event, _marker) = map(
            PlayerEvent::VolumeChanged { volume: 0 },
            &mut state,
            &mixer,
            0,
            None,
        );
        assert_eq!(
            event,
            Some(SourceEvent::RemoteCommand(RemoteCommand::Volume(
                modplayer_audio_source::VolumePercent::new(0)
            )))
        );
    }

    #[test]
    fn end_of_track_emits_event_and_track_end_marker() {
        let mixer = HostMixer::new(50);
        let mut state = MapperState::default();
        let (event, marker) = map(
            PlayerEvent::EndOfTrack {
                play_request_id: 1,
                track_id: librespot_core::SpotifyUri::Unknown {
                    kind: "test".into(),
                    id: String::new(),
                },
            },
            &mut state,
            &mixer,
            1234,
            None,
        );
        assert_eq!(event, Some(SourceEvent::EndOfTrack));
        assert_eq!(marker, Some(Marker::track_end(1234)));
    }
}

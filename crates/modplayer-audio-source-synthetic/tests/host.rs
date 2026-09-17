// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `SyntheticHost` pins the `SourceHost` seam's additive guarantee
//! (contracts/audio-source-host.md §5): `attach` returns a
//! `SyntheticSource` at the requested position, and every 001 synthetic
//! test (continuity/determinism/segments) keeps passing unchanged
//! (verified by those still-present test files alongside this one).
//!
//! 004-search-and-library-browse (T014, contracts/catalog-source.md §4/§5)
//! adds: `SyntheticHost` answers every catalog command with
//! `Err(CatalogError::Unsupported)`; `ScriptedHost`'s scripted catalog
//! replies honour the injected delay and arrive in the order scripted.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modplayer_audio_source::{
    AudioSource, CatalogError, LibrarySet, SearchGroupPage, SearchKind, SearchPage, SourceCommand,
    SourceEvent, SourceHost,
};
use modplayer_audio_source_synthetic::{ScriptedHost, SyntheticHost};

#[test]
fn synthetic_host_attach_restores_position() {
    let mut host = SyntheticHost::new(44_100);
    let source = host.attach(12_345);
    assert_eq!(source.position(), 12_345);
    assert_eq!(source.sample_rate(), 44_100);
}

#[test]
fn synthetic_host_poll_after_initialize_yields_exactly_registered() {
    let mut host = SyntheticHost::new(44_100);
    let events = host.poll();
    assert!(events.is_empty(), "no events before any command");

    host.command(SourceCommand::Initialize {
        device_name: "ModPlayer on Test".to_string(),
        device_id: "0123456789abcdef".to_string(),
    });
    let events = host.poll();
    assert_eq!(
        events,
        vec![SourceEvent::Registered {
            device_name: "ModPlayer on Test".to_string()
        }]
    );
}

#[test]
fn synthetic_host_answers_every_catalog_command_with_unsupported() {
    let mut host = SyntheticHost::new(44_100);

    host.command(SourceCommand::SearchCatalog {
        request_id: 1,
        query: "test".to_string(),
        kinds: vec![SearchKind::Track],
        offset: 0,
        limit: 20,
    });
    host.command(SourceCommand::FetchLibrary {
        request_id: 2,
        set: LibrarySet::SavedTracks,
        page: None,
        limit: 50,
    });
    host.command(SourceCommand::FetchTrackList {
        request_id: 3,
        source: modplayer_audio_source::TrackListSource::Album(
            modplayer_audio_source::AlbumId::new("spotify:album:abc")
                .unwrap_or_else(|_| unreachable!()),
        ),
    });

    let events = host.poll();
    assert_eq!(events.len(), 3);
    assert!(matches!(
        &events[0],
        SourceEvent::SearchResult {
            request_id: 1,
            result: Err(CatalogError::Unsupported)
        }
    ));
    assert!(matches!(
        &events[1],
        SourceEvent::LibraryPage {
            request_id: 2,
            result: Err(CatalogError::Unsupported)
        }
    ));
    assert!(matches!(
        &events[2],
        SourceEvent::TrackList {
            request_id: 3,
            result: Err(CatalogError::Unsupported)
        }
    ));
}

#[test]
fn synthetic_host_hydrate_reports_every_requested_id_missing() {
    let mut host = SyntheticHost::new(44_100);
    let track = modplayer_audio_source::TrackId::new("spotify:track:abc")
        .unwrap_or_else(|_| unreachable!());
    host.command(SourceCommand::HydrateRefs {
        request_id: 9,
        tracks: vec![track.clone()],
        albums: vec![],
        artists: vec![],
    });
    let events = host.poll();
    assert!(matches!(
        events.as_slice(),
        [SourceEvent::Hydrated { request_id: 9, tracks, albums, artists, missing }]
            if tracks.is_empty() && albums.is_empty() && artists.is_empty()
                && missing == &vec![track.to_string()]
    ));
}

fn search_reply(kind: SearchKind) -> Result<SearchPage, CatalogError> {
    Ok(SearchPage {
        groups: vec![SearchGroupPage {
            kind,
            items: vec![],
            next_offset: None,
        }],
        unsupported: vec![],
    })
}

/// A mutable, clock-swappable `Instant`, shared with `ScriptedHost` via
/// `set_clock`, so a test can "advance time" without sleeping.
fn fake_clock(
    start: Instant,
) -> (
    Arc<Mutex<Instant>>,
    impl Fn() -> Instant + Send + Sync + 'static,
) {
    let now = Arc::new(Mutex::new(start));
    let read = Arc::clone(&now);
    #[allow(clippy::unwrap_used)]
    let getter = move || *read.lock().unwrap();
    (now, getter)
}

#[test]
fn scripted_catalog_reply_is_held_until_the_delay_elapses() {
    let host = ScriptedHost::new();
    let start = Instant::now();
    let (clock, getter) = fake_clock(start);
    host.set_clock(getter);
    host.script_catalog_delay(Duration::from_millis(100));
    host.script_search("beatles", search_reply(SearchKind::Track));

    let mut host = host;
    host.command(SourceCommand::SearchCatalog {
        request_id: 1,
        query: "beatles".to_string(),
        kinds: vec![SearchKind::Track],
        offset: 0,
        limit: 20,
    });

    assert!(
        host.poll().is_empty(),
        "reply must not appear before the scripted delay elapses"
    );

    #[allow(clippy::unwrap_used)]
    {
        *clock.lock().unwrap() = start + Duration::from_millis(100);
    }
    let events = host.poll();
    assert!(matches!(
        events.as_slice(),
        [SourceEvent::SearchResult {
            request_id: 1,
            result: Ok(_)
        }]
    ));
}

#[test]
fn scripted_catalog_replies_for_the_same_query_arrive_in_scripted_order() {
    let mut host = ScriptedHost::new();
    host.script_search("beatles", search_reply(SearchKind::Track));
    host.script_search("beatles", search_reply(SearchKind::Album));

    host.command(SourceCommand::SearchCatalog {
        request_id: 1,
        query: "beatles".to_string(),
        kinds: vec![SearchKind::Track],
        offset: 0,
        limit: 20,
    });
    host.command(SourceCommand::SearchCatalog {
        request_id: 2,
        query: "beatles".to_string(),
        kinds: vec![SearchKind::Album],
        offset: 0,
        limit: 20,
    });

    let events = host.poll();
    let kinds: Vec<SearchKind> = events
        .into_iter()
        .filter_map(|event| match event {
            SourceEvent::SearchResult {
                result: Ok(page), ..
            } => page.groups.first().map(|g| g.kind),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, vec![SearchKind::Track, SearchKind::Album]);
}

#[test]
fn scripted_host_record_commands_exposes_every_command_received() {
    let mut host = ScriptedHost::new();
    assert!(host.record_commands().is_empty());
    host.command(SourceCommand::SearchCatalog {
        request_id: 1,
        query: "no request issued".to_string(),
        kinds: vec![SearchKind::Track],
        offset: 0,
        limit: 20,
    });
    assert_eq!(host.record_commands().len(), 1);
}

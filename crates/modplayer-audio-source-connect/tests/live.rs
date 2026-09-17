// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T044: live manual test against the real Spotify Connect protocol.
//! `#[ignore = "manual"]` — needs a live, valid `MODPLAYER_TEST_ACCESS_TOKEN`
//! (scope `streaming`) and a real Premium account; never run in CI
//! (contracts/connect-source.md §7).
//!
//! Run explicitly with:
//! `MODPLAYER_TEST_ACCESS_TOKEN=... cargo test -p modplayer-audio-source-connect --test live -- --ignored --nocapture`

use std::sync::Arc;
use std::time::Duration;

use modplayer_audio_source::{
    AudioSource, DecodedStore, LibrarySet, Program, SearchKind, SourceCommand, SourceEvent,
    SourceHost, TrackId,
};
use modplayer_audio_source_connect::{
    ConnectConfig, ConnectSource, CredentialError, ReceiverCredentials,
};

struct EnvCredentials;

impl ReceiverCredentials for EnvCredentials {
    fn access_token(&self) -> Result<String, CredentialError> {
        std::env::var("MODPLAYER_TEST_ACCESS_TOKEN").map_err(|_| CredentialError::Unavailable)
    }
}

/// A well-known, long-lived, freely-playable Spotify track (used by
/// librespot's own examples): "Sanctuary" is not assumed available
/// forever, so this is deliberately overridable via
/// `MODPLAYER_TEST_TRACK_URI` for whoever runs the manual suite next.
fn test_track_uri() -> String {
    std::env::var("MODPLAYER_TEST_TRACK_URI")
        .unwrap_or_else(|_| "spotify:track:6rqhFgbbKwnb9MLmUQDhG6".to_string())
}

#[test]
#[ignore = "manual"]
fn plays_five_seconds_of_a_real_track() {
    let Ok(_) = std::env::var("MODPLAYER_TEST_ACCESS_TOKEN") else {
        panic!("set MODPLAYER_TEST_ACCESS_TOKEN to run this manual test");
    };

    let mut source = ConnectSource::new(ConnectConfig {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
        credentials: Arc::new(EnvCredentials),
    });

    let rt = source.attach(0);
    let shared = source.rt_shared();

    source.command(SourceCommand::Initialize {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
    });

    let track = TrackId::new(test_track_uri()).expect("valid test track uri");
    let mut registered = false;
    let mut track_started = false;
    let mut playing = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);

    while std::time::Instant::now() < deadline && !(registered && track_started && playing) {
        for event in source.poll() {
            match event {
                SourceEvent::Registered { .. } => {
                    registered = true;
                    source.command(SourceCommand::LoadProgram(Program {
                        order: vec![track.clone()],
                        cursor_index: 0,
                        position_ms: 0,
                        start_playing: true,
                        repeat_all: false,
                        repeat_one: false,
                        generation: 1,
                    }));
                }
                SourceEvent::TrackStarted { .. } => track_started = true,
                SourceEvent::Playing { .. } => playing = true,
                SourceEvent::Health(health) => println!("health: {health:?}"),
                other => println!("event: {other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(registered, "must register with the service");
    assert!(track_started, "must start the requested track");
    assert!(playing, "must reach the Playing state");

    // Pull ~5 s of audio through the RT half and confirm it is not
    // silence throughout (contract §7's "non-silent frames consumed").
    let mut rt = rt;
    let mut buffer = vec![0.0f32; 4096];
    let mut non_silent_frames: u64 = 0;
    let target_frames: u64 = 44_100 * 5;
    let read_deadline = std::time::Instant::now() + Duration::from_secs(15);
    while shared.consumed_frames() < target_frames && std::time::Instant::now() < read_deadline {
        rt.fill(&mut buffer);
        non_silent_frames += buffer
            .chunks(2)
            .filter(|frame| frame[0].abs() > 1e-6 || frame[1].abs() > 1e-6)
            .count() as u64;
        std::thread::sleep(Duration::from_millis(20));
    }

    // Require ≥ 4 s of the ~5 s window to be non-silent. The original
    // "all 220_500 frames non-silent" demanded zero silence across the
    // whole window, which no real network stream meets — there is always a
    // brief initial buffering gap (measured ≈ 90 ms / ~4 k frames on a
    // verified live run, i.e. ~4.9 s of the 5 s was audible). 4 s still
    // proves sustained real music, not a decode blip.
    assert!(
        non_silent_frames >= 176_400,
        "expected at least 176400 non-silent frames (4s @ 44.1kHz), got {non_silent_frames}"
    );

    source.command(SourceCommand::Shutdown);
}

/// T056 (US3, contracts/connect-source-delta.md §5): the decode-ahead
/// thread must cover the whole track well before real-time playback has
/// consumed even a quarter of it, and a seek hint into a still-undecoded
/// region must be covered within a few seconds.
#[test]
#[ignore = "manual"]
fn decode_ahead_fills_store_faster_than_playback() {
    let Ok(_) = std::env::var("MODPLAYER_TEST_ACCESS_TOKEN") else {
        panic!("set MODPLAYER_TEST_ACCESS_TOKEN to run this manual test");
    };

    let mut source = ConnectSource::new(ConnectConfig {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
        credentials: Arc::new(EnvCredentials),
    });

    let rt = source.attach(0);
    let shared = source.rt_shared();

    source.command(SourceCommand::Initialize {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
    });

    let track = TrackId::new(test_track_uri()).expect("valid test track uri");
    let mut registered = false;
    let mut store: Option<Arc<DecodedStore>> = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);

    while std::time::Instant::now() < deadline && store.is_none() {
        for event in source.poll() {
            match event {
                SourceEvent::Registered { .. } => {
                    registered = true;
                    source.command(SourceCommand::LoadProgram(Program {
                        order: vec![track.clone()],
                        cursor_index: 0,
                        position_ms: 0,
                        start_playing: true,
                        repeat_all: false,
                        repeat_one: false,
                        generation: 1,
                    }));
                }
                SourceEvent::DecodedStore { store: s, .. } => store = Some(s),
                SourceEvent::Health(health) => println!("health: {health:?}"),
                other => println!("event: {other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(registered, "must register with the service");
    let store = store.expect("a SourceEvent::DecodedStore within the deadline");
    let len_frames = store.len_frames().max(1);
    let quarter_frames = len_frames / 4;

    // Pull audio through the RT half at real time while independently
    // polling the decode-ahead's own coverage (never through the store's
    // raw sample accessor — `covered_frames`/`covers` only, Constitution V).
    let mut rt = rt;
    let mut buffer = vec![0.0f32; 4096];
    let read_deadline = std::time::Instant::now() + Duration::from_secs(60);
    let mut reached_full_coverage = false;
    while std::time::Instant::now() < read_deadline {
        rt.fill(&mut buffer);
        if store.covered_frames() >= len_frames {
            reached_full_coverage = true;
            break;
        }
        if shared.consumed_frames() >= quarter_frames {
            break; // failure: 25% consumed before full coverage
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        reached_full_coverage,
        "decode-ahead must cover the whole track ({len_frames} frames) before playback \
         consumes 25% of it (consumed {} frames, covered {} frames)",
        shared.consumed_frames(),
        store.covered_frames()
    );

    // A seek hint at 75% must be covered within 3s.
    let target_frame = (len_frames * 3) / 4;
    let target_ms = ((target_frame * 1000) / u64::from(store.sample_rate())) as u32;
    source.command(SourceCommand::Seek(target_ms));
    let seek_deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < seek_deadline && !store.covers(target_frame) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        store.covers(target_frame),
        "a 75% seek hint must be covered within 3s"
    );

    source.command(SourceCommand::Shutdown);
}

/// research R14 V1: confirms (or refutes) mercury `searchview` still
/// answers a Keymaster session in 2026 — record the outcome back in
/// `research.md` R14 before wiring a UI task that assumes four search
/// groups (contracts/catalog-source.md §1, `catalog::search`).
#[test]
#[ignore = "manual"]
fn search_probe() {
    let Ok(_) = std::env::var("MODPLAYER_TEST_ACCESS_TOKEN") else {
        panic!("set MODPLAYER_TEST_ACCESS_TOKEN to run this manual test");
    };
    let query =
        std::env::var("MODPLAYER_TEST_SEARCH_QUERY").unwrap_or_else(|_| "daft punk".to_string());

    let mut source = ConnectSource::new(ConnectConfig {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
        credentials: Arc::new(EnvCredentials),
    });
    let _rt = source.attach(0);

    source.command(SourceCommand::Initialize {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
    });

    let mut registered = false;
    let mut requested = false;
    let mut result = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);

    while std::time::Instant::now() < deadline && result.is_none() {
        for event in source.poll() {
            match event {
                SourceEvent::Registered { .. } => {
                    registered = true;
                    source.command(SourceCommand::SearchCatalog {
                        request_id: 1,
                        query: query.clone(),
                        kinds: vec![
                            SearchKind::Track,
                            SearchKind::Album,
                            SearchKind::Artist,
                            SearchKind::Playlist,
                        ],
                        offset: 0,
                        limit: 20,
                    });
                    requested = true;
                }
                SourceEvent::SearchResult {
                    request_id: 1,
                    result: r,
                } => result = Some(r),
                SourceEvent::Health(health) => println!("health: {health:?}"),
                other => println!("event: {other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(registered, "must register with the service");
    assert!(requested, "must have issued the SearchCatalog command");
    let result = result.expect("a SearchResult reply within the deadline");
    match result {
        Ok(page) => {
            // `searchview` answered — redacted item counts only, per group.
            for group in &page.groups {
                println!("group {:?}: {} hit(s)", group.kind, group.items.len());
            }
            println!("unsupported: {:?}", page.unsupported);
            println!("strategy: searchview (or context-resolve fallback, both surface as Ok)");
        }
        Err(err) => println!("search_probe: every strategy failed: {err}"),
    }
}

/// research R14 V2: confirms (or refutes) `/collection/v2/paging` accepts
/// the session bearer for `collection`/`artist`, and the rootlist for
/// `Playlists` — record the outcome back in `research.md` R14 before
/// treating all three collection sets as confirmed (contracts/catalog-
/// source.md §1, `catalog::collection`/`catalog::playlists`).
#[test]
#[ignore = "manual"]
fn library_sets_probe() {
    let Ok(_) = std::env::var("MODPLAYER_TEST_ACCESS_TOKEN") else {
        panic!("set MODPLAYER_TEST_ACCESS_TOKEN to run this manual test");
    };

    let mut source = ConnectSource::new(ConnectConfig {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
        credentials: Arc::new(EnvCredentials),
    });
    let _rt = source.attach(0);
    source.command(SourceCommand::Initialize {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
    });

    let sets = [
        (1u64, LibrarySet::SavedTracks),
        (2u64, LibrarySet::SavedAlbums),
        (3u64, LibrarySet::FollowedArtists),
        (4u64, LibrarySet::Playlists),
    ];
    let mut registered = false;
    let mut requested = false;
    let mut results: std::collections::HashMap<u64, _> = std::collections::HashMap::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);

    while std::time::Instant::now() < deadline && results.len() < sets.len() {
        for event in source.poll() {
            match event {
                SourceEvent::Registered { .. } => {
                    registered = true;
                    for (request_id, set) in sets {
                        source.command(SourceCommand::FetchLibrary {
                            request_id,
                            set,
                            page: None,
                            limit: 50,
                        });
                    }
                    requested = true;
                }
                SourceEvent::LibraryPage { request_id, result } => {
                    results.insert(request_id, result);
                }
                SourceEvent::Health(health) => println!("health: {health:?}"),
                other => println!("event: {other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(registered, "must register with the service");
    assert!(requested, "must have issued every FetchLibrary command");
    for (request_id, set) in sets {
        match results.get(&request_id) {
            Some(Ok(page)) => println!(
                "{set:?}: {} item(s), next_page={:?} (strategy answered)",
                page.items.len(),
                page.next_page
            ),
            Some(Err(err)) => println!("{set:?}: failed: {err} (fallback/Unsupported path)"),
            None => println!("{set:?}: no reply within the deadline"),
        }
    }
}

/// research R14 V4: confirms `Track.availability`/`restrictions` correctly
/// distinguish region-lock from removal for the session country, against a
/// known region-locked track (`MODPLAYER_TEST_RESTRICTED_TRACK_URI`) —
/// record the outcome back in `research.md` R14 (contracts/catalog-
/// source.md §3 "Availability mapping").
#[test]
#[ignore = "manual"]
fn availability_probe() {
    let Ok(_) = std::env::var("MODPLAYER_TEST_ACCESS_TOKEN") else {
        panic!("set MODPLAYER_TEST_ACCESS_TOKEN to run this manual test");
    };
    let Ok(restricted_uri) = std::env::var("MODPLAYER_TEST_RESTRICTED_TRACK_URI") else {
        panic!(
            "set MODPLAYER_TEST_RESTRICTED_TRACK_URI to a track known to be \
             region-locked/removed for the test account's session country"
        );
    };
    let track_id = TrackId::new(restricted_uri).expect("valid test track uri");

    let mut source = ConnectSource::new(ConnectConfig {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
        credentials: Arc::new(EnvCredentials),
    });
    let _rt = source.attach(0);
    source.command(SourceCommand::Initialize {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
    });

    let mut registered = false;
    let mut requested = false;
    let mut reply = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);

    while std::time::Instant::now() < deadline && reply.is_none() {
        for event in source.poll() {
            match event {
                SourceEvent::Registered { .. } => {
                    registered = true;
                    source.command(SourceCommand::HydrateRefs {
                        request_id: 1,
                        tracks: vec![track_id.clone()],
                        albums: vec![],
                        artists: vec![],
                    });
                    requested = true;
                }
                SourceEvent::Hydrated {
                    request_id: 1,
                    tracks,
                    missing,
                    ..
                } => reply = Some((tracks, missing)),
                SourceEvent::Health(health) => println!("health: {health:?}"),
                other => println!("event: {other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(registered, "must register with the service");
    assert!(requested, "must have issued the HydrateRefs command");
    let (tracks, missing) = reply.expect("a Hydrated reply within the deadline");
    match tracks.first() {
        Some(track) => println!(
            "availability_probe: {} -> {:?} (missing: {missing:?})",
            track.id, track.availability
        ),
        None => println!("availability_probe: track missing entirely: {missing:?}"),
    }
}

/// research R14 V1/V2 *diagnostic*: the strategy probes above only see the
/// receiver's classified `CatalogError`, which by design (contracts/
/// catalog-source.md §2 rule 4) hides *which* strategy answered and *why*
/// the other refused. This probe drives a raw `Session` at the same
/// endpoints and prints only redacted diagnostics — librespot `ErrorKind`
/// / HTTP status, item counts, and the JSON *key names* of a `searchview`
/// reply (never a value, never a body) — so R14 can be recorded from
/// evidence rather than inference. Never run in CI.
///
/// 2026-09-17 outcome (recorded in research.md R14): `searchview` →
/// `ErrorKind::Unavailable` (mercury retired); `/collection/v2/paging` →
/// HTTP 400 for both sets; `spotify:user:<u>:collection` context → OK.
#[test]
#[ignore = "manual"]
fn catalog_wire_probe() {
    use http::header::CONTENT_TYPE;
    use http::{HeaderMap, HeaderValue, Method};
    use librespot_core::authentication::Credentials;
    use librespot_core::config::SessionConfig;
    use librespot_core::session::Session;

    let Ok(token) = std::env::var("MODPLAYER_TEST_ACCESS_TOKEN") else {
        panic!("set MODPLAYER_TEST_ACCESS_TOKEN to run this manual test");
    };
    let query =
        std::env::var("MODPLAYER_TEST_SEARCH_QUERY").unwrap_or_else(|_| "daft punk".to_string());

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async move {
        let session = Session::new(
            SessionConfig {
                device_id: "0123456789abcdef0123456789abcdef".to_string(),
                ..SessionConfig::default()
            },
            None,
        );
        session
            .connect(Credentials::with_access_token(token), false)
            .await
            .expect("session connect");
        // The country code arrives on its own packet shortly after connect.
        tokio::time::sleep(Duration::from_secs(2)).await;
        println!(
            "session: country={} username_len={}",
            session.country(),
            session.username().len()
        );
        match session.spclient().client_token().await {
            Ok(token) => println!("client_token: ok len={}", token.len()),
            Err(e) => println!("client_token: kind={:?}", e.kind),
        }

        // -- V1: mercury searchview -----------------------------------------
        let url = format!(
            "hm://searchview/km/v4/search/{}?entityVersion=2&limit=20&offset=0&catalogue=&country={}&locale=en&username={}&imageSize=large",
            query.replace(' ', "+"),
            session.country(),
            session.username()
        );
        match session.mercury().get(url) {
            Err(e) => println!("searchview: mercury get refused: kind={:?}", e.kind),
            Ok(future) => match future.await {
                Err(e) => println!("searchview: mercury error: kind={:?}", e.kind),
                Ok(response) => {
                    let body: Vec<u8> = response.payload.into_iter().flatten().collect();
                    println!(
                        "searchview: status={} body_len={}",
                        response.status_code,
                        body.len()
                    );
                    let keys = |v: &serde_json::Value| -> Vec<String> {
                        v.as_object()
                            .map(|o| o.keys().cloned().collect())
                            .unwrap_or_default()
                    };
                    match serde_json::from_slice::<serde_json::Value>(&body) {
                        Err(_) => println!("searchview: body is not JSON"),
                        Ok(value) => {
                            println!("searchview: top-level keys={:?}", keys(&value));
                            let groups = value
                                .get("results")
                                .and_then(|r| r.as_object())
                                .into_iter()
                                .flatten();
                            for (group, inner) in groups {
                                let hits = inner.get("hits").and_then(|h| h.as_array());
                                println!(
                                    "searchview: results.{group} keys={:?} hits={:?} first_hit_keys={:?}",
                                    keys(inner),
                                    hits.map(|h| h.len()),
                                    hits.and_then(|h| h.first()).map(keys),
                                );
                            }
                        }
                    }
                }
            },
        }

        // -- V2 fallback: context-resolve of the user's collection ---------------
        let uri = format!("spotify:user:{}:collection", session.username());
        match session.spclient().get_context(&uri).await {
            Ok(ctx) => println!(
                "context collection: pages={} tracks={}",
                ctx.pages.len(),
                ctx.pages.iter().map(|p| p.tracks.len()).sum::<usize>()
            ),
            Err(e) => println!("context collection: kind={:?}", e.kind),
        }

        // -- V2: collection2v2 paging -------------------------------------------
        for set in ["collection", "artist"] {
            let mut body = Vec::new();
            {
                let mut out = protobuf::CodedOutputStream::vec(&mut body);
                let _ = out.write_string(1, &session.username());
                let _ = out.write_string(2, set);
                let _ = out.write_int32(4, 50);
                let _ = out.flush();
            }
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-protobuf"),
            );
            match session
                .spclient()
                .request(
                    &Method::POST,
                    "/collection/v2/paging",
                    Some(headers),
                    Some(&body),
                )
                .await
            {
                Ok(bytes) => println!("collection2v2 set={set}: ok body_len={}", bytes.len()),
                Err(e) => println!("collection2v2 set={set}: kind={:?} ({})", e.kind, e.error),
            }
        }
    });
}

/// research R14 V4 *diagnostic*: dumps the raw `Restriction` fields
/// (catalogues, type, whether the session country is in the allowed /
/// forbidden lists) for the first saved tracks, next to the receiver's
/// computed `Availability`, so a false "Unavailable in your region" can be
/// attributed to a specific restriction shape. Prints track names (the
/// maintainer's own library) — no tokens, no URLs.
#[test]
#[ignore = "manual"]
fn restrictions_probe() {
    use librespot_core::SpotifyUri;
    use librespot_core::authentication::Credentials;
    use librespot_core::config::SessionConfig;
    use librespot_core::session::Session;
    use librespot_metadata::{Metadata, Track};
    use modplayer_audio_source_connect::catalog::hydrate::availability_for;

    let Ok(token) = std::env::var("MODPLAYER_TEST_ACCESS_TOKEN") else {
        panic!("set MODPLAYER_TEST_ACCESS_TOKEN to run this manual test");
    };
    let limit: usize = std::env::var("MODPLAYER_TEST_RESTRICTIONS_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async move {
        let session = Session::new(
            SessionConfig {
                device_id: "0123456789abcdef0123456789abcdef".to_string(),
                ..SessionConfig::default()
            },
            None,
        );
        session
            .connect(Credentials::with_access_token(token), false)
            .await
            .expect("session connect");
        tokio::time::sleep(Duration::from_secs(2)).await;
        let country = session.country();
        println!("session country={country}");

        let uri = format!("spotify:user:{}:collection", session.username());
        let ctx = session.spclient().get_context(&uri).await.expect("collection context");
        let uris: Vec<String> = ctx
            .pages
            .iter()
            .flat_map(|p| p.tracks.iter())
            .filter_map(|t| t.uri.clone())
            .take(limit)
            .collect();
        for uri in uris {
            let Ok(spotify_uri) = SpotifyUri::from_uri(&uri) else { continue };
            let Ok(track) = Track::get(&session, &spotify_uri).await else {
                println!("{uri}: Track::get failed");
                continue;
            };
            let computed = availability_for(&session, &track.restrictions);
            println!(
                "{} | {:?} | restrictions={} | availability(sale periods)={}",
                track.name,
                computed,
                track.restrictions.len(),
                track.availability.len()
            );
            for r in track.restrictions.iter() {
                let allowed = r.countries_allowed.as_ref().map(|a| (a.len(), a.iter().any(|c| c == &country)));
                let forbidden = r.countries_forbidden.as_ref().map(|f| (f.len(), f.iter().any(|c| c == &country)));
                println!(
                    "    catalogues={:?} strs={:?} type={:?} allowed(len,has_country)={:?} forbidden(len,has_country)={:?}",
                    r.catalogues.iter().collect::<Vec<_>>(),
                    r.catalogue_strs,
                    r.restriction_type,
                    allowed,
                    forbidden
                );
            }
        }
    });
}

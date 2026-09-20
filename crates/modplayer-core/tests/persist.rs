// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `library::persist` (contracts/library-and-search-core.md §5-6, data-
//! model.md §3.4/§3.5): atomic write, crash-mid-write safety, newer-schema
//! degrade, sign-out deletion (the same file-level behaviours
//! `settings::store` already established) plus two Constitution VIII
//! state-serialization proptests over the two new persisted formats —
//! `index_file_round_trips_any_index` (arbitrary `LibraryIndex` contents:
//! every set, refs incl. Unicode names, optional fields, every
//! `Availability`, sync meta) and `play_log_file_round_trips_any_log`
//! (arbitrary `last_played`/`play_count`/`first_played` maps and `refs`:
//! save -> load == original, `recent` re-derives identically).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, Availability, PlaylistId, PlaylistRef, ReleaseDate,
    TrackId, TrackListSource, TrackRef,
};
use modplayer_core::library::index::{LibraryIndex, SavedEntry, SyncMeta, SyncOutcome};
use modplayer_core::library::persist::{
    self, LibraryPaths, LoadWarning, load_index, load_play_log, save_index, save_play_log,
};
use modplayer_core::library::play_log::PlayLog;
use modplayer_core::settings::{AudioSettings, SettingsStore};
use proptest::prelude::*;

fn temp_paths(tag: &str) -> LibraryPaths {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "modplayer-persist-test-{tag}-{}-{}",
        std::process::id(),
        unique
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    LibraryPaths::with_dir(dir)
}

fn sample_track(id: &str, availability: Availability) -> TrackRef {
    TrackRef::new(
        TrackId::new(id).unwrap(),
        "Café ☕",
        vec!["Sigur Rós".to_string()],
        Some("Ágætis byrjun".to_string()),
        Some("https://i.scdn.co/image/abc".to_string()),
        201_000,
        availability,
    )
    .with_extras(modplayer_audio_source::TrackRefExtras {
        artist_ids: vec![ArtistId::new("spotify:artist:sigur").unwrap()],
        album_id: Some(AlbumId::new("spotify:album:agaetis").unwrap()),
        explicit: true,
        release_date: Some(ReleaseDate {
            year: 1999,
            month: Some(6),
            day: None,
        }),
    })
}

#[test]
fn atomic_write_survives_a_simulated_crash_mid_write() {
    let paths = temp_paths("crash");
    let index = LibraryIndex::new();
    save_index(&paths, &index).expect("save");
    std::fs::write(
        paths.index_path.with_file_name("index.json.tmp"),
        "garbage, never renamed",
    )
    .expect("write tmp");
    let outcome = load_index(&paths);
    assert_eq!(outcome.warning, None);
    assert_eq!(outcome.index, index);
}

#[test]
fn a_newer_schema_degrades_to_empty_with_one_warning() {
    let paths = temp_paths("newer-schema");
    std::fs::create_dir_all(paths.index_path.parent().unwrap()).unwrap();
    std::fs::write(&paths.index_path, r#"{"schema":999999}"#).expect("write");
    let outcome = load_index(&paths);
    assert_eq!(outcome.warning, Some(LoadWarning::NewerSchema));
    assert_eq!(outcome.index, LibraryIndex::new());
}

#[test]
fn sign_out_deletes_both_files() {
    let paths = temp_paths("sign-out");
    save_index(&paths, &LibraryIndex::new()).expect("save index");
    save_play_log(&paths, &PlayLog::new()).expect("save log");
    assert!(paths.index_path.exists());
    assert!(paths.play_log_path.exists());
    persist::delete_all(&paths);
    assert!(!paths.index_path.exists());
    assert!(!paths.play_log_path.exists());
}

#[test]
fn a_realistic_index_with_every_set_and_availability_round_trips() {
    let paths = temp_paths("realistic-index");
    let mut tracks = HashMap::new();
    tracks.insert(
        TrackId::new("spotify:track:available").unwrap(),
        sample_track("spotify:track:available", Availability::Available),
    );
    tracks.insert(
        TrackId::new("spotify:track:region").unwrap(),
        sample_track("spotify:track:region", Availability::UnavailableRegion),
    );
    tracks.insert(
        TrackId::new("spotify:track:gone").unwrap(),
        sample_track("spotify:track:gone", Availability::Removed),
    );

    let mut albums = HashMap::new();
    albums.insert(
        AlbumId::new("spotify:album:a").unwrap(),
        AlbumRef {
            id: AlbumId::new("spotify:album:a").unwrap(),
            name: "アルバム".to_string(),
            artists: vec!["アーティスト".to_string()],
            artwork_url: None,
            release_date: Some(ReleaseDate {
                year: 2001,
                month: None,
                day: None,
            }),
            track_count: 10,
        },
    );

    let mut artists = HashMap::new();
    artists.insert(
        ArtistId::new("spotify:artist:a").unwrap(),
        ArtistRef {
            id: ArtistId::new("spotify:artist:a").unwrap(),
            name: "Björk".to_string(),
            artwork_url: None,
        },
    );

    let mut playlist_refs = HashMap::new();
    playlist_refs.insert(
        PlaylistId::new("spotify:playlist:a").unwrap(),
        PlaylistRef {
            id: PlaylistId::new("spotify:playlist:a").unwrap(),
            name: "Roadtrip 🚗".to_string(),
            owner_name: "Álex".to_string(),
            editable: false,
            artwork_url: None,
            track_count: 3,
            revision: Some("rev-1".to_string()),
        },
    );

    let mut track_lists = HashMap::new();
    track_lists.insert(
        TrackListSource::Album(AlbumId::new("spotify:album:a").unwrap()),
        vec![TrackId::new("spotify:track:available").unwrap()],
    );

    let mut sync_tokens = HashMap::new();
    sync_tokens.insert(
        modplayer_audio_source::LibrarySet::SavedTracks,
        "token-xyz".to_string(),
    );

    let index = LibraryIndex::from_parts(
        vec![SavedEntry {
            id: TrackId::new("spotify:track:available").unwrap(),
            added_at: Some(1_700_000_000_000),
        }],
        vec![SavedEntry {
            id: AlbumId::new("spotify:album:a").unwrap(),
            added_at: None,
        }],
        vec![ArtistId::new("spotify:artist:a").unwrap()],
        vec![PlaylistId::new("spotify:playlist:a").unwrap()],
        tracks,
        albums,
        artists,
        playlist_refs,
        track_lists,
        SyncMeta {
            last_synced_at: Some(1_700_000_000_000),
            last_outcome: SyncOutcome::Partial,
            sync_tokens,
        },
    );

    save_index(&paths, &index).expect("save");
    let outcome = load_index(&paths);
    assert_eq!(outcome.warning, None);
    assert_eq!(outcome.index, index);
}

// -- Constitution VIII: state-serialization proptests -----------------------

fn arb_availability() -> impl Strategy<Value = Availability> {
    prop_oneof![
        Just(Availability::Available),
        Just(Availability::UnavailableRegion),
        Just(Availability::Removed),
    ]
}

/// A handful of tracks with varied Unicode/optional-field content and every
/// `Availability` variant is enough to exercise the DTO round trip without
/// the strategy graph becoming unreadable — real `LibraryIndex` values are
/// built the same way (`merge_page` growing `saved_tracks`/`tracks`
/// together), so this mirrors the shape persistence actually sees.
fn arb_index() -> impl Strategy<Value = LibraryIndex> {
    (
        proptest::collection::vec(0u32..20, 0..5),
        proptest::collection::vec(arb_availability(), 0..5),
        proptest::option::of(1u64..2_000_000_000_000),
        prop_oneof![
            Just(SyncOutcome::Never),
            Just(SyncOutcome::Ok),
            Just(SyncOutcome::Partial),
            Just(SyncOutcome::RateLimited),
            Just(SyncOutcome::Failed),
        ],
    )
        .prop_map(|(ids, availabilities, last_synced_at, last_outcome)| {
            let mut tracks = HashMap::new();
            let mut saved_tracks = Vec::new();
            for (i, &n) in ids.iter().enumerate() {
                let id = TrackId::new(format!("spotify:track:{n}")).unwrap();
                let availability = availabilities
                    .get(i % availabilities.len().max(1))
                    .copied()
                    .unwrap_or(Availability::Available);
                let track = TrackRef::new(
                    id.clone(),
                    format!("Título {n} ünïcode"),
                    vec![format!("Artist {n}")],
                    if n % 2 == 0 {
                        Some(format!("Album {n}"))
                    } else {
                        None
                    },
                    None,
                    (n * 1000) % 600_000,
                    availability,
                )
                .with_extras(modplayer_audio_source::TrackRefExtras {
                    explicit: n % 3 == 0,
                    release_date: if n % 2 == 0 {
                        Some(ReleaseDate {
                            year: 1990 + (n as u16 % 30),
                            month: None,
                            day: None,
                        })
                    } else {
                        None
                    },
                    ..Default::default()
                });
                tracks.insert(id.clone(), track);
                if !saved_tracks
                    .iter()
                    .any(|e: &SavedEntry<TrackId>| e.id == id)
                {
                    saved_tracks.push(SavedEntry {
                        id,
                        added_at: if n % 2 == 0 { Some(u64::from(n)) } else { None },
                    });
                }
            }
            let meta = SyncMeta {
                last_synced_at,
                last_outcome,
                ..SyncMeta::default()
            };
            LibraryIndex::from_parts(
                saved_tracks,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                tracks,
                HashMap::new(),
                HashMap::new(),
                HashMap::new(),
                HashMap::new(),
                meta,
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Constitution VIII: arbitrary `LibraryIndex` contents survive a
    /// save -> load round trip byte-for-byte (data-model.md §3.4).
    #[test]
    fn index_file_round_trips_any_index(index in arb_index()) {
        let paths = temp_paths("proptest-index");
        save_index(&paths, &index).expect("save");
        let outcome = load_index(&paths);
        prop_assert_eq!(outcome.warning, None);
        prop_assert_eq!(outcome.index, index);
    }

    /// Constitution VIII: arbitrary `last_played`/`play_count`/
    /// `first_played`/`refs` survive a save -> load round trip, and
    /// `recent()` re-derives identically on both sides (contracts/
    /// library-and-search-core.md §6).
    #[test]
    fn play_log_file_round_trips_any_log(
        entries in proptest::collection::vec(
            (0u32..30, 1u64..2_000_000_000_000, 1u32..50),
            0..12,
        )
    ) {
        let mut last_played = HashMap::new();
        let mut play_count = HashMap::new();
        let mut first_played = HashMap::new();
        let mut refs = HashMap::new();
        for (n, ts, count) in entries {
            let id = TrackId::new(format!("spotify:track:{n}")).unwrap();
            last_played.insert(id.clone(), ts);
            play_count.insert(id.clone(), count);
            first_played.insert(id.clone(), ts.saturating_sub(1000));
            refs.insert(
                id.clone(),
                TrackRef::new(
                    id,
                    format!("Song {n} 🎵"),
                    vec![format!("Artist {n}")],
                    None,
                    None,
                    1000,
                    Availability::Available,
                ),
            );
        }
        let original = PlayLog::from_parts(last_played, play_count, first_played, refs);

        let paths = temp_paths("proptest-play-log");
        save_play_log(&paths, &original).expect("save");
        let outcome = load_play_log(&paths);
        prop_assert_eq!(outcome.warning, None);
        prop_assert_eq!(&outcome.play_log, &original);
        prop_assert_eq!(outcome.play_log.recent(), original.recent());
    }

    /// 013-key-and-tempo-plugin (US4, Constitution VIII, contracts/
    /// getting-started-card.md S1): `[onboarding] getting_started_dismissed`
    /// survives an arbitrary save -> load round trip through the real
    /// `settings.toml` file, for both boolean values.
    #[test]
    fn settings_getting_started_flag_round_trips_any_bool(dismissed in any::<bool>()) {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-persist-settings-onboarding-{}-{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let store = SettingsStore::with_path(dir.join("settings.toml"));

        let settings = AudioSettings {
            getting_started_dismissed: dismissed,
            ..AudioSettings::default()
        };
        prop_assert!(store.save(&settings).is_ok());

        let outcome = store.load();
        prop_assert_eq!(outcome.settings.getting_started_dismissed, dismissed);
        prop_assert!(outcome.warnings.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

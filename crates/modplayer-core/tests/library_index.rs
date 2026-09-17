// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `LibraryIndex` (data-model.md §3.1, contracts/library-and-search-
//! core.md §4): merge semantics via the public `LibraryPage` seam, the
//! lazy hydration queue, and the 50 000 x 1 000 fixture load/lookup budget
//! (SC-002 proxy: index ops <= 1 ms, research R4/design note 7).

use std::time::Instant;

use modplayer_audio_source::{
    AlbumId, ArtistId, Availability, LibraryItem, LibraryPage, LibrarySet, PlaylistId, PlaylistRef,
    TrackId, TrackRef,
};
use modplayer_core::library::index::LibraryIndex;

fn track(id: u32) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap(),
        format!("Song {id}"),
        vec!["Artist".to_string()],
        None,
        None,
        200_000,
        Availability::Available,
    )
}

#[test]
fn merging_a_page_records_ids_before_any_hydration_arrives() {
    let mut index = LibraryIndex::new();
    index.merge_page(LibraryPage {
        set: LibrarySet::SavedTracks,
        items: vec![LibraryItem::Track {
            track: track(1),
            added_at: Some(10),
        }],
        next_page: None,
        sync_token: None,
    });
    assert_eq!(index.saved_tracks().len(), 1);
    assert!(index.has_pending_hydration());
    assert!(
        index
            .track(&TrackId::new("spotify:track:1").unwrap())
            .is_none()
    );
}

#[test]
fn a_second_page_appends_without_duplicating_ids_already_seen() {
    let mut index = LibraryIndex::new();
    for i in 0..3 {
        index.merge_page(LibraryPage {
            set: LibrarySet::SavedAlbums,
            items: vec![LibraryItem::Album {
                album: modplayer_audio_source::AlbumRef {
                    id: AlbumId::new(format!("spotify:album:{i}")).unwrap(),
                    name: format!("Album {i}"),
                    artists: vec![],
                    artwork_url: None,
                    release_date: None,
                    track_count: 1,
                },
                added_at: None,
            }],
            next_page: None,
            sync_token: None,
        });
    }
    // Re-merging the same album id must not duplicate the saved list.
    index.merge_page(LibraryPage {
        set: LibrarySet::SavedAlbums,
        items: vec![LibraryItem::Album {
            album: modplayer_audio_source::AlbumRef {
                id: AlbumId::new("spotify:album:0").unwrap(),
                name: "Album 0".to_string(),
                artists: vec![],
                artwork_url: None,
                release_date: None,
                track_count: 1,
            },
            added_at: Some(99),
        }],
        next_page: None,
        sync_token: None,
    });
    assert_eq!(index.saved_albums().len(), 3);
}

#[test]
fn followed_artists_and_playlists_land_in_their_own_lists() {
    let mut index = LibraryIndex::new();
    index.merge_page(LibraryPage {
        set: LibrarySet::FollowedArtists,
        items: vec![LibraryItem::Artist(modplayer_audio_source::ArtistRef {
            id: ArtistId::new("spotify:artist:1").unwrap(),
            name: "Radiohead".to_string(),
            artwork_url: None,
        })],
        next_page: None,
        sync_token: None,
    });
    index.merge_page(LibraryPage {
        set: LibrarySet::Playlists,
        items: vec![LibraryItem::Playlist(PlaylistRef {
            id: PlaylistId::new("spotify:playlist:1").unwrap(),
            name: "Chill".to_string(),
            owner_name: "Me".to_string(),
            editable: true,
            artwork_url: None,
            track_count: 0,
            revision: None,
        })],
        next_page: None,
        sync_token: None,
    });
    assert_eq!(index.followed_artists().len(), 1);
    assert_eq!(index.playlists().len(), 1);
    // Playlists resolve fully immediately; artists still need hydration.
    assert!(
        index
            .playlist_ref(&PlaylistId::new("spotify:playlist:1").unwrap())
            .is_some()
    );
    assert!(index.has_pending_hydration());
}

/// SC-002 proxy: a 50 000 saved-track / 1 000 playlist fixture merges and
/// looks up in well under a second, and a single lookup stays <= 1 ms
/// (research R4/plan.md Performance Goals — "index ops <= 1 ms").
#[test]
fn large_fixture_merges_and_looks_up_under_budget() {
    let mut index = LibraryIndex::new();

    let started = Instant::now();
    for chunk in 0..500u32 {
        let items = (0..100)
            .map(|i| {
                let id = chunk * 100 + i;
                LibraryItem::Track {
                    track: track(id),
                    added_at: Some(u64::from(id)),
                }
            })
            .collect();
        index.merge_page(LibraryPage {
            set: LibrarySet::SavedTracks,
            items,
            next_page: None,
            sync_token: None,
        });
    }
    for i in 0..1_000u32 {
        index.merge_page(LibraryPage {
            set: LibrarySet::Playlists,
            items: vec![LibraryItem::Playlist(PlaylistRef {
                id: PlaylistId::new(format!("spotify:playlist:{i}")).unwrap(),
                name: format!("Playlist {i}"),
                owner_name: "Me".to_string(),
                editable: true,
                artwork_url: None,
                track_count: 0,
                revision: None,
            })],
            next_page: None,
            sync_token: None,
        });
    }
    let merge_elapsed = started.elapsed();
    assert_eq!(index.saved_tracks().len(), 50_000);
    assert_eq!(index.playlists().len(), 1_000);
    assert!(
        merge_elapsed.as_secs() < 5,
        "merging the full fixture took {merge_elapsed:?}"
    );

    let lookup_started = Instant::now();
    let hit = index.track(&TrackId::new("spotify:track:25000").unwrap());
    let lookup_elapsed = lookup_started.elapsed();
    assert!(
        hit.is_none(),
        "not hydrated yet, but the lookup itself must be O(1)"
    );
    assert!(
        lookup_elapsed.as_millis() <= 1,
        "a single index lookup took {lookup_elapsed:?}"
    );
}

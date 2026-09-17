// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Catalog fixture -> ref mapping (contracts/catalog-source.md §2 rule 5,
//! §5): `catalog::search::parse_searchview_json` drops any hit whose URI is
//! not one of the four music kinds before it ever reaches a `SearchHit`;
//! `catalog::collection`'s `"collection"` URI-prefix split
//! (004-search-and-library-browse, research R3); `catalog::playlists`'
//! rootlist -> `PlaylistRef` URI mapping; and error classification never
//! leaking the raw error text.

use modplayer_audio_source::{
    Availability, CatalogError, LibraryItem, LibrarySet, SearchHit, SearchKind,
};
use modplayer_audio_source_connect::catalog::collection::{
    RawItem, RawPageResponse, album_page, track_page,
};
use modplayer_audio_source_connect::catalog::hydrate::availability_from;
use modplayer_audio_source_connect::catalog::playlists::playlist_uris_from_rootlist;
use modplayer_audio_source_connect::catalog::search::parse_searchview_json;
use modplayer_audio_source_connect::catalog::{classify_http_status, classify_session_error};

/// A `searchview`-shaped fixture whose Tracks group mixes in a podcast
/// episode and a show URI alongside two real tracks — neither non-music
/// hit may ever surface as a `SearchHit` (contract §2 rule 5).
const FIXTURE: &str = r#"{
  "results": {
    "tracks": {
      "hits": [
        { "uri": "spotify:track:track1", "name": "Real Track", "artists": [{"name": "Real Artist"}], "duration_ms": 200000 },
        { "uri": "spotify:episode:not-music", "name": "A Podcast Episode" },
        { "uri": "spotify:show:not-music", "name": "A Podcast Show" },
        { "uri": "spotify:track:track2", "name": "Another Track", "artists": [], "duration_ms": 150000 }
      ]
    },
    "albums": { "hits": [ { "uri": "spotify:album:album1", "name": "Real Album", "artists": [{"name": "Real Artist"}] } ] },
    "artists": { "hits": [] },
    "playlists": { "hits": [ { "uri": "spotify:playlist:pl1", "name": "Real Playlist", "owner": {"name": "Alex"} } ] }
  }
}"#;

#[test]
fn non_music_hits_are_dropped() {
    let page = parse_searchview_json(
        FIXTURE.as_bytes(),
        &[
            SearchKind::Track,
            SearchKind::Album,
            SearchKind::Artist,
            SearchKind::Playlist,
        ],
        0,
        20,
    )
    .expect("valid fixture JSON parses");

    let tracks_group = page
        .groups
        .iter()
        .find(|g| g.kind == SearchKind::Track)
        .expect("a Tracks group is present");
    assert_eq!(
        tracks_group.items.len(),
        2,
        "the podcast episode and show must be dropped, only the two real tracks remain"
    );
    for hit in &tracks_group.items {
        let SearchHit::Track(track) = hit else {
            panic!("expected only Track hits in the Tracks group, got {hit:?}");
        };
        assert!(track.id.as_str().starts_with("spotify:track:"));
    }
}

#[test]
fn every_group_maps_to_its_own_kind() {
    let page = parse_searchview_json(
        FIXTURE.as_bytes(),
        &[
            SearchKind::Track,
            SearchKind::Album,
            SearchKind::Artist,
            SearchKind::Playlist,
        ],
        0,
        20,
    )
    .expect("valid fixture JSON parses");

    let album_group = page
        .groups
        .iter()
        .find(|g| g.kind == SearchKind::Album)
        .expect("an Albums group is present");
    assert_eq!(album_group.items.len(), 1);
    assert!(matches!(album_group.items[0], SearchHit::Album(_)));

    let artist_group = page
        .groups
        .iter()
        .find(|g| g.kind == SearchKind::Artist)
        .expect("an Artists group is present (even if empty)");
    assert!(artist_group.items.is_empty());

    let playlist_group = page
        .groups
        .iter()
        .find(|g| g.kind == SearchKind::Playlist)
        .expect("a Playlists group is present");
    assert_eq!(playlist_group.items.len(), 1);
    assert!(matches!(playlist_group.items[0], SearchHit::Playlist(_)));

    assert!(page.unsupported.is_empty());
}

#[test]
fn a_group_missing_from_the_reply_is_reported_unsupported() {
    let fixture = r#"{ "results": { "tracks": { "hits": [] } } }"#;
    let page = parse_searchview_json(
        fixture.as_bytes(),
        &[
            SearchKind::Track,
            SearchKind::Album,
            SearchKind::Artist,
            SearchKind::Playlist,
        ],
        0,
        20,
    )
    .expect("valid fixture JSON parses");

    assert_eq!(page.groups.len(), 1);
    assert_eq!(page.groups[0].kind, SearchKind::Track);
    assert_eq!(
        page.unsupported,
        vec![SearchKind::Album, SearchKind::Artist, SearchKind::Playlist]
    );
}

#[test]
fn malformed_json_classifies_as_an_unavailable_catalog_error() {
    let err = parse_searchview_json(b"not json", &[SearchKind::Track], 0, 20).unwrap_err();
    assert!(matches!(err, CatalogError::Unavailable(_)));
}

#[test]
fn a_full_page_reports_a_next_offset() {
    let mut hits = String::from(r#"{ "results": { "tracks": { "hits": ["#);
    for i in 0..20 {
        if i > 0 {
            hits.push(',');
        }
        hits.push_str(&format!(r#"{{"uri":"spotify:track:t{i}","name":"T{i}"}}"#));
    }
    hits.push_str("] } } }");

    let page = parse_searchview_json(hits.as_bytes(), &[SearchKind::Track], 0, 20)
        .expect("valid fixture JSON parses");
    assert_eq!(page.groups[0].items.len(), 20);
    assert_eq!(page.groups[0].next_offset, Some(20));
}

// -- collection2v2 URI-prefix split (research R3) ---------------------------

fn mixed_raw_page() -> RawPageResponse {
    RawPageResponse {
        items: vec![
            RawItem {
                uri: "spotify:track:t1".to_string(),
                added_at: 100,
                removed: false,
            },
            RawItem {
                uri: "spotify:album:a1".to_string(),
                added_at: 200,
                removed: false,
            },
            RawItem {
                uri: "spotify:track:removed".to_string(),
                added_at: 300,
                removed: true,
            },
        ],
        next_page_token: Some(b"cursor".to_vec()),
    }
}

#[test]
fn collection_split_keeps_only_tracks_for_the_saved_tracks_set() {
    let page = track_page(mixed_raw_page());
    assert_eq!(page.set, LibrarySet::SavedTracks);
    assert_eq!(page.items.len(), 1);
    assert!(matches!(
        &page.items[0],
        LibraryItem::Track { track, added_at }
            if track.id.as_str() == "spotify:track:t1" && *added_at == Some(100)
    ));
}

#[test]
fn collection_split_keeps_only_albums_for_the_saved_albums_set() {
    let page = album_page(mixed_raw_page());
    assert_eq!(page.set, LibrarySet::SavedAlbums);
    assert_eq!(page.items.len(), 1);
    assert!(matches!(
        &page.items[0],
        LibraryItem::Album { album, added_at }
            if album.id.as_str() == "spotify:album:a1" && *added_at == Some(200)
    ));
}

#[test]
fn a_removed_item_never_surfaces_in_either_split() {
    let raw = RawPageResponse {
        items: vec![RawItem {
            uri: "spotify:track:gone".to_string(),
            added_at: 1,
            removed: true,
        }],
        next_page_token: None,
    };
    assert!(track_page(raw).items.is_empty());
}

// -- rootlist -> PlaylistRef URI mapping (verified pattern, 003) ------------

fn rootlist_fixture(uris: &[&str]) -> Vec<u8> {
    use librespot_protocol::playlist4_external::{Item, ListItems, SelectedListContent};
    use protobuf::{Message, MessageField};

    let items: Vec<Item> = uris
        .iter()
        .map(|uri| Item {
            uri: Some((*uri).to_string()),
            ..Item::default()
        })
        .collect();
    let contents = ListItems {
        pos: Some(0),
        truncated: Some(false),
        items,
        ..ListItems::default()
    };
    let msg = SelectedListContent {
        contents: MessageField::some(contents),
        ..SelectedListContent::default()
    };
    msg.write_to_bytes().expect("encode fixture")
}

#[test]
fn rootlist_mapping_keeps_only_playlist_prefixed_uris() {
    let bytes = rootlist_fixture(&[
        "spotify:playlist:a",
        "spotify:folder:b",
        "spotify:playlist:c",
        "spotify:user:someone:playlist:d",
    ]);
    let (uris, total) = playlist_uris_from_rootlist(&bytes).expect("parse fixture");
    assert_eq!(total, 4, "the raw item count includes non-playlist entries");
    assert_eq!(
        uris,
        vec![
            "spotify:playlist:a".to_string(),
            "spotify:playlist:c".to_string()
        ]
    );
}

#[test]
fn a_malformed_rootlist_reply_classifies_without_leaking_raw_bytes() {
    let err = playlist_uris_from_rootlist(b"\xff\xff not a valid protobuf message at all")
        .expect_err("malformed bytes must not parse");
    let CatalogError::Unavailable(message) = err else {
        panic!("expected Unavailable, got {err:?}");
    };
    assert_eq!(message, "malformed rootlist");
    assert!(!message.contains('\u{ff}'));
}

// -- Error classification never leaks raw text (contract §2 rule 4) --------

#[test]
fn every_classified_error_display_is_a_fixed_redacted_string() {
    use librespot_core::error::ErrorKind;
    for kind in [
        ErrorKind::Unavailable,
        ErrorKind::DeadlineExceeded,
        ErrorKind::ResourceExhausted,
        ErrorKind::NotFound,
        ErrorKind::PermissionDenied,
        ErrorKind::Internal,
    ] {
        let classified = classify_session_error(kind);
        let text = classified.to_string();
        assert!(!text.contains("http"), "must never carry a raw URL: {text}");
    }
    for status in [429u16, 404, 403, 500] {
        let classified = classify_http_status(status, None);
        let text = classified.to_string();
        assert!(
            matches!(
                classified,
                CatalogError::RateLimited { .. }
                    | CatalogError::NotFound
                    | CatalogError::Unsupported
                    | CatalogError::Offline
            ),
            "unexpected classification for {status}: {text}"
        );
    }
}

// -- Region availability mapping (gate V4, US3 T068) ------------------------
//
// `availability_from` is the pure core of `availability_for` (which only
// adds `session.user_data()`'s country and `catalogue` attribute), so
// these fixtures need no `Session` at all. `Removed` (the other half of
// gate V4's table) is the host's own mapping of `Hydrated.missing`
// (contract rule 7) and is pinned end to end in
// `modplayer-core::library::index`'s
// `a_missing_track_uri_resolves_to_removed_rather_than_re_queueing_forever`.

fn restriction(
    catalogue_strs: &[&str],
    countries_allowed: Option<Vec<String>>,
    countries_forbidden: Option<Vec<String>>,
) -> librespot_metadata::restriction::Restrictions {
    use librespot_metadata::restriction::{Restriction, RestrictionCatalogues, Restrictions};
    Restrictions(vec![Restriction {
        catalogues: RestrictionCatalogues(vec![]),
        restriction_type: Default::default(),
        catalogue_strs: catalogue_strs.iter().map(|s| s.to_string()).collect(),
        countries_allowed,
        countries_forbidden,
    }])
}

#[test]
fn availability_marks_unavailable_region_when_the_country_is_forbidden() {
    let restrictions = restriction(&["premium"], None, Some(vec!["BR".to_string()]));
    assert_eq!(
        availability_from("BR", "premium", &restrictions),
        Availability::UnavailableRegion,
        "a premium restriction forbidding the session's own country must region-lock the track"
    );
}

#[test]
fn availability_marks_unavailable_region_when_the_country_is_not_in_the_allowed_list() {
    let restrictions = restriction(
        &["premium"],
        Some(vec!["US".to_string(), "GB".to_string()]),
        None,
    );
    assert_eq!(
        availability_from("BR", "premium", &restrictions),
        Availability::UnavailableRegion,
        "the session's country is absent from the allow-list, so the track must be region-locked"
    );
}

#[test]
fn availability_is_available_when_no_restriction_matches_the_session_country() {
    // No restrictions at all.
    let none = librespot_metadata::restriction::Restrictions(vec![]);
    assert_eq!(
        availability_from("BR", "premium", &none),
        Availability::Available
    );

    // An allow-list that does include the session's country.
    let allowed = restriction(&["premium"], Some(vec!["BR".to_string()]), None);
    assert_eq!(
        availability_from("BR", "premium", &allowed),
        Availability::Available
    );
}

/// The relinked-track shape seen live on 2026-09-17 (research R14 V4):
/// one restriction naming no catalogue with an *empty* allow-list. It
/// applies to nobody — the track plays through `Track.alternatives` — so
/// it must not grey the row.
#[test]
fn availability_ignores_a_restriction_that_names_no_catalogue() {
    let relinked = restriction(&[], Some(vec![]), None);
    assert_eq!(
        availability_from("BR", "premium", &relinked),
        Availability::Available,
        "a catalogue-less restriction (relinked track) must not region-lock"
    );
}

/// A restriction aimed at another tier (free/"ad" catalogue) never
/// applies to this session's catalogue, whatever its country lists say.
#[test]
fn availability_ignores_a_restriction_for_another_catalogue() {
    let free_only = restriction(&["free"], None, Some(vec!["BR".to_string()]));
    assert_eq!(
        availability_from("BR", "premium", &free_only),
        Availability::Available
    );
    // …but the same restriction does apply when the session *is* on that tier.
    assert_eq!(
        availability_from("BR", "free", &free_only),
        Availability::UnavailableRegion
    );
}

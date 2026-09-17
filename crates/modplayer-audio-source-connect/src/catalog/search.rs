// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SourceCommand::SearchCatalog` fulfilment (contracts/catalog-source.md
//! §1-3, research R2): an ordered strategy list — mercury `searchview`
//! (all four groups in one round trip) first, a tracks-only
//! `context-resolve` fallback second, hydrated through `catalog::hydrate`.
//! Strategy (1)'s viability is gated by `tests/live.rs::search_probe`
//! (`#[ignore = "manual"]`, research R14 V1); either strategy answers only
//! music kinds — `parse_searchview_json` drops any hit whose URI is not
//! `track|album|artist|playlist` before it ever reaches a `SearchHit`
//! (contract §2 rule 5), and the context-resolve fallback only ever
//! produces `SearchKind::Track` hits.
//!
//! The `searchview` JSON response shape is undocumented for 2026 (research
//! R2) — the field names below are this slice's best-effort mapping,
//! written so a differently-shaped real reply degrades to an empty/
//! `Unsupported` group rather than panicking; `live.rs::search_probe`
//! (research R14) is what confirms or corrects it.

use librespot_core::Session;
use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, Availability, CatalogError, PlaylistId, PlaylistRef,
    ReleaseDate, SearchGroupPage, SearchHit, SearchKind, SearchPage, TrackId, TrackRef,
};
use serde_json::Value;

/// Fulfil one `SearchCatalog` command (contracts/catalog-source.md §1):
/// mercury `searchview` first: on any failure (network, refused, malformed
/// reply) the strategy list falls back to `context-resolve` — tracks only,
/// with the other requested kinds reported `unsupported` rather than
/// surfacing the first strategy's own failure.
pub async fn search(
    session: &Session,
    query: &str,
    kinds: &[SearchKind],
    offset: u32,
    limit: u8,
) -> Result<SearchPage, CatalogError> {
    match searchview(session, query, kinds, offset, limit).await {
        Ok(page) => Ok(page),
        Err(_) => context_resolve_tracks(session, query, kinds, offset, limit).await,
    }
}

// -- Strategy 1: mercury searchview --------------------------------------

async fn searchview(
    session: &Session,
    query: &str,
    kinds: &[SearchKind],
    offset: u32,
    limit: u8,
) -> Result<SearchPage, CatalogError> {
    let url = searchview_url(session, query, kinds, offset, limit);
    let future = session
        .mercury()
        .get(url)
        .map_err(|e| super::classify_session_error(e.kind))?;
    let response = future
        .await
        .map_err(|e| super::classify_session_error(e.kind))?;
    if !(200..300).contains(&response.status_code) {
        return Err(super::classify_http_status(
            response.status_code.max(0) as u16,
            None,
        ));
    }
    let body: Vec<u8> = response.payload.into_iter().flatten().collect();
    parse_searchview_json(&body, kinds, offset, limit)
}

fn searchview_url(
    session: &Session,
    query: &str,
    kinds: &[SearchKind],
    offset: u32,
    limit: u8,
) -> String {
    // Mercury URIs are `+`-delimited, not percent-encoded (matches
    // `spclient.get_context`'s own `spotify:search:<search+query>` scheme).
    let encoded_query = query.replace(' ', "+");
    let catalogue = if kinds.len() == 4 {
        String::new()
    } else {
        kinds
            .iter()
            .map(|k| kind_catalogue(*k))
            .collect::<Vec<_>>()
            .join(",")
    };
    let country = session.country();
    let username = session.username();
    format!(
        "hm://searchview/km/v4/search/{encoded_query}?entityVersion=2&limit={limit}&offset={offset}&catalogue={catalogue}&country={country}&locale=en&username={username}&imageSize=large"
    )
}

fn kind_catalogue(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Track => "track",
        SearchKind::Album => "album",
        SearchKind::Artist => "artist",
        SearchKind::Playlist => "playlist",
    }
}

fn group_json_key(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Track => "tracks",
        SearchKind::Album => "albums",
        SearchKind::Artist => "artists",
        SearchKind::Playlist => "playlists",
    }
}

/// Parse a `searchview` JSON body into a `SearchPage` (pure, no I/O —
/// exercised directly by `tests/catalog_mapping.rs`). A group absent from
/// the reply (no `results.<group>.hits` array) is reported `unsupported`
/// rather than `Empty`, since that means the strategy could not answer it
/// at all, not that it found nothing (contracts/catalog-source.md §2).
pub fn parse_searchview_json(
    body: &[u8],
    requested_kinds: &[SearchKind],
    offset: u32,
    limit: u8,
) -> Result<SearchPage, CatalogError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| CatalogError::Unavailable("malformed search response".to_string()))?;
    let results = value.get("results");

    let mut groups = Vec::new();
    let mut unsupported = Vec::new();
    for kind in requested_kinds.iter().copied() {
        let hits = results
            .and_then(|r| r.get(group_json_key(kind)))
            .and_then(|g| g.get("hits"))
            .and_then(Value::as_array);
        match hits {
            Some(hits) => groups.push(build_group_page(kind, hits, offset, limit)),
            None => unsupported.push(kind),
        }
    }
    Ok(SearchPage {
        groups,
        unsupported,
    })
}

fn build_group_page(kind: SearchKind, hits: &[Value], offset: u32, limit: u8) -> SearchGroupPage {
    let items: Vec<SearchHit> = hits
        .iter()
        .filter_map(|hit| hit_from_json(kind, hit))
        .collect();
    // A full page received is the only offline-checkable signal that a
    // further page might exist (the reply's own `total`, if present, is
    // unverified for 2026 — research R2/R14).
    let next_offset = if items.len() >= limit as usize {
        Some(offset + items.len() as u32)
    } else {
        None
    };
    SearchGroupPage {
        kind,
        items,
        next_offset,
    }
}

fn hit_from_json(kind: SearchKind, hit: &Value) -> Option<SearchHit> {
    match kind {
        SearchKind::Track => track_hit(hit),
        SearchKind::Album => album_hit(hit),
        SearchKind::Artist => artist_hit(hit),
        SearchKind::Playlist => playlist_hit(hit),
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

/// contracts/catalog-source.md §2 rule 5: any hit whose URI is not
/// `track|album|artist|playlist` never reaches a `SearchHit` — a defensive
/// check even within an already-grouped reply, and what lets
/// `tests/catalog_mapping.rs::non_music_hits_are_dropped` exercise this
/// function directly with a stray podcast/episode URI mixed into a group.
fn matches_kind(uri: &str, kind: SearchKind) -> bool {
    let prefix = match kind {
        SearchKind::Track => "spotify:track:",
        SearchKind::Album => "spotify:album:",
        SearchKind::Artist => "spotify:artist:",
        SearchKind::Playlist => "spotify:playlist:",
    };
    uri.starts_with(prefix)
}

fn track_hit(hit: &Value) -> Option<SearchHit> {
    let uri = str_field(hit, "uri")?;
    if !matches_kind(uri, SearchKind::Track) {
        return None;
    }
    let id = TrackId::new(uri).ok()?;
    let name = str_field(hit, "name").unwrap_or_default();
    let artists = hit
        .get("artists")
        .and_then(Value::as_array)
        .map(|artists| {
            artists
                .iter()
                .filter_map(|a| str_field(a, "name"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let album = hit
        .get("album")
        .and_then(|a| str_field(a, "name"))
        .map(str::to_string);
    let artwork_url = str_field(hit, "image_url").map(str::to_string);
    let duration_ms = hit.get("duration_ms").and_then(Value::as_u64).unwrap_or(0) as u32;
    let explicit = hit
        .get("explicit")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut track = TrackRef::new(
        id,
        name,
        artists,
        album,
        artwork_url,
        duration_ms,
        Availability::Available,
    );
    if explicit {
        track = track.with_extras(modplayer_audio_source::TrackRefExtras {
            explicit: true,
            ..Default::default()
        });
    }
    Some(SearchHit::Track(track))
}

fn album_hit(hit: &Value) -> Option<SearchHit> {
    let uri = str_field(hit, "uri")?;
    if !matches_kind(uri, SearchKind::Album) {
        return None;
    }
    let id = AlbumId::new(uri).ok()?;
    let name = str_field(hit, "name").unwrap_or_default().to_string();
    let artists = hit
        .get("artists")
        .and_then(Value::as_array)
        .map(|artists| {
            artists
                .iter()
                .filter_map(|a| str_field(a, "name"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let artwork_url = str_field(hit, "image_url").map(str::to_string);
    let release_date = hit
        .get("year")
        .and_then(Value::as_u64)
        .map(|year| ReleaseDate {
            year: year as u16,
            month: None,
            day: None,
        });
    let track_count = hit.get("total_tracks").and_then(Value::as_u64).unwrap_or(0) as u32;
    Some(SearchHit::Album(AlbumRef {
        id,
        name,
        artists,
        artwork_url,
        release_date,
        track_count,
    }))
}

fn artist_hit(hit: &Value) -> Option<SearchHit> {
    let uri = str_field(hit, "uri")?;
    if !matches_kind(uri, SearchKind::Artist) {
        return None;
    }
    let id = ArtistId::new(uri).ok()?;
    let name = str_field(hit, "name").unwrap_or_default().to_string();
    let artwork_url = str_field(hit, "image_url").map(str::to_string);
    Some(SearchHit::Artist(ArtistRef {
        id,
        name,
        artwork_url,
    }))
}

fn playlist_hit(hit: &Value) -> Option<SearchHit> {
    let uri = str_field(hit, "uri")?;
    if !matches_kind(uri, SearchKind::Playlist) {
        return None;
    }
    let id = PlaylistId::new(uri).ok()?;
    let name = str_field(hit, "name").unwrap_or_default().to_string();
    let owner_name = hit
        .get("owner")
        .and_then(|o| str_field(o, "name"))
        .unwrap_or_default()
        .to_string();
    let artwork_url = str_field(hit, "image_url").map(str::to_string);
    let track_count = hit.get("total_tracks").and_then(Value::as_u64).unwrap_or(0) as u32;
    Some(SearchHit::Playlist(PlaylistRef {
        id,
        name,
        owner_name,
        // A search hit never carries the session's own user id inline
        // (contracts/catalog-source.md leaves ownership to `FetchLibrary`);
        // never editable from a search result.
        editable: false,
        artwork_url,
        track_count,
        revision: None,
    }))
}

// -- Strategy 2: context-resolve, tracks only ----------------------------

async fn context_resolve_tracks(
    session: &Session,
    query: &str,
    kinds: &[SearchKind],
    offset: u32,
    limit: u8,
) -> Result<SearchPage, CatalogError> {
    let encoded_query = query.replace(' ', "+");
    let uri = format!("spotify:search:{encoded_query}");
    let ctx = session
        .spclient()
        .get_context(&uri)
        .await
        .map_err(|e| super::classify_session_error(e.kind))?;

    let track_uris: Vec<String> = ctx
        .pages
        .iter()
        .flat_map(|page| page.tracks.iter())
        .filter_map(|track| track.uri.clone())
        .filter(|uri| matches_kind(uri, SearchKind::Track))
        .skip(offset as usize)
        .take(limit as usize)
        .collect();
    let track_ids: Vec<TrackId> = track_uris
        .into_iter()
        .filter_map(|uri| TrackId::new(uri).ok())
        .collect();
    let requested = track_ids.len();
    let (tracks, _albums, _artists, _missing) =
        super::hydrate::hydrate(session, &track_ids, &[], &[]).await;

    let mut groups = Vec::new();
    if kinds.contains(&SearchKind::Track) {
        let next_offset = if requested >= limit as usize {
            Some(offset + requested as u32)
        } else {
            None
        };
        groups.push(SearchGroupPage {
            kind: SearchKind::Track,
            items: tracks.into_iter().map(SearchHit::Track).collect(),
            next_offset,
        });
    }
    let unsupported = kinds
        .iter()
        .copied()
        .filter(|kind| *kind != SearchKind::Track)
        .collect();
    Ok(SearchPage {
        groups,
        unsupported,
    })
}

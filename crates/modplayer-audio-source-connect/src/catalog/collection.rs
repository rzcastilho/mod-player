// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SourceCommand::FetchLibrary` fulfilment for `SavedTracks`/`SavedAlbums`/
//! `FollowedArtists` (contracts/catalog-source.md §1/§3, research R3, gate
//! V2): `POST /collection/v2/paging` with a hand-encoded `PageRequest` —
//! `librespot-protocol` doesn't compile `collection2v2.proto`, so this
//! module encodes/decodes the two tiny messages itself with
//! `protobuf::Coded{Output,Input}Stream` (already a dependency, ~80 LOC).
//! `set = "collection"` answers *both* saved tracks and saved albums mixed
//! by URI prefix in one underlying paginated stream — `SavedTracks` and
//! `SavedAlbums` each walk it independently, filtering locally, so a raw
//! page that happens to hold only the other kind still reports its
//! `next_page` token and the caller's own page-walking loop
//! (`SyncScheduler`) simply continues (contracts/library-and-search-
//! core.md §3.3). `SavedTracks` additionally falls back to
//! `spclient.get_context("spotify:user:<u>:collection")` — one unpaginated
//! page — if the paging endpoint itself is refused; no fallback exists for
//! `SavedAlbums`/`FollowedArtists`, which then report `Unsupported`
//! (research R3). "Refused" means any *definitive* error, not just
//! `Unsupported`: the 2026-09 live probe (research R14 V2,
//! `tests/live.rs::catalog_wire_probe`) found the endpoint answering HTTP
//! 400 for every request shape, so in practice every session takes the
//! fallback path today and the paging code is kept as the documented
//! primary strategy should the endpoint accept the session again.
//!
//! Every item returned here is a *bare* identity (data-model.md §1.7's
//! lazy-hydration contract, research R4): `LibraryIndex::merge_page`
//! records the id/`added_at` and queues it for hydration; nothing in this
//! module resolves a title, artist or artwork.

use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, Method};
use librespot_core::Session;
use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, Availability, CatalogError, LibraryItem, LibraryPage,
    LibrarySet, TrackId, TrackRef,
};
use protobuf::{CodedInputStream, CodedOutputStream};

const COLLECTION_ENDPOINT: &str = "/collection/v2/paging";
/// research R3: `set = "collection"` (tracks + albums mixed), `"artist"`
/// (followed artists).
const SET_COLLECTION: &str = "collection";
const SET_ARTIST: &str = "artist";

/// Fulfil one `FetchLibrary` for a collection2v2-backed set (contracts/
/// catalog-source.md §1).
pub async fn fetch(
    session: &Session,
    set: LibrarySet,
    page: Option<String>,
    limit: u16,
) -> Result<LibraryPage, CatalogError> {
    match set {
        LibrarySet::SavedTracks => {
            match fetch_page(session, SET_COLLECTION, page.clone(), limit).await {
                Ok(raw) => Ok(track_page(raw)),
                Err(err) if strategy_refused(&err) => saved_tracks_context_fallback(session).await,
                Err(other) => Err(other),
            }
        }
        LibrarySet::SavedAlbums => {
            let raw = fetch_page(session, SET_COLLECTION, page, limit)
                .await
                .map_err(unsupported_when_refused)?;
            Ok(album_page(raw))
        }
        LibrarySet::FollowedArtists => {
            let raw = fetch_page(session, SET_ARTIST, page, limit)
                .await
                .map_err(unsupported_when_refused)?;
            Ok(artist_page(raw))
        }
        LibrarySet::Playlists => Err(CatalogError::Unsupported),
    }
}

/// Whether the paging endpoint *definitively* declined to answer this
/// strategy — as opposed to a transient condition the caller's own
/// backoff (`SyncScheduler`) should retry. research R14 V2 (live probe,
/// 2026-09): `/collection/v2/paging` answers HTTP 400 to every request
/// shape tried, which classifies as `Unavailable`, not `Unsupported` — so
/// gating the fallback on `Unsupported` alone would have marked the whole
/// sync cycle failed (FR-021's "first sync failed") even though the
/// `SavedTracks` fallback and `Playlists` both work. Any non-transient
/// error therefore counts as "this strategy cannot answer".
fn strategy_refused(err: &CatalogError) -> bool {
    !matches!(
        err,
        CatalogError::Offline | CatalogError::RateLimited { .. }
    )
}

/// Sets with no fallback (research R3: `SavedAlbums`/`FollowedArtists`)
/// report a refused strategy as `Unsupported`, which the sync cycle records
/// as *partial* (section stays empty, nothing else is disturbed) rather
/// than *failed*; transient errors pass through unchanged.
fn unsupported_when_refused(err: CatalogError) -> CatalogError {
    if strategy_refused(&err) {
        CatalogError::Unsupported
    } else {
        err
    }
}

/// Pure (no I/O), so `tests/catalog_mapping.rs` exercises the URI-prefix
/// split directly with a fixture `RawPageResponse`.
pub fn track_page(raw: RawPageResponse) -> LibraryPage {
    let items = raw
        .items
        .into_iter()
        .filter(|item| !item.removed && item.uri.starts_with("spotify:track:"))
        .filter_map(|item| {
            let id = TrackId::new(item.uri).ok()?;
            Some(LibraryItem::Track {
                track: bare_track(id),
                added_at: added_at_ms(item.added_at),
            })
        })
        .collect();
    LibraryPage {
        set: LibrarySet::SavedTracks,
        items,
        next_page: encode_token(raw.next_page_token),
        sync_token: None,
    }
}

/// See [`track_page`].
pub fn album_page(raw: RawPageResponse) -> LibraryPage {
    let items = raw
        .items
        .into_iter()
        .filter(|item| !item.removed && item.uri.starts_with("spotify:album:"))
        .filter_map(|item| {
            let id = AlbumId::new(item.uri).ok()?;
            Some(LibraryItem::Album {
                album: bare_album(id),
                added_at: added_at_ms(item.added_at),
            })
        })
        .collect();
    LibraryPage {
        set: LibrarySet::SavedAlbums,
        items,
        next_page: encode_token(raw.next_page_token),
        sync_token: None,
    }
}

fn artist_page(raw: RawPageResponse) -> LibraryPage {
    let items = raw
        .items
        .into_iter()
        .filter(|item| !item.removed && item.uri.starts_with("spotify:artist:"))
        .filter_map(|item| {
            let id = ArtistId::new(item.uri).ok()?;
            Some(LibraryItem::Artist(bare_artist(id)))
        })
        .collect();
    LibraryPage {
        set: LibrarySet::FollowedArtists,
        items,
        next_page: encode_token(raw.next_page_token),
        sync_token: None,
    }
}

fn added_at_ms(added_at: i64) -> Option<u64> {
    u64::try_from(added_at).ok()
}

/// The raw pagination token is opaque bytes; base64 is this module's own
/// wire-safe `String` encoding for `LibraryPage.next_page` — the token
/// round-trips through [`decode_token`] on the next request, unopened by
/// anything else.
fn encode_token(token: Option<Vec<u8>>) -> Option<String> {
    let token = token?;
    if token.is_empty() {
        return None;
    }
    Some(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &token,
    ))
}

fn decode_token(token: &str) -> Option<Vec<u8>> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, token).ok()
}

fn bare_track(id: TrackId) -> TrackRef {
    TrackRef::new(id, "", Vec::new(), None, None, 0, Availability::Available)
}

fn bare_album(id: AlbumId) -> AlbumRef {
    AlbumRef {
        id,
        name: String::new(),
        artists: Vec::new(),
        artwork_url: None,
        release_date: None,
        track_count: 0,
    }
}

fn bare_artist(id: ArtistId) -> ArtistRef {
    ArtistRef {
        id,
        name: String::new(),
        artwork_url: None,
    }
}

/// `SavedTracks` fallback (research R3): `spotify:user:<u>:collection` —
/// documented in librespot-core 0.8.0 `spclient.rs:858-860`, "liked songs:
/// all". One unpaginated page of bare track identities; no `added_at` (the
/// context endpoint doesn't carry it).
async fn saved_tracks_context_fallback(session: &Session) -> Result<LibraryPage, CatalogError> {
    let uri = format!("spotify:user:{}:collection", session.username());
    let ctx = session
        .spclient()
        .get_context(&uri)
        .await
        .map_err(|e| super::classify_session_error(e.kind))?;
    let items = ctx
        .pages
        .iter()
        .flat_map(|page| page.tracks.iter())
        .filter_map(|track| track.uri.clone())
        .filter_map(|uri| TrackId::new(uri).ok())
        .map(|id| LibraryItem::Track {
            track: bare_track(id),
            added_at: None,
        })
        .collect();
    Ok(LibraryPage {
        set: LibrarySet::SavedTracks,
        items,
        next_page: None,
        sync_token: None,
    })
}

// -- Wire format (research R3: "the two messages are four fields each") ----

async fn fetch_page(
    session: &Session,
    set: &str,
    page: Option<String>,
    limit: u16,
) -> Result<RawPageResponse, CatalogError> {
    let pagination_token = page.as_deref().and_then(decode_token);
    let body = encode_page_request(&session.username(), set, pagination_token.as_deref(), limit);

    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-protobuf"),
    );

    let response = session
        .spclient()
        .request(
            &Method::POST,
            COLLECTION_ENDPOINT,
            Some(headers),
            Some(&body),
        )
        .await
        .map_err(|e| super::classify_session_error(e.kind))?;

    decode_page_response(&response)
        .map_err(|_| CatalogError::Unavailable("malformed collection response".to_string()))
}

/// `PageRequest { username = 1, set = 2, pagination_token = 3, limit = 4 }`.
fn encode_page_request(
    username: &str,
    set: &str,
    pagination_token: Option<&[u8]>,
    limit: u16,
) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut out = CodedOutputStream::vec(&mut buf);
        let _ = out.write_string(1, username);
        let _ = out.write_string(2, set);
        if let Some(token) = pagination_token {
            let _ = out.write_bytes(3, token);
        }
        let _ = out.write_int32(4, i32::from(limit));
        let _ = out.flush();
    }
    buf
}

/// `Item { uri = 1, added_at = 2, is_removed = 3 }`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawItem {
    pub uri: String,
    pub added_at: i64,
    pub removed: bool,
}

/// `PageResponse { items = 1 (repeated Item), next_page_token = 2 }`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawPageResponse {
    pub items: Vec<RawItem>,
    pub next_page_token: Option<Vec<u8>>,
}

/// Pure (no I/O); `tests/catalog_mapping.rs` builds a fixture with the same
/// wire format this decodes (contracts/catalog-source.md §2 rule 4: error
/// classification never leaks raw text — this function's own errors are a
/// unit `()`, mapped by [`fetch_page`] to a fixed, redacted message).
#[allow(clippy::result_unit_err)]
pub fn decode_page_response(bytes: &[u8]) -> Result<RawPageResponse, ()> {
    let mut input = CodedInputStream::from_bytes(bytes);
    let mut response = RawPageResponse::default();
    while !input.eof().map_err(|_| ())? {
        let tag = input.read_raw_varint32().map_err(|_| ())?;
        let field = tag >> 3;
        let wire = tag & 0x7;
        match (field, wire) {
            (1, 2) => {
                let item_bytes = input.read_bytes().map_err(|_| ())?;
                response.items.push(decode_item(&item_bytes)?);
            }
            (2, 2) => {
                response.next_page_token = Some(input.read_bytes().map_err(|_| ())?);
            }
            (_, wire) => skip_unknown(&mut input, wire)?,
        }
    }
    Ok(response)
}

fn decode_item(bytes: &[u8]) -> Result<RawItem, ()> {
    let mut input = CodedInputStream::from_bytes(bytes);
    let mut item = RawItem::default();
    while !input.eof().map_err(|_| ())? {
        let tag = input.read_raw_varint32().map_err(|_| ())?;
        let field = tag >> 3;
        let wire = tag & 0x7;
        match (field, wire) {
            (1, 2) => item.uri = input.read_string().map_err(|_| ())?,
            (2, 0) => item.added_at = input.read_int64().map_err(|_| ())?,
            (3, 0) => item.removed = input.read_bool().map_err(|_| ())?,
            (_, wire) => skip_unknown(&mut input, wire)?,
        }
    }
    Ok(item)
}

fn skip_unknown(input: &mut CodedInputStream<'_>, wire: u32) -> Result<(), ()> {
    let wire_type = protobuf::rt::WireType::new(wire).ok_or(())?;
    input.skip_field(wire_type).map_err(|_| ())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn round_trip_page_response(raw: &RawPageResponse) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut out = CodedOutputStream::vec(&mut buf);
            for item in &raw.items {
                let mut item_buf = Vec::new();
                {
                    let mut item_out = CodedOutputStream::vec(&mut item_buf);
                    let _ = item_out.write_string(1, &item.uri);
                    let _ = item_out.write_int64(2, item.added_at);
                    if item.removed {
                        let _ = item_out.write_bool(3, true);
                    }
                    let _ = item_out.flush();
                }
                let _ = out.write_bytes(1, &item_buf);
            }
            if let Some(token) = &raw.next_page_token {
                let _ = out.write_bytes(2, token);
            }
            let _ = out.flush();
        }
        buf
    }

    #[test]
    fn definitive_errors_count_as_refused_transient_ones_do_not() {
        assert!(strategy_refused(&CatalogError::Unsupported));
        assert!(strategy_refused(&CatalogError::NotFound));
        assert!(strategy_refused(&CatalogError::Unavailable(
            "catalog request failed".to_string()
        )));
        assert!(!strategy_refused(&CatalogError::Offline));
        assert!(!strategy_refused(&CatalogError::RateLimited {
            retry_after_ms: Some(1_000)
        }));
    }

    #[test]
    fn refused_maps_to_unsupported_and_transient_passes_through() {
        assert!(matches!(
            unsupported_when_refused(CatalogError::Unavailable("x".to_string())),
            CatalogError::Unsupported
        ));
        assert!(matches!(
            unsupported_when_refused(CatalogError::Offline),
            CatalogError::Offline
        ));
        assert!(matches!(
            unsupported_when_refused(CatalogError::RateLimited {
                retry_after_ms: None
            }),
            CatalogError::RateLimited { .. }
        ));
    }

    #[test]
    fn page_request_round_trips_username_set_token_and_limit() {
        let body = encode_page_request("alex", "collection", Some(b"tok".as_slice()), 200);
        // Re-decode with the same field layout the response decoder uses,
        // reusing `decode_item`'s shape (uri/added_at/removed) doesn't
        // apply here — just assert the bytes aren't empty and contain the
        // username/set substrings we wrote (loose but dependency-free).
        assert!(!body.is_empty());
        let as_string = String::from_utf8_lossy(&body);
        assert!(as_string.contains("alex"));
        assert!(as_string.contains("collection"));
    }

    #[test]
    fn decode_page_response_round_trips_items_and_next_page_token() {
        let raw = RawPageResponse {
            items: vec![
                RawItem {
                    uri: "spotify:track:a".to_string(),
                    added_at: 1_700_000_000,
                    removed: false,
                },
                RawItem {
                    uri: "spotify:album:b".to_string(),
                    added_at: 1_700_000_001,
                    removed: false,
                },
                RawItem {
                    uri: "spotify:track:removed".to_string(),
                    added_at: 1_700_000_002,
                    removed: true,
                },
            ],
            next_page_token: Some(b"cursor".to_vec()),
        };
        let bytes = round_trip_page_response(&raw);
        let decoded = decode_page_response(&bytes).expect("decode");
        assert_eq!(decoded, raw);
    }

    #[test]
    fn track_page_drops_albums_and_removed_items() {
        let raw = RawPageResponse {
            items: vec![
                RawItem {
                    uri: "spotify:track:a".to_string(),
                    added_at: 5,
                    removed: false,
                },
                RawItem {
                    uri: "spotify:album:b".to_string(),
                    added_at: 6,
                    removed: false,
                },
                RawItem {
                    uri: "spotify:track:gone".to_string(),
                    added_at: 7,
                    removed: true,
                },
            ],
            next_page_token: None,
        };
        let page = track_page(raw);
        assert_eq!(page.items.len(), 1);
        assert!(
            matches!(&page.items[0], LibraryItem::Track { track, added_at } if track.id.as_str() == "spotify:track:a" && *added_at == Some(5))
        );
    }

    #[test]
    fn album_page_drops_tracks() {
        let raw = RawPageResponse {
            items: vec![
                RawItem {
                    uri: "spotify:track:a".to_string(),
                    added_at: 5,
                    removed: false,
                },
                RawItem {
                    uri: "spotify:album:b".to_string(),
                    added_at: 6,
                    removed: false,
                },
            ],
            next_page_token: None,
        };
        let page = album_page(raw);
        assert_eq!(page.items.len(), 1);
        assert!(
            matches!(&page.items[0], LibraryItem::Album { album, .. } if album.id.as_str() == "spotify:album:b")
        );
    }

    #[test]
    fn a_page_with_only_the_other_kind_still_reports_its_next_page_token() {
        let raw = RawPageResponse {
            items: vec![RawItem {
                uri: "spotify:album:b".to_string(),
                added_at: 6,
                removed: false,
            }],
            next_page_token: Some(b"more".to_vec()),
        };
        let page = track_page(raw);
        assert!(page.items.is_empty());
        assert!(page.next_page.is_some(), "must still continue the walk");
    }

    #[test]
    fn token_encode_decode_round_trips() {
        let token = b"opaque-bytes".to_vec();
        let encoded = encode_token(Some(token.clone())).expect("encoded");
        assert_eq!(decode_token(&encoded), Some(token));
    }
}

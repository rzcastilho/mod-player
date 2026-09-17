// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SourceCommand::FetchLibrary { set: Playlists }` fulfilment (contracts/
//! catalog-source.md §1/§3): `spclient.get_rootlist` parsed as the raw
//! `SelectedListContent` proto — the same pattern 003's `account_read.rs`
//! verified live (T098 M1) — then `Playlist::get` per playlist for name/
//! owner/length/revision. Unlike `collection.rs`'s sets, a `Playlists` page
//! is already fully resolved (`PlaylistRef`, not a bare id) — no hydration
//! entry is ever queued for it (`LibraryIndex::merge_page`).
//!
//! `account_read.rs` (003's "Play from account" scaffold, and this crate's
//! own short-lived `fetch_account_tracks` that folded it here) is fully
//! retired as of T061/T062 — this module now only fulfils `FetchLibrary`.

use librespot_core::Session;
use librespot_core::spotify_uri::SpotifyUri;
use librespot_metadata::Metadata;
use librespot_metadata::playlist::list::Playlist;
use modplayer_audio_source::{
    CatalogError, LibraryItem, LibraryPage, LibrarySet, PlaylistId, PlaylistRef,
};
use protobuf::Message;

/// `get_rootlist`'s page size when the caller doesn't cap it lower
/// (contracts/catalog-source.md §1: `FetchLibrary.limit` is capped at 200).
const DEFAULT_LIMIT: usize = 200;

/// Fulfil one `FetchLibrary { set: Playlists }` (contracts/catalog-
/// source.md §1): `page` is the opaque `from` offset into the rootlist,
/// encoded as a plain decimal string.
pub async fn fetch(
    session: &Session,
    page: Option<String>,
    limit: u16,
) -> Result<LibraryPage, CatalogError> {
    let from = page
        .as_deref()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let length = if limit == 0 {
        DEFAULT_LIMIT
    } else {
        usize::from(limit).min(DEFAULT_LIMIT)
    };

    let bytes = session
        .spclient()
        .get_rootlist(from, Some(length))
        .await
        .map_err(|e| super::classify_session_error(e.kind))?;
    let (uris, page_len) = playlist_uris_from_rootlist(&bytes)?;

    let mut playlist_items = Vec::with_capacity(uris.len());
    for uri in &uris {
        let Ok(id) = PlaylistId::new(uri.clone()) else {
            continue;
        };
        let Ok(spotify_uri) = SpotifyUri::from_uri(uri) else {
            continue;
        };
        let Ok(playlist) = Playlist::get(session, &spotify_uri).await else {
            continue;
        };
        playlist_items.push(LibraryItem::Playlist(playlist_to_ref(
            session, id, &playlist,
        )));
    }

    let next_page = if page_len >= length {
        Some((from + page_len).to_string())
    } else {
        None
    };
    Ok(LibraryPage {
        set: LibrarySet::Playlists,
        items: playlist_items,
        next_page,
        sync_token: None,
    })
}

/// Parse a raw rootlist reply into its playlist URIs (`spotify:playlist:`
/// prefix only — the rootlist's items are *playlist* references, not the
/// playlist's own track content, so the typed `metadata::
/// SelectedListContent` built for a playlist's tracks rejects them; read
/// the raw proto directly, exactly as 003's `account_read.rs` did) plus the
/// raw item count (for the `next_page` continuation check). Pure — no I/O —
/// so `tests/catalog_mapping.rs` exercises this directly with a fixture.
pub fn playlist_uris_from_rootlist(bytes: &[u8]) -> Result<(Vec<String>, usize), CatalogError> {
    let msg = <Playlist as Metadata>::Message::parse_from_bytes(bytes)
        .map_err(|_| CatalogError::Unavailable("malformed rootlist".to_string()))?;
    let uris = msg
        .contents
        .items
        .iter()
        .filter_map(|item| item.uri.clone())
        .filter(|uri| uri.starts_with("spotify:playlist:"))
        .collect();
    Ok((uris, msg.contents.items.len()))
}

fn playlist_to_ref(session: &Session, id: PlaylistId, playlist: &Playlist) -> PlaylistRef {
    let owner_name = match &playlist.id {
        SpotifyUri::Playlist {
            user: Some(name), ..
        } => name.clone(),
        _ => String::new(),
    };
    let editable = !owner_name.is_empty() && owner_name == session.username();
    PlaylistRef {
        id,
        name: playlist.name().to_string(),
        owner_name,
        editable,
        artwork_url: playlist
            .attributes
            .picture_sizes
            .first()
            .map(|size| size.url.clone()),
        track_count: u32::try_from(playlist.length.max(0)).unwrap_or(0),
        revision: (!playlist.revision.is_empty()).then(|| hex_encode(&playlist.revision)),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn hex_encode_matches_known_bytes() {
        assert_eq!(hex_encode(&[0x00, 0xab, 0xff]), "00abff");
    }
}

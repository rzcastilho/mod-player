// SPDX-License-Identifier: MIT OR Apache-2.0

//! "Play from account" over the session (spec Amendment 2026-09-16): the
//! public Web API 429s a Keymaster token, so real tracks are sourced through
//! the session's own metadata client instead. The chain is
//! rootlist → first playlist → its tracks → per-track metadata → `TrackRef`.

use librespot_core::Session;
use librespot_core::spotify_uri::SpotifyUri;
use librespot_metadata::playlist::list::Playlist;
use librespot_metadata::{Metadata, Track};
use modplayer_audio_source::{AccountReadError, Availability, TrackId, TrackRef};
use protobuf::Message;

/// Fetch up to `limit` playable tracks from the signed-in account's first
/// playlist. `Err(Unavailable)` when the session cannot supply any (no
/// rootlist, no playlist, or a read failure); `Ok(vec)` (possibly empty)
/// otherwise.
pub(crate) async fn fetch_account_tracks(
    session: Session,
    limit: u8,
) -> Result<Vec<TrackRef>, AccountReadError> {
    let limit = limit.clamp(1, 50) as usize;

    // The rootlist is the user's playlist-of-playlists; there is no typed
    // `Metadata::get` for it, so fetch the bytes and parse the same proto
    // `Playlist::get` would.
    let bytes = session
        .spclient()
        .get_rootlist(0, Some(50))
        .await
        .map_err(|_| AccountReadError::Unavailable)?;
    // The rootlist is a `SelectedListContent` proto, but its items are
    // *playlist* references, so the typed `metadata::SelectedListContent`
    // (built for a playlist's *track* content) rejects them with `InvalidId`.
    // Read the raw proto: `contents.items[].uri` are the playlist URIs.
    let msg = <Playlist as Metadata>::Message::parse_from_bytes(&bytes)
        .map_err(|_| AccountReadError::Unavailable)?;
    let playlist_uri = msg
        .contents
        .items
        .iter()
        .filter_map(|item| item.uri.as_deref())
        .find(|uri| uri.starts_with("spotify:playlist:"))
        .and_then(|uri| SpotifyUri::from_uri(uri).ok())
        .ok_or(AccountReadError::Unavailable)?;

    let playlist = Playlist::get(&session, &playlist_uri)
        .await
        .map_err(|_| AccountReadError::Unavailable)?;

    let track_uris: Vec<SpotifyUri> = playlist.tracks().take(limit).cloned().collect();
    let mut refs = Vec::with_capacity(track_uris.len());
    for uri in &track_uris {
        if let Ok(track) = Track::get(&session, uri).await
            && let Some(track_ref) = track_to_ref(uri, &track)
        {
            refs.push(track_ref);
        }
    }
    Ok(refs)
}

/// Map a librespot `Track` to a host `TrackRef`, dropping tracks whose URI
/// cannot be represented as a `TrackId` (>64 bytes or non-ASCII — never true
/// for a `spotify:track:` URI, but the trait's invariant is enforced here).
fn track_to_ref(uri: &SpotifyUri, track: &Track) -> Option<TrackRef> {
    let id = TrackId::new(uri.to_uri().ok()?).ok()?;
    let artists = track.artists.0.iter().map(|a| a.name.clone()).collect();
    let album = Some(track.album.name.clone()).filter(|name| !name.is_empty());
    let duration_ms = track.duration.max(0) as u32;
    Some(TrackRef::new(
        id,
        track.name.clone(),
        artists,
        album,
        None,
        duration_ms,
        Availability::Available,
    ))
}

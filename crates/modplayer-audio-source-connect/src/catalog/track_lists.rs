// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SourceCommand::FetchTrackList` fulfilment (contracts/catalog-source.md
//! §1/§3): album discs (order preserved), playlist tracks (playlist
//! order), and an artist's top-10 tracks for `session.country()` — every
//! bare URI resolved through `catalog::hydrate` so a `TrackList` reply
//! already carries full `TrackRef`s (`LibraryIndex::merge_track_list`
//! never re-queues them for hydration).

use librespot_core::{Session, SpotifyUri};
use librespot_metadata::{Album, Artist, Metadata, Playlist};
use modplayer_audio_source::{CatalogError, TrackId, TrackList, TrackListSource};

/// contracts/catalog-source.md §3: "`FetchTrackList{ArtistTop}` returns
/// <= 10 tracks for `session.country()`".
const ARTIST_TOP_LIMIT: usize = 10;

/// Fulfil one `FetchTrackList` (contracts/catalog-source.md §1).
pub async fn fetch(session: &Session, source: &TrackListSource) -> Result<TrackList, CatalogError> {
    let tracks = match source {
        TrackListSource::Album(id) => album_tracks(session, id.as_str()).await?,
        TrackListSource::Playlist(id) => playlist_tracks(session, id.as_str()).await?,
        TrackListSource::ArtistTop(id) => artist_top_tracks(session, id.as_str()).await?,
    };
    Ok(TrackList {
        source: source.clone(),
        tracks,
    })
}

async fn album_tracks(
    session: &Session,
    uri: &str,
) -> Result<Vec<modplayer_audio_source::TrackRef>, CatalogError> {
    let spotify_uri = SpotifyUri::from_uri(uri)
        .map_err(|_| CatalogError::Unavailable("invalid album id".to_string()))?;
    let album = Album::get(session, &spotify_uri)
        .await
        .map_err(|e| super::classify_session_error(e.kind))?;
    let ids: Vec<TrackId> = album
        .tracks()
        .filter_map(|uri| uri.to_uri().ok())
        .filter_map(|uri| TrackId::new(uri).ok())
        .collect();
    let (tracks, _albums, _artists, _missing) =
        super::hydrate::hydrate(session, &ids, &[], &[]).await;
    Ok(order_by(&ids, tracks))
}

async fn playlist_tracks(
    session: &Session,
    uri: &str,
) -> Result<Vec<modplayer_audio_source::TrackRef>, CatalogError> {
    let spotify_uri = SpotifyUri::from_uri(uri)
        .map_err(|_| CatalogError::Unavailable("invalid playlist id".to_string()))?;
    let playlist = Playlist::get(session, &spotify_uri)
        .await
        .map_err(|e| super::classify_session_error(e.kind))?;
    let ids: Vec<TrackId> = playlist
        .tracks()
        .filter_map(|uri| uri.to_uri().ok())
        .filter_map(|uri| TrackId::new(uri).ok())
        .collect();
    let (tracks, _albums, _artists, _missing) =
        super::hydrate::hydrate(session, &ids, &[], &[]).await;
    Ok(order_by(&ids, tracks))
}

async fn artist_top_tracks(
    session: &Session,
    uri: &str,
) -> Result<Vec<modplayer_audio_source::TrackRef>, CatalogError> {
    let spotify_uri = SpotifyUri::from_uri(uri)
        .map_err(|_| CatalogError::Unavailable("invalid artist id".to_string()))?;
    let artist = Artist::get(session, &spotify_uri)
        .await
        .map_err(|e| super::classify_session_error(e.kind))?;
    let top = artist.top_tracks.for_country(&session.country());
    let ids: Vec<TrackId> = top
        .iter()
        .filter_map(|uri| uri.to_uri().ok())
        .filter_map(|uri| TrackId::new(uri).ok())
        .take(ARTIST_TOP_LIMIT)
        .collect();
    let (tracks, _albums, _artists, _missing) =
        super::hydrate::hydrate(session, &ids, &[], &[]).await;
    Ok(order_by(&ids, tracks))
}

/// `hydrate::hydrate` resolves ids in the order given and skips any it
/// can't resolve — re-order its output back onto `ids`' order (rather than
/// trusting it stayed the same) so a track a service quirk answered
/// out-of-order never desyncs album-disc/playlist order (FR-013).
fn order_by(
    ids: &[TrackId],
    tracks: Vec<modplayer_audio_source::TrackRef>,
) -> Vec<modplayer_audio_source::TrackRef> {
    let mut by_id: std::collections::HashMap<TrackId, modplayer_audio_source::TrackRef> =
        tracks.into_iter().map(|t| (t.id.clone(), t)).collect();
    ids.iter().filter_map(|id| by_id.remove(id)).collect()
}

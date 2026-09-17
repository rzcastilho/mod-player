// SPDX-License-Identifier: MIT OR Apache-2.0

//! Batched identity hydration (contracts/catalog-source.md §1
//! `HydrateRefs`, §2 rule 7, §3, research R4): resolves bare
//! `TrackId`/`AlbumId`/`ArtistId` uris into full `TrackRef`/`AlbumRef`/
//! `ArtistRef`s, mapping explicit/release-date/availability/artwork per
//! entity (gates V4/V5). Used both to fulfil `SourceCommand::HydrateRefs`
//! directly (`worker.rs`) and by `catalog::search`'s strategy-2 fallback,
//! which only gets bare track URIs from `get_context`.
//!
//! Uses librespot-metadata's typed per-entity `Track::get`/`Album::get`/
//! `Artist::get` (`account_read.rs`'s pattern, verified live in 003) rather
//! than one wire-level batched `get_extended_metadata` call: that call
//! returns each entity's metadata as an `Any`-wrapped payload keyed by
//! `ExtensionKind`, a hand-parsed protocol surface this slice leaves for
//! the live verification gate (research R14 V3) to inform — nothing above
//! this function (the `HydrateRefs` contract, the `TrackRef`/`AlbumRef`/
//! `ArtistRef` shapes) changes if a later pass switches the transport.
//! Requests run sequentially (the ≤ 100-id batch cap keeps this bounded);
//! a later pass may fan them out concurrently with no contract change.

use librespot_core::{Session, SpotifyUri};
use librespot_metadata::image::Images;
use librespot_metadata::restriction::Restrictions;
use librespot_metadata::{Album, Artist, Metadata, Track};
use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, Availability, ReleaseDate, TrackId, TrackRef,
    TrackRefExtras,
};

/// Resolve every requested id, in order, reporting a URI that fails to
/// resolve as `missing` (contracts/catalog-source.md §2 rule 7: "the host
/// maps a missing *track* to `Availability::Removed`" — this function only
/// reports the URI; that mapping is the host's job).
pub async fn hydrate(
    session: &Session,
    tracks: &[TrackId],
    albums: &[AlbumId],
    artists: &[ArtistId],
) -> (Vec<TrackRef>, Vec<AlbumRef>, Vec<ArtistRef>, Vec<String>) {
    let mut hydrated_tracks = Vec::with_capacity(tracks.len());
    let mut hydrated_albums = Vec::with_capacity(albums.len());
    let mut hydrated_artists = Vec::with_capacity(artists.len());
    let mut missing = Vec::new();

    for id in tracks {
        match hydrate_track(session, id.as_str()).await {
            Ok(track) => hydrated_tracks.push(track),
            Err(uri) => missing.push(uri),
        }
    }
    for id in albums {
        match hydrate_album(session, id.as_str()).await {
            Ok(album) => hydrated_albums.push(album),
            Err(uri) => missing.push(uri),
        }
    }
    for id in artists {
        match hydrate_artist(session, id.as_str()).await {
            Ok(artist) => hydrated_artists.push(artist),
            Err(uri) => missing.push(uri),
        }
    }

    (hydrated_tracks, hydrated_albums, hydrated_artists, missing)
}

async fn hydrate_track(session: &Session, uri: &str) -> Result<TrackRef, String> {
    let spotify_uri = SpotifyUri::from_uri(uri).map_err(|_| uri.to_string())?;
    let track = Track::get(session, &spotify_uri)
        .await
        .map_err(|_| uri.to_string())?;
    track_to_ref(session, uri, &track).ok_or_else(|| uri.to_string())
}

async fn hydrate_album(session: &Session, uri: &str) -> Result<AlbumRef, String> {
    let spotify_uri = SpotifyUri::from_uri(uri).map_err(|_| uri.to_string())?;
    let album = Album::get(session, &spotify_uri)
        .await
        .map_err(|_| uri.to_string())?;
    album_to_ref(session, uri, &album).ok_or_else(|| uri.to_string())
}

async fn hydrate_artist(session: &Session, uri: &str) -> Result<ArtistRef, String> {
    let spotify_uri = SpotifyUri::from_uri(uri).map_err(|_| uri.to_string())?;
    let artist = Artist::get(session, &spotify_uri)
        .await
        .map_err(|_| uri.to_string())?;
    artist_to_ref(session, uri, &artist).ok_or_else(|| uri.to_string())
}

/// Map a librespot `Track` to a host `TrackRef` (data-model.md §1.3,
/// FR-008): explicit badge, release date (from the album), region
/// availability (gate V4), artwork URL (gate V5). `None` when the URI
/// cannot be represented as a `TrackId` (never true for a well-formed
/// `spotify:track:` URI).
fn track_to_ref(session: &Session, uri: &str, track: &Track) -> Option<TrackRef> {
    let id = TrackId::new(uri).ok()?;
    let artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
    let artist_ids: Vec<ArtistId> = track
        .artists
        .iter()
        .filter_map(|a| a.id.to_uri().ok())
        .filter_map(|uri| ArtistId::new(uri).ok())
        .collect();
    let album_name = Some(track.album.name.clone()).filter(|name| !name.is_empty());
    let album_id = track
        .album
        .id
        .to_uri()
        .ok()
        .and_then(|uri| AlbumId::new(uri).ok());
    let duration_ms = track.duration.max(0) as u32;
    let availability = availability_for(session, &track.restrictions);
    let artwork_url = image_url(session, &track.album.covers);
    let release_date = release_date_of(&track.album.date);

    let base = TrackRef::new(
        id,
        track.name.clone(),
        artists,
        album_name,
        artwork_url,
        duration_ms,
        availability,
    );
    Some(base.with_extras(TrackRefExtras {
        artist_ids,
        album_id,
        explicit: track.is_explicit,
        release_date,
    }))
}

fn album_to_ref(session: &Session, uri: &str, album: &Album) -> Option<AlbumRef> {
    let id = AlbumId::new(uri).ok()?;
    Some(AlbumRef {
        id,
        name: album.name.clone(),
        artists: album.artists.iter().map(|a| a.name.clone()).collect(),
        artwork_url: image_url(session, &album.covers),
        release_date: release_date_of(&album.date),
        track_count: album.tracks().count() as u32,
    })
}

fn artist_to_ref(session: &Session, uri: &str, artist: &Artist) -> Option<ArtistRef> {
    let id = ArtistId::new(uri).ok()?;
    Some(ArtistRef {
        id,
        name: artist.name.clone(),
        artwork_url: image_url(session, &artist.portraits),
    })
}

/// The catalogue a restriction must name to apply to this session — the
/// `catalogue` user attribute, `"premium"` when the service didn't send
/// one (the same default librespot-playback's own availability check
/// uses, `librespot_metadata::audio::item::allowed_for_user`).
const DEFAULT_USER_CATALOGUE: &str = "premium";

/// Region availability for the session (gate V4, contracts/catalog-
/// source.md §3): [`availability_from`] with the session's country and
/// catalogue.
pub fn availability_for(session: &Session, restrictions: &Restrictions) -> Availability {
    let user_data = session.user_data();
    let catalogue = user_data
        .attributes
        .get("catalogue")
        .map_or(DEFAULT_USER_CATALOGUE, String::as_str);
    availability_from(&user_data.country, catalogue, restrictions)
}

/// Pure core of [`availability_for`]: only restrictions whose
/// `catalogue_strs` name the user's own catalogue apply; among those, a
/// `countries_allowed` list that excludes the country, or a
/// `countries_forbidden` list that includes it, marks the entity
/// `UnavailableRegion`.
///
/// Restrictions naming *no* catalogue are ignored on purpose — the
/// 2026-09-17 live probe (`tests/live.rs::restrictions_probe`, research
/// R14 V4) found that every "relinked" saved track (an old track id
/// Spotify replaced, playable through `Track.alternatives`) carries
/// exactly one such restriction with an *empty* `countries_allowed`, and
/// applying it greyed a third of a real library as "Unavailable in your
/// region". This mirrors librespot-playback, which plays those tracks
/// fine via their alternatives. `Removed` is never produced here — it is
/// the host's mapping of a `Hydrated.missing` URI (contract rule 7,
/// tested end to end in `modplayer-core::library::index`'s
/// `a_missing_track_uri_resolves_to_removed_rather_than_re_queueing_forever`),
/// not a property of a successfully-fetched entity. `pub` so
/// `tests/catalog_mapping.rs::availability_*` (US3 T068) can pin this
/// receiver-side half of gate V4 directly, without a live session.
pub fn availability_from(
    country: &str,
    user_catalogue: &str,
    restrictions: &Restrictions,
) -> Availability {
    let applies = |restriction: &&librespot_metadata::restriction::Restriction| {
        restriction
            .catalogue_strs
            .iter()
            .any(|catalogue| catalogue == user_catalogue)
    };
    for restriction in restrictions.iter().filter(applies) {
        if let Some(allowed) = &restriction.countries_allowed
            && !allowed.iter().any(|c| c == country)
        {
            return Availability::UnavailableRegion;
        }
        if let Some(forbidden) = &restriction.countries_forbidden
            && forbidden.iter().any(|c| c == country)
        {
            return Availability::UnavailableRegion;
        }
    }
    Availability::Available
}

/// The public CDN artwork URL for the first cover/portrait image, if any
/// (research R8, gate V5): `session`'s own `image-url` user attribute
/// template (the same one `spclient.get_image` reads), with `{file_id}`
/// substituted — never a protocol call of its own.
fn image_url(session: &Session, images: &Images) -> Option<String> {
    let image = images.first()?;
    let template = session.get_user_attribute("image-url")?;
    let hex = image.id.to_base16().ok()?;
    Some(template.replace("{file_id}", &hex))
}

fn release_date_of(date: &librespot_core::date::Date) -> Option<ReleaseDate> {
    let year = date.year();
    if year <= 0 {
        return None;
    }
    Some(ReleaseDate {
        year: year as u16,
        month: Some(u8::from(date.month())),
        day: Some(date.day()),
    })
}

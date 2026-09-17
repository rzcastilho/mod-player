// SPDX-License-Identifier: MIT OR Apache-2.0

//! Catalog identities and payload types (data-model.md §1.4-1.7,
//! contracts/catalog-source.md §1-2), added by
//! 004-search-and-library-browse. This module stays `std`-only like the
//! rest of the crate (Constitution IV, research R13): no `serde`, no
//! secrets.

use std::fmt;

use crate::types::TrackRef;

/// Declares an id newtype with the same invariant as `TrackId`
/// (non-empty, ASCII, <= 64 bytes service URI).
macro_rules! id_newtype {
    ($name:ident, $err:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        #[doc = concat!("`", stringify!($name), "::new` rejected its input.")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $err {
            /// Empty, non-ASCII, or longer than 64 bytes.
            Invalid,
        }

        impl fmt::Display for $err {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    f,
                    concat!(
                        "invalid ",
                        stringify!($name),
                        ": must be non-empty ASCII, <= 64 bytes"
                    )
                )
            }
        }

        impl std::error::Error for $err {}

        impl $name {
            /// Construct from a URI, validating the invariant.
            pub fn new(uri: impl Into<String>) -> Result<Self, $err> {
                let uri = uri.into();
                if uri.is_empty() || uri.len() > 64 || !uri.is_ascii() {
                    return Err($err::Invalid);
                }
                Ok(Self(uri))
            }

            /// The URI as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_newtype!(
    AlbumId,
    AlbumIdError,
    "Newtype over the service's stable album identifier \
     (`spotify:album:<base62>`), same invariant as `TrackId` (data-model.md §1.1)."
);
id_newtype!(
    ArtistId,
    ArtistIdError,
    "Newtype over the service's stable artist identifier \
     (`spotify:artist:<base62>`), same invariant as `TrackId` (data-model.md §1.1)."
);
id_newtype!(
    PlaylistId,
    PlaylistIdError,
    "Newtype over the service's stable playlist identifier \
     (`spotify:playlist:<base62>`), same invariant as `TrackId` (data-model.md §1.1)."
);

/// Release-date precision as supplied by the service (data-model.md §1.3):
/// day/month are optional, year is not. Rows show `year` only (FR-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseDate {
    pub year: u16,
    pub month: Option<u8>,
    pub day: Option<u8>,
}

/// Album metadata (data-model.md §1.4).
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumRef {
    pub id: AlbumId,
    pub name: String,
    pub artists: Vec<String>,
    pub artwork_url: Option<String>,
    pub release_date: Option<ReleaseDate>,
    pub track_count: u32,
}

/// Artist metadata (data-model.md §1.5).
#[derive(Debug, Clone, PartialEq)]
pub struct ArtistRef {
    pub id: ArtistId,
    pub name: String,
    pub artwork_url: Option<String>,
}

/// Playlist metadata (data-model.md §1.6).
#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistRef {
    pub id: PlaylistId,
    pub name: String,
    /// Display name of the owner; "Owner: <name>" label when `!editable`.
    pub owner_name: String,
    /// `owner == session user` — informational only in this slice (FR-010).
    pub editable: bool,
    pub artwork_url: Option<String>,
    pub track_count: u32,
    /// Opaque "last synced version", for later slices.
    pub revision: Option<String>,
}

/// The four account library sets FR-009 lists (data-model.md §1.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibrarySet {
    SavedTracks,
    SavedAlbums,
    FollowedArtists,
    Playlists,
}

/// The four search result groups, fixed order (data-model.md §1.7, FR-001).
/// `#[repr(u8)]` discriminants back `pack_request_id`/`unpack_request_id`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchKind {
    Track = 0,
    Album = 1,
    Artist = 2,
    Playlist = 3,
}

/// Where a full ordered track list comes from (data-model.md §1.7).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TrackListSource {
    Album(AlbumId),
    Playlist(PlaylistId),
    ArtistTop(ArtistId),
}

/// One search hit, one of the four music kinds (data-model.md §1.7,
/// contracts/catalog-source.md §2 rule 5: never a podcast/episode/show).
#[derive(Debug, Clone, PartialEq)]
pub enum SearchHit {
    Track(TrackRef),
    Album(AlbumRef),
    Artist(ArtistRef),
    Playlist(PlaylistRef),
}

/// One page of one search group (data-model.md §1.7).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchGroupPage {
    pub kind: SearchKind,
    pub items: Vec<SearchHit>,
    /// `Some` when a further "Show more" page exists.
    pub next_offset: Option<u32>,
}

/// A full search reply: every group that answered, plus which kinds the
/// strategy in use could not fulfil (data-model.md §1.7, research R2).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchPage {
    pub groups: Vec<SearchGroupPage>,
    pub unsupported: Vec<SearchKind>,
}

/// One item of an account library page (data-model.md §1.7).
#[derive(Debug, Clone, PartialEq)]
pub enum LibraryItem {
    Track {
        track: TrackRef,
        added_at: Option<u64>,
    },
    Album {
        album: AlbumRef,
        added_at: Option<u64>,
    },
    Artist(ArtistRef),
    Playlist(PlaylistRef),
}

/// One page of one library set (data-model.md §1.7).
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryPage {
    pub set: LibrarySet,
    pub items: Vec<LibraryItem>,
    /// Opaque continuation token for the next page.
    pub next_page: Option<String>,
    /// Opaque delta-sync token, for later slices.
    pub sync_token: Option<String>,
}

/// A full ordered track list for an album, playlist or an artist's top
/// tracks (data-model.md §1.7).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackList {
    pub source: TrackListSource,
    pub tracks: Vec<TrackRef>,
}

/// Why a catalog request failed (data-model.md §1.7, contracts/catalog-
/// source.md §2 rule 4). Never carries raw error text or a URL with a
/// token — `Unavailable`'s string is pre-redacted by the classifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    Offline,
    RateLimited {
        retry_after_ms: Option<u32>,
    },
    NotFound,
    /// The strategy/endpoint this reply would need is refused or not
    /// implemented (research R2/R3 fallbacks); the UI omits the group/set.
    Unsupported,
    /// Redacted reason string — never the raw error text.
    Unavailable(String),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => f.write_str("offline"),
            Self::RateLimited { retry_after_ms } => {
                write!(f, "rate limited (retry after {retry_after_ms:?} ms)")
            }
            Self::NotFound => f.write_str("not found"),
            Self::Unsupported => f.write_str("unsupported"),
            Self::Unavailable(reason) => write!(f, "unavailable: {reason}"),
        }
    }
}

impl std::error::Error for CatalogError {}

/// Pack a `SearchSession` generation and result kind into an opaque
/// `request_id` (data-model.md §2.1: `generation << 8 | kind`). Meaningless
/// to any `SourceHost` implementor (contracts/catalog-source.md §2 rule 2)
/// — only `SearchSession` encodes/decodes it, to drop a reply whose
/// generation is no longer current.
///
/// ```
/// use modplayer_audio_source::catalog::{pack_request_id, unpack_request_id, SearchKind};
///
/// let request_id = pack_request_id(7, SearchKind::Playlist);
/// assert_eq!(unpack_request_id(request_id), (7, SearchKind::Playlist as u8));
/// ```
pub fn pack_request_id(generation: u64, kind: SearchKind) -> u64 {
    (generation << 8) | (kind as u64)
}

/// Inverse of [`pack_request_id`]: `(generation, kind discriminant byte)`.
///
/// ```
/// use modplayer_audio_source::catalog::{pack_request_id, unpack_request_id, SearchKind};
///
/// let request_id = pack_request_id(1, SearchKind::Album);
/// let (generation, kind) = unpack_request_id(request_id);
/// assert_eq!(generation, 1);
/// assert_eq!(kind, SearchKind::Album as u8);
/// ```
pub fn unpack_request_id(request_id: u64) -> (u64, u8) {
    (request_id >> 8, (request_id & 0xFF) as u8)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn album_id_rejects_empty_too_long_and_non_ascii() {
        assert!(AlbumId::new("").is_err());
        assert!(AlbumId::new("x".repeat(65)).is_err());
        assert!(AlbumId::new("spotify:album:caf\u{e9}").is_err());
        assert!(AlbumId::new("spotify:album:abc123").is_ok());
    }

    #[test]
    fn artist_id_rejects_empty_too_long_and_non_ascii() {
        assert!(ArtistId::new("").is_err());
        assert!(ArtistId::new("x".repeat(65)).is_err());
        assert!(ArtistId::new("spotify:artist:caf\u{e9}").is_err());
        assert!(ArtistId::new("spotify:artist:abc123").is_ok());
    }

    #[test]
    fn playlist_id_rejects_empty_too_long_and_non_ascii() {
        assert!(PlaylistId::new("").is_err());
        assert!(PlaylistId::new("x".repeat(65)).is_err());
        assert!(PlaylistId::new("spotify:playlist:caf\u{e9}").is_err());
        assert!(PlaylistId::new("spotify:playlist:abc123").is_ok());
    }

    #[test]
    fn id_newtypes_display_as_their_uri() {
        let id = AlbumId::new("spotify:album:abc123").expect("valid");
        assert_eq!(id.to_string(), "spotify:album:abc123");
        assert_eq!(id.as_str(), "spotify:album:abc123");
    }

    #[test]
    fn request_id_round_trips_generation_and_kind() {
        for kind in [
            SearchKind::Track,
            SearchKind::Album,
            SearchKind::Artist,
            SearchKind::Playlist,
        ] {
            for generation in [0u64, 1, 42, u64::MAX >> 8] {
                let packed = pack_request_id(generation, kind);
                let (decoded_generation, decoded_kind) = unpack_request_id(packed);
                assert_eq!(decoded_generation, generation);
                assert_eq!(decoded_kind, kind as u8);
            }
        }
    }

    #[test]
    fn request_id_packing_matches_the_documented_formula() {
        assert_eq!(pack_request_id(1, SearchKind::Album), (1 << 8) | 1);
        assert_eq!(pack_request_id(7, SearchKind::Playlist), (7 << 8) | 3);
    }
}

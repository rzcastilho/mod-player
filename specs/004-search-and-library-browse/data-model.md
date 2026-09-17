# Data Model: Catalog Search and Library Browsing

**Feature**: 004-search-and-library-browse | **Date**: 2026-09-16 | **Plan**: [plan.md](plan.md)

Types are grouped by the crate that owns them. All are plain data
(`Clone + Debug + PartialEq`, no secrets). Persistence formats are host DTOs
in `modplayer-core`; the trait crate stays serde-free (research R13).

## 1. Catalog references — `modplayer-audio-source` (extended, research R10)

### 1.1 `TrackId`, `AlbumId`, `ArtistId`, `PlaylistId`

Newtypes over the service URI (`spotify:<kind>:<base62>`), same invariant as
003's `TrackId` (non-empty, ASCII, ≤ 64 bytes). `TrackId` is unchanged and
remains the key for all per-track state (DM-2).

### 1.2 `Availability` (widened — DM-2)

| Variant | Meaning | Row rendering (FR-019) |
|---|---|---|
| `Available` | playable | normal |
| `UnavailableRegion` | not licensed for the session country | greyed, "Unavailable in your region" |
| `Removed` | no longer on the service | greyed, "Removed from the service" |

003's `Unavailable` is renamed `UnavailableRegion` (two call sites).

### 1.3 `TrackRef` (extended — DM-2, FR-008)

| Field | Type | Notes |
|---|---|---|
| `id` | `TrackId` | primary key |
| `title` | `String` | "Unknown title" fallback kept |
| `artists` | `Vec<String>` | display names, in credit order |
| `artist_ids` | `Vec<ArtistId>` | **new**; parallel to `artists`; may be empty for 003-era refs |
| `album` | `Option<String>` | |
| `album_id` | `Option<AlbumId>` | **new** |
| `artwork_url` | `Option<String>` | public CDN URL (research R8) |
| `duration_ms` | `u32` | shown as m:ss |
| `explicit` | `bool` | **new**; "E" badge |
| `release_date` | `Option<ReleaseDate>` | **new** |
| `availability` | `Availability` | |

`ReleaseDate { year: u16, month: Option<u8>, day: Option<u8> }` — the
service supplies day/month/year precision; rows show `year` only.

Constructor: `TrackRef::new(id, title, artists, album, artwork_url, duration_ms, availability)`
unchanged; new fields default (`explicit=false`, `release_date=None`,
ids empty) and are set through `TrackRef::with_extras(TrackRefExtras)`.

### 1.4 `AlbumRef` (DM-3)

| Field | Type |
|---|---|
| `id` | `AlbumId` |
| `name` | `String` |
| `artists` | `Vec<String>` |
| `artwork_url` | `Option<String>` |
| `release_date` | `Option<ReleaseDate>` |
| `track_count` | `u32` |

### 1.5 `ArtistRef` (DM-3)

| Field | Type |
|---|---|
| `id` | `ArtistId` |
| `name` | `String` |
| `artwork_url` | `Option<String>` |

### 1.6 `PlaylistRef` (DM-3)

| Field | Type | Notes |
|---|---|---|
| `id` | `PlaylistId` | |
| `name` | `String` | |
| `owner_name` | `String` | display name; "Owner: <name>" label when `!editable` |
| `editable` | `bool` | `owner == session user` — informational only in this slice (FR-010) |
| `artwork_url` | `Option<String>` | |
| `track_count` | `u32` | |
| `revision` | `Option<String>` | "last synced version"; opaque, for later slices |

### 1.7 Catalog request/reply payloads (`catalog` module)

```text
LibrarySet        = SavedTracks | SavedAlbums | FollowedArtists | Playlists
SearchKind        = Track | Album | Artist | Playlist        (fixed order)
TrackListSource   = Album(AlbumId) | Playlist(PlaylistId) | ArtistTop(ArtistId)

SearchHit         = Track(TrackRef) | Album(AlbumRef) | Artist(ArtistRef) | Playlist(PlaylistRef)
SearchGroupPage   { kind: SearchKind, items: Vec<SearchHit>, next_offset: Option<u32> }
SearchPage        { groups: Vec<SearchGroupPage>, unsupported: Vec<SearchKind> }

LibraryItem       = Track{ track: TrackRef, added_at: Option<u64> }
                  | Album{ album: AlbumRef, added_at: Option<u64> }
                  | Artist(ArtistRef)
                  | Playlist(PlaylistRef)
LibraryPage       { set: LibrarySet, items: Vec<LibraryItem>, next_page: Option<String>, sync_token: Option<String> }
TrackList         { source: TrackListSource, tracks: Vec<TrackRef> }

CatalogError      = Offline | RateLimited { retry_after_ms: Option<u32> } | NotFound
                  | Unsupported | Unavailable(String /* redacted reason */)
```

`unsupported` / `Unsupported` exist only for the research R2/R3 fallbacks;
the UI omits an unsupported group, and the sync scheduler records a
`Partial` outcome.

## 2. Search state — `modplayer-core::search` (research R7)

### 2.1 `SearchSession`

| Field | Type | Rule |
|---|---|---|
| `raw_query` | `String` | as typed |
| `query` | `String` | trimmed; empty ⇒ pre-search state, no request (FR-001) |
| `debounce_deadline` | `Option<Instant>` | `last_edit + 150 ms` |
| `generation` | `u64` | incremented per issued query; embedded in `request_id` (`generation << 8 \| kind`) |
| `groups` | `[GroupState; 4]` | fixed order Tracks, Albums, Artists, Playlists |
| `refreshing` | `bool` | any group `RateLimited` (FR-015) |
| `offline` | `bool` | mirrors research R5; blocks requests and selects the offline state (FR-018) |

`GroupState = Idle | Pending | Loaded { items: Vec<SearchHit>, next_offset: Option<u32>, loading_more: bool } | Empty | Unsupported | RateLimited { stale: Option<Vec<SearchHit>> }`

State transitions:

```text
Idle ──edit──▶ (deadline armed) ──tick past deadline, online, query non-empty──▶ Pending (all 4, generation++)
Pending ──reply(gen==current, items>0)──▶ Loaded
Pending ──reply(gen==current, items==0)──▶ Empty            (group omitted; all four Empty ⇒ no-results state)
Pending ──reply(gen==current, Unsupported)──▶ Unsupported   (omitted)
Pending|Loaded ──reply(RateLimited)──▶ RateLimited{stale = previous Loaded items}   (refreshing…, backed-off retry of the newest query)
* ──reply(gen<current)──▶ unchanged (discarded)
* ──edit to empty──▶ Idle (all groups), generation++ (poisons in-flight replies)
* ──offline──▶ groups untouched, offline=true (view shows the offline state), in-flight replies discarded
Loaded ──Show more──▶ Loaded{loading_more=true} ──page reply──▶ Loaded (items appended, next_offset updated)
```

## 3. Library index and sync — `modplayer-core::library` (research R6)

### 3.1 `LibraryIndex`

| Field | Type | Notes |
|---|---|---|
| `saved_tracks` | `Vec<SavedEntry<TrackId>>` | `{ id, added_at }`, service order (newest added first) |
| `saved_albums` | `Vec<SavedEntry<AlbumId>>` | |
| `followed_artists` | `Vec<ArtistId>` | |
| `playlists` | `Vec<PlaylistId>` | rootlist order |
| `tracks` | `HashMap<TrackId, TrackRef>` | hydrated refs (lazy, research R4) |
| `albums` | `HashMap<AlbumId, AlbumRef>` | |
| `artists` | `HashMap<ArtistId, ArtistRef>` | |
| `playlist_refs` | `HashMap<PlaylistId, PlaylistRef>` | |
| `track_lists` | `HashMap<TrackListSource, Vec<TrackId>>` | cached full order for albums/playlists, top tracks for artists (DM-3 "cached track list") |
| `meta` | `SyncMeta` | below |

Invariants: an id in a set list without a hydrated ref renders as a
skeleton row and is queued for hydration; ids are unique per set.

### 3.2 `SyncMeta`

| Field | Type |
|---|---|
| `last_synced_at` | `Option<u64>` (unix ms) |
| `last_outcome` | `SyncOutcome = Never \| Ok \| Partial \| RateLimited \| Failed` |
| `sync_tokens` | `HashMap<LibrarySet, String>` (opaque, for delta sync later) |

### 3.3 `SyncScheduler` (pure, injectable clock)

```text
states:  Idle | Syncing { set, page: Option<String>, started: Instant } | BackingOff { until: Instant, attempt: u8 }
triggers: Launch | Interval(15 min since last completed sync) | Reconnect(Transient→Ok) | Retry(user, FR-021 only)
Idle ──trigger, online──▶ Syncing{SavedTracks}
Syncing ──page ok──▶ Syncing (same set, next page) | next set | Idle{outcome=Ok|Partial}
Syncing ──RateLimited──▶ BackingOff{ 2^attempt × 15 s, max 4 min }  (refreshing… visible)
BackingOff ──until reached, online──▶ Syncing (resume same set/page)
Syncing ──other error, snapshot exists──▶ Idle{outcome=Failed}      (silent, FR-011)
Syncing ──other error, no snapshot──▶ Idle{outcome=Failed, first_sync_failed=true}   (FR-021 state)
* ──offline──▶ Idle (in-flight replies discarded; resumes on Reconnect)
```

Derived view flags: `refreshing = matches!(state, BackingOff)`,
`first_sync_failed = outcome==Failed && last_synced_at.is_none()`,
`loading = index not yet loaded from disk`.

### 3.4 Persistence DTOs (`library::persist`)

`index.json` — `{ schema: 1, sets…, refs…, meta }` (serde DTO mirrors of
§1.3–1.6; unknown fields ignored; a newer `schema` loads as empty with one
warning, like 001's settings). Written atomically after each completed set
and on shutdown. Deleted on sign-out / revocation. Both this file and
`play_log.json` (§3.5) are covered by proptest save → load round-trips
(Constitution VIII; contracts/library-and-search-core.md §6).

### 3.5 `PlayLog` (Recently Played — FR-012, DM-2 `last played`)

| Field | Type |
|---|---|
| `last_played` | `HashMap<TrackId, u64>` (unix ms; unbounded, every track ever played) |
| `play_count` | `HashMap<TrackId, u32>` |
| `first_played` | `HashMap<TrackId, u64>` |
| `recent` | `Vec<TrackId>` — derived: top 100 by `last_played` desc, distinct |
| `refs` | `HashMap<TrackId, TrackRef>` — refs for the 100 recent entries only (so rows render offline) |

`record(track: &TrackRef, now)` — upsert, recompute `recent`, mark dirty;
flushed atomically to `play_log.json` ≤ 1 s later on the persistence
thread and on shutdown. Never merged with 003's in-memory `Queue.history`.

### 3.6 Connectivity fact (`library::Connectivity`)

`Online | Offline` derived from `SourceHealth` + `Registered` (research R5);
one place, read by search, the scheduler and the UI.

## 4. Row model — `modplayer-ui::rows` (FR-004/FR-008/FR-024)

```text
RowEntity  = Track(TrackRef) | Album(AlbumRef) | Artist(ArtistRef) | Playlist(PlaylistRef)
RowOrigin  = SearchTracks | SavedTracks | RecentlyPlayed | AlbumTracks(AlbumId) | PlaylistTracks(PlaylistId) | ArtistTop(ArtistId)
RowAction  = PlayNow | PlayNext | AddToQueue | AddToPlaylist | SaveToLibrary | PinForOffline
ActingList = { tracks: Vec<TrackRef>, cursor: usize }          (FR-006 rule)
```

`RowAction::{AddToPlaylist, SaveToLibrary, PinForOffline}` are placeholders:
raise `Severity::Info` notification `coming-soon`, no other effect (FR-005).

Initials rule (FR-020): `initials(name) -> Initials = Letters(String /* 1–2 uppercase chars */) | Glyph`.

## 5. Notifications added (keys in `locales/en-US/library.ftl`)

| Key | Severity | When |
|---|---|---|
| `coming-soon` | Info | placeholder action activated |
| `library-first-sync-failed` | (inline state, not a toast) | FR-021 |

No new notification *actions*; the existing `Retry` pattern is reused for
the FR-021 inline retry button.

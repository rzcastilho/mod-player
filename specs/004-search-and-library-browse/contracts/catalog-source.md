# Contract: Catalog reads over the `SourceHost` seam

**Crates**: `modplayer-audio-source` (types), `modplayer-audio-source-connect` (fulfilment), `modplayer-audio-source-synthetic` (`ScriptedHost` double)
**Extends**: 003 `contracts/audio-source-host.md` §2–3 — additive only; no existing variant changes shape.
**Research**: R1–R4, R8, R10, R12

## 1. New `SourceCommand` variants

| Variant | Fields | Reply |
|---|---|---|
| `SearchCatalog` | `request_id: u64, query: String, kinds: Vec<SearchKind>, offset: u32, limit: u8 (≤ 50)` | `SourceEvent::SearchResult` |
| `FetchLibrary` | `request_id: u64, set: LibrarySet, page: Option<String>, limit: u16 (≤ 200)` | `SourceEvent::LibraryPage` |
| `FetchTrackList` | `request_id: u64, source: TrackListSource` | `SourceEvent::TrackList` |
| `HydrateRefs` | `request_id: u64, tracks: Vec<TrackId>, albums: Vec<AlbumId>, artists: Vec<ArtistId>` (≤ 100 total) | `SourceEvent::Hydrated` |
| `CancelCatalog` | `request_id: u64` | none (best effort; a late reply is still delivered and the host discards it by id) |

`ListAccountTracks` is removed in this slice (research R12) once the
above land; `AccountTracks` goes with it.

## 2. New `SourceEvent` variants

| Variant | Fields |
|---|---|
| `SearchResult` | `request_id, result: Result<SearchPage, CatalogError>` |
| `LibraryPage` | `request_id, result: Result<LibraryPage, CatalogError>` |
| `TrackList` | `request_id, result: Result<TrackList, CatalogError>` |
| `Hydrated` | `request_id, tracks: Vec<TrackRef>, albums: Vec<AlbumRef>, artists: Vec<ArtistRef>, missing: Vec<String /* uri */>` |

Rules (all implementors):

1. Exactly one reply per `request_id`, in any order, never on the
   real-time path; replies are plain data with no secrets.
2. `request_id` is opaque to the source; the host encodes generation +
   kind into it (data-model §2.1).
3. A request while the source has no session ⇒ `Err(CatalogError::Offline)`
   within one `poll()` cycle (no hang).
4. Errors are classified: I/O-class / `Unavailable` / `DeadlineExceeded` ⇒
   `Offline`; HTTP 429 or `ResourceExhausted` ⇒ `RateLimited{retry_after_ms}`
   (from `Retry-After` when present); 404 / `NotFound` ⇒ `NotFound`;
   endpoint refused (403/410/`Unimplemented`) ⇒ `Unsupported`; anything
   else ⇒ `Unavailable(redacted)` — never the raw error text, never a URL
   with a token.
5. `SearchCatalog` returns only music kinds (FR-003): any hit whose URI is
   not `track|album|artist|playlist` is dropped before reply.
6. `FetchTrackList{ArtistTop}` returns ≤ 10 tracks for `session.country()`.
7. `Hydrated.missing` lists URIs the service no longer knows; the host maps
   a missing *track* to `Availability::Removed`.

## 3. Receiver-crate mapping (`modplayer-audio-source-connect`)

| Command | librespot call(s) | Notes |
|---|---|---|
| `SearchCatalog` | strategy 1: `session.mercury().get("hm://searchview/km/v4/search/…")`; strategy 2: `spclient.get_context("spotify:search:…")` + `HydrateRefs` path | research R2; verification gate V1 |
| `FetchLibrary{Playlists}` | `spclient.get_rootlist` → raw `SelectedListContent` (as `account_read.rs`) → `Playlist::get` per playlist (name, owner, length, revision) | verified in 003 |
| `FetchLibrary{SavedTracks\|SavedAlbums\|FollowedArtists}` | `POST /collection/v2/paging` with hand-encoded `PageRequest`; `"collection"` split by URI prefix; `"artist"` | research R3; gate V2; saved-tracks fallback `get_context("spotify:user:<u>:collection")` |
| `FetchTrackList{Album}` | `Album::get` → discs → tracks (order preserved) → batched hydrate | |
| `FetchTrackList{Playlist}` | `Playlist::get` → `tracks()` (playlist order) → batched hydrate | |
| `FetchTrackList{ArtistTop}` | `Artist::get` → `top_tracks.for_country(country)` → take 10 → hydrate | |
| `HydrateRefs` | `spclient.get_extended_metadata(BatchedEntityRequest)` with `TRACK_V4`/`ALBUM_V4`/`ARTIST_V4`, parsed as `librespot_metadata::{Track,Album,Artist}` | research R4; gate V3 |

Availability mapping (gate V4): `Track.availability`/`restrictions` for
`session.country()` ⇒ `UnavailableRegion`; missing from
`Hydrated`/`NotFound` ⇒ `Removed`; otherwise `Available`.

Artwork: `Album.covers` / `Artist.portraits` / playlist `picture` `FileId`
⇒ `https://i.scdn.co/image/<hex>` (template read from the `image-url`
user attribute at session start; gate V5).

Threads: every command runs as a `runtime.spawn`ed task on the worker's
tokio runtime (as `ListAccountTracks` does at `worker.rs:620-631`), never
on the command loop, never on the RT thread. Concurrency cap: 4 in-flight
catalog tasks; further commands queue FIFO (a `tokio::sync::Semaphore`).

## 4. Scripted double (`ScriptedHost`)

`script_search(request_matcher, reply)`, `script_library(set, pages: Vec<Result<LibraryPage, CatalogError>>)`,
`script_track_list(source, reply)`, `script_hydrate(reply)`, `script_catalog_delay(Duration)`
— replies are queued and delivered on the next `poll()` after the scripted
delay (using the host-injected clock), so debounce/race/rate-limit
scenarios are deterministic. A `record_commands()` accessor exposes the
commands received, for asserting "no request issued" (FR-001, FR-005,
FR-018).

`SyntheticHost` answers every catalog command with `Err(Unsupported)`.

## 5. Tests pinning this contract

- `modplayer-audio-source/src/catalog.rs` unit tests: id newtypes, initials
  helper is *not* here (UI), `request_id` packing round-trip.
- `modplayer-audio-source-connect/tests/catalog_mapping.rs`: fixture
  protos/JSON → refs (explicit, release date, availability, non-music
  dropped, `collection` split, error classification without leaking text).
- `modplayer-audio-source-connect/tests/live.rs`: `search_probe`,
  `library_sets_probe`, `availability_probe` — all `#[ignore = "manual"]`.
- `modplayer-audio-source-synthetic/tests/host.rs`: scripted catalog
  replies honour delay/order; `Unsupported` from synthetic.

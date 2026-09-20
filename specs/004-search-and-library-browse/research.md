# Research: Catalog Search and Library Browsing

**Feature**: 004-search-and-library-browse | **Date**: 2026-09-16 | **Plan**: [plan.md](plan.md)

Every decision below was taken headlessly against the spec, the constitution,
the 003 plan/contracts (the shipped *session-sourced read* pattern), the
librespot 0.8.0 sources vendored in the Cargo registry, and the existing
crates. Where a protocol surface could not be verified offline, the decision
names a **verification gate** (a `#[ignore = "manual"]` live probe) and the
fallback the design already accommodates — no host-side code depends on
which probe wins.

Technical Context carried no `NEEDS CLARIFICATION` markers; the items below
are the dependency/integration research the plan template requires plus the
protocol unknowns forced by the single-credential constraint recorded in
003 (spec Assumptions: "the public per-track/library Web API is known to
return HTTP 429 under this project's single desktop-receiver credential …
and MUST NOT be relied on as the primary path").

---

## R1. Where catalog/library reads execute: extend the `SourceHost` seam additively

**Decision**: All catalog and library reads (search, library sets, album /
playlist / artist track lists) are issued as new **`SourceCommand`** variants
and answered by new **`SourceEvent`** variants, exactly like the shipped
`ListAccountTracks` → `AccountTracks` pair from 003's Amendment. The receiver
crate (`modplayer-audio-source-connect`) fulfils them on its tokio worker
against the same `librespot_core::Session` that streams audio; the
`ScriptedHost` test double scripts replies; `SyntheticHost` answers
`Err(CatalogError::Unsupported)`. Payload types live in a new
`modplayer_audio_source::catalog` module (dependency-free `std` types).

**Rationale**:
- Constitution IV: only `audio-source-connect` may speak the protocol, and
  only the binary may depend on it. `modplayer-core` therefore cannot call
  librespot; the only sanctioned path core → protocol is the
  `SourceHost` seam that 003 already threads through `PlaybackController`.
- Constitution X: no new trait is needed — `SourceHost` already has three
  implementors and a command/event mpsc pair; adding a second
  `CatalogHost` trait would duplicate the worker thread, the channel and
  the `App` generic parameter for no gain.
- Session reuse: the spclient/mercury reads need the *same* authenticated
  session the receiver already holds (003 R8 amendment: a Keymaster
  session token is the only credential that works). One session, one
  worker.
- Precedent: `ListAccountTracks` proved the pattern end-to-end in 003
  T098 M1 (live, real Premium account).

**Alternatives considered**:
- *New `modplayer-catalog` crate with its own `librespot` session* —
  violates Constitution IV (second protocol speaker), doubles the login
  and the credential read.
- *Separate `CatalogHost` trait + second generic parameter on
  `PlaybackController`/`App`* — Constitution X: a second seam with the same
  three implementors; rejected as ceremony.
- *Public Web API through `modplayer-account`* — the 003 R9 reads exist
  but 429 under the single credential; kept only as what was tried, not
  used.

**Consequence**: `SourceHost` grows by 5 commands / 5 events (see
[contracts/catalog-source.md](contracts/catalog-source.md)). The controller
routes catalog events to the new `modplayer-core::library` and
`modplayer-core::search` state rather than to the transport reducer, as it
already does for `AccountTracks`.

---

## R2. Catalog search over the session — strategy list with a verification gate

**Decision**: `SourceCommand::SearchCatalog { request_id, query, kinds, offset, limit }`
is fulfilled by `account_read::search` in the receiver crate using an
**ordered strategy list**, first-success wins, each normalised into the
same `SearchPage` value:

1. **Mercury searchview** — `session.mercury().get("hm://searchview/km/v4/search/{q}?entityVersion=2&limit={n}&offset={o}&catalogue=&country={cc}&locale=en&username={u}&imageSize=large")`,
   JSON body with `results.{tracks,albums,artists,playlists}.hits[]`. Returns
   all four groups in one round trip. (`MercuryManager::get` is public in
   librespot-core 0.8.0 `mercury/mod.rs:79`; `session.country()`/`username()`
   exist at `session.rs:569/527`.) Known to have worked for librespot-java /
   librespot-python's `SearchManager`; **not verifiable offline for 2026**.
2. **Context-resolve search** — `session.spclient().get_context("spotify:search:{q+}")`
   (documented in librespot-core 0.8.0 `spclient.rs:862-878`: "search:
   `spotify:search:<search+query>`"). Returns **tracks only** (one
   `ContextPage` of `ContextTrack{uri}`), which are then hydrated through
   batched extended metadata (R4). Used when (1) fails; supplies the
   Tracks group and reports `Albums/Artists/Playlists` as
   `GroupStatus::Unsupported` so the UI omits them.

Strategy (1)'s viability is a **verification gate**: task order must put
`crates/modplayer-audio-source-connect/tests/live.rs::search_probe`
(`#[ignore = "manual"]`, real Premium account) before any UI wiring that
assumes four groups, and the outcome is recorded back here. If (1) is dead,
a third strategy (pathfinder `searchDesktop` GraphQL via
`api-partner.spotify.com` with `spclient.client_token()` + a
`token_provider()` bearer and a persisted-query hash) is the documented
next candidate — it is *not* implemented in this slice unless the probe
fails, because its query hash tracks the official client's release train.

**Rationale**: the spec's four-group contract needs a grouped endpoint; the
only grouped endpoint reachable with the session credential is the
searchview one. Keeping the strategies inside one receiver-crate function
lets the host stay oblivious (Constitution IV) and lets the probe result
change *one* file.

**Alternatives considered**: Web API `GET /v1/search` — 429 (003 R8);
local index search — explicitly out of scope (spec FR-023).

**Non-music exclusion (FR-003)**: the request only asks for the four
music kinds (`catalogue`/`kinds`), and the normaliser drops any hit whose
URI is not `spotify:track:`, `spotify:album:`, `spotify:artist:` or
`spotify:playlist:` — podcasts/episodes/shows/audiobooks never map.

---

## R3. Library sets over the session — collection v2 paging with context-resolve fallback

**Decision**: `SourceCommand::FetchLibrary { request_id, set, page_token }`
for `LibrarySet::{SavedTracks, SavedAlbums, FollowedArtists, Playlists}`:

- **Playlists** → `spclient.get_rootlist(from, len)` parsed as the raw
  `playlist4_external::SelectedListContent` proto exactly as
  `account_read.rs` already does (verified live in 003 T098 M1), then
  `Playlist::get` per playlist for name/owner/length. `owner ==
  session.username()` ⇒ `editable`.
- **SavedTracks / SavedAlbums / FollowedArtists** → `POST /collection/v2/paging`
  with the `collection2v2.PageRequest { username, set, pagination_token, limit }`
  proto (`set = "collection"` for tracks+albums, `"artist"` for followed
  artists), response `PageResponse { items[]{uri, added_at, is_removed}, next_page_token }`.
  `librespot-protocol` 0.8.0 does **not** compile `collection2v2.proto`
  (checked `build.rs`), so the receiver crate hand-encodes these two tiny
  messages with `protobuf::CodedOutputStream`/`CodedInputStream` (already a
  dependency) — ~80 LOC, no build-script or new dependency. `"collection"`
  mixes `spotify:track:` and `spotify:album:` URIs; the receiver splits
  them by URI prefix.
- **Fallback for SavedTracks** if the paging endpoint is refused:
  `spclient.get_context("spotify:user:{username}:collection")` — documented
  in librespot 0.8.0 `spclient.rs:858-860` ("liked songs: all"). No
  documented fallback exists for saved albums / followed artists; those sets
  then report `Err(CatalogError::Unsupported)` and the Library section
  shows its own empty copy with the sync status still honest (`last
  outcome: partial`).

**Verification gate**: `live.rs::library_sets_probe` (`#[ignore]`) runs all
three collection sets and the rootlist before the sync scheduler is wired.

**Rationale**: the collection v2 service is what the official desktop
client uses for "Liked Songs"/"Your Library" and is reachable on the
spclient host with the session's own bearer (the same `request_with_protobuf`
plumbing `get_extended_metadata` uses). It gives `added_at`, needed for
a stable "date added" order and for later slices' sort.

**Alternatives considered**: `your_library` esperanto service — not a
network endpoint (in-client Cosmos); Web API `/v1/me/tracks` etc. — 429.

---

## R4. Hydrating identities into rows — batched extended metadata

**Decision**: Every place that yields bare URIs (collection pages, playlist
track lists, album discs, artist top tracks, context-resolve search) is
hydrated with **one** `spclient.get_extended_metadata(BatchedEntityRequest)`
per ≤ 100 URIs (`ExtensionKind::TRACK_V4`, `ALBUM_V4`, `ARTIST_V4`), parsed
with the typed `librespot_metadata::{Track, Album, Artist}` message types.
`Track.is_explicit`, `Album.date` (release date), `Album.covers`
(artwork `FileId`), `Track.restrictions`/`availability` (→
`Availability::UnavailableRegion` when the session country is not in the
allowed list, `Removed` when the metadata call returns not-found) map onto
the extended `TrackRef` (data-model §1). Artist top tracks:
`Artist::get` → `top_tracks.for_country(session.country())`, capped at 10.

**Rationale**: `account_read.rs` currently does one `Track::get` per track
(fine for 20, not for a 50-track playlist or a 50 000-track first sync).
The batched endpoint is already wrapped by librespot (`spclient.rs:580`).

**Alternatives**: per-track `Track::get` — N round trips; rejected for
scale.

**Sync-scale note**: the first sync of a 50 000-track library is 500
metadata batches. The scheduler (R6) hydrates **pages lazily**: the index
stores identity + `added_at` for every item immediately (so counts and
scrolling work), and hydrates rows on demand as they scroll into view plus
a background sweep at ≤ 2 batches/s. This is what keeps the 300 ms /
no-degradation budgets independent of library size.

---

## R5. "Online" / "offline" for search gating and sync triggers

**Decision**: There is no connectivity monitor in 001–003 (grep: none).
This slice derives connectivity from what exists: **online ⇔ the source
health is `SourceHealth::Ok` and the receiver has reported `Registered`**;
`Transient`/`Unavailable` ⇒ offline. A catalog reply of
`Err(CatalogError::Offline)` (I/O-class librespot error) also flips the
derived fact to offline until the next `Health(Ok)`. The
`Transient → Ok` transition is the "regained connectivity" trigger for
FR-011's immediate sync.

**Rationale**: matches the spec's own usage ("while online" = the service
is reachable), reuses 003's health classification (`health.rs`) and avoids
a platform network-reachability adapter (Constitution X).

**Alternatives**: OS reachability APIs — three platform adapters for a fact
the session already knows; rejected.

---

## R6. Library index persistence and sync scheduling (host-owned, `modplayer-core`)

**Decision**: `modplayer-core::library` owns:
- `LibraryIndex` — in-memory mirror of the four sets + per-entity refs,
  persisted to `<data_local_dir>/ModPlayer/library/index.json` (`serde_json`,
  already a workspace dependency; written atomically temp-file + rename like
  001's settings store), loaded on a background thread at launch so the UI
  never blocks; a load in progress renders skeleton rows (FR-016).
- `SyncScheduler` — pure state machine with an injectable clock (002/003
  pattern): triggers `Launch`, `Interval(15 min)`, `Reconnect`; issues
  `FetchLibrary` commands one set at a time; merges pages into the index;
  records `last_synced_at`, `last_outcome ∈ {Ok, Partial, RateLimited,
  Failed, Never}`. Rate-limited replies (`CatalogError::RateLimited{retry_after}`)
  set the `refreshing…` flag and schedule a backed-off retry; any other
  failure with a prior snapshot is silent (FR-011); no snapshot + failure ⇒
  `FirstSyncFailed` (FR-021).
- Recently Played `PlayLog` — `library/play_log.json`; `record(track, now)`
  keeps `last_played` for every track ever played (unbounded map of
  `TrackId → timestamp`, ~100 B/entry) and derives the 100-entry view.

**Rationale**: EC-2.6 names "local index"; core services are "off the
real-time path" (Constitution I) and are where 001–003 put persisted
host state. JSON at 50 000 tracks is ~15 MB and loads in well under a
second off-thread; a binary format is a later optimisation with no
contract impact.

**Alternatives**: SQLite (`rusqlite`) — a new C dependency and `unsafe`
FFI crate for a read-mostly mirror; TOML — line-oriented, slow at 50 k
rows; both rejected under Constitution X's "why is std/an existing
dependency insufficient".

**Directory**: `directories::ProjectDirs::from("", "ModPlayer", "ModPlayer").data_local_dir()`
(settings stay in `config_dir()`); account-scoped: the index file is deleted
on sign-out / session revocation (FR-8.1.6 spirit; `clear_for_sign_out`).

---

## R7. Search-as-you-type state (host-owned, `modplayer-core::search`)

**Decision**: `SearchSession` is a pure struct: `query`, `debounce_deadline`
(150 ms after the last edit, injectable clock), `generation: u64`
(incremented per issued query; replies carry the generation via
`request_id` and older ones are dropped on arrival), four `GroupState`s
(`Pending | Loaded{items, next_offset, exhausted} | Empty | Unsupported |
RateLimited`), and a `refreshing` flag. `tick(now)` returns the
`SourceCommand`s to issue. "Show more" issues a per-group page request
with `offset = items.len()`, `limit = 20`.

**Rationale**: keeps every rule in FR-001/002/015/018 unit-testable
without egui or a network, as 003 did for the queue and transport
reducer.

---

## R8. Artwork: public CDN URL, fetched and decoded in `modplayer-ui`

**Decision**: the receiver maps an image `FileId` to
`https://i.scdn.co/image/<hex file id>` (the template librespot itself
reads from the `image-url` user attribute, `spclient.rs:845-853`) and puts
it in `TrackRef.artwork_url` / `AlbumRef.artwork_url` etc. `modplayer-ui::artwork`
owns an `ArtworkCache`: a bounded worker pool (2 threads) that fetches with
**`ureq`** (already a workspace dependency, used by 002) and decodes with
**`image`** (`default-features = false, features = ["jpeg"]`) into
`egui::ColorImage` → `TextureHandle`, LRU-bounded (256 textures), with
in-flight de-duplication and a negative cache for failures (→ initials
placeholder, FR-020). Only visible rows request artwork.

**Rationale**: the CDN URL is public (no credential, no protocol), so
fetching it outside the receiver crate does not breach Constitution IV;
`std` cannot decode JPEG, hence one new dependency (`image`, MIT OR
Apache-2.0; its `zune-jpeg` backend is MIT OR Apache-2.0 OR Zlib —
`deny.toml`'s allow-list already covers it). `ureq` is new *to the ui
crate* but not to the workspace.

**Alternatives**: `spclient.get_image` inside the receiver (routes pixel
bytes through the `SourceEvent` channel — adds a large-payload event and
keeps the receiver in the UI's cache lifetime); `egui_extras` `image`
loader (pulls the full `image` feature set and `ehttp`; heavier). Both
rejected.

---

## R9. Virtualised lists in egui 0.36

**Decision**: every list uses `egui::ScrollArea::vertical().show_rows(ui, row_height, total_rows, |ui, range| …)`
with a fixed row height (56 px track rows, 72 px album/artist/playlist
rows) so only the visible range is laid out; the same `ListRow` widget
renders the range for Search groups, Library sections and detail views
(FR-004/FR-014). Focus handling: each row is a single focusable
`egui::Response` (`sense(click)`, `Role::ListItem`), Enter ⇒ Play now,
`Menu`/`Shift+F10`/right-click ⇒ the per-row actions menu
(`Role::Menu`, accessible name "Actions for <name>").

**Rationale**: `show_rows` is egui's built-in virtualisation and is
what SC-002 (50 000 rows) needs; fixed heights make the row index ⇄ scroll
offset mapping O(1).

**Alternative**: `show_viewport` with hand-rolled row math — no benefit
for uniform rows.

---

## R10. Extending `TrackRef` and `Availability` (shared with 003)

**Decision**: additive field growth on `TrackRef` (`explicit: bool`,
`release_date: Option<ReleaseDate{year, month?, day?}>`,
`artist_ids: Vec<ArtistId>`, `album_id: Option<AlbumId>`) and a widening of
`Availability` to `{Available, UnavailableRegion, Removed}`.
`Availability::Unavailable` is referenced in exactly two places
(`modplayer-account/src/spotify.rs:509,813`, the R9 Web-API mapper), both
updated to `UnavailableRegion`. `TrackRef::new` keeps its signature by
taking a `TrackRefExtras` default (`..Default::default()`) so 003 call sites
compile unchanged except the two mapper lines. New ref types `AlbumRef`,
`ArtistRef`, `PlaylistRef` per DM-3 live next to `TrackRef` in the trait
crate so `SourceEvent` can carry them.

**Rationale**: FR-008 requires the extra fields; DM-2's availability enum
has three values; keeping the types in the dependency-free trait crate is
what lets core, connect and account all name them (003's own rationale in
`types.rs`).

---

## R11. Recording Recently Played at "audio output begins"

**Decision**: the controller records a `PlayLog` entry on the transport
reducer's transition into `Active{ intent: Playing, buffering: false }`
for a track id that differs from the last recorded one **within the same
`TrackStarted` generation**, i.e. when `SourceEvent::Playing` follows
`TrackStarted`. Tracks marked unavailable (`SourceEvent::Unavailable`) never
reach that state; queued/browsed tracks never do either. Transfer-in tracks
go through the same `TrackStarted → Playing` path, so they are recorded
(FR-012).

**Rationale**: the reducer already derives `buffering` from
`Loading`/`Playing` (003 design note 3); "audio output begins" is exactly
`Playing` after `TrackStarted`. No engine change needed.

---

## R12. Removing the 003 developer scaffold (FR-022)

**Decision**: delete `crates/modplayer-ui/src/settings/developer.rs`'s
"Play from account" state/button and the `developer.play_from_account`
settings-registry descriptor; keep `SourceCommand::ListAccountTracks` /
`SourceEvent::AccountTracks` and `account_read::fetch_account_tracks` for
one release as the *implementation* behind `FetchLibrary{Playlists}`'s
first-playlist read path is refactored out of it, then remove the pair in
the same PR once `Playlists` + `FetchTracks` cover it (the scripted host's
`script_account_tracks` and the 003 UI tests move to the new commands).
The `playback.ftl` keys for the scaffold are removed and `fluent_keys.rs`
updated.

---

## R13. Dependency additions (Constitution X justification)

| Dependency | Crate | Why `std`/existing is insufficient |
|---|---|---|
| `image` 0.25 (`default-features = false`, `jpeg`) | `modplayer-ui` | JPEG decoding for artwork; `std` has no image codec. Licence MIT OR Apache-2.0 (backend `zune-jpeg` MIT OR Apache-2.0 OR Zlib — allow-list already covers MIT). |
| `ureq` (workspace, existing) | `modplayer-ui` (new user) | Artwork HTTP GET; already the workspace's blocking HTTP client (002). |
| `serde_json` (workspace, existing) | `modplayer-core` (new user) | Library index / play log persistence; `toml` is unsuited to 50 k-row arrays. |
| `serde` (existing) | `modplayer-audio-source` | **Not added** — the trait crate stays dependency-free; core wraps refs in its own serde DTOs (`library::persist`). |
| `protobuf` (existing) | `modplayer-audio-source-connect` | Hand-encoded `collection2v2` messages (already a dependency). |

No new crates in the workspace. No feature flags.

---

## R14. Open verifications (recorded, gated, non-blocking for the design)

| # | What | Gate | Fallback already designed |
|---|---|---|---|
| V1 | `hm://searchview/km/v4/search` still answers a Keymaster session in 2026 | `live.rs::search_probe` | R2 strategy 2 (tracks only) + Complexity Tracking row |
| V2 | `/collection/v2/paging` accepts the session bearer for sets `collection`/`artist` | `live.rs::library_sets_probe` | R3 context-resolve fallback for saved tracks; `Unsupported` for the other two |
| V3 | `get_extended_metadata` batch size ceiling (assumed 100) | same probe | halve batch size on `InvalidArgument` |
| V4 | `Track.availability`/`restrictions` correctly distinguish region-lock from removal for the session country | `live.rs::availability_probe` with a known region-locked track | mark as `UnavailableRegion` when any restriction applies; `Removed` only on not-found |
| V5 | `i.scdn.co` image template still matches the `image-url` user attribute | read the attribute at session start; log a warning (no secrets) if it differs and use the attribute's template instead | template from attribute |

### R14 outcomes — live probe run 2026-09-17 (T079)

Run against a signed-in Premium account (session country `BR`, token from
the app's own Keychain entry) with
`MODPLAYER_LIVE=1 cargo test -p modplayer-audio-source-connect --test live -- --ignored search_probe library_sets_probe catalog_wire_probe restrictions_probe`.
`catalog_wire_probe` and `restrictions_probe` are new diagnostics added by this run: the two
strategy probes only see the receiver's *classified* `CatalogError`, which
by contract hides which strategy answered and why the other refused, so the
wire probe drives a raw `Session` and prints only `ErrorKind`/status and
counts.

| # | Outcome | Evidence | Consequence |
|---|---|---|---|
| V1 | **Refuted.** `hm://searchview/km/v4/search` no longer answers: the mercury request fails with `ErrorKind::Unavailable` immediately and after a 2 s settle. Strategy 2 (`spotify:search:<q>` context-resolve) answered every time. | `search_probe`: `group Track: 20 hit(s)`, `unsupported: [Album, Artist, Playlist]`; `catalog_wire_probe`: `searchview: mercury error: kind=Unavailable` | Search shows the Tracks group only; Albums/Artists/Playlists are reported `unsupported` and omitted (the Complexity Tracking row's documented deviation from FR-001's four groups is now the actual behaviour). Documented next strategy remains the pathfinder `searchDesktop` GraphQL path — out of scope for this feature. |
| V2 | **Refuted.** `POST /collection/v2/paging` answers HTTP 400 (`ErrorKind::InvalidArgument`) for both `collection` and `artist`, with every variant tried: `Content-Type` `application/x-protobuf` / `application/protobuf`, with and without `Accept`, with and without spclient's `product=0&country&salt` decoration, resolved AP host and `spclient.wg.spotify.com`, body with/without `pagination_token`, `limit`, `username`; `client-token` header was present (`client_token: ok`). | `catalog_wire_probe`: `collection2v2 set=collection: kind=InvalidArgument (Response status code: 400 Bad Request)`, same for `artist` | The R3 fallback `spotify:user:<u>:collection` context **works** (`pages=1 tracks=395`), so `SavedTracks` is served by it (one unpaginated page, no `added_at`). `SavedAlbums`/`FollowedArtists` report `Unsupported` → sync cycle *partial*, sections empty. **Defect found and fixed**: the fallback was gated on `CatalogError::Unsupported` only, but a 400 classifies as `Unavailable`, so before the fix all three sets failed and the whole cycle was marked failed (FR-021 "first sync failed") despite Playlists working — `collection.rs::strategy_refused` now treats any non-transient error as a refused strategy. After the fix: `SavedTracks: 395 item(s)`, `SavedAlbums: unsupported`, `FollowedArtists: unsupported`, `Playlists: 24 item(s)`. |
| V3 | **Not exercised.** Neither probe issues a ≥ 100-id `get_extended_metadata` batch; the 395-track fallback page hydrates lazily per visible rows in the app. | — | Halving on `InvalidArgument` stays as designed; verify during the M7 manual walk (scroll Saved Tracks) if a hydrate failure is ever observed. |
| V4 | **Partially verified — and the mapping was wrong.** `availability_probe` (needs a known region-locked URI) was not run, but the new `restrictions_probe` dumped the raw `Restriction`s of the first 30 saved tracks: every track the app greyed as "Unavailable in your region" (≈ ⅓ of the library, including Brazilian releases for a `BR` session) carried exactly one restriction with **no catalogue** and an **empty `countries_allowed`** — the shape of a *relinked* track (retired id, playable through `Track.alternatives`), which the official client plays silently. The receiver applied every restriction regardless of catalogue. | `restrictions_probe`: `catalogues=[] strs=[] type=STREAMING allowed(len,has_country)=Some((0, false))` on each greyed track; 0 restrictions on the rest | `hydrate.rs::availability_from` now applies only restrictions whose `catalogue_strs` name the session's catalogue (`user_data().attributes["catalogue"]`, default `"premium"`), mirroring librespot-playback's `allowed_for_user`; fixtures in `catalog_mapping.rs::availability_*` pin the relinked and other-tier shapes. Relinked tracks additionally start under the alternative's uri, which `events.rs`/`program.rs::resolve` now attribute to the expected program slot. Re-run `availability_probe` with a genuinely region-locked URI when one is known. |
| V5 | **Not exercised** by these probes (no artwork request in the probe path). | — | Unchanged. |

Manual-scenario impact (quickstart.md): M1's "groups in order Tracks, Albums,
Artists, Playlists" is expected to show **Tracks only** with the live
service; M5 (album/playlist from search) is therefore unreachable from
Search until a grouped search strategy lands, though album/playlist detail
from Library still works. M7's Saved Tracks tab is served by the context
fallback (no `added_at` ordering source, single page); Saved Albums and
Followed Artists tabs render their empty state.

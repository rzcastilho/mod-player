# Contract: Host-side search, library index, sync, play log (`modplayer-core`)

**Modules**: `search.rs` (new), `library/{mod.rs, index.rs, sync.rs, play_log.rs, persist.rs, connectivity.rs}` (new), `queue.rs` (+2 methods), `controller.rs` (routing + new commands)
**Research**: R5, R6, R7, R11
**Extends**: 003 `contracts/transport-and-queue.md` §1 (controller surface), additive.

## 1. `PlaybackController` additions

| Method | Effect |
|---|---|
| `search(&self) -> &SearchSession` / `search_mut(&mut self)` | read/edit query; `tick()` issues commands per `SearchSession::tick` |
| `search_show_more(kind)` | page request for one group |
| `library(&self) -> &LibraryIndex` | read the snapshot (never blocks; may be `loading`) |
| `library_status(&self) -> LibraryStatus { loading, refreshing, first_sync_failed, connectivity }` | drives FR-015/016/021 states |
| `library_retry_sync()` | FR-021 retry trigger |
| `library_track_list(source) -> TrackListState { Cached(Vec<TrackRef>) \| Loading \| Failed(CatalogError) }` | issues `FetchTrackList` on first miss; caches in `LibraryIndex.track_lists` |
| `library_hydrate_visible(ids)` | UI hint: prioritise these ids in the hydration sweep |
| `recently_played(&self) -> &[TrackRef]` | ≤ 100, newest first |
| `queue_replace_at(tracks, cursor)` | `Queue::replace_context_at` — **Play now** on a track row (FR-006) |
| `queue_play_next_tracks(tracks)` | `Queue::play_next_tracks` — **Play next** on any row (FR-007) |
| `queue_add_context(tracks)` | existing `Queue::add_context` — **Add to queue** (FR-007) |
| `on_connectivity(Connectivity)` (internal) | derived in `drain_source_events` from `Health`/`Registered` (R5) |

`tick()` additionally: runs `SearchSession::tick(now)` and
`SyncScheduler::tick(now, connectivity)`, forwarding the returned
`SourceCommand`s; routes `SearchResult`/`LibraryPage`/`TrackList`/`Hydrated`
events by `request_id` to search or library (never to the transport
reducer); records `PlayLog` on the `TrackStarted → Playing` transition
(R11); flushes dirty persistence ≤ 1 s later on the persistence thread.

`clear_for_sign_out()` additionally clears the index, play log and
search state and deletes `library/index.json` + `play_log.json`.

## 2. `Queue` additions (contract unchanged otherwise)

- `replace_context_at(tracks: Vec<TrackRef>, cursor: usize) -> QueueChange`
  — `replace_context` then cursor moved to `cursor` (clamped); `Restart`.
- `play_next_tracks(tracks: Vec<TrackRef>) -> QueueChange` — repeated
  `play_next_track` preserving order (FIFO tail of the play-next block).

Proptest extension: the existing queue operation-sequence proptest gains
both operations.

## 3. Acting-list rule (FR-006/007) — implemented in `modplayer-ui::rows::acting_list`

| Row | Acting list | Cursor |
|---|---|---|
| Track in `SearchTracks`/`SavedTracks`/`RecentlyPlayed`/`AlbumTracks`/`PlaylistTracks`/`ArtistTop` | rows **currently loaded** in that list, displayed order | that track |
| Album / Playlist | full track list (`library_track_list`, fetched if needed; in-place loading indicator on the action) | 0 |
| Artist | top tracks (≤ 10) | 0 |

`PlayNow` ⇒ `queue_replace_at` + `play()`; `PlayNext` ⇒ `queue_play_next_tracks`;
`AddToQueue` ⇒ `queue_add_context`. Unavailable rows are included
unchanged — 003's skip-with-notice applies at play time (FR-019).

## 4. Timing constants (internal, not contract — spec Assumptions)

| Constant | Value |
|---|---|
| `SEARCH_DEBOUNCE` | 150 ms |
| `SEARCH_PAGE` | 20 |
| `SYNC_INTERVAL` | 15 min |
| `SYNC_BACKOFF` | 15 s × 2^attempt, max 4 min |
| `SYNC_PAGE` | 200 items |
| `HYDRATE_BATCH` | 100 ids, ≤ 2 batches/s background sweep |
| `RECENT_WINDOW` | 100 |
| `PERSIST_DEBOUNCE` | 1 s |

## 5. Persistence files

| File | Content | Written |
|---|---|---|
| `<data_local_dir>/ModPlayer/library/index.json` | data-model §3.4 | after each completed set, on shutdown; atomic |
| `<data_local_dir>/ModPlayer/library/play_log.json` | data-model §3.5 | ≤ 1 s after a change, on shutdown; atomic |

Both readable only by the user (`0o600` on Unix); contain metadata only —
no credentials, no audio (Constitution V/VI unaffected).

## 6. Tests pinning this contract (`modplayer-core`)

- `tests/search_session.rs`: debounce (149 ms no request / 150 ms request),
  trim + empty ⇒ no request and reset, stale-generation discard, group
  order + omission, show-more offsets, rate-limit keeps stale + refreshing,
  offline ⇒ no request, in-flight + offline ⇒ discarded.
- `tests/library_sync.rs`: launch/interval/reconnect triggers, page
  walking, rate-limit backoff, silent failure with snapshot, first-sync
  failed without, resume after offline, partial outcome on `Unsupported`.
- `tests/library_index.rs`: merge semantics, lazy hydration queue, 50 000 ×
  1 000 fixture load/lookup under budget (SC-002 proxy: index ops ≤ 1 ms).
- `tests/play_log.rs`: record on `TrackStarted→Playing` only; dedupe;
  100-window; `last_played` outlives the window; restart round-trip;
  transfer-in recorded; unavailable-skipped not recorded.
- `tests/persist.rs`: atomic write, crash-mid-write keeps prior, newer
  schema ⇒ empty + one warning, sign-out deletes; **proptests**
  (Constitution VIII, state serialization)
  `index_file_round_trips_any_index` — `proptest!` over arbitrary
  `LibraryIndex` contents (every set, refs incl. Unicode names, optional
  fields, every `Availability`, sync meta timestamps): save → load ==
  original; `play_log_file_round_trips_any_log` — arbitrary
  `last_played`/`play_count`/`first_played` maps and `refs`: save → load
  == original and `recent` re-derives identically.
- `tests/queue_proptest.rs` (extended) and `tests/controller_streaming.rs`
  (extended: catalog event routing, connectivity derivation).

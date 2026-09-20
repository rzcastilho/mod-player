# Contract: Connect receiver delta (extends 003 contracts/connect-source.md)

**Crate**: `crates/modplayer-audio-source-connect` (only importer of
`librespot-*`, `symphonia`; only `crates/modplayer` depends on it).
Implements FR-010, FR-021; research R2, R3, R9; data-model.md §2.

## 1. Decode-ahead thread (`decode_ahead.rs`)

Spawned by the worker's player-event task on every
`PlayerEvent::TrackChanged { audio_item }` (after stopping the previous
one). Inputs: `session.clone()`, the `AudioItem`, a fresh
`Arc<DecodedStore>` (`len_frames = duration_ms × 44 100 / 1000`),
`stop: Arc<AtomicBool>`, `seek_hint: Arc<AtomicU64>`, the runtime handle.

| Step | Behaviour | On failure |
|---|---|---|
| format | first of `[OGG_VORBIS_160, MP3_160, OGG_VORBIS_96, MP3_96, MP3_256, OGG_VORBIS_320, MP3_320]` present in `audio_item.files` (the `Player`'s Bitrate160 list) | `set_failed` |
| open | `runtime.block_on(AudioFile::open(&session, file_id, bytes_per_second))` then `audio_key().request(track_id, file_id)` (missing key → `None`, as the `Player`) | `set_failed` |
| reader | `Subfile::new(AudioDecrypt::new(key, file), offset = 0xa7 for Ogg else 0, len)` implementing `symphonia::io::MediaSource { is_seekable: true, byte_len: Some(len) }` | `set_failed` |
| probe | `symphonia::default::get_probe().format(hint(mime), MediaSourceStream, ..)`; `get_codecs().make(track.codec_params)` | `set_failed` |
| loop | `next_packet` → decode → `SampleBuffer<f32>` interleaved → `store.write_frames(packet.ts, ..)`; skip frames before the current chunk boundary after a seek; on `Ok(None)`/EOF: jump to lowest unfilled chunk via `seek(Accurate, Time)`; none left → `set_complete(exact_len)` | decode error other than `DecodeError` on a single packet (which is skipped, symphonia convention) → `set_failed` |
| seek hint | checked between packets; if the hinted frame's chunk is unfilled → `seek` to that chunk's start and continue from there | seek error → continue sequentially |
| stop | checked between packets; exits, dropping the `AudioFile` (closes the fetch) | — |

Thread name `decode-ahead`, priority `ThreadPriority::Min` via
`thread-priority` (failure to set → logged, continue). Never touches the
ring, the marker ring, `Spirc`, or the `Player`.

Debug-only toggle (quickstart M15, same pattern as 004's
`MODPLAYER_ARTWORK_FORCE_FAIL`): `MODPLAYER_DECODE_FORCE_FAIL=1` makes the
decode-ahead thread call `set_failed` after the first 5 s of audio
(partial-then-failed) or, with `=0s`, immediately. Read once at worker
spawn; never affects the `Player`.

## 2. Worker / `ConnectSource` changes

- `Marker::track_start(written, store)` — the `TrackStart` marker carries
  the new track's `Arc<DecodedStore>`; pushed **before** the
  corresponding `TrackStarted`/`BecameActive` event is sent.
- After that event: `SourceEvent::DecodedStore { track, store }`.
- `SourceCommand::Seek(ms)` → additionally stores `ms × 44.1` into the
  current decode-ahead's `seek_hint`.
- `SourceCommand::Stop`/`Shutdown`/next `TrackChanged` → `stop` the current
  decode-ahead (join with a 2 s budget on `Shutdown` only).
- **Retirement ring** (`lib.rs`, created next to the sample/marker rings):
  `RingBuffer::<Arc<DecodedStore>>::new(RETIRED_CAPACITY)`, `RETIRED_CAPACITY
  = MARKER_CAPACITY + 1` (65). Producer → `ConnectRtSource::new`; consumer
  → the worker (`retired_rx`), handed over with `sample_tx`/`marker_tx`.
- Worker `command_loop`: `drain_retired(&mut retired_rx)` (`while let
  Ok(store) = retired_rx.pop() { drop(store) }`) runs **immediately before
  every `marker_tx.push`** and once at the top of every loop iteration,
  and once more when the session ends. Because at most one marker is
  pushed between two drains, the RT can never have more than
  `MARKER_CAPACITY + 1` retirements outstanding, so its `push` never
  returns `Full`. The drop that frees a store's chunk table always runs
  here, on the worker thread.
- `ConnectSource` keeps only `current_store` (for `BufferStatus`); there
  is **no** host-side "previous store" bookkeeping and no `track_seq`
  comparison — the RT never drops a store, so no other holder's drop
  order matters.
- `BufferStatus::current_prefetched` now reports `store.state() ==
  Complete` (was hard-coded `false`); no consumer changes behaviour.

## 3. `ConnectRtSource` feed rules (real-time half)

Exactly data-model.md §2.2. Guarantees:

| # | Guarantee | Test |
|---|---|---|
| 1 | `seek(f)` with `store.covers(f)` → the next `fill` returns the store's samples from frame `f` exactly | `rt_seek_into_store_is_sample_exact` |
| 2 | `seek(f)` with `!store.covers(f)` → ring behaviour identical to 003 (`seek_discards_readable_ring_content` still passes) | existing + `rt_seek_outside_store_uses_ring` |
| 3 | on `Store` feed, the ring is drained by exactly the delivered frame count when available (never blocks, never waits) | `rt_store_feed_drains_ring_in_lockstep` |
| 4 | `Reposition` markers never move `cursor` on `Store` feed | `rt_reposition_marker_ignored_on_store_feed` |
| 5 | `TrackStart` marker → `feed = Ring`, `cursor = 0`, store swapped; the replaced `Arc` is moved into the retirement ring (its strong count is unchanged by the RT) | `rt_track_start_swaps_store_retires_old` |
| 5a | The RT never releases the last reference to a store: with every other `Arc` to A, B and C already dropped, applying `TrackStart` A→B→C (in one `fill` or across several) leaves `Weak::upgrade` succeeding for each until the worker drains the ring | `store_drop_never_on_rt` |
| 5b | `MARKER_CAPACITY + 1` `TrackStart` markers applied without any drain never hit `Full` (and `parked` stays `None`) | `rt_retire_never_full` |
| 6 | `Ring` underrun with the store covering `cursor` → switches to `Store` and delivers audio in the same `fill` | `rt_store_rescues_ring_underrun` |
| 7 | `Store` feed hitting an unfilled chunk → switches to `Ring` at `ring_pos` (or keeps `cursor` if unknown) and sets `underrun` only if the ring is empty | `rt_store_exhaustion_falls_back_to_ring` |
| 8 | `fill`/`seek` allocate nothing with a store attached (`assert_no_alloc`) | `rt_no_alloc` (extended) |
| 9 | `position()` on `Store` feed equals `cursor` exactly after N fills of M frames | `rt_position_tracks_store_cursor` |

## 4. Threads (delta to 003 §"Threads")

| Thread | Priority | Touches |
|---|---|---|
| `decode-ahead` (one per current track) | below normal | own `AudioFile`, `DecodedStore` (writer) |
| audio callback | RT | ring consumer, marker consumer, retirement-ring producer, `DecodedStore` (reader; never dropped here) |
| worker (`command_loop`) | normal | retirement-ring consumer — the only place a store's last `Arc` is dropped |

## 5. Live probe (`tests/live.rs`, `#[ignore = "manual"]`)

`decode_ahead_fills_store_faster_than_playback`: with the maintainer's
credential, load a track, assert `covered_frames()` reaches the full
length before `consumed_frames` reaches 25 % of it, and that a
`seek_hint` at 75 % makes `covers(75 %)` true within 3 s.

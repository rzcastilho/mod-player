# Data Model: Now-Playing View with Waveform

**Feature**: 005-now-playing-waveform | **Date**: 2026-09-17 |
**Spec**: [spec.md](spec.md) | **Research**: [research.md](research.md)

Crate ownership: §1 `modplayer-audio-source` (trait crate), §2
`modplayer-audio-source-connect` (receiver internals), §3
`modplayer-core::analysis`, §4 `modplayer-core::transport` delta, §5
`modplayer-ui::waveform`, §6 persisted format.

All frame counts are at the source rate (44 100 Hz for Connect; the
synthetic host's rate otherwise). `ms ↔ frames` conversions happen only
at the seams noted (`SourceCommand::Seek` stays ms).

## 1. Retained decoded store (trait crate) — research R1

### 1.1 `DecodedStore`

| Field | Type | Notes |
|---|---|---|
| `sample_rate` | `u32` | constant for the store's life |
| `len_frames` | `AtomicU64` | best-known track length; set from `duration_ms` at creation, refined to the exact decoded length on completion (never grows past the slot table) |
| `state` | `AtomicU8` → `StoreState` | `Filling` → `Complete` \| `Failed` (terminal) |
| `chunks` | `Box<[Chunk]>` | `ceil(min(len_frames, MAX_STORE_FRAMES) / CHUNK_FRAMES)` slots |

Constants: `CHUNK_FRAMES = 65 536`, `MAX_STORE_FRAMES = 33 554 432`
(256 MiB of stereo `f32`).

`Chunk { samples: OnceLock<Box<[AtomicU32]>>, filled: AtomicU32 }` —
`samples.len() == CHUNK_FRAMES * 2`; `filled` = frames valid from the
chunk's first frame (invariant: a chunk is filled from its start, R1/R3).

Operations:

| Method | Caller | Behaviour |
|---|---|---|
| `new(sample_rate, len_frames) -> Arc<Self>` | host writer | allocates the slot table only |
| `write_frames(from_frame, samples: &[f32]) -> usize` | writer thread only | writes contiguous frames; allocates chunks lazily; publishes `filled` with `Release`; ignores frames beyond the cap; returns frames written |
| `set_complete(exact_len_frames)` / `set_failed()` | writer | terminal state; `set_complete` also stores `len_frames` |
| `state() -> StoreState`, `len_frames() -> u64` | any | atomics |
| `covers(frame) -> bool` | RT / any | `chunk(frame).filled > frame % CHUNK_FRAMES` (Acquire) |
| `read_frames(from_frame, out: &mut [f32]) -> usize` | **real-time half only** (guard test) | copies up to `out.len()/2` contiguous covered frames; no alloc/lock |
| `coverage() -> Coverage` | analysis / tests | `Vec<u32>` of `filled` per chunk (lossy) |
| `fold_peaks(from_frame, bucket_frames, out: &mut [PeakBucket]) -> usize` | analysis thread | folds whole, fully covered buckets only; mono-folded min-of-mins / max-of-maxes; returns buckets written |

`Debug` prints `sample_rate`, `len_frames`, `state`, covered-frame count
only. `PartialEq` for `Arc<DecodedStore>` inside `SourceEvent` is
`Arc::ptr_eq`.

### 1.2 `SourceEvent::DecodedStore { track: TrackId, store: Arc<DecodedStore> }` (additive)

Raised by a `SourceHost` immediately after the `TrackStarted` (or
`BecameActive { context: Some(..) }`) whose track the store belongs to.
Hosts that never decode ahead never raise it; the analysis status for
such tracks stays `Pending` unless cached.

### 1.3 `PeakBucket { min: i8, max: i8 }` (trait crate, shared with §3)

Quantisation: `round(clamp(sample, −1, 1) × 127)`.

## 2. Receiver internals (`modplayer-audio-source-connect`) — research R2/R3

### 2.1 `Marker` (existing, extended)

`MarkerKind::TrackStart` now carries `store: Arc<DecodedStore>`;
`Marker` is no longer `Copy` (moved through the `rtrb` marker ring).
`Reposition`/`TrackEnd` unchanged.

### 2.2 `ConnectRtSource` state (RT half)

| Field | Type | Notes |
|---|---|---|
| `cursor` | `u64` | track position played and reported (`position()`) |
| `feed` | `Feed` | `Ring` \| `Store` (research R2 rules) |
| `ring_pos` | `Option<u64>` | ring's marker-derived track position; `None` after a flush until the next `Reposition` |
| `store` | `Option<Arc<DecodedStore>>` | current track's store — the only store `Arc` the RT holds |
| `retired` | `rtrb::Producer<Arc<DecodedStore>>` | retirement ring, capacity `RETIRED_CAPACITY = MARKER_CAPACITY + 1`; the RT *moves* every replaced store into it and never drops one (R1) |
| `parked` | `Option<Arc<DecodedStore>>` | defensive slot for the unreachable `Full` case; re-pushed at the top of every `fill` |
| `samples`, `markers`, `shared`, `track_seq` | as 003 | |

State transitions (`fill` per callback):

```
TrackStart marker  → old := store.replace(marker.store); retired.push(old) (never drop; Full → parked);
                     then track_seq += 1; cursor := 0, feed := Ring
seek(f)            → cursor := f, flush ring, ring_pos := None,
                     feed := if store.covers(f) { Store } else { Ring }
feed == Store      → read store at cursor; pop-and-drop the same frame count from the ring
                     if !store.covers(cursor) at any frame → feed := Ring, cursor := ring_pos.unwrap_or(cursor)
feed == Ring       → 003 behaviour; on underrun, if store.covers(cursor) → feed := Store
Reposition marker  → ring_pos := marker.position_frames; if feed == Ring { cursor := ring_pos }
```

### 2.3 `DecodeAhead` (worker-owned)

| Field | Type | Notes |
|---|---|---|
| `store` | `Arc<DecodedStore>` | |
| `stop` | `Arc<AtomicBool>` | set on next `TrackChanged` / `Stop` / `Shutdown` |
| `seek_hint` | `Arc<AtomicU64>` | `u64::MAX` = none; frame of the last host seek |
| `thread` | `JoinHandle<()>` | below-normal priority (R9) |

`ConnectSource` keeps only `current_store` (`Option<Arc<DecodedStore>>`,
for `BufferStatus::current_prefetched`) and drops it freely on the next
`DecodedStore`/`Stop`. The worker owns the retirement ring's consumer
(`retired_rx: rtrb::Consumer<Arc<DecodedStore>>`, created next to the
sample/marker rings) and pops-and-drops it before every marker push and
once per command-loop iteration — the last reference to a store is
therefore always released on the worker thread (R1).

## 3. Analysis (`modplayer-core::analysis`) — research R4–R7

### 3.1 `AnalysisStatus`

`Pending` → `Partial` → `Complete`; `Pending`/`Partial` → `Failed`.
`Complete` and `Failed` are terminal for the session (a `Failed` track
id is remembered in `AnalysisService::failed: HashSet<TrackId>`).

### 3.2 `WaveformPeaks`

| Field | Type | Notes |
|---|---|---|
| `sample_rate` | `u32` | |
| `len_frames` | `u64` | |
| `levels` | `Vec<PeakLevel>` | finest first: 128, 1 024, 8 192, 65 536 frames/bucket (levels with < 2 buckets omitted) |

`PeakLevel { frames_per_bucket: u32, buckets: Vec<PeakBucket>, present:
Vec<u64> /* bitmap, 1 = bucket fully covered */ }`.
Invariants: `buckets.len() == ceil(len_frames / frames_per_bucket)`;
a coarse bucket is present iff all 8 finer buckets it folds are present
(the last one: all that exist).

### 3.3 `AnalysisSnapshot` (immutable, `Arc`-shared with the UI)

| Field | Type | Notes |
|---|---|---|
| `track` | `TrackId` | |
| `analyzer_version` | `u32` | `ANALYZER_VERSION` |
| `status` | `AnalysisStatus` | |
| `peaks` | `Option<Arc<WaveformPeaks>>` | `None` while `Pending`, for a silent-track `Failed`, and for `Failed` before any bucket existed |
| `from_cache` | `bool` | true when `Complete` came from disk (SC-004 test) |

Beat grid / key / loudness (DM-5) are **not** fields here; they are
future `.mpwf` sections (§6) and future snapshot fields.

### 3.4 `AnalysisService` (owned by `PlaybackController`)

| Field | Type | Notes |
|---|---|---|
| `commands` | `mpsc::Sender<AnalysisCommand>` | `Attach { track, sample_rate, len_frames, store: Option<Arc<DecodedStore>> }`, `Detach`, `Shutdown` |
| `progress` | `mpsc::Receiver<Arc<AnalysisSnapshot>>` | drained in `tick()` |
| `latest` | `Option<Arc<AnalysisSnapshot>>` | last snapshot for the current track |
| `paths` | `AnalysisPaths` | cache dir (R7) |

Thread-side state (`worker.rs`): `attached: Option<Attachment { track,
store: Option<Arc<DecodedStore>>, peaks: WaveformPeaks, next_level0_bucket:
u64, last_publish: Instant }>`, `failed: HashSet<TrackId>`.

Publishing rules: on cache hit → one `Complete { from_cache: true }`;
progressive → at most every 250 ms or per 0.5 s of new audio, status
`Partial`; terminal → `Complete` (after the cache write) or `Failed`.

## 4. Transport delta (`modplayer-core::transport`) — research R8

- `Input::Seek { position_ms: u32, position_frames: Option<u64>, buffer_ready: bool }`
- `Effect::SeekTo { position_ms: u32, position_frames: Option<u64> }` — the
  controller pushes `Command::Seek(position_frames.unwrap_or(ms_to_frames
  (position_ms)))` to the engine and `SourceCommand::Seek(position_ms)` to
  the source.
- `PlaybackController::seek_frames(frame: u64)`; `seek(Duration)` passes
  `None`.
- Reducer rules T6–T8 (clamp at track end, paused stays paused, stopped →
  paused) unchanged; the clamp applies to `position_ms` and, when
  present, to `position_frames` via `track_len_ms`.

## 5. UI state (`modplayer-ui::waveform`) — research R11–R13

### 5.1 `DetailWindow`

| Field | Type | Notes |
|---|---|---|
| `start_frame` | `u64` | `0 ≤ start ≤ len − width` |
| `width_frames` | `u64` | `200 ms × rate ≤ width ≤ len` |
| `follow` | `bool` | default `true` |

Pure methods: `initial(playhead, len, rate)` (30 s centred), `zoom_about
(anchor_frame, factor)`, `zoom_step(playhead, ×2 | ÷2)`, `reset(len)`,
`pan(delta_frames)`, `follow_playhead(playhead, playing)`, `recenter
(frame)`, `suspend_follow_if_outside(playhead)`, `contains(frame)`.

### 5.2 `WaveformState` (session, owned by `App`)

| Field | Type | Notes |
|---|---|---|
| `detail` | `Option<DetailWindow>` | `None` until first shown with a track; width survives track changes (re-centred) |
| `drag` | `Option<DragPreview { target_frame: u64, origin: Overview \| Detail }>` | live preview; cleared on release (commit) or `Esc` (cancel) |
| `last_track` | `Option<TrackId>` | to re-centre on track change |

### 5.3 `TimeSpace` (per draw)

`{ rect: egui::Rect, window: Range<u64>, sample_rate: u32 }` with
`x_of(frame) -> f32`, `frame_at(x) -> u64` (clamped), `frames_per_pixel()
-> f64`, `visible_window() -> Range<u64>`. Returned in `WaveformResponse
{ response: egui::Response, space: TimeSpace }` from both widgets
(FR-014's overlay attachment point).

### 5.4 Derived view values (not stored)

- Playhead frame = `controller.shared()` position via `PositionClock`
  (003), or `drag.target_frame` while dragging.
- Elapsed label = `m:ss` of the playhead; remaining = `-m:ss` of
  `len − playhead` (both from the preview while dragging).
- Overview highlight = `detail` window projected through the overview's
  `TimeSpace`; description text `waveform-detail-window` with `m:ss` of
  its bounds.

## 6. Persisted format `.mpwf` — research R7

Little-endian; see research R7 for the byte layout. Validation on read:
magic, `format_version == 1`, `analyzer_version == ANALYZER_VERSION`,
`id == track`, `|len_frames − expected| ≤ 2 s`, every level's
`bucket_count == ceil(len_frames / frames_per_bucket)`, payload lengths
consistent; a `WAVE` section is required. Files are written only for
`Complete` and always with every level's `present` bitmap all-ones (so
the bitmap is not stored; it is reconstructed as full on load).

Property test (Constitution VIII): `encode(decode(encode(p))) ==
encode(p)` and `decode(encode(p)) == p` for arbitrary `WaveformPeaks`
with 1–4 levels and up to 5 000 buckets; a truncated or bit-flipped
header decodes to `Err`.

## 7. Validation rules traced to requirements

| Rule | Requirement |
|---|---|
| store never exceeds `MAX_STORE_FRAMES`; only the current track's store is held (the replaced store is retired through the ring and freed on the worker thread at its next drain; the RT never drops a store) | FR-021, NFR-2.7 |
| `read_frames` callers ⊆ {`modplayer-audio-source*`, `modplayer-engine`} (guard test) | FR-021, Constitution V |
| only `Complete` snapshots are written; `Failed`/`Partial` never touch disk | FR-006, FR-016 |
| `analyzer_version` mismatch ⇒ entry invalid, file unlinked, re-analysed | FR-007 |
| silence ⇒ `Failed` only when every level-0 bucket has `max − min == 0` after `Complete` | FR-016 |
| `failed` set is per session (memory only) | FR-016 |
| `DetailWindow` bounds `[200 ms, len]`, clamped start | FR-011, FR-012 |
| elapsed `m:ss`, remaining `-m:ss`, no leading-zero minutes | FR-001 |

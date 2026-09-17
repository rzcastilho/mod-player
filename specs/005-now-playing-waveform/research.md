# Research: Now-Playing View with Waveform

**Feature**: 005-now-playing-waveform | **Date**: 2026-09-17 | **Spec**: [spec.md](spec.md)

Every unknown the Technical Context raised is resolved below (none
remain open). Decisions are numbered R1–R16 and referenced from plan.md,
data-model.md, the contracts and quickstart.md. Where the spec's
Clarifications already fixed a value (zoom bounds, bindings, cache
location, ladder constraints) this file only records *how* it is met.

## Starting facts (verified against the tree, 2026-09-17)

- **Decode path today (003)**: librespot 0.8 `Player` decodes on its own
  thread and writes decoded stereo `f32` into `RingSink` → `rtrb` ring
  (8 192 frames ≈ 186 ms) → `ConnectRtSource::fill`. `RingSink::write`
  parks under back-pressure, so *decoding is paced by the audio callback*;
  only the **encrypted** stream is pre-fetched at connection speed
  (`AudioFetchParams.read_ahead_during_playback = 1 h`, research 003 R5).
  No decoded PCM is retained anywhere.
- **Position markers** (`program.rs::Marker`) are stamped with the sink's
  cumulative `written_frames` by the *event task* when it observes a
  `PlayerEvent`; the sink keeps writing meanwhile, so a marker is accurate
  to "a few ms", and `Reposition` markers carry `position_ms` (ms
  precision). The ring's track position is therefore never sample-exact.
- **Seek path today**: `PlaybackController::seek(Duration)` → reducer
  `Input::Seek { position_ms }` → `Effect::SeekTo { position_ms }` →
  engine `Command::Seek(frames)` (RT flushes the ring, sets its baseline)
  + `SourceCommand::Seek(ms)` (Spirc seeks the `Player`, ring refills from
  the new position, a `Seeked` marker re-anchors).
- **Engine**: `Processor::render` already handles `Command::Seek(u64
  frames)` at buffer boundaries; buffer presets 128 / 256 / 1024 frames
  (Performance / Balanced / Safe) at 44.1 kHz = 2.9 / 5.8 / 23.2 ms.
- **Spirc mirrors the `Player`**: `Paused`/`Playing`/`EndOfTrack` player
  events drive Spirc's cluster state and its next-track advance. Any
  design that lets the `Player` run ahead of, or stall behind, the audible
  position breaks Connect state and 003's gapless/queue contracts.
- **librespot public API available for a second decode path**:
  `librespot_metadata::audio::AudioItem` (already in `TrackChanged`),
  `librespot_audio::{AudioFile::open, AudioDecrypt, AudioFetchParams,
  StreamLoaderController}`, `session.audio_key().request(track_id,
  file_id)`, `librespot_playback::decoder::{SymphoniaDecoder,
  AudioDecoder}`. `Subfile` (the 0xa7 Spotify-Ogg-header skip) is private
  to `player.rs` but is ~40 lines. `symphonia` 0.5.5 (`ogg`, `vorbis`,
  `mp3`, `flac`) is already resolved in the tree via `librespot-playback`.
  `PlayerConfig::default()` has `normalisation: false` and
  `Bitrate::Bitrate160`, so the `Player`'s output is the raw decode.
- **UI**: egui 0.36 + accesskit; 003's seek slider lives in
  `crates/modplayer-ui/src/now_playing.rs::show_seek_slider`; 004's
  `ArtworkCache::get` and `widgets::initials::initials_placeholder` exist.
- **Persistence conventions** (004): `ProjectDirs::from("", "ModPlayer",
  "ModPlayer").data_local_dir()/<sub>`, env override for tests, temp-file +
  rename, user-only permissions, JSON via serde for structured state.
- **Threads**: `std` cannot set OS thread priority; no crate in the tree
  does.

---

## R1 — Retained decoded store: where it lives and how the RT reads it

**Decision**: a new `DecodedStore` type in the dependency-free trait crate
`crates/modplayer-audio-source` (module `decoded.rs`). One store **per
track**, allocated by whichever `SourceHost` implementor owns decoding,
shared as `Arc<DecodedStore>` with (a) that host's real-time half, (b) the
host's own decode-ahead writer and (c) the Analysis Service. Layout:

- Fixed slot table `Box<[Chunk]>` sized at creation from the track's
  duration (`ceil(len_frames / CHUNK_FRAMES)`), `CHUNK_FRAMES = 65 536`
  (1.486 s, 512 KiB of stereo `f32` per chunk).
- `Chunk { samples: OnceLock<Box<[AtomicU32]>>, filled: AtomicU32 }` —
  `samples` holds interleaved stereo `f32` **bit patterns** in `AtomicU32`
  (`f32::to_bits`/`from_bits`); `filled` is the number of frames valid
  from the chunk's start. Writer: fill samples with `Relaxed` stores, then
  `filled.store(n, Release)`. Reader: `filled.load(Acquire)`, then
  `Relaxed` sample loads. `OnceLock::get` is a single atomic load once
  initialised (never blocks); allocation of a chunk happens only in
  `OnceLock::get_or_init` on the **writer** thread.
- Header atomics: `len_frames: AtomicU64` (best-known length; refined at
  EOF), `state: AtomicU8` (`Filling | Complete | Failed`), `sample_rate:
  u32`.
- Invariant "valid from the chunk's start" is kept by the writer (R3
  seeks the decoder to a chunk boundary), so coverage is exactly the set
  of `filled` counters — no range list, no gap arithmetic on the RT.
- Cap: `MAX_STORE_FRAMES = 33 554 432` (= 256 MiB / 8 B; 12 min 40 s at
  44.1 kHz). Slots beyond the cap are never allocated; the store then
  can never reach `Complete` (`Partial` in FR-005/021 terms) and seeks
  into the uncovered tail use the ring (streamed) path.
- Sample format **f32** (not i16): bit-identical to what the ring already
  carries, no re-quantisation, keeps SC-012 trivially true; a 10-minute
  track = 26.46 M frames × 8 B = 211.7 MB ≤ 256 MB (FR-021).
- Public surface: `coverage()` (lossy: per-chunk filled frames), `state()`,
  `len_frames()`, `fold_peaks(range, bucket_frames, out)` (lossy min/max
  aggregates, used by the analysis thread) and `read_frames(from, out) ->
  usize` (raw samples, **real-time-half only**). Constitution V is
  enforced by a guard test in the `modplayer` binary
  (`tests/decoded_store_boundary.rs`) that greps every crate outside
  `modplayer-audio-source*` and `modplayer-engine` for `read_frames(` —
  the same mechanical style as 003's `single_dependent.rs`.

**Rationale**: the RT half must read decoded audio without allocation,
locks, or `unsafe` (Constitution I, VII `#![forbid(unsafe_code)]`);
`OnceLock` + atomics give lock-free reads in safe Rust. Per-track stores
make "released when the track ceases to be current" (FR-021) a plain
`Arc` drop. Defining the type in the trait crate lets the synthetic and
scripted hosts fill it too, so every other crate builds and tests without
the receiver (Constitution IV).

**Alternatives considered**: (a) `Vec<f32>` behind a `RwLock` — RT cannot
lock; (b) raw-pointer chunk table — needs `unsafe`; (c) one store reused
across tracks with generation counters — avoids realloc but cannot shrink
without `&mut`, and contradicts FR-021's "released"; (d) i16 samples —
halves memory but re-quantises and breaks bit-identity with the ring path
for no need (the bound is met with f32); (e) putting the store in the
engine crate — the engine never sees track identity and the receiver's RT
half is where the feed choice (R2) is made.

**Drop discipline — the RT never drops a store `Arc`, period**: the RT
half receives each new store inside the `TrackStart` marker (R2) and
holds exactly one `Arc` (`store`). On a `TrackStart` it *moves* the old
`Arc` into a dedicated **retirement ring** `rtrb::RingBuffer<Arc<
DecodedStore>>` (RT = producer, worker = consumer; `push` moves the value
without touching the refcount — no drop, no dealloc) and only then bumps
`track_seq`. The worker's command loop pops the retirement ring — and
drops the stores there, on the worker thread — before **every single**
marker push and once per loop iteration. Capacity `RETIRED_CAPACITY =
MARKER_CAPACITY + 1` (65): between two consecutive drains at most one
marker is pushed, so the RT can apply at most the ≤ 64 markers already in
the marker ring plus that one, i.e. at most 65 retirements can ever be
outstanding — `push` never returns `Full`. The `Err(Full(arc))` arm is
therefore unreachable; it is still written without a drop (the `Arc` is
kept in a single `parked` slot the next `fill` re-pushes first) and is
covered by `rt_retire_never_full`. No host-side "previous store" rule is
needed or allowed: the host and the analysis/decode-ahead threads drop
their `Arc`s whenever they like, because the RT's copy is never released
by the RT itself — for any sequence of track changes (A→B→C→…, including
several `TrackStart` markers applied inside one `fill`), the last
reference is always dropped on the worker thread. Verified by
`store_drop_never_on_rt` (three consecutive track changes with every
other `Arc` already gone; `Weak::upgrade` must still succeed after each
`fill` until the ring is drained) and `rt_retire_never_full`.

## R2 — Feeding the audio callback: store + ring, chosen at restart points

**Decision**: `ConnectRtSource` keeps the 003 ring (`RingSink` → `rtrb`)
**and** the current track's `DecodedStore`. It tracks `cursor` (the
track position it plays and reports) and a `feed`:

- `feed = Store` — `fill` reads samples at `cursor` from the store and
  **discards the same number of frames from the ring** (pop-and-drop,
  never blocking) so the librespot `Player` stays paced exactly as today
  and Spirc's position/EndOfTrack/preload behaviour is unchanged.
  `Reposition` markers update only the ring's baseline, never `cursor`.
- `feed = Ring` — exactly 003's behaviour (`cursor` follows markers).
- The feed is chosen only at **restart points**: `seek(frame)` picks
  `Store` iff the store covers `frame` (sample-accurate from the very next
  callback, FR-010), otherwise `Ring`; a `TrackStart` marker always starts
  the new track on `Ring` (the new store is empty at that instant — this
  is what keeps gapless transitions on librespot's own output, 003
  SC-004) and swaps in the marker's store.
- Two rescue transitions, both at moments that are already
  discontinuous: `Ring` underrun while the store covers `cursor` →
  switch to `Store` (the store rescues a stall); `Store` running into an
  unfilled chunk → switch to `Ring` at the ring's marker baseline (a jump
  of at most the marker imprecision, a few ms, in a situation where 003
  would already be silent/buffering).
- No mid-play switch otherwise: the ring's position is only marker-
  accurate (see facts), so blending feeds mid-stream would click.
- `seek` still flushes the ring (stale pre-seek audio) and the host still
  sends `SourceCommand::Seek(ms)` so Spirc seeks the `Player` — required
  to keep cluster state and the ring's future content correct.

**Rationale**: FR-010 needs a sample-exact source at seek targets; FR-021
forbids reading the ring; 003's gapless and Connect-state guarantees need
librespot's `Player` to keep running at audible pace and to remain the
audio at track boundaries. Choosing the feed only at discontinuities is
the only design that is glitch-free without sample-exact ring positions.

**Alternatives considered**: (a) play from the store only, ring discarded
— breaks gapless (the next track's store cannot exist before it becomes
current, FR-021/FR-017) and adds ~0.5 s of silence per transition;
(b) exact lockstep (per-frame ring position bookkeeping) — impossible,
markers are stamped by the event task racing the sink, and
`position_ms` is ms-granular; (c) make the ring itself the store (unbounded
sink) — the `Player` would decode the whole track in seconds, Spirc would
report end-of-track and load the next track while the user is still
listening (see facts); (d) pause the `Player` when far ahead — `Paused`
player events flip Spirc's cluster state.

## R3 — Decode-ahead: a second, independent decoder in the receiver

**Decision**: on every `PlayerEvent::TrackChanged { audio_item }` the
receiver's worker spawns a **decode-ahead** OS thread (below-normal
priority, R9) for that track: it resolves the same `(format, file_id)`
the `Player` picks (Bitrate160 preference list, replicated), opens its
own `AudioFile::open(&session, file_id, bytes_per_second)` and audio key
on the worker's tokio runtime, wraps `AudioDecrypt` + a local `Subfile`
(0xa7 header skip for Ogg) as a `symphonia::io::MediaSource`, and drives
`symphonia` **directly** (`probe` → `FormatReader` + `Decoder`) writing
interleaved `f32` into the `DecodedStore` at `packet.ts` (the exact frame
index the container reports). Order:

1. Sequential from frame 0 while the lowest unfilled chunk is the next
   one.
2. On `SourceCommand::Seek(ms)` whose target chunk is unfilled, the
   thread re-points: `FormatReader::seek(Accurate, Time(chunk_start))`,
   discards decoded frames before the chunk boundary (keeping R1's
   "valid from chunk start" invariant), then continues forward.
3. At EOF or on reaching an already-filled chunk, it jumps to the lowest
   unfilled chunk; when none remain it sets `state = Complete` and
   records the exact length.
4. Any unrecoverable decoder/IO error → `state = Failed` (EC-5.9); a
   stopped flag (set on the next `TrackChanged`, `Stop`, or `Shutdown`)
   ends the thread, which drops its `AudioFile` (closing the fetch).

The store is created **before** the `TrackStart` marker is pushed (so
the marker carries it) and is announced to the host as a new, additive
`SourceEvent::DecodedStore { track, store }` right after `TrackStarted`
(or after `BecameActive { context }` on transfer-in).

**Rationale**: FR-021 requires decoding "as fast as the source delivers
audio, not throttled to playback"; the `Player`'s decoder cannot be
detached from its sink pacing (R2 facts), and its `AudioFileStreaming`
is owned by the `Player`. Using `symphonia` directly (already in the
tree) instead of librespot's `SymphoniaDecoder` wrapper gives exact
sample timestamps after seeks (`SeekedTo::actual_ts`), which librespot
truncates to ms — required for a sample-exact store. The seek-following
order (step 2) is what makes a seek-ahead's neighbourhood decodable
quickly: the second `AudioFile` prioritises the range its reader asks
for, exactly like the `Player`'s.

**Cost, recorded in Complexity Tracking**: the encrypted file is fetched
**twice** (Player + decode-ahead) and decoded twice. At 003's default
160 kbps a 10-minute track is 12 MB; two copies at 10 Mbit/s take ≈ 19 s,
inside SC-011's 30 s; Vorbis decode runs > 50× real time, so decode is
never the bottleneck. The second temp file holds only AES-encrypted bytes
in 003's private temp dir (Constitution V unchanged).

**Alternatives considered**: reading the `Player`'s temp file directly —
`NamedTempFile` owned by the `Player`, download map private, fragile;
enabling librespot's `Cache` so the second decoder opens the cached file
— the cache is written only when the download completes, violating
SC-001's "begins filling within 1 s"; forking librespot to expose the
loader — `unknown-git = "deny"` and a permanent merge burden (003 R6
already rejected forks).

## R4 — Peak ladder and bucket format

**Decision**: mono-folded, 8-bit signed `min`/`max` per bucket (`i8`,
`round(sample × 127)` clamped), four levels with `frames_per_bucket` =
**128 / 1 024 / 8 192 / 65 536** (2.9 ms / 23 ms / 186 ms / 1.49 s).
Level 0 is folded straight from the store (`DecodedStore::fold_peaks`);
each coarser level is folded ×8 from the previous one. A level exists
only if it has ≥ 2 buckets for the track.

- Spec bounds: level 0 at 2.9 ms ≤ 3 ms (the 200 ms max-zoom window =
  69 buckets ≥ 60); level 2 draws a 10-minute track from 3 230 buckets
  ≤ 4 096; tracks longer than ~1 h 41 min still render (the view folds
  the coarsest level at draw time), only the "≤ 4 096 buckets" bound is
  satisfied up to that length.
- Size: 10 minutes → 206 719 + 25 840 + 3 230 + 404 buckets × 2 B ≈
  472 KB in memory and on disk ≤ 1 MB (FR-006). A 60-minute podcast ≈
  2.8 MB — over the bound but the bound is stated for 10 minutes.
- Bucket count per level = `ceil(len_frames / frames_per_bucket)`; the
  last bucket may be partial.
- A bucket is *present* only when all its frames are covered; presence
  is a bitmap per level (`Vec<u64>` words) so the UI can draw
  placeholders per bucket rather than from a single watermark
  (multi-gap coverage after seek-ahead, FR-005).

**Rationale**: powers of two make level folding and pixel-column
selection integer arithmetic; 8-bit signed keeps `min` meaningful (the
spec's `max − min` silence test) and the file small; ×8 steps keep the
total ≈ 1.14× the finest level.

**Alternatives considered**: per-channel peaks (2× size, no consumer in
this slice); RMS in addition to min/max (not asked; later slices can add
a section, R7); ×4 steps (more levels, no rendering benefit).

## R5 — Analysis Service placement and thread model

**Decision**: `crates/modplayer-core/src/analysis/` (`mod.rs` = the
service + `Analysis`/`AnalysisSnapshot` types, `peaks.rs` = ladder
folding, `cache.rs` = file format + paths, `worker.rs` = the thread).
`pub const ANALYZER_VERSION: u32 = 1` in `analysis/mod.rs` (FR-007; the
spec's "analysis crate" is this module — core services are one crate per
Constitution VII and no second consumer justifies a new crate,
Constitution X). One long-lived thread named `analysis`, spawned by
`AnalysisService::new`, at below-normal OS priority (R9), driven by an
`mpsc` command channel (`Attach { track, len_frames, store: Option<Arc<
DecodedStore>> }`, `Detach`, `Shutdown`) and replying on an `mpsc`
progress channel with `Arc<AnalysisSnapshot>` values (immutable, shared
with the UI without locks; `std::sync::mpsc` is the same lock-free
pattern 001–004 drain in `tick()`). Loop per attached track:

1. Cache lookup by `(TrackId, ANALYZER_VERSION)` (R7) — hit → publish
   `Complete` from the file within a few ms and idle (FR-006: no analysis
   pass); miss/stale → delete the stale file and continue.
2. If the track is in the session's `failed` set → publish `Failed` and
   idle (FR-016, no retry this session).
3. Otherwise fold newly covered level-0 buckets from the store in passes
   of ≤ 2 048 buckets, refold coarser levels, publish a snapshot whenever
   ≥ 0.5 s of new audio was folded or 250 ms elapsed (SC-002's ≤ 2 s lag),
   park 50 ms when nothing new is covered.
4. On store `Complete`: if every level-0 bucket quantises to
   `max − min == 0` → `Failed` (silence, FR-016, no cache write);
   else write the cache file (R7) and publish `Complete`.
5. On store `Failed`: publish `Failed` keeping whatever buckets exist
   (`peaks: Some(partial)` if any bucket is present, else `None`).

`Detach` drops the store `Arc` and the partial ladder (FR-017 — nothing
incomplete is written). Attaching the same `TrackId` that is already
attached is a no-op (edge case "same track loaded twice").

**Rationale**: the store's raw samples never leave the trait crate (R1);
the service only consumes `fold_peaks`, so it can live with the other
host services in `modplayer-core` and be tested with the scripted host.
One thread keeps FR-003's "one dedicated analysis thread" literal and
bounds the work per pass.

**Alternatives considered**: a new `modplayer-analysis` crate — rejected
by Constitution X until a second consumer (plugins, beat grid) exists;
running analysis inside the receiver — Constitution IV; a shared
`Mutex<WaveformPeaks>` polled by the UI — a lock the UI would take every
frame, and the spec asks for a channel.

## R6 — How the controller and UI consume analysis

**Decision**: `PlaybackController` owns the `AnalysisService`; on
`SourceEvent::TrackStarted` it sends `Attach { track, len_frames, store:
None }` (so a cache hit shows before any store exists — SC-004) and on
`SourceEvent::DecodedStore` it sends the store for the same track (the
worker merges it into the current attachment); on a current-track change
or `stop()` it sends `Detach`; `shutdown()` sends `Shutdown`. `tick()`
drains progress into `self.analysis: Option<Arc<AnalysisSnapshot>>`,
exposed as `pub fn analysis(&self) -> Option<&Arc<AnalysisSnapshot>>`.
The UI reads the snapshot every frame; no analysis I/O ever runs on the
UI thread.

**Rationale**: matches 003/004's "everything is `mpsc` drained in
`tick()`" rule and keeps the controller's public surface one accessor.

## R7 — Cache file format and location

**Decision**: `<data_local_dir>/ModPlayer/analysis/<sha256(track_id)
hex>.mpwf`, override `MODPLAYER_ANALYSIS_DIR` (tests, portable use —
mirrors 004's `MODPLAYER_LIBRARY_DIR`). Hand-written little-endian
binary (no serde; the payload is byte arrays):

```
magic "MPWF" | u32 format_version = 1 | u32 analyzer_version
u16 id_len | id bytes (TrackId) | u32 sample_rate | u64 len_frames
u32 section_count
  per section: 4-byte tag | u64 byte_len | payload
section "WAVE": u32 level_count; per level: u32 frames_per_bucket,
  u32 bucket_count, then bucket_count × (i8 min, i8 max)
```

Read validates magic, `format_version`, `analyzer_version ==
ANALYZER_VERSION`, `id == track` and `len_frames` within 2 s of the
track's known duration; any mismatch or truncation → treat as absent
(and unlink). Write = temp file in the same directory + `rename`
(NFR-2.8 convention), user-only permissions on Unix (as 004). Only
`Complete` entries are written; no eviction in this slice; not deleted
on sign-out (peaks are not account data — recorded as an assumption).
Unknown sections are skipped on read, so beat grid / key / loudness
(DM-5) can be added as sections without a format bump — this is the
"schema shaped to hold them" the spec asks for, without dead fields.

**Rationale**: `sha256` (workspace `sha2`) makes any `TrackId` a safe
filename on all three platforms (`:` is illegal on Windows); a sectioned
binary keeps a 10-minute entry at ≈ 472 KB (JSON would be > 2 MB and
slower to parse on the analysis thread).

**Alternatives considered**: JSON via serde like 004's index — 4–5× the
size and no benefit for byte arrays; one directory per analyzer version —
stale entries would never be cleaned; SQLite — new dependency, no
second consumer.

## R8 — Sample-accurate seek through the host

**Decision**: add `PlaybackController::seek_frames(frame: u64)`; the
reducer's `Input::Seek` and `Effect::SeekTo` gain `position_frames:
Option<u64>` next to `position_ms` (ms stays for `SourceCommand::Seek`
and Connect reporting; the exact frame goes into engine
`Command::Seek(frame)`). `seek(Duration)` keeps its signature and passes
`None`. The UI converts the pointer/keyboard target to a frame via the
coordinate space (R12) and calls `seek_frames`. The engine is
**unchanged**: `Command::Seek(u64)` already carries frames and applies at
the next buffer boundary.

**Latency**: the 10 ms budget (FR-010, measured from the UI event to the
first new-position sample handed to the callback) is one command-queue
push plus at most one buffer period: 2.9 ms (Performance) and 5.8 ms
(Balanced, the default) are inside the budget; at the **Safe** preset
(1 024 frames) the bound is 23 ms. Recorded as a known constraint in
plan.md; SC-003 is verified at the default preset.

**Alternatives considered**: a second engine command — no need; changing
`Command::Seek` to a struct — churn in 003's tests for nothing.

## R9 — Below-normal thread priority

**Decision**: add the `thread-priority` crate (MIT, cross-platform:
`nice`/`SetThreadPriority`/mach policy) to `modplayer-core` (analysis
thread) and `modplayer-audio-source-connect` (decode-ahead thread), used
as `ThreadBuilder::default().priority(ThreadPriority::Min)` /
`set_current_thread_priority(ThreadPriority::Min)` on thread entry;
failure to set the priority is logged-and-ignored (the work is still
chunked and yields between passes). Verification at implementation:
`cargo add thread-priority` + `cargo deny check`; if the crate cannot
be added (offline), the two threads run at normal priority and the
deviation is recorded in plan.md's Complexity Tracking.

**Rationale**: `std::thread` has no priority API (Constitution X's "why
is `std` insufficient" answered); FR-003 makes below-normal priority a
MUST; the crate confines the platform calls to itself (Constitution X
platform-adapter rule holds at the dependency boundary, as with
`keyring`).

**Alternatives considered**: hand-written `libc`/`windows-sys` FFI in an
adapter crate — `unsafe` in a non-listed crate (Constitution VII);
ignoring the requirement — a MUST.

## R10 — Progressive fill and placeholders in the UI

**Decision**: the UI draws each pixel column from the finest ladder level
whose `frames_per_bucket ≤ frames_per_pixel`; a column whose buckets are
all present draws `min`/`max` bars; a column with any absent bucket
draws the **placeholder** (a hatched/dimmed band at 40 % height in the
theme's muted colour) — so multi-gap coverage renders correctly with no
special case. `Pending` (no snapshot yet) draws the whole area as
placeholder; `Failed` with `peaks: None` draws the "analysis unavailable"
label centred in the waveform area (still a functioning seek surface);
`Failed` with partial peaks draws the partial as-is.

**Rationale**: per-column presence is what FR-005 asks ("every undecoded
region … a distinct placeholder") and costs nothing beyond the bitmap
from R4.

## R11 — Overview replaces the seek slider; detail view and keyboard map

**Decision**: `now_playing.rs::show_seek_slider` and the
`transport-position` label are removed; a new `waveform` module
(`crates/modplayer-ui/src/waveform/{mod.rs, coords.rs, overview.rs,
detail.rs, paint.rs, state.rs}`) provides two widgets sharing a
`WaveformState` kept in `App` for the session (detail window, follow
flag, drag preview). Both widgets are `ui.interact(rect, id,
Sense::click_and_drag())` responses with `widget_info(WidgetInfo::slider
(enabled, value_secs, text))` so accesskit exposes role slider with name
`transport-seek` (overview) / `waveform-detail` (detail), value text
`m:ss / m:ss`, and the detail view's description `waveform-detail-window
{ $start } { $end }`. Keyboard handling runs only when the widget has
focus (`response.has_focus()`), with the exact table of spec FR-013;
`Esc` cancels an active drag. Wheel/pinch: `input.zoom_delta()` (pinch
and Ctrl/Cmd+wheel) and `smooth_scroll_delta.y` zoom about the pointer;
`smooth_scroll_delta.x` (trackpad) and `Shift`+wheel pan. Click vs drag
uses egui's own `drag_started`/`clicked` distinction (its drag threshold).

**Rationale**: reusing egui's slider `WidgetInfo` keeps 003's
accessibility test (`transport-seek` role slider, value text) passing
against the overview without new a11y plumbing.

## R12 — Track-time coordinate space (FR-014)

**Decision**: `waveform::coords::TimeSpace { rect: Rect, window: Range<u64
/*frames*/>, sample_rate: u32 }` with `x_of(frame) -> f32`, `frame_at(x)
-> u64` (clamped to the window), `frames_per_pixel()`, `visible_window()`.
Every draw of either widget constructs its `TimeSpace` from the current
window and returns it in the widget's `WaveformResponse`, so later
slices attach playhead/markers/loop/plugin overlays by painting through
the returned space — no widget redesign. Proptest: `frame_at(x_of(f))`
is within `frames_per_pixel()` of `f` for any window and rect
(Constitution VIII marker/loop arithmetic property tests start here).

## R13 — Detail window, zoom, follow (FR-011/012)

**Decision**: `DetailWindow { start_frame: u64, width_frames: u64,
follow: bool }` with pure methods: `zoom_about(anchor_frame, factor)`
(clamp width to `[200 ms, len]`, keep the anchor's screen position),
`pan(delta_frames)`, `reset(len)`, `follow_playhead(playhead, playing)`
(page forward when `playhead ≥ start + width`, place playhead at the
left edge, clamp to `[0, len − width]`), `recenter(frame)` (any seek
outside the window; re-enables follow), `suspend_follow_if_outside
(playhead)` after user pan/zoom. Defaults: 30 s window centred on the
playhead at first show; width persists for the session and applies to the
next track (re-centred); nothing moves while paused/stopped. Zoom steps:
×2 / ÷2 for keys; wheel maps `zoom_delta` continuously (clamped). All
pure, unit-tested without egui.

## R14 — Scripted and synthetic hosts (Constitution IV test surface)

**Decision**: `SyntheticHost` creates a `DecodedStore` for its built-in
track on `Initialize`, fills it synchronously from `track::fill`, and
emits `DecodedStore` after every `TrackStarted` it raises. `ScriptedHost`
gains `handle.script_decode(DecodeScript)` with variants `Instant`,
`Progressive { frames_per_tick }` (advances on each `poll()`), `FailAt {
frame }` (marks `Failed` after that many frames), `Silent` (fills zeros),
`None` (never emits a store). Its RT double (`ScriptedRt`) keeps
generating its tone; the store is only there for the analysis/UI tests.
`ConnectRtSource`'s feed logic is unit-tested in the receiver crate with
a hand-filled store and ring (no network), plus `assert_no_alloc` on
`fill`/`seek` with a store attached.

## R15 — Strings and locale

**Decision**: new keys in `locales/en-US/playback.ftl` (the file that
owns Now Playing): `now-playing-pick-a-track`, `now-playing-album`,
`waveform-unavailable`, `waveform-detail`, `waveform-detail-window`
(`{ $start }`, `{ $end }`), `time-elapsed` (`{ $time }`), `time-remaining`
(`{ $time }`), `waveform-overview-desc`. `transport-seek` is reused
unchanged; `transport-position` is deleted (the `fluent_keys` test is
updated). en-US only (FR-019).

## R16 — Dependency additions (Constitution X justification)

| Crate | Where | Why `std`/existing is insufficient |
|---|---|---|
| `thread-priority` (new) | `modplayer-core`, `modplayer-audio-source-connect` | `std` has no thread-priority API; FR-003 MUST (R9) |
| `symphonia` 0.5 (already resolved) | `modplayer-audio-source-connect` | exact `SeekedTo::actual_ts` and packet `ts`; librespot's wrapper truncates to ms (R3). Features `ogg`, `vorbis`, `mp3` — identical to librespot-playback's, no new transitive surface |
| `sha2` (workspace) | `modplayer-core` | cache filename from arbitrary `TrackId` (R7) |
| `proptest` (dev, workspace) | `modplayer-ui`, `modplayer-core` | coordinate-space and cache round-trip properties (Constitution VIII) |

No new feature flags. No new crate. `cargo deny check`: all MIT/Apache.

## Open verifications (to run during implementation, not blockers)

1. `thread-priority` version pin and `cargo deny` result (R9).
2. `symphonia` seeking on a `MediaSource` whose bytes arrive lazily
   (`AudioFileStreaming`) — proven by librespot's own seek path, but the
   decode-ahead thread's re-point (R3 step 2) should be exercised in the
   live `#[ignore = "manual"]` probe before the manual scenarios.
3. egui 0.36 `Shift`+wheel axis behaviour on macOS trackpads (R11) —
   if the platform already swaps axes, do not swap twice.
4. Actual per-track memory after a 4-minute track (expect ≈ 85 MB RSS
   growth, released on the next track change).
5. Manual scenarios M1-M10, M12-M15 (quickstart.md) could not be run live
   from the implementing agent's session in Phases 3-7: no WindowServer/
   Aqua attachment (`launchctl managername` = Background), so no eframe
   window could be created or screen-captured despite a valid build and
   Keychain session credential. Each was substituted with deterministic
   automated tests exercising the same production code path (never a mock);
   see tasks.md's T038/T048/T057/T065/T073/T075 notes and T078's summary
   table for the full per-scenario evidence. M11 needed no live session and
   passed as an automated equivalent (T074). This is a test-environment gap
   only — no behaviour described in this file or plan.md is known to
   deviate from what a live walk would show; a future session run from an
   interactive terminal with a live Premium credential should still perform
   the live walks (a real pointer/keyboard/trackpad session, a real 10-
   minute soak with Activity Monitor open, and a real Spotify stream for
   the decode-ahead timing scenarios) to close this out fully.

# Contract: Retained decoded store (`modplayer-audio-source::decoded`)

**Crate**: `crates/modplayer-audio-source` (dependency-free, `#![forbid(unsafe_code)]`).
Implements FR-021; research R1; data-model.md §1.

## 1. Types

```rust
pub const CHUNK_FRAMES: u64 = 65_536;
pub const MAX_STORE_FRAMES: u64 = 33_554_432; // 256 MiB of stereo f32

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreState { Filling, Complete, Failed }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PeakBucket { pub min: i8, pub max: i8 }

pub struct DecodedStore { /* data-model.md §1.1 */ }

impl DecodedStore {
    pub fn new(sample_rate: u32, len_frames: u64) -> Arc<Self>;
    pub fn sample_rate(&self) -> u32;
    pub fn len_frames(&self) -> u64;
    pub fn state(&self) -> StoreState;
    pub fn covers(&self, frame: u64) -> bool;
    pub fn covered_frames(&self) -> u64;          // sum of `filled`
    pub fn coverage(&self) -> Vec<u32>;           // `filled` per chunk (lossy)
    pub fn fold_peaks(&self, from_frame: u64, bucket_frames: u32, out: &mut [PeakBucket]) -> usize;

    // Writer side (one writer thread per store).
    pub fn write_frames(&self, from_frame: u64, interleaved: &[f32]) -> usize;
    pub fn set_complete(&self, exact_len_frames: u64);
    pub fn set_failed(&self);

    // Real-time half ONLY (see §4).
    #[doc(hidden)]
    pub fn read_frames(&self, from_frame: u64, out: &mut [f32]) -> usize;
}
```

Additive event on the existing seam (`types.rs`):

```rust
SourceEvent::DecodedStore { track: TrackId, store: Arc<DecodedStore> }
```

`Arc<DecodedStore>` inside `SourceEvent`: `PartialEq` = `Arc::ptr_eq`,
`Debug` = redacted summary (rate, len, state, covered frames).

## 2. Rules

1. **Writer invariant**: `write_frames(from, s)` may only start at a frame
   that is either `chunk_start` or exactly `chunk_start + filled` of that
   chunk (append), and never spans a chunk whose `filled` is already
   larger — i.e., every chunk is valid from its first frame. Violations
   are rejected (return 0), never panic.
2. **Publication**: sample stores are `Relaxed`, followed by one `Release`
   store of `filled`; readers `Acquire` `filled` first. `covers`,
   `read_frames`, `fold_peaks` never observe partially written frames.
3. **No blocking, no allocation on the read side**: `covers`,
   `read_frames`, `state`, `len_frames` perform only atomic loads and
   `OnceLock::get`. Verified by `assert_no_alloc` in the receiver crate's
   `rt_no_alloc` test with a store attached.
4. **Cap**: frames ≥ `MAX_STORE_FRAMES` are silently not stored; `covers`
   returns `false` for them; `set_complete` is a no-op (state stays
   `Filling`) when any slot within the cap is unfilled **or** the track is
   longer than the cap — such a store is `Partial` for FR-005/FR-021.
5. **Terminal states**: after `set_complete`/`set_failed` no further
   writes are accepted.
6. **`fold_peaks`** folds only buckets whose every frame is covered and
   that lie fully inside `len_frames` (the last bucket may be short);
   returns the count of *leading* foldable buckets from `from_frame`
   (stops at the first incomplete bucket) so the analysis thread can
   advance a watermark per pass. Mono fold: `min = min(minL, minR)`,
   `max = max(maxL, maxR)`, quantised `round(clamp(x, −1, 1) × 127)`.
7. **Memory**: `Drop` frees every chunk; the type is never dropped on
   the audio thread by construction — the RT half never drops a store
   `Arc` at all, it moves every replaced store into a retirement ring
   drained on the worker thread (research R1 drop discipline; verified
   by `receiver::store_drop_never_on_rt`, which applies three consecutive
   track changes with every non-RT `Arc` already gone and asserts each
   store is still alive after every `fill` until the ring is drained, and
   by `rt_retire_never_full`).

## 3. `SourceHost` implementors

| Host | When it raises `DecodedStore` | Fill behaviour |
|---|---|---|
| `ConnectSource` | after each `TrackStarted` / `BecameActive { context: Some }` | decode-ahead thread (contracts/connect-source-delta.md) |
| `SyntheticHost` | after each `TrackStarted` it raises on `Initialize` | synchronous full fill from `track::fill`, `Complete` |
| `ScriptedHost` | after each scripted `TrackStarted`, unless `DecodeScript::None` | per `DecodeScript`: `Instant`, `Progressive { frames_per_tick }` (advances in `poll()`), `FailAt { frame }`, `Silent`, `None` |

## 4. Constitution V boundary

`read_frames` is the only raw-sample accessor. Allowed callers: crates
`modplayer-audio-source`, `modplayer-audio-source-synthetic`,
`modplayer-audio-source-connect`, `modplayer-engine`. The binary crate's
test `tests/decoded_store_boundary.rs` reads every `crates/*/src/**/*.rs`
and `crates/*/tests/**/*.rs` outside that list and fails on any
occurrence of `read_frames(`. The analysis path uses only `fold_peaks`
(lossy, non-invertible) — Constitution V is satisfied by construction,
not by convention.

## 5. Tests pinning this contract (`modplayer-audio-source`)

- `store_write_then_covers_and_reads_back_exact` — write 3 chunks + a
  partial 4th; `covers` true up to the watermark, false after;
  `read_frames` returns the exact bit patterns.
- `store_rejects_non_aligned_writes` — a write starting mid-chunk into
  an empty chunk returns 0 and changes nothing.
- `store_fold_peaks_mono_min_max_quantised` — known samples → known
  `i8` pairs; L/R folding; stops at the first incomplete bucket.
- `store_cap_never_allocates_past_bound` — a 20-minute store: slots ≤
  cap only; writes beyond return 0; `set_complete` leaves `Filling`.
- `store_terminal_states_reject_writes`.
- `store_debug_is_redacted` — `format!("{store:?}")` contains no sample
  values.
- proptest `store_read_back_is_identity_for_any_aligned_write_sequence`.

# Contract: Analysis Service — waveform path (`modplayer-core::analysis`)

**Crate**: `crates/modplayer-core` (module `analysis/`). Implements
AR-11 (waveform overview only), DM-5 waveform subset, FR-003–FR-008,
FR-016, FR-017; research R4–R7, R9; data-model.md §3, §6.

## 1. Public surface

```rust
pub const ANALYZER_VERSION: u32 = 1;   // bump by hand on any peak/format change (FR-007)

pub enum AnalysisStatus { Pending, Partial, Complete, Failed }

pub struct PeakLevel { pub frames_per_bucket: u32, pub buckets: Vec<PeakBucket>, present: Vec<u64> }
impl PeakLevel { pub fn is_present(&self, bucket: usize) -> bool; pub fn len(&self) -> usize; }

pub struct WaveformPeaks { pub sample_rate: u32, pub len_frames: u64, pub levels: Vec<PeakLevel> }
impl WaveformPeaks {
    pub const LADDER: [u32; 4] = [128, 1_024, 8_192, 65_536];
    pub fn level_for(&self, frames_per_pixel: f64) -> &PeakLevel; // finest level with fpb <= fpp (or level 0)
}

pub struct AnalysisSnapshot { pub track: TrackId, pub analyzer_version: u32,
    pub status: AnalysisStatus, pub peaks: Option<Arc<WaveformPeaks>>, pub from_cache: bool }

pub struct AnalysisPaths { pub dir: PathBuf }
impl AnalysisPaths { pub fn resolve() -> Option<Self>; pub fn with_dir(dir) -> Self; pub fn file_for(&self, track: &TrackId) -> PathBuf; }
pub const ANALYSIS_DIR_ENV: &str = "MODPLAYER_ANALYSIS_DIR";

pub struct AnalysisService { .. }
impl AnalysisService {
    pub fn new(paths: Option<AnalysisPaths>) -> Self;       // spawns the `analysis` thread
    pub fn attach(&mut self, track: TrackId, sample_rate: u32, len_frames: u64);
    pub fn attach_store(&mut self, track: TrackId, store: Arc<DecodedStore>);
    pub fn detach(&mut self);
    pub fn drain(&mut self) -> bool;                         // called from tick(); true if `latest` changed
    pub fn latest(&self) -> Option<&Arc<AnalysisSnapshot>>;
    pub fn shutdown(self);                                   // join with a 2 s budget
}

pub mod cache {                                              // research R7
    pub fn encode(peaks: &WaveformPeaks, track: &TrackId) -> Vec<u8>;
    pub fn decode(bytes: &[u8], track: &TrackId, expected_len_frames: u64) -> Result<WaveformPeaks, CacheError>;
    pub fn load(paths: &AnalysisPaths, track: &TrackId, expected_len_frames: u64) -> Option<WaveformPeaks>; // stale/invalid → unlink, None
    pub fn store(paths: &AnalysisPaths, track: &TrackId, peaks: &WaveformPeaks) -> Result<(), CacheError>;  // temp + rename
}
```

`PlaybackController` additions: `pub fn analysis(&self) ->
Option<&Arc<AnalysisSnapshot>>`; internally `attach` on `TrackStarted`
(and on `BecameActive { context: Some }`), `attach_store` on
`SourceEvent::DecodedStore`, `detach` when the current track changes or
on `stop()`/`clear_for_sign_out()`, `shutdown` from `shutdown()`.

## 2. Rules

| # | Rule | Requirement |
|---|---|---|
| A1 | `attach` for a track with a valid cache entry publishes `Complete { from_cache: true }` without ever reading a store, within 200 ms of the call on a local disk | FR-006, SC-004 |
| A2 | A cache entry whose `analyzer_version ≠ ANALYZER_VERSION`, whose id or length mismatch, or which is truncated is unlinked and ignored; analysis proceeds as for a new track | FR-007, SC-005 |
| A3 | `attach` for a track in this session's `failed` set publishes `Failed` immediately (with `peaks: None`) and does nothing else — no store read, no retry, even after `detach`/`attach` cycles | FR-016 |
| A4 | Without a store, status stays `Pending` (no snapshot published beyond an initial `Pending` one) | FR-005 |
| A5 | With a store: the first `Partial` snapshot is published within 1 s of the first covered level-0 bucket; subsequent snapshots at most every 250 ms; a snapshot's present buckets never lag the store's coverage by more than 2 s of audio | SC-001, SC-002 |
| A6 | Level-0 buckets fold only from `DecodedStore::fold_peaks` (pre-effect, pre-volume audio by construction) | FR-003, SC-012 |
| A7 | Store `Complete` ⇒ if every level-0 bucket has `max − min == 0` → `Failed`, `peaks: None`, track added to `failed`, **no file written**; else `cache::store` then `Complete` | FR-006, FR-016 |
| A8 | Store `Failed` ⇒ `Failed` with `peaks: Some(partial)` if ≥ 1 bucket present else `None`; track added to `failed`; no file written | FR-016 |
| A9 | `detach` drops the store and partial peaks; nothing is written | FR-017 |
| A10 | `attach` with the same track id as the current attachment is a no-op (in-flight analysis reused) | edge case "loaded twice" |
| A11 | The thread never touches the engine, the audio callback, or any `SourceHost`; it holds only its `Arc<DecodedStore>` | Constitution I |
| A12 | Thread priority below normal (`thread-priority`); a failure to set it is not an error | FR-003 |
| A13 | Every disk access (load/store/unlink) happens on the analysis thread; the controller/UI thread never blocks | 004 convention |

## 3. Tests pinning this contract (`modplayer-core/tests/analysis.rs`)

- `cache_hit_publishes_complete_without_a_store` (A1, SC-004: elapsed <
  200 ms with a pre-written file in a temp dir).
- `stale_analyzer_version_is_unlinked_and_recomputed` (A2, SC-005): write
  a file with `analyzer_version + 1`, attach with a scripted `Instant`
  store → `Complete { from_cache: false }` and the file rewritten with
  the current version.
- `failed_track_is_not_retried_this_session` (A3).
- `progressive_store_publishes_partial_then_complete` (A5, SC-002) with
  `DecodeScript::Progressive` and an injected clock.
- `silent_track_fails_and_writes_nothing` (A7).
- `decoder_failure_keeps_partial_peaks` (A8).
- `detach_drops_partial_without_writing` (A9).
- `only_complete_entries_reach_disk` (A7/A8/A9 combined: the temp dir is
  empty except after `Complete`).
- `peaks_independent_of_master_volume` (SC-012): two stores with the
  same samples, controller volume 20 % vs 100 % → byte-identical
  `encode` output.
- proptest `cache_round_trips_any_peaks` and
  `cache_rejects_any_truncation` (data-model.md §6).
- `ladder_levels_fold_by_eight_and_present_bits_follow` (data-model.md
  §3.2 invariant).

# Contract: Average-Energy (RMS) Analysis & Cache

**Feature**: 022-waveform-legibility | Extends [005 contracts/decoded-store.md](../../005-now-playing-waveform/contracts/decoded-store.md) rule 6 and [005 contracts/analysis-service.md](../../005-now-playing-waveform/contracts/analysis-service.md). Types: [data-model.md §1–§2](../data-model.md).

Contract IDs `AR1…` are referenced by tests and tasks.

## AR1 — `DecodedStore::fold_peaks` (public API; signature unchanged)

```rust
pub fn fold_peaks(&self, from_frame: u64, bucket_frames: u32, out: &mut [PeakBucket]) -> usize
```
- Each written bucket additionally carries `rms = round(clamp(sqrt((Σl² + Σr²)/(2n)), 0, 1) × 127)` over the bucket's `n` frames.
- Coverage/stop/short-last-bucket rules unchanged (005 rule 6).
- No allocation per call beyond what exists today; stack accumulators only.
- Called only from the Analysis Service thread (005 FR-003). Never from the audio callback (Constitution I, FR-014).

Test vectors (`crates/modplayer-audio-source/tests/decoded.rs`):

| Input (both channels unless noted) | Expected `rms` |
|---|---|
| silence `0.0` | `0` |
| DC `0.5` | `64` (`round(0.5×127)=63.5→64`) |
| full-scale square `±1.0` | `127` |
| L = `+0.5`, R = `−0.5` (out of phase) | `64` (not 0) |
| any input | `rms ≤ max(|min|, |max|)` (proptest) |

## AR2 — Ladder refold

`refold_coarser_levels` sets `rms` of coarse bucket *k* to `round(sqrt(Σ wᵢ·rmsᵢ² / Σ wᵢ))` over its present finer buckets (weights = finer bucket frame counts). Presence/watermark rules unchanged. Property: a uniform-RMS track yields the same `rms` (±1) at every level.

## AR3 — `ANALYZER_VERSION`

`pub const ANALYZER_VERSION: u32 = 2;` (re-exported from `modplayer_core`). Any cached entry with a different value is rejected (`VersionMismatch`), unlinked and recomputed on next play — no migration code.

## AR4 — `.mpwf` sections

- Writer (`cache::encode`): `section_count = 2`, `WAVE` (unchanged layout) then `RMS8`.
- Reader (`cache::decode`): requires `WAVE` and `RMS8`; validation V1–V5 ([data-model §2.1](../data-model.md#21-mpwf-cache-entry-layout-research-r3)); unknown tags skipped. Every malformed/truncated input is `Err`, never a panic (existing proptest `cache_rejects_any_truncation` extended to v2 entries).
- `FORMAT_VERSION` stays `1`.

## AR5 — Size budget

A complete entry for a 600 s track encodes to ≤ 1 048 576 bytes at 44 100 Hz and at 48 000 Hz (expected ≈ 709 KB / ≈ 771 KB).

## AR6 — Snapshot consumers

`AnalysisSnapshot.peaks` (`Arc<WaveformPeaks>`) shape is unchanged; UI reads `bucket.rms` directly. Partially analysed tracks expose `rms` for exactly the buckets marked present — same presence semantics as `min`/`max`.

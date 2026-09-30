# Data Model: Waveform Legibility and Scrub Feedback

**Feature**: 022-waveform-legibility | **Date**: 2026-09-28 | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Entities extend 005 (`PeakBucket`, `PeakLevel`, `WaveformPeaks`, `.mpwf` cache), 006 (`LoopRegion`, `loop_state`) and 014/017 (`Roles`, `MARKER_PALETTE`). Nothing here is persisted except §1–§2 (cache entry). No marker/region persistence changes (FR-015).

---

## 1. `PeakBucket` (extended) — `modplayer-audio-source/src/decoded.rs`

| Field | Type | Meaning | Rule |
|---|---|---|---|
| `min` | `i8` | Lowest quantised sample, both channels (005, unchanged) | `round(clamp(s,-1,1)×127)` |
| `max` | `i8` | Highest quantised sample, both channels (005, unchanged) | same |
| `rms` | `u8` | **New.** Root-mean-square over both channels' samples in the bucket | `round(clamp(sqrt((Σl²+Σr²)/(2n)),0,1)×127)`, range `0..=127` |

**Invariants**
- B1: `rms ≤ max(|min|, |max|)` (RMS never exceeds peak magnitude; holds by construction, asserted in tests).
- B2: silent bucket (`min == max == 0`) ⇒ `rms == 0`.
- B3: `Default` is `{0, 0, 0}` (unchanged derive).

Computed in `DecodedStore::fold_peaks` (same pass, analysis thread only — FR-014).

## 2. `PeakLevel` / `WaveformPeaks` (semantics extended) — `modplayer-core/src/analysis`

No new fields — `PeakLevel::buckets: Vec<PeakBucket>` now carries `rms` at every ladder level (FR-013).

**Coarse fold rule** (`peaks::refold_coarser_levels`, research R2):
`rms_coarse = round(sqrt(Σ wᵢ·rmsᵢ² / Σ wᵢ))`, `wᵢ` = frames covered by finer bucket *i* (last bucket may be short). `min`/`max` fold unchanged.

**Constant**: `ANALYZER_VERSION: u32 = 2` (was `1`).

### 2.1 `.mpwf` cache entry layout (research R3)

```text
header   MAGIC "MPWF" | FORMAT_VERSION u32 = 1 | ANALYZER_VERSION u32 = 2 | id_len u16 | id | sample_rate u32 | len_frames u64
sections section_count u32 = 2
  "WAVE" | byte_len u64 | level_count u32 | per level: fpb u32, n u32, n × (min i8, max i8)     (unchanged)
  "RMS8" | byte_len u64 | level_count u32 | per level: fpb u32, n u32, n × rms u8               (new)
```

**Validation** (additions to 005 data-model §6):
- V1: `ANALYZER_VERSION != 2` → `VersionMismatch` → unlink → recompute (existing path; covers every v1 entry).
- V2: `RMS8` missing → `Malformed`.
- V3: `RMS8.level_count != WAVE.level_count`, or any level's `fpb`/`n` differ from `WAVE`'s → `Malformed`.
- V4: any `rms > 127` → `Malformed`.
- V5: unknown sections still skipped.

**Size budget** (FR-013, research R4): complete 10-min entry ≈ 709 KB @ 44.1 kHz, ≈ 771 KB @ 48 kHz; test asserts `≤ 1 048 576` bytes for both.

## 3. `WaveformRoles` token table — `modplayer-ui/src/theme/waveform.rs` (new)

One `static` table per appearance, selected by `waveform_roles(roles: &Roles) -> &'static WaveformRoles` (compares `roles` against the four `tokens::{LIGHT, DARK, LIGHT_HIGH_CONTRAST, DARK_HIGH_CONTRAST}` tables — `Roles: PartialEq` — falling back to `LIGHT`'s table; a pure function of the already-selected `Roles`, preserving 017 FR-017's single selection site).

| Field | Light | Dark | Light HC | Dark HC | Source/derivation |
|---|---|---|---|---|---|
| `played_peak` | `#8FBCEB` | `#2C5A8C` | `#6FA3DE` | `#3A6FA8` | accent tint toward background |
| `played_average` | `#0A63C9` | `#5AA9FF` | `#0050A8` | `#74B6FF` | = appearance `accent` (LHC darkened) |
| `unplayed_peak` | `#D4D4D8` | `#34343B` | `#B4B4BA` | `#44444C` | neutral, light |
| `unplayed_average` | `#8E8E93` | `#74747C` | `#5B5B60` | `#A8A8B0` | neutral, strong |
| `playhead_core` | `#1C1C1E` | `#F2F2F7` | `#1C1C1E` | `#F2F2F7` | = `text_primary` |
| `playhead_casing` | `#FFFFFF` | `#141417` | `#FFFFFF` | `#141417` | = `surface_base` |
| `hover_line` | `#5B5B60` | `#A8A8B0` | `#1C1C1E` | `#F2F2F7` | = `text_secondary` |
| `hover_label_text` | `#1C1C1E` | `#F2F2F7` | `#1C1C1E` | `#F2F2F7` | = `text_primary` |
| `hover_label_bg` | `#E8E8EA` | `#26262C` | `#E8E8EA` | `#26262C` | = `surface_raised` |

Fields that equal a `Roles` value are initialised from that `Roles` constant (e.g. `playhead_core: LIGHT.text_primary`), not re-typed, so they cannot drift.

**Invariants** (automated, `tests/design_token_contrast.rs`):
- W1 (FR-002, SC-005): the four fill tokens are pairwise distinct in each appearance.
- W2 (FR-003, SC-001): for every backdrop *b* in §6, `max(ratio(core, b), ratio(casing, b)) ≥ 3.0`. Measured worst case at plan time: Dark HC vs `played_average` = 4.68:1; Dark vs `unplayed_average` = 4.15:1.
- W3 (FR-019): `hover_line` is a distinct field from `playhead_core` (same value allowed in HC; distinguished by width/casing/z-order).

### 3.1 Named paint constants (`theme/waveform.rs`)

| Constant | Value | Requirement |
|---|---|---|
| `PLAYHEAD_CORE_WIDTH` | `2.0` px | FR-003 (≥ 2 px) |
| `PLAYHEAD_CASING_WIDTH` | `4.0` px (1 px each side) | FR-003 |
| `HOVER_LINE_WIDTH` | `1.0` px | FR-006, FR-019 (< core) |
| `PLACEHOLDER_ALPHA` | `0.4` | 005 (moved literal, unchanged value) |
| `DETAIL_HIGHLIGHT_ALPHA` | `0.25` | 005 (moved literal, unchanged value) |

### 3.2 Loop-shading constants (`theme/markers.rs`, research R10)

| Constant | Value | Requirement |
|---|---|---|
| `LOOP_ARMED_FILL_ALPHA` | `0.25` | FR-004 (006 value, unchanged) |
| `LOOP_HATCH_ALPHA` | `0.6` | FR-004 (006 value, unchanged) |
| `LOOP_HATCH_SPACING` | `8.0` px | FR-004 (006 value, unchanged) |
| `LOOP_IDLE_FILL_ALPHA` | `0.10` | FR-005 (`≤ LOOP_ARMED_FILL_ALPHA / 2`, compile-time `const _: () = assert!(..)`) |
| `LOOP_OUTLINE_WIDTH` | `1.0` px | FR-005 |

## 4. `ColumnPaint` (extended) — `modplayer-ui/src/waveform/paint.rs`

```text
enum ColumnPaint { Present { min: i8, max: i8, rms: u8 }, Placeholder }
```
`rms` = max of the column's buckets' `rms` (same fold as `min`/`max`, research R6).

## 5. `WaveformPaint` (extended) — `modplayer-ui/src/waveform/paint.rs`

| Field | Type | Status |
|---|---|---|
| `status`, `peaks`, `unavailable_text`, `highlight` | — | unchanged |
| `playhead` | `Option<u64>` | unchanged type; **now also** the played/unplayed boundary (displayed position incl. drag preview, FR-002) |
| `hover_suppressed` | `bool` | **new** — `true` while a 005 seek-drag or 006 marker-drag is active (FR-009) |

## 6. Playhead backdrop set (research R9)

Per appearance *A* (`Roles` r, `WaveformRoles` w), with `c(fg, α, bg) = theme::contrast::composite`:

```text
base  = { r.surface_base,
          w.played_peak, w.played_average, w.unplayed_peak, w.unplayed_average,
          c(r.text_secondary, PLACEHOLDER_ALPHA, r.surface_base) }
base' = base ∪ { c(r.accent, DETAIL_HIGHLIGHT_ALPHA, b) | b ∈ base }          -- overview highlight
set   = base' ∪ MARKER_PALETTE
        ∪ { c(p, α, b) | p ∈ MARKER_PALETTE, α ∈ {LOOP_ARMED_FILL_ALPHA, LOOP_HATCH_ALPHA, LOOP_IDLE_FILL_ALPHA}, b ∈ base' }
        ∪ { c(p2, α2, c(p1, LOOP_IDLE_FILL_ALPHA, b)) | … }                    -- one level of overlapping-region compositing
```

## 7. `LoopShade` — `modplayer-ui/src/markers.rs` (research R10)

```text
enum LoopShade { ArmedActive, ArmedInactive, Idle }
fn loop_shade(armed: bool, loop_state: u8) -> LoopShade
```

| `armed` | `loop_state` | Result |
|---|---|---|
| true | 2 | `ArmedActive` |
| true | 0, 1 | `ArmedInactive` |
| false | any | `Idle` |

**Selection rule** (FR-004/FR-005/FR-018): for each `LoopRegion` in `TrackMarkers::regions()` whose `span()` is `Some` — incomplete regions draw nothing. Draw order: every `Idle` region (in `regions()` order), then the armed region (006 I6: at most one). Hue = the A marker's `MARKER_PALETTE` colour (fallback palette 0, as today). `current_region()` is **not** consulted for shading.

## 8. `HoverIndicator` — `modplayer-ui/src/waveform/hover.rs` (new, research R11/R12)

| Field | Type | Meaning |
|---|---|---|
| `x` | `f32` | Pointer x, clamped to `[rect.left(), rect.right()]` |
| `frame` | `u64` | `space.frame_at(x)` — 005 coordinate space |
| `text` | `String` | `format_mmss_millis(frame, sample_rate)` → `m:ss.mmm` |
| `label_rect` | `Rect` | Pill rect, right of line by `space::XS`, flipped left on overflow, clamped fully inside `space.rect` |

**Lifecycle**: none stored. Recomputed every frame: `Some` iff widget `enabled` ∧ `response.hover_pos()` is `Some` ∧ `!hover_suppressed`. Never produces a `WaveformEvent`; never touches AccessKit (FR-008, FR-016, FR-019).

**Validation**: `label_rect ⊆ space.rect` for every `x` in the rect and every rect ≥ the 64 px / 120 px minimum heights (proptest).

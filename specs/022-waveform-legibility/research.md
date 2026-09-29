# Research: Waveform Legibility and Scrub Feedback

**Feature**: 022-waveform-legibility | **Date**: 2026-09-28 | **Plan**: [plan.md](./plan.md)

Every Technical Context unknown is resolved below. Format per item: **Decision / Rationale / Alternatives considered**. Code references are to this worktree as of commit `87f6a9f`.

---

## R1 — Where the average-energy (RMS) value is computed

**Decision**: Extend `modplayer_audio_source::PeakBucket` with a third field `rms: u8` and compute it inside the existing `DecodedStore::fold_peaks` loop (`crates/modplayer-audio-source/src/decoded.rs`), in the same single pass that already computes `min`/`max`. Per level-0 bucket: `rms = round(clamp(sqrt((Σ l² + Σ r²) / (2·n)), 0, 1) × 127)` — the mean square over both channels of the same mono-fold input, on the same 0…127 scale as `max`. Accumulation uses an `f64` running sum on the stack (no allocation).

**Rationale**: `fold_peaks` is the only place that sees raw samples on the analysis side; it already runs only on the Analysis Service's below-normal-priority thread (005 FR-003, `analysis/worker.rs` → `peaks::fold_level0`). Adding one accumulator to the existing loop keeps a single pass over the samples, keeps the real-time callback untouched (Constitution I, FR-014), and keeps the audio-source crate the sole owner of sample access (Principle IV — this is an additive field on an existing type, no new dependency edge).

**Alternatives considered**:
- *Parallel `rms: Vec<u8>` on `PeakLevel`, computed by a second `DecodedStore::fold_energy` method* — rejected: a second full pass over the decoded samples per analysis pass (2× the analysis thread's memory traffic) and two watermarks that could diverge.
- *Deriving an "average" from `min`/`max` (e.g. `(max − min) / 2`)* — rejected: that is still a peak measure; the spec (Clarifications, FR-013) requires RMS, the convention the review names (`UX-23` "peak/RMS distinction").
- *RMS of `(l + r) / 2`* — rejected: cancels out-of-phase stereo content to zero energy while the peak envelope (which folds `min(minL, minR)`/`max(maxL, maxR)`) still shows it; averaging the squares of both channels keeps RMS ≤ peak for every bucket, which the paint relies on (R6).

## R2 — Folding RMS up the ×8 ladder

**Decision**: In `peaks::refold_coarser_levels`, a coarse bucket's RMS is the frame-weighted quadratic mean of its finer buckets: `rms_c = round(sqrt(Σ wᵢ·rmsᵢ² / Σ wᵢ))` where `wᵢ` is the finer bucket's frame count (`frames_per_bucket`, or the shorter remainder for the track's last bucket). Integer math in `u64` for `Σ wᵢ·rmsᵢ²`, one `f64::sqrt` per coarse bucket.

**Rationale**: Mean of mean-squares is exact for RMS given exact inputs; re-squaring 8-bit values adds at most ±1 LSB of quantization error, invisible at any zoom level. Weighting only matters for the final short bucket but costs nothing.

**Alternatives considered**: *Max of finer RMS* — rejected, turns RMS into a peak-of-averages that inflates coarse levels; *arithmetic mean of RMS* — rejected, systematically underestimates energy of uneven passages.

## R3 — Cache format: where the RMS bytes live, and invalidation

**Decision**: Keep the `WAVE` section byte layout and `FORMAT_VERSION = 1` unchanged; add a new `RMS8` section after it (per level: `frames_per_bucket: u32`, `bucket_count: u32`, then `bucket_count` × `u8`). Bump `analysis::ANALYZER_VERSION` from `1` to `2`. Decoding an analyzer-version-2 entry requires both `WAVE` and `RMS8`; a missing `RMS8` or a per-level count mismatch with `WAVE` is `CacheError::Malformed` (entry unlinked, recomputed — 005 FR-007's existing path).

**Rationale**: `cache.rs::decode` already skips unknown section tags ("so future DM-5 sections … can be added without a format bump") — `RMS8` is exactly that designed extension point. The analyzer bump alone forces every existing entry to fail `VersionMismatch` *before* any section is parsed, so no migration code exists (FR-013, spec Clarifications). Existing 005 proptests (`cache_rejects_any_truncation`, round-trip) cover the new section with only fixture updates.

**Alternatives considered**: *Interleave `rms` into `WAVE` (3 bytes/bucket) and bump `FORMAT_VERSION`* — rejected: two version bumps for one change, and it discards the cache's existing section-extension design; *store RMS only at level 0 and derive coarser on load* — rejected: spec requires "stored at every ladder level" and load-time refolding would put work on the UI thread's first-snapshot path.

## R4 — Cache size budget (005 FR-006 ≤ 1 MB for a complete 10-minute entry)

**Decision**: Accept the +1 byte/bucket cost; pin it with a test.

**Rationale**: Ladder 128 / 1 024 / 8 192 / 65 536 frames per bucket (`WaveformPeaks::LADDER`). 10 min at 44.1 kHz = 26 460 000 frames → 206 719 + 25 840 + 3 230 + 404 = 236 193 buckets. 2 B → 472 KB today; 3 B → **709 KB** (+ ≈ 40 B headers). At 48 kHz: 257 111 buckets → **771 KB**. Both under 1 MB. The Connect source decodes at 44.1 kHz (005), so 48 kHz is the stated worst case the test pins; a 96 kHz source would exceed it, but none exists (Principle X — no speculative handling).

**Alternatives considered**: 4-bit RMS packing — rejected, unnecessary under budget and visibly banded.

## R5 — Fill tokens (four per appearance) and their home

**Decision**: New module `crates/modplayer-ui/src/theme/waveform.rs` holding a `WaveformRoles` struct with four static tables (`LIGHT`, `DARK`, `LIGHT_HIGH_CONTRAST`, `DARK_HIGH_CONTRAST`) and a selector `waveform_roles(roles: &Roles) -> &'static WaveformRoles` keyed on the already-selected `Roles` table (017's single-selection-site rule: no call site branches on the appearance flag). Values (see [data-model.md §3](./data-model.md#3-waveformroles-token-table)): played tones are accent-hued (accent itself for average, a tint toward the background for peak); unplayed tones are neutral greys with the same peak-lighter-than-average ordering. Played vs unplayed therefore differ in **both hue and luminance**, peak vs average differ in luminance.

**Rationale**: Fits 014's "tokens in `theme/**`, no literals elsewhere" rule (`tests/design_token_literals.rs` scans for it) and 017's four-table precedent. A separate struct (rather than 7 more fields on `Roles`) avoids touching every existing `Roles` literal and the 10-role contract (014 T9) while keeping the tokens per appearance.

**Alternatives considered**: *Add fields to `Roles`* — rejected, breaks 014's "ten semantic roles" contract and its tests; *derive tones at paint time via `gamma_multiply`* — rejected, alpha-derived colours over a variable backdrop are not fixed values the pairwise-distinct test (SC-005) can pin.

## R6 — Two-tone column paint

**Decision**: `ColumnPaint::Present` gains `rms: u8` (max RMS across the column's buckets — the column is the fold of its buckets, so max matches the existing `min`/`max` folding semantics). Paint per present column: (1) the `min…max` bar in the peak tone; (2) a centred band `mid_y ± rms/127 × half_height` in the average tone, clipped to the peak bar. The band is omitted when it would be < 1 px. Colours come from the played/unplayed pair chosen per column (R7).

**Rationale**: Standard DAW/editor rendering (outer peak, inner RMS). Clipping to the peak bar keeps DC-offset buckets (asymmetric min/max) honest. `rms ≤ max(|min|, |max|)` holds by construction (R1), so the clip is a safety net, not the normal case.

**Alternatives considered**: *Separate RMS polyline* — rejected, less legible at the 64 px overview minimum; *per-bucket (not per-column) RMS mean* — rejected, a column is already a max-fold everywhere else.

## R7 — Played/unplayed boundary

**Decision**: `paint_columns` receives `playhead_x: Option<f32>` (from `WaveformPaint::playhead`, which `now_playing.rs` already sets to the **drag-preview frame** when a 005 drag is active — `waveform.preview_frame()` at `now_playing.rs:779`). Column `col` is *played* iff its centre `x0 + 0.5 < playhead_x`. `None` (no track) → everything unplayed.

**Rationale**: Purely positional per spec Clarifications; follows preview automatically and reverts on `Esc` because the preview frame is cleared there. Centre test gives a crisp 1-column boundary with no half-coloured column.

**Alternatives considered**: Split a straddling column into two rects — rejected, sub-pixel detail hidden under the ≥ 2 px playhead anyway.

## R8 — Playhead: meeting ≥ 3:1 against every backdrop in four appearances

**Decision**: Playhead = casing stroke (`PLAYHEAD_CASING_WIDTH = 4.0` px) in `WaveformRoles::playhead_casing`, then core stroke (`PLAYHEAD_CORE_WIDTH = 2.0` px) in `WaveformRoles::playhead_core`, centred on the same x. Core = the appearance's `text_primary` (near-black light / near-white dark); casing = the appearance's `surface_base` (the opposite extreme). Both are theme tokens copied into `WaveformRoles` so the contrast test names them directly.

**Rationale**: For any backdrop luminance L, the better of a near-black (L≈0.011) and near-white (L≈0.89) stroke reaches at least ≈ 3.9:1 (the crossover where both ratios are equal is at L ≈ 0.19). So "either core or casing ≥ 3:1" (FR-003) holds **by construction** for every backdrop, including ones added later (plugin overlays, future beat grid). The automated test still enumerates the finite backdrop set (R9) so a later token edit that breaks the property fails the build. Today's 1.5 px `strong_text_color` line fails because it has no casing: over the dark theme's accent-blue bars (`#5AA9FF`, L≈0.38) near-white only reaches ≈ 2.2:1.

**Alternatives considered**: *Single accent-contrasting hue (e.g. warning amber)* — rejected, no single colour clears 3:1 against both a near-white background and mid-luminance fills in light theme; *wider single stroke* — rejected, width does not change contrast ratio.

## R9 — The backdrop set the contrast test enumerates

**Decision**: For each appearance: `surface_base` (waveform background = `extreme_bg_color`); the four fill tokens; the placeholder composite `composite(text_secondary, 0.4, surface_base)`; the overview detail-window highlight `composite(accent, 0.25, x)`; and each of the three loop treatments composited over each of the preceding colours for **every** `MARKER_PALETTE` entry (armed-active fill α 0.25, hatch stroke α 0.6, idle fill α `LOOP_IDLE_FILL_ALPHA`), plus the palette colours themselves (marker lines/outline strokes sit directly behind the playhead when co-located). Plugin overlay layers (011) are out of the enumerated set — plugin-chosen colours are unbounded — but are covered by the by-construction argument in R8.

**Rationale**: This is the literal list FR-003 names, made finite. Uses the existing `theme::contrast::{ratio, composite}` helpers — the same blend egui paints with.

**Alternatives considered**: Pixel-sampling a rendered frame — rejected for CI (no GPU); kept as the manual M1 capture instead.

## R10 — Loop-region shading: every region, treatment from its own `armed` flag

**Decision**: Rewrite the region half of `markers::paint_overlay`: iterate `TrackMarkers::regions()`, skip regions without a complete span (`LoopRegion::span` → `None`), compute `LoopShade` via a pure function `loop_shade(region.armed, loop_state)`:

| `region.armed` | engine `loop_state` | `LoopShade` | Paint |
|---|---|---|---|
| `true` | `2` | `ArmedActive` | fill `palette × LOOP_ARMED_FILL_ALPHA (0.25)` (006, unchanged) |
| `true` | `0` or `1` | `ArmedInactive` | diagonal hatch, spacing `LOOP_HATCH_SPACING (8 px)`, stroke `palette × LOOP_HATCH_ALPHA (0.6)` (006, unchanged) |
| `false` | any | `Idle` | 1 px outline `palette` + fill `palette × LOOP_IDLE_FILL_ALPHA (0.10)` |

Paint order: all `Idle` regions in `regions()` order, then the (at most one, 006 I6) armed region last. High contrast keeps 017's extra `marker_outline` stroke on the fill treatments. The treatment constants move from literals in `markers.rs` to named constants in `theme/markers.rs`.

**Rationale**: `loop_state` is the engine's state *of the armed region* (006 contracts/engine-loop.md) — it says nothing about non-armed regions, so today's use of it to shade "the current region" is the bug the clarify pass found (an armed non-current region gets no armed shading, a disarmed current region can be painted as armed). `armed && loop_state == 0` is a transient (UI armed, engine not yet acknowledged) and is painted as armed-inactive rather than idle so the shading never flickers to idle on arm. `0.10 ≤ 0.25 / 2` satisfies FR-005; idle is also the only treatment with *both* outline and fill, and the only one without hatch, so the three states differ by pattern and opacity, not hue.

**Alternatives considered**: *Idle = outline only (006's existing disarmed look)* — rejected, the spec explicitly requires an idle fill so the span reads as a region at the 64 px overview height; *different hue per state* — rejected, hue carries region identity (A-marker palette colour, 014 § 5.3 carve-out).

## R11 — Hover scrub indicator: state, input, suppression

**Decision**: New pure module `crates/modplayer-ui/src/waveform/hover.rs` with `hover_indicator(space, pointer: Option<Pos2>, suppressed: bool, label_galley_size: Vec2) -> Option<HoverIndicator>` returning the line x, the frame (`space.frame_at(x)`), the `m:ss.mmm` text and the clamped label rect (flip-left rule). `overview`/`detail` compute `pointer = response.hover_pos()` only when `enabled` (a click would seek) and paint the indicator **after** `overlays` and **before** `paint::playhead`. `suppressed` = `WaveformPaint::hover_suppressed`, set by `now_playing.rs` to `waveform.drag.is_some() || waveform.marker_drag.is_some()` (005 seek-drag and 006 marker-drag). No state is stored — hover is recomputed from egui's pointer every frame, so it disappears the frame the pointer leaves and "reappears on next pointer move" after a drag with no bookkeeping.

**Rationale**: Stateless = nothing to get stuck. egui repaints on pointer movement, so line-follows-pointer within one frame (SC-003) comes for free. `hover_pos()` returns `None` outside the rect. `Sense::click_and_drag()` already senses hover. Hover emits no `WaveformEvent`, so it cannot commit a seek (FR-008); no `accesskit_node_builder` call changes (FR-016/FR-019).

**Alternatives considered**: *Mirror the hover line on the other view* — rejected by spec (hovered view only); *store hover in `WaveformState`* — rejected, adds state with no consumer.

## R12 — Hover timestamp format and style

**Decision**: Promote `markers.rs::format_mmss_millis_frames` to `waveform::format_mmss_millis` (`pub(crate)`), reused by both the marker list and the hover label. Label painted with `theme::mono_font_id()` (014 `mono` role; egui's monospace family has tabular digits) in `WaveformRoles::hover_label_text` (= `text_primary`) on a `WaveformRoles::hover_label_bg` (= `surface_raised`) pill with `radius::SM`, padding `space::XS`. Placement: top of the rect + `space::XS`, right of the line by `space::XS`; if `x + gap + width > rect.right()` place left of the line; then clamp the rect fully inside the waveform rect.

**Rationale**: One formatter for every `m:ss.mmm` in the UI (006 `UX-24` format) and no new strings (numeric only, FR-017 needs no Fluent key). `text_primary` on `surface_raised` already clears 4.5:1 per 014 C1 (asserted by `every_text_role_clears_its_floor`), so the label is legible over any waveform content.

**Alternatives considered**: Localised time format via Fluent — rejected, 005/006 already render times as unlocalised numerics.

## R13 — Hover line token

**Decision**: `WaveformRoles::hover_line` = the appearance's `text_secondary`, 1 px (`HOVER_LINE_WIDTH = 1.0`). In high contrast `text_secondary == text_primary` (017 FR-005), so the hover line matches the playhead core colour but remains distinguishable by width (1 px vs 2 px core + 4 px casing) and by the casing; z-order keeps the playhead on top.

**Rationale**: Subordinate (FR-019: thinner, different token, below). A dedicated token entry keeps the "different token" requirement testable even where the underlying value coincides in HC.

**Alternatives considered**: Accent-coloured hover line — rejected, accent is the played-fill hue and would vanish over played columns.

## R14 — Heights and minimums

**Decision**: No change. `layout::waveform_heights` (018/021; overview ≥ 64 px, detail ≥ 120 px) stays the only height source; all new paint is proportional to `space.rect` or fixed-pixel (strokes, label) and fits inside 64 px (label ≈ 14 px + 2 × 4 px padding).

**Rationale**: Scope boundary (FR-010).

## R15 — Test placement

**Decision**:
- `crates/modplayer-audio-source/tests/decoded.rs` — RMS of known signals (DC, full-scale square, silence, out-of-phase stereo).
- `crates/modplayer-core/src/analysis/peaks.rs` unit + `crates/modplayer-core/tests/analysis.rs` — ladder RMS refold, `ANALYZER_VERSION == 2`, v1 entry rejected, `RMS8` round-trip, missing-section → `Malformed`, 10-min 44.1/48 kHz entry ≤ 1 MB; extend the existing truncation proptest.
- `crates/modplayer-ui/tests/design_token_contrast.rs` — playhead floor over the R9 backdrop set × 4 appearances; four fill tokens pairwise distinct × 4 appearances.
- `crates/modplayer-ui/tests/waveform.rs` — played/unplayed split (shape capture), two-tone band, hover indicator presence/absence/flip/clamp/suppression, hover never emits an event, a11y node unchanged.
- `crates/modplayer-ui/tests/markers.rs` — `loop_shade` truth table, every region shaded, armed non-current region armed, armed drawn last, idle α ≤ ½ armed α.
- `crates/modplayer-ui/tests/design_token_literals.rs` — unchanged expectation of 0 literals outside `theme/**` (must keep passing).

**Rationale**: Each file already hosts the corresponding 005/006/014/017 tests and helpers (shape capture via `ClippedShape`, `Context` + `RawInput`), satisfying Principle VIII test-first placement without new harnesses.

# Contract: Meter Colour Bands and Scale Marks

**Feature**: 015-control-variants | **Spec**: [../spec.md](../spec.md) |
**Plan**: [../plan.md](../plan.md) |
**Data model**: [../data-model.md](../data-model.md) §6

Covers `crates/modplayer-ui/src/widgets/peak_meter.rs` and
`crates/modplayer-ui/src/widgets/chain_meters.rs::level_pair`.
`chain_meters::spectrum` is **out of scope** — it is not a level meter and
FR-012 names neither it nor a spectrum band rule.

---

## M — Bands (FR-012, FR-012a)

| id | Rule | Pinned by |
|---|---|---|
| **M1** | `BAND_WARNING_DB == -6.0`. | `theme::controls::tests::band_boundaries` |
| **M2** | `band(db, boundary)` evaluates in the **fixed order** `danger` → `warning` → `positive`: `danger` if `db >= boundary`, else `warning` if `db >= -6.0`, else `positive`. | `theme::controls::tests::band_selects_in_the_fixed_order` |
| **M3** | A boundary value belongs to the **higher** band, matching today's `peak_db >= ceiling_db` test. | `theme::controls::tests::boundary_belongs_to_the_higher_band` |
| **M4** | A danger boundary at or below −6 dBFS yields an **empty** warning band — `positive` below the boundary, `danger` at or above it. No band is ever inverted or drawn backwards. | `theme::controls::tests::low_ceiling_empties_the_warning_band` |
| **M5** | `band_color` resolves only to the `positive`/`warning`/`danger` 014 roles. | `theme::controls::tests::band_colours_come_from_roles` |
| **M6** | The fill is **segmented**: each horizontal portion is drawn in the band its own dB position falls in, so a bar driven past the boundary shows `positive`, then `warning`, then `danger` across its length. | `tests/meter_bands.rs::fill_is_segmented_by_db_position`; quickstart **M6** |
| **M7** | At `level >= boundary` the **rightmost filled column** is `danger` while lower portions keep their own bands (SC-005). | `tests/meter_bands.rs::rightmost_column_is_danger_over_the_boundary` |
| **M8** | The peak meter's danger boundary is the **active limiter ceiling** (preserving today's over-ceiling behaviour); the level pair's is a fixed **0 dBFS**, it having no ceiling input. | `tests/meter_bands.rs::each_meter_uses_its_own_boundary` |
| **M9** | In the level pair the **RMS sub-bar bands identically** to the peak sub-bar and its `gamma_multiply(0.7)` dim is **removed**. Neither sub-bar reads `selection.bg_fill` any more. | `tests/meter_bands.rs::rms_bands_and_is_not_dimmed` (FR-012a) |

---

## K — Scale marks (FR-013)

| id | Rule | Pinned by |
|---|---|---|
| **K1** | Both meters draw ticks at **−6 dBFS** and **0 dBFS**, `SCALE_MARK_WIDTH == 1.0`. | `tests/meter_bands.rs::both_meters_draw_both_marks` |
| **K2** | The peak meter additionally draws its ceiling tick at `CEILING_MARK_WIDTH == 2.0`, so the two kinds stay distinguishable. | `tests/meter_bands.rs::ceiling_tick_is_two_px` |
| **K3** | A mark is drawn in `surface_base` where the fill has reached its position (a gap cut through the band) and in `text_secondary` where it has not. One rule, both meters, both themes. | `theme::controls::tests::mark_colour_follows_the_fill` |
| **K4** | K3 needs **no new contrast floor**: `positive`/`warning`/`danger` ≥ 4.5:1 against `surface_base` (014 FR-014) and `text_secondary` ≥ 4.5:1 against it (014 FR-011) are already verified. | `design_token_contrast.rs` (unchanged) |
| **K5** | `peak_meter.rs`'s `warn_fg_color` ceiling tick is **removed** — after M6 it would be invisible over a `warning` band. | `tests/meter_bands.rs::ceiling_tick_is_not_warn_fg`; `design_token_literals.rs` |
| **K6** | The 0 dBFS mark sits at the scale maximum and is **inset by its own width** so it stays visible against the meter's border stroke. | `tests/meter_bands.rs::zero_db_mark_is_inset` |
| **K7** | Mark positions are `fraction_of(-6.0)` and `fraction_of(0.0)` of the track width, on the meters' existing `SCALE_MIN_DB..=SCALE_MAX_DB` scale, which this feature does not change. | `tests/meter_bands.rs::mark_positions` |

---

## R — Readouts, roles and regressions (FR-014, FR-015)

| id | Rule | Pinned by |
|---|---|---|
| **R1** | Every meter's numeric readout stays in the `mono` role and stays visible where it is visible today — not removed, not unlabelled, not made hover-only. Digit columns of adjacent readouts stay aligned (SC-006). | `design_token_roles.rs` (unchanged, FR-014) |
| **R2** | Each meter keeps its accessible value (`WidgetInfo::labeled(ProgressIndicator, …)`) unchanged, so the level state is never carried by colour alone (NFR-6.4). | `accessibility.rs` (unchanged) |
| **R3** | `plugins_view.rs::health_color` keeps resolving from the `positive`/`warning`/`danger` roles and keeps spelling the health state out in words. This feature changes neither mapping nor threshold logic. | `plugins_view.rs` suite (unchanged, FR-015) |
| **R4** | No meter reads engine state differently: the caller still hands in the already-read values each frame. Nothing is added to the real-time path. | no file under `crates/modplayer-engine/` or `crates/modplayer-effects/` is modified (Constitution I) |

# Contract: High-Contrast Tokens (H1–H18)

**Feature**: 017-high-contrast-appearance
**Covers**: FR-002, FR-005, FR-005a, FR-006, FR-007, FR-008, FR-009,
FR-010, FR-017, FR-018, FR-019, FR-020 · SC-001, SC-006, SC-007, SC-008

Each clause names the test that pins it. A clause with no test is not a
contract. Values are from [data-model.md](../data-model.md) §2–§3;
measurements from [research.md](../research.md) R4/R5/R6.

Target suite unless stated: `crates/modplayer-ui/tests/high_contrast.rs`
(new), with the contrast floors in
`crates/modplayer-ui/tests/design_token_contrast.rs` (extended).

---

## §1 — The table set

| # | Clause | Test |
|---|---|---|
| **H1** | Exactly four `Roles` tables exist: `LIGHT`, `DARK`, `LIGHT_HIGH_CONTRAST`, `DARK_HIGH_CONTRAST`. `for_theme(dark, hc)` returns each for its own `(dark, hc)` pair and nothing else. | `for_theme_selects_all_four_tables` |
| **H2** | `for_dark_mode(d) == for_theme(d, false)` — the pre-existing selector keeps meaning "normal mode". | `for_dark_mode_is_the_normal_mode_selector` |
| **H3** | `LIGHT` and `DARK` hold their **present literal values, field by field** (`text_primary`, `text_secondary`, `text_disabled`, `surface_base`, `surface_raised`, `accent`, `text_on_accent`, `positive`, `warning`, `danger`, `disabled_alpha`), and `divider_alpha == 0.08`, `high_contrast == false`. *This is the regression net for FR-002 / US1 AS-3: it fails if an implementation recolours in place instead of adding tables.* | `normal_tables_are_unchanged` |
| **H4** | `high_contrast` is `false` on both normal tables and `true` on both high-contrast tables. | `the_axis_field_marks_exactly_the_two_high_contrast_tables` |

## §2 — What high contrast does **not** change (FR-002, FR-006, FR-020)

| # | Clause | Test |
|---|---|---|
| **H5** | For each theme, the high-contrast table's `surface_base` and `surface_raised` are **identical** to the normal table's. | `surfaces_are_identical_across_the_axis` |
| **H6** | For each theme, `text_primary`, `text_disabled`, `disabled_alpha` and `text_on_accent` are identical across the axis. (`text_on_accent`: research R5 — FR-010's conditional does not fire.) | `unaffected_roles_are_identical_across_the_axis` |
| **H7** | `controls::HOVER_ALPHA == 0.04` and `PRESSED_ALPHA == 0.08` unchanged, and for every table `hover_fill(r) != pressed_fill(r)` with the ordering `0 < hover < pressed` preserved (FR-020). | `interaction_overlays_are_unchanged` + the **unmodified** `tests/interaction_states.rs` |
| **H8** | `controls::FOCUS_RING_GAP == 1.0` in all four tables — FR-008 widens the ring, not the gap. `tests/design_token_roles.rs` and `style.rs::no_geometry_or_interaction_field_changes` pass **verbatim**. | `focus_ring_gap_is_unchanged` + the two named suites unmodified |
| **H9** | `MARKER_PALETTE`'s eight entries are byte-identical to today's (Scope boundary). | `tests/design_token_contrast.rs::marker_palette_clears_its_floor`, unmodified |
| **H10** | The six type-scale sizes and `text_scale()` are unchanged; `Style::text_styles` has 7 entries in **all four** built styles (FR-014). | `type_scale_is_identical_across_the_axis` |

## §3 — The promoted secondary role (FR-005)

| # | Clause | Test |
|---|---|---|
| **H11** | In both high-contrast tables, `text_secondary == text_primary`. | `high_contrast_promotes_secondary_text` |
| **H12** | In both **normal** tables, `text_secondary != text_primary`. *This is the precondition `is_high_contrast` reads (research R2); without it the predicate silently misclassifies.* | `normal_tables_keep_secondary_distinct` |
| **H13** | In a style built with `high_contrast = true`, `visuals.weak_text_color == Some(roles.text_primary)` and `weak_text_alpha == 1.0`. Consequently `visuals.weak_text_color()` — the value `ui.weak()`, the switch's off thumb, the unchecked mark and `overlay_color(Neutral, ..)` all read — is `text_primary`. | `weak_text_follows_the_promotion` |

## §4 — The divider (FR-007)

| # | Clause | Test |
|---|---|---|
| **H14** | `divider_color_for(r) == r.text_primary.gamma_multiply(r.divider_alpha)`; `divider_alpha` is `0.08` in the normal tables and `1.00` in the high-contrast ones. No eleventh role and no call-site literal is introduced (014 contract T10). | `divider_is_the_primary_text_colour_scaled` |
| **H15** | In a high-contrast style, all five `visuals.widgets.*.bg_stroke` **and** `visuals.window_stroke` equal `Stroke::new(1.0, divider)` with `divider == roles.text_primary`. | `every_border_follows_the_divider` |

## §5 — The focus ring (FR-008)

| # | Clause | Test |
|---|---|---|
| **H16** | `focus_ring(r).width` is `2.0` for both normal tables and `3.0` for both high-contrast tables; `focus_ring(r).color == r.accent` in all four, i.e. the ring uses the **high-contrast accent** in high contrast (FR-008's second half). | `focus_ring_thickens_only_in_high_contrast` |

## §6 — Contrast floors (FR-009, FR-010, FR-018 · SC-008)

Target: `crates/modplayer-ui/tests/design_token_contrast.rs`. The five
existing tests are kept **verbatim**, iterating `[&LIGHT, &DARK]` at the
4.5/3.0 floors (research R14). A **new** block adds:

| # | Clause | Test |
|---|---|---|
| **H17** | For both high-contrast tables, and against **both** `surface_base` and `surface_raised`: `accent`, `positive`, `warning`, `danger`, `text_primary` and `text_secondary` each measure **≥ 7.0**; the divider measures **≥ 3.0**. Sixteen role/surface pairs for the four named roles, plus eight for the two text values, plus four for the divider — SC-008's "100% of the pairs named in FR-018". | `high_contrast_roles_clear_the_enhanced_floor`, `high_contrast_text_clears_the_enhanced_floor`, `high_contrast_divider_clears_the_non_text_floor` |
| **H18** | For both high-contrast tables, `ratio(text_on_accent, accent) >= 4.5`. | `text_on_accent_still_pairs_with_the_high_contrast_accent` |

**Measured margins** (research R4/R6) — the assertions pass today, and
the tightest is light `danger` vs `surface.raised` at **7.00**:

| | Light HC | Dark HC |
|---|---|---|
| `accent` base / raised | 8.65 / 7.07 | 8.65 / 7.08 |
| `positive` | 8.72 / 7.13 | 8.68 / 7.10 |
| `warning` | 8.65 / 7.07 | 8.65 / 7.08 |
| `danger` | 8.57 / **7.00** | 9.00 / 7.37 |
| `text_primary` (= `text_secondary`) | 17.01 / 13.91 | 16.48 / 13.48 |
| divider (floor 3.0) | 17.01 / 13.91 | 16.48 / 13.48 |
| `text_on_accent` vs `accent` (floor 4.5) | 8.65 | 8.65 |

**Forbidden assertion**: "every high-contrast role differs from its
normal-mode value". Dark `warning` is deliberately identical (research
R4); such a test would be wrong.

## §7 — No literals (FR-019)

| # | Clause | Test |
|---|---|---|
| **H19** | `tests/design_token_literals.rs`'s `EXPECTED_BASELINE_HITS` stays **`0`**. Every value this feature adds lives under `src/theme/**` (excluded by construction); every outline stroke drawn in `src/markers.rs` / `src/plugin_overlays.rs` is obtained from a `theme::` function, never constructed locally. | `no_colour_or_font_literals_outside_theme`, unmodified |

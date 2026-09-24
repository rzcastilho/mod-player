# Contract: Marker & Overlay Outline (M1–M12), Plugin Reach (P1–P6)

**Feature**: 017-high-contrast-appearance
**Covers**: FR-011, FR-012, FR-013, FR-017 · SC-002, SC-003

Sites are from [data-model.md](../data-model.md) §4; the measured basis
is [research.md](../research.md) R6–R9. Target suites:
`crates/modplayer-ui/tests/markers.rs`,
`crates/modplayer-ui/tests/plugin_overlays.rs`, and the new
`crates/modplayer-ui/tests/high_contrast.rs`.

---

## §1 — The outline value (FR-012)

| # | Clause | Test |
|---|---|---|
| **M1** | `theme::markers::marker_outline(r)` is `None` for both normal tables and `Some(Stroke::new(1.0, r.text_primary))` for both high-contrast tables. | `marker_outline_exists_only_in_high_contrast` |
| **M2** | `MARKER_OUTLINE_WIDTH == 1.0`. Chosen over a thicker stroke so the glyph's own focused/unfocused difference (2.0 vs 1.5 px) is not swamped — FR-012's second sentence. | `marker_outline_width_is_one_pixel` |
| **M3** | For both high-contrast tables and **all eight** `MARKER_PALETTE` entries, `ratio(marker_outline_colour, surface_base) >= 7.0` and `ratio(.., surface_raised) >= 7.0`. The outline is `text_primary`, so this is palette-independent by construction — which is exactly FR-012's claim ("no palette entry can defeat the treatment"), and the test states it over the palette anyway so the claim is machine-checked rather than argued. Measured: 17.01/13.91 light, 16.48/13.48 dark (research R6). | `marker_outline_clears_the_enhanced_floor_against_every_palette_entry` |

**Explicitly not asserted**: outline-vs-fill contrast. Measured at
2.73–5.58 (worst: dark `#f2f2f7` around palette entry 7 olive
`#8F9A4B`). No requirement states a floor for it, and raising it would
require the per-entry tuning FR-012 rules out. See research R6 — the
absence is a decision, not an omission.

## §2 — Where the outline is drawn (FR-011)

Headless assertions inspect the `Shape` list a frame produces
(`Context::run_ui` + `ClippedShape` inspection, the route
`tests/design_token_contrast.rs:12-16` and `tests/plugin_overlays.rs`
already use). "Gains" = a shape in the outline colour that is absent from
the same frame rendered in normal mode.

| # | Site (data-model §4) | Clause | Test |
|---|---|---|---|
| **M4** | O1–O3 detail-lane glyphs | With markers placed and high contrast on, each region bracket, point glyph and cue glyph gains an outline shape in `text_primary`; with high contrast off, none does. | `detail_lane_glyphs_gain_an_outline` |
| **M5** | O4, O6 overview-lane line + clamped mark | Same, in the overview lane. | `overview_lane_markers_gain_an_outline` |
| **M6** | O5 loop-region span | An armed loop region's span shading gains a `rect_stroke` in `text_primary`; its translucent fill colour is unchanged. | `loop_region_span_gains_an_outline` |
| **M7** | O12 Markers panel colour swatch | The swatch (`egui::Button::new("").fill(color)`, `src/markers.rs:601`) carries a 1 px `text_primary` frame stroke in high contrast — **inherited from the divider (FR-007), with no swatch-specific paint code** (research R7). Pinned because it is a consequence: a later `.stroke(Stroke::NONE)` would silently drop an accessibility requirement. | `panel_swatch_inherits_the_divider_outline` |
| **M8** | all | The palette **fill** colour at every site above is byte-identical between normal and high contrast (FR-011: "MUST NOT change value"). US3 AS-1. | `palette_fills_are_never_recoloured` |
| **M9** | O1, O4 | A focused glyph still strokes wider than an unfocused one by the same delta as in normal mode (2.0 vs 1.5 detail; 2.0 vs 1.0 overview) — the casing adds the same width to both. FR-012, Edge Case 4. | `focus_width_difference_survives_the_outline` |
| **M10** | O3 | The cue glyph's slot digit is still painted in `text_on_accent` and is not outlined (FR-020, Edge Case 5). | `cue_digit_is_unchanged` |

## §3 — Plugin overlays (FR-011, FR-013)

| # | Clause | Test |
|---|---|---|
| **M11** | `overlay_outline(token, visuals)` is `Some(1 px text_primary)` for `Positive` and `Warning` **only when** `visuals` is a high-contrast style, and `None` for `Accent`, `Secondary` and `Neutral` in **every** style. FR-011's last sentence, and research R9's reason for a token-keyed function rather than a palette scan. | `overlay_outline_covers_exactly_the_palette_tokens` |
| **M12** | O7–O10: with high contrast on, a plugin `Line`, `Region`, `Label` and host `Glyph` coloured `Positive`/`Warning` each gain an outline; the same primitives coloured `Accent`/`Secondary`/`Neutral` gain none. O11: the `Package`-glyph fallback (`weak_text_color()`) gains none. | `plugin_palette_primitives_gain_an_outline`, `plugin_role_primitives_do_not` |

| # | Clause (FR-013, SC-003) | Test |
|---|---|---|
| **P1** | `overlay_color(Accent, v) == v.selection.bg_fill`, `(Secondary) == v.hyperlink_color`, `(Neutral) == v.weak_text_color()`, `(Positive) == MARKER_PALETTE[2]`, `(Warning) == MARKER_PALETTE[3]` — the 011 mapping is **unchanged** (`tests/plugin_overlays.rs` and `theme/markers.rs:161-183` pass unmodified). | existing tests, unmodified |
| **P2** | In a high-contrast style, `overlay_color(Accent, v) == LIGHT_HIGH_CONTRAST.accent` (resp. dark) and `overlay_color(Secondary, v)` likewise, because `style.rs:161,163` assign them from the role table. A plugin's accent overlay is therefore at the high-contrast value with no plugin change. | `plugin_role_overlays_follow_the_high_contrast_roles` |
| **P3** | In a high-contrast style, `overlay_color(Neutral, v) == roles.text_primary` — FR-005's promotion reaching a plugin (`weak_text_color` chain, H13). | `plugin_neutral_overlay_follows_the_promotion` |
| **P4** | A docked plugin panel's text and control colours equal the host chrome's: both read the same `ctx.style_of(theme)` object, which `apply_tokens_for` set once. Asserted by rendering a stub panel beside host chrome in one high-contrast frame and comparing the resolved colours. SC-003's "zero low-contrast elements". | `docked_plugin_panel_matches_host_chrome` |
| **P5** | Toggling high contrast produces **no plugin-side call**: no gateway request, no script invocation, no new API surface. The panel simply re-reads the applied style on its next frame (US4 AS-2). Asserted by the capability-gateway API-reference regeneration diff being empty and by no new call on the stub panel's recorded call log. | `api_reference.rs` regeneration diff (existing, unmodified) + `toggling_high_contrast_calls_no_plugin` |
| **P6** | `crates/modplayer-capability-gateway/api/v1.toml` and `docs/plugin-api/v1.md` are **byte-identical** to their pre-feature contents. `OverlayColor` gains no variant; `HostGlyph` gains none (Constitution IX). | `api_reference.rs`, unmodified |

## §4 — One selection site (FR-017)

| # | Clause | Test |
|---|---|---|
| **S1** | `is_high_contrast(v)` returns `true` for both styles built with `high_contrast = true`, `false` for both built with `false`, and `false` for a bare `egui::Visuals::light()` / `::dark()` (whose `weak_text_color` is `None`). That last case keeps the existing unit tests that pass bare visuals classifying as normal mode (research R2). | `high_contrast_is_recoverable_from_the_applied_visuals` |
| **S2** | `roles(v)` returns the table matching `(v.dark_mode, is_high_contrast(v))` for all four built styles. | `roles_selects_the_table_the_style_was_built_from` |
| **S3** | No file outside `crates/modplayer-ui/src/theme/**` and
`crates/modplayer-ui/src/{app.rs, settings/appearance.rs}` mentions
`high_contrast`. `app.rs` passes it to `apply_tokens_for`;
`settings/appearance.rs` owns the control. **No paint site, no view, and no plugin-facing file branches on it** — the mechanical form of FR-017. | `high_contrast_is_named_only_at_its_selection_and_control_sites` (a source scan in the shape of `design_token_literals.rs`) |
| **S4** | `apply_tokens(ctx)` (no flag) still installs the normal-mode pair, and `apply_tokens_for(ctx, false)` installs the same `Arc`s (`Arc::ptr_eq`). The 25 existing test call sites are unmodified. | `apply_tokens_defaults_to_normal_mode`, plus `style.rs::apply_tokens_installs_both_themes` and `::apply_tokens_is_idempotent` passing verbatim |
| **S5** | `apply_tokens_for` allocates nothing after the first call: repeated calls at either setting return `Arc::ptr_eq` styles. This is what makes FR-004/SC-005 ("next frame, no flash") structural rather than a timing hope. | `apply_tokens_for_is_allocation_free_after_the_first_call` |

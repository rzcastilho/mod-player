# Contract: Design Tokens (host-wide)

**Feature**: 014-design-tokens-and-type-scale | **Status**: proposed with
this plan | **Consumers**: every `modplayer-ui` module, every plugin-
contributed panel (through `contracts/ui-panels.md` A4), `crates/modplayer`

This contract states the rules the token layer must satisfy. Each rule
carries the name of the test that pins it. Rules are numbered **T**
(tokens), **A** (application), **C** (contrast), **U** (usage).
Requirement ids trace to [../spec.md](../spec.md).

> **Not a plugin API change.** Tokens reach plugin panels through the
> shared `Style`/`Visuals` that A4 already binds them to. `api/v1.toml` is
> untouched, no request/event/DTO changes, and Constitution IX's written
> change-request requirement is **not** triggered (FR-015b).

---

## T — Token identity and values

**T1** (FR-001). `crates/modplayer-ui/src/theme/` is the only module in the
workspace that declares a colour, font size, spacing value or corner
radius. Its public surface is [../data-model.md](../data-model.md) §1.
*Test*: `design_token_literals::no_colour_or_font_literals_outside_theme`
(see [literal-scan.md](literal-scan.md)).

**T2** (FR-003). The six type roles carry exactly the sizes, families,
case and tracking of data-model.md §2. Sizes are logical pixels at scale
1.0 and are expressed as `BASE * text_scale()` with `text_scale()` a
private `const fn` returning `1.0` (FR-020).
*Test*: `tokens::text_styles_cover_every_role`,
`tokens::role_sizes_are_base_times_scale`.

**T3** (FR-003a). `Style::text_styles` contains exactly seven entries:
`Heading`, `Body`, `Button`, `Small`, `Monospace`, `Name("display")`,
`Name("section")`, mapped as data-model.md §2. A widget naming no style
therefore renders from the scale.
*Test*: `tokens::built_in_styles_are_remapped_onto_roles`.

**T4** (FR-004). `mono` resolves to `FontFamily::Monospace`; every digit
`'0'..='9'` has one advance width, and two equal-length `mono` strings lay
out to the same width.
*Test*: `tokens::mono_digits_are_tabular`.

**T5** (FR-003). Adjacent roles are separable without a weight axis:
`body` (14) ≠ `secondary` (13) in size, and a row's title colour
(`text_primary`) ≠ its secondary line's colour (`text_secondary`).
*Test*: `tokens::body_and_secondary_are_visibly_different`.

**T6** (FR-003b). `theme::section_label(s)` returns a `RichText` whose
style is `Name("section")`, whose text is `s.to_uppercase()` (Unicode
default full case mapping — correct for en-US and pt-BR; a locale-tailored
path would need ICU and no shipping locale needs one), and whose
`extra_letter_spacing` is `0.52` (0.04 em at 13 px). No Fluent catalogue
entry is uppercased, duplicated or re-cased in any locale.
*Test*: `tokens::section_label_uppercases_at_draw_time`,
existing `fluent_keys.rs` (unchanged, must still pass).

**T7** (FR-007). `space::{XS,SM,MD,LG,XL,XXL}` = `4, 8, 12, 16, 24, 32`
exactly. `xl` is **24** (research R19).
*Test*: `tokens::spacing_scale_is_the_four_pixel_scale`.

**T8** (FR-009). `radius::SM` = 4, `radius::MD` = 8, and
`radius::full(h)` = `round(h/2)` clamped to `0..=255`, so a badge is a pill
at any height.
*Test*: `tokens::radius_full_is_a_pill_at_any_height`.

**T9** (FR-010, FR-010a). `Roles` carries exactly the ten colour roles plus
`disabled_alpha`, with the light and dark values of data-model.md §5.1.
`theme::roles(visuals)` selects by `visuals.dark_mode` and allocates
nothing (`&'static Roles`).
*Test*: `tokens::roles_table_matches_the_contract`,
`tokens::roles_selects_by_dark_mode`.

**T10** (FR-008). `divider` is computed inside the module as `text_primary`
at 8 % alpha and exposed as `theme::divider(ui)` / `theme::divider_color`.
It is not an eleventh role and no call site writes an alpha or a colour.
*Test*: `tokens::divider_is_primary_text_at_eight_percent`.

**T11** (FR-015, FR-015a). `MARKER_PALETTE` holds the eight values of
data-model.md §5.4, is theme-independent, stays indexed by `PaletteIndex`,
and `overlay_color` still maps `OverlayColor::Positive`→`[2]` and
`Warning`→`[3]`. The module's doc comment names the two `surface_base`
values the palette is verified against.
*Test*: existing `markers::marker_color_indexes_the_fixed_palette`,
`markers::overlay_color_maps_every_token`,
`markers::marker_palette_has_eight_distinct_colours` (all must still pass
unchanged), plus C6.

---

## A — Application

**A1** (FR-002). `theme::apply_tokens(ctx)` installs a complete `Style` for
`egui::Theme::Light` **and** `egui::Theme::Dark` via `set_style_of`, from
`Arc<Style>`s built once into a `OnceLock`. It is idempotent and allocation-
free after the first call.
*Test*: `style::apply_tokens_installs_both_themes`,
`style::apply_tokens_is_idempotent`.

**A2** (FR-002). `apply_tokens` is called from `App::new` before the first
paint and as the first statement of `App::update` on every frame. A theme
switch requires no view code to run and no view to hold a colour.
*Test*: `app_tokens::update_reapplies_tokens_every_frame`,
`app_tokens::theme_switch_changes_every_slot_with_no_view_change`.

**A3** (FR-010b). Every colour field of `Visuals` listed in data-model.md
§5.3 is assigned from a role or a derived value; none is left at
`Visuals::light()`/`dark()`. A test walks the constructed `Visuals` and
asserts each listed field equals its mapped role.
*Test*: `style::every_visuals_colour_comes_from_a_role`.

**A4** (FR-010b, US1). `weak_text_color` is `Some(text_secondary)` and
`weak_text_alpha` is `1.0`, so no secondary text is produced by an alpha
multiply.
*Test*: `style::weak_text_is_the_secondary_role_not_an_alpha`.

**A5** (R11, FR-012). `Visuals::disabled_alpha` is the per-theme value of
data-model.md §5.1 (light 0.55, dark 0.44), chosen so egui's disabled
composite equals the `text_disabled` token.
*Test*: `style::disabled_alpha_matches_the_disabled_token`.

**A6** (FR-007, FR-008). `Style::spacing` is assigned per research R18.
Component *sizes* (`interact_size`, `slider_width`, `combo_width`,
`text_edit_width`, `tooltip_width`, `menu_width`, `default_area_size`,
`scroll.*`) keep egui's values — changing them would be the layout work
FR-019 forbids.
*Test*: `style::spacing_uses_only_scale_steps`.

**A7** (FR-009). `widgets.*.corner_radius` = `radius::SM`;
`window_corner_radius` and `menu_corner_radius` = `radius::MD`.
*Test*: `style::radii_come_from_the_scale`.

**A8** (FR-019). `apply_tokens` changes no geometry or behaviour field:
`widgets.*.expansion`, `interaction`, `animation_time`, `striped`,
`handle_shape`, `interact_cursor` and the component sizes of A6 keep their
egui values.
*Test*: `style::no_geometry_or_interaction_field_changes`.

**A9** (FR-015b). `apply_tokens` adds no plugin API surface;
`crates/modplayer-capability-gateway/api/v1.toml` and
`docs/plugin-api/v1.md` are byte-identical before and after this feature.
*Test*: existing `api_reference.rs` regeneration diff (must show no diff).

---

## C — Contrast (FR-011 … FR-014, FR-018b)

All ratios are WCAG 2.x relative-luminance ratios computed from the token
values by `theme::contrast::ratio`; compositing uses
`bg.blend(fg.gamma_multiply(α))`, the operations egui itself paints with
(research R16).

| Rule | Assertion | Test |
|---|---|---|
| **C1** (FR-011) | `text_primary` and `text_secondary` ≥ **4.5:1** against `surface_base` **and** `surface_raised`, in **both** themes (8 assertions) | `contrast::every_text_role_clears_its_floor` |
| **C2** (FR-012) | `text_disabled` ≥ **3:1** against both surfaces in both themes (4 assertions) | `contrast::every_text_role_clears_its_floor` |
| **C3** (FR-012, R11) | the disabled **composite** (`text_primary` × `disabled_alpha` over each surface) ≥ **3:1**, both themes (4 assertions) | `contrast::disabled_composite_clears_the_non_text_floor` |
| **C4** (FR-013) | `surface_raised` ≥ **1.2:1** against `surface_base`, both themes, with raised darker than base in light and lighter in dark | `contrast::raised_surface_is_separated` |
| **C5** (FR-010a, FR-014) | `text_on_accent` ≥ **4.5:1** vs `accent`; `accent`/`positive`/`warning`/`danger` ≥ **4.5:1** vs `surface_base` — both themes | `contrast::on_accent_is_readable`, `contrast::status_roles_clear_the_text_floor` |
| **C6** (FR-015) | all **8** `MARKER_PALETTE` entries ≥ **3:1** against **both** `surface_base` values (16 assertions) | `contrast::marker_palette_clears_three_to_one` |
| **C7** | `theme::contrast::ratio` is symmetric, returns 1.0 for equal colours and 21.0 for black-on-white | `contrast::ratio_matches_the_wcag_reference` |

A failure message names the pair, the measured ratio and the floor, so a
future value change reports what to fix rather than "assertion failed".

---

## U — Usage rules for views (what tasks must change)

**U1** (FR-005). Every timestamp, dB reading, CPU/memory figure and
duration renders in `mono`: `markers.rs`, `transport_view.rs`,
`now_playing.rs`, `plugins_view.rs`, `widgets/peak_meter.rs`,
`widgets/chain_meters.rs`, `waveform/paint.rs`, `settings/audio.rs`
(buffer/latency figures), `detail_view.rs` (track durations),
`queue_view.rs`, `library_view.rs`, `search_view.rs`.
*Test*: `type_roles::numeric_fields_use_the_mono_role` (asserts the helper
each numeric formatter routes through carries `TextStyle::Monospace`).

**U2** (FR-006). Prose blocks are capped at `theme::body_measure(ctx)` via
`set_max_width(available.min(measure))`; single-line labels, row titles,
table cells, tooltips and notifications are **not** capped.
*Test*: `measure::body_measure_is_seventy_two_zero_widths`,
`measure::a_long_paragraph_wraps_within_the_measure`.

**U3** (FR-008). Panels are separated by `ui.add_space(space::XL)`; the
three in-panel group rules use `theme::divider(ui)`; no `ui.separator()`
call remains in `crates/modplayer-ui/src/**` (research R20).
*Test*: `design_token_literals::no_separator_between_panels` (a source
scan for `ui.separator()`, same mechanism as the literal scan).

**U4** (FR-016, Edge Cases). Health dots and the over-ceiling meter select
`positive`/`warning`/`danger` through **existing, unchanged** threshold
logic; this feature supplies only the values the selection resolves to.
Colour is never the sole carrier (NFR-6.4): every dot keeps its existing
text key and accessible name.
*Test*: existing `plugins_view.rs` and `accessibility.rs` suites (must
still pass unchanged), plus
`type_roles::health_dot_colours_come_from_roles`.

**U5** (FR-016). No opaque white label remains: `markers.rs` 256/273/282,
`plugin_overlays.rs` 177 and the two `Color32::WHITE` uses inside
`paint_host_glyph` all resolve to a role.
*Test*: covered by T1's scan plus
`type_roles::marker_labels_use_a_role_colour`.

**U6** (US5 AC3, SC-007). Switching themes repaints every screen from the
other installed `Style` with no per-view code change and no stale colour.
*Test*: `app_tokens::theme_switch_changes_every_slot_with_no_view_change`;
manual scenario **M6** in [../quickstart.md](../quickstart.md).

---

## Named tests, consolidated

`crates/modplayer-ui/src/theme/` unit tests: `tokens::*`, `style::*`,
`markers::*` (existing three, unchanged).
`crates/modplayer-ui/tests/design_token_contrast.rs`: `contrast::*` (C1–C7).
`crates/modplayer-ui/tests/design_token_literals.rs`: T1, U3.
`crates/modplayer-ui/tests/design_token_roles.rs`: `type_roles::*`,
`measure::*`, `app_tokens::*`.

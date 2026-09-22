# Phase 1 Data Model: Design Tokens, Type Scale, and Spacing

**Feature**: 014-design-tokens-and-type-scale | **Date**: 2026-09-22 |
**Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md) |
**Research**: [research.md](research.md)

This feature persists nothing and changes no wire format. Its "data model"
is the **token set**: the in-memory, compile-time values the whole app
renders from, plus the exact mapping from those values onto the toolkit's
own style structures. Every value below is normative; the tests named in
[contracts/design-tokens.md](contracts/design-tokens.md) pin them.

---

## 1. Module layout and public surface

```text
crates/modplayer-ui/src/theme/
├── mod.rs        pub use …; apply(); apply_tokens(); roles(); divider(); section_label(); body_measure()
├── tokens.rs     Roles, LIGHT, DARK, space, radius, text (the values)
├── style.rs      build_style(Theme) -> Style  (text_styles, Spacing, Visuals)
├── contrast.rs   ratio(), relative_luminance(), composite()
└── markers.rs    MARKER_PALETTE, marker_color(), overlay_color(), paint_host_glyph()
```

Public items (all re-exported from `theme::`, so existing call sites keep
their import form):

| Item | Signature | Purpose |
|---|---|---|
| `apply` | `fn(&Context, modplayer_engine::Theme)` | unchanged: sets `ThemePreference` |
| `apply_tokens` | `fn(&Context)` | installs both themes' `Style` (idempotent, per frame) |
| `roles` | `fn(&Visuals) -> &'static Roles` | the active theme's colour table (`visuals.dark_mode`) |
| `divider` | `fn(&mut Ui)` | draws the 1 px, 8 %-α rule (FR-008) |
| `divider_color` | `fn(&Visuals) -> Color32` | the same colour for painters |
| `section_label` | `fn(&str) -> RichText` | `section` role: style + locale uppercase + tracking |
| `body_measure` | `fn(&Context) -> f32` | 72 × advance width of `body`'s `'0'` (FR-006) |
| `text::{DISPLAY,TITLE,SECTION,BODY,SECONDARY,MONO}` | `TextStyle` | the six roles |
| `space::{XS,SM,MD,LG,XL,XXL}` | `f32` | the six spacing steps |
| `radius::{SM,MD}` / `radius::full` | `CornerRadius` / `fn(f32) -> CornerRadius` | the three radius steps |
| `MARKER_PALETTE`, `marker_color`, `overlay_color`, `paint_host_glyph` | unchanged | the sanctioned A4 exception |

No other module in the workspace declares a colour, font size, spacing or
radius value (FR-001, enforced by
[contracts/literal-scan.md](contracts/literal-scan.md)).

---

## 2. Type scale (FR-003, FR-003a)

Sizes are **logical pixels at scale factor 1.0**; the toolkit applies the
platform DPI factor on top and the values are never pre-multiplied
(spec Clarifications). Family: egui's embedded default stack — `Proportional`
(Ubuntu-Light) for five roles, `Monospace` (Hack-Regular) for `mono`
(research R2). No weight axis exists in that stack, so every role resolves
to the single available weight and separates on size, case and colour
(research R3, FR-003's fallback clause).

| Role | `TextStyle` | Size | Family | Case / tracking | Emphasis colour | Used for |
|---|---|---|---|---|---|---|
| `display` | `Name("display")` | 22 | Proportional | — | `text.primary` | Now Playing track title, welcome heading |
| `title` | `Heading` | 17 | Proportional | — | `text.primary` | screen headings, detail headers |
| `section` | `Name("section")` | 13 | Proportional | UPPERCASE, +0.52 px (0.04 em) | `text.primary` | panel and group headers |
| `body` | `Body` | 14 | Proportional | — | `text.primary` | row titles, field labels, paragraphs |
| `secondary` | `Small` | 13 | Proportional | — | `text.secondary` | artist/album, help text, metadata |
| `mono` | `Monospace` | 13 | Monospace | tabular by construction | inherits | timestamps, dB, CPU/memory, durations |

**Built-in remap (FR-003a)** — the complete `Style::text_styles` map, so a
widget that names no style still renders from the scale:

```text
Heading           -> FontId::new(17.0, Proportional)   // title
Body              -> FontId::new(14.0, Proportional)   // body
Button            -> FontId::new(14.0, Proportional)   // body
Small             -> FontId::new(13.0, Proportional)   // secondary
Monospace         -> FontId::new(13.0, Monospace)      // mono
Name("display")   -> FontId::new(22.0, Proportional)
Name("section")   -> FontId::new(13.0, Proportional)
```

`Style::drag_value_text_style` = `Monospace` (a `DragValue` shows a number
— FR-005).

**Future text-scale hook (FR-020)**: every size above is
`BASE_<ROLE> * text_scale()`, where `text_scale()` is a private
`const fn` returning `1.0` today. `UX-40` changes that one function, not
six literals. No setting, no reflow, no UI in this feature.

---

## 3. Spacing scale (FR-007)

| Token | Value | Canonical use |
|---|---|---|
| `space::XS` | 4.0 | vertical item gap, icon spacing |
| `space::SM` | 8.0 | horizontal item gap, row vertical padding (FR-008), button padding x |
| `space::MD` | 12.0 | intra-group gaps |
| `space::LG` | 16.0 | panel content padding (FR-008), indent |
| `space::XL` | 24.0 | **panel-to-panel separation** (FR-008; research R19) |
| `space::XXL` | 32.0 | screen-level separation, empty-state insets |

Normalization rule for an existing arbitrary value: nearest step, ties
round **up** (FR-007). The egui `Spacing` field map is research R18 /
§5.3 below.

---

## 4. Radius scale (FR-009)

| Token | Value | Use |
|---|---|---|
| `radius::SM` | `CornerRadius::same(4)` | inputs, chips, meters, skeletons |
| `radius::MD` | `CornerRadius::same(8)` | cards, panels, windows, menus, artwork |
| `radius::full(h)` | `CornerRadius::same((h * 0.5).round().clamp(0.0, 255.0) as u8)` | badges — a pill at any height |

---

## 5. Colour

### 5.1 Semantic roles (FR-010, FR-010a; values per research R12/R13)

```rust
pub struct Roles {
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_disabled: Color32,
    pub surface_base: Color32,
    pub surface_raised: Color32,
    pub accent: Color32,
    pub text_on_accent: Color32,
    pub positive: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub disabled_alpha: f32,
}
```

| Role | Light | vs base / raised | Dark | vs base / raised | Floor |
|---|---|---|---|---|---|
| `text_primary` | `#1c1c1e` | 17.01 / 13.91 | `#f2f2f7` | 16.48 / 13.48 | 4.5 both |
| `text_secondary` | `#5b5b60` | 6.75 / 5.52 | `#a8a8b0` | 7.79 / 6.37 | 4.5 both |
| `text_disabled` | `#828283` | 3.84 / 3.14 | `#76767a` | 4.06 / 3.32 | 3.0 both |
| `surface_base` | `#ffffff` | — | `#141417` | — | — |
| `surface_raised` | `#e8e8ea` | 1.224 vs base | `#26262c` | 1.222 vs base | ≥1.2 vs base |
| `accent` | `#0a63c9` | 5.77 / 4.71 | `#5aa9ff` | 7.49 / 6.13 | 4.5 vs base |
| `text_on_accent` | `#ffffff` | 5.77 vs accent | `#141417` | 7.49 vs accent | 4.5 vs accent |
| `positive` | `#1f7a44` | 5.35 / 4.37 | `#4caf50` | 6.61 / 5.41 | 4.5 vs base |
| `warning` | `#8a5a00` | 5.93 / 4.84 | `#e0a92a` | 8.65 / 7.08 | 4.5 vs base |
| `danger` | `#b3261e` | 6.54 / 5.34 | `#ff6b5e` | 6.58 / 5.38 | 4.5 vs base |
| `disabled_alpha` | 0.55 | composite 3.84 / 3.60 | 0.44 | composite 4.06 / 3.83 | 3.0 both |

Two values deviate from the spec's FR-010 table, both under the spec's own
"floor governs, swatch adjusts" rule: `surface_raised` (FR-013 mandates it;
research R12) and `text_disabled` (research R13 — the spec's swatches fail
the 3:1 floor against `surface.raised`, which FR-012 requires).

### 5.2 Derived values (never a role, never a call-site literal)

| Derived | Formula | Use |
|---|---|---|
| `divider` | `text_primary` @ 8 % α (`gamma_multiply(0.08)`) | 1 px in-panel rules (FR-008), `widgets.*.bg_stroke`, `window_stroke` |
| disabled text | `text_primary` × `disabled_alpha`, composited by egui | every `ui.disable()`d widget (research R11) |
| every unnamed `Visuals` slot | §5.3 | FR-010b |

### 5.3 `Visuals` slot map (FR-010b — nothing left at a toolkit default)

Each theme's `Visuals` starts from `Visuals::light()` / `Visuals::dark()`
for non-colour fields (shadows, `handle_shape`, `text_cursor`,
`interact_cursor`, …) and then assigns **every** colour field:

| Slot | Role |
|---|---|
| `dark_mode` | `false` / `true` |
| `panel_fill`, `window_fill` | `surface_base` (scrolling content and windows) |
| `extreme_bg_color`, `text_edit_bg_color` | `surface_base` |
| `faint_bg_color`, `code_bg_color` | `surface_raised` |
| `widgets.noninteractive.bg_fill` / `weak_bg_fill` | `surface_raised` |
| `widgets.inactive/hovered/active/open.bg_fill` / `weak_bg_fill` | `surface_raised` |
| `widgets.*.fg_stroke.color` | `text_primary` |
| `widgets.*.bg_stroke` | `Stroke::new(1.0, divider)` |
| `widgets.*.corner_radius` | `radius::SM` |
| `widgets.*.expansion` | unchanged (geometry — FR-019) |
| `override_text_color` | `None` (roles reach text through `fg_stroke`) |
| `weak_text_color` | `Some(text_secondary)` — **the US1 fix** (research R10) |
| `weak_text_alpha` | `1.0` (inert once `weak_text_color` is set) |
| `selection.bg_fill` | `accent` |
| `selection.stroke` | `Stroke::new(1.0, text_on_accent)` |
| `hyperlink_color` | `accent` |
| `warn_fg_color` / `error_fg_color` | `warning` / `danger` |
| `window_stroke` | `Stroke::new(1.0, divider)` |
| `window_corner_radius`, `menu_corner_radius` | `radius::MD` |
| `disabled_alpha` | `Roles::disabled_alpha` |
| `striped` | unchanged (behaviour — FR-019) |

Chrome that frames content (nav rail, plugin dock, transport bar) draws its
own `Frame` fill from `roles.surface_raised`; scrolling content areas use
`panel_fill` = `surface_base` (FR-010b).

`Spacing` assignments: research R18's table (`item_spacing (8,4)`,
`button_padding (8,4)`, `window_margin 16`, `menu_margin 8`, `indent 16`,
`icon_spacing 4`, `menu_spacing 4`); component *sizes* keep egui's values.

### 5.4 Marker palette (FR-015, FR-015a — theme-independent, `PaletteIndex`-keyed)

| # | Hue | Value | vs `#ffffff` | vs `#141417` | Change |
|---|---|---|---|---|---|
| 0 | red | `#D94F4F` | 4.05 | 4.54 | — |
| 1 | blue | `#3E8EDE` | 3.43 | 5.37 | — |
| 2 | green | `#3D8B43` | 4.22 | 4.35 | re-toned from `#4CAF50` |
| 3 | amber | `#B0711A` | 4.02 | 4.57 | re-toned from `#E09B1A` |
| 4 | violet | `#9C5CE0` | 4.18 | 4.40 | — |
| 5 | teal | `#00868A` | 4.40 | 4.18 | re-toned from `#00ACB0` |
| 6 | pink | `#C2519A` | 4.25 | 4.32 | re-toned from `#DE6FB4` |
| 7 | olive | `#8F9A4B` | 3.05 | 6.03 | — (thin pass, pinned by test) |

Defaults by kind are unchanged (loop region `0`, point `1`, cue `2`), as is
`overlay_color`'s `Positive`→`[2]` / `Warning`→`[3]` mapping (FR-015a).

---

## 6. Call-site model — what each existing surface renders as

| Surface | Today | After |
|---|---|---|
| Now Playing track title | default `Body` | `display` |
| Welcome heading | `Heading` | `display` |
| Screen headings, detail headers | `Heading` | `title` (unchanged style name, new size) |
| Panel/group headers ("Markers", "Effect chain", settings groups) | plain label | `theme::section_label(..)` |
| List row title (`rows.rs`) | default | `body` + `text_primary` |
| List row secondary line | `ui.weak()` | `secondary` + `text_secondary` (US1, via `weak_text_color`) |
| Marker timestamps (`markers.rs`) | `FontId::monospace(9.0)` | `mono` |
| Transport position/duration | default | `mono` |
| Plugin CPU/memory (`plugins_view.rs`) | default | `mono` |
| Peak-meter dB readout, chain meters | default | `mono` |
| Waveform time labels (`waveform/paint.rs`) | `FontId::proportional(h*0.35)` | `mono` |
| Plugin-overlay labels (`plugin_overlays.rs`) | `FontId::proportional(10.0)` + `Color32::WHITE` | `secondary` + `text_on_accent`/role colour |
| Initials avatar (`widgets/initials.rs`) | `FontId::proportional(size*0.4)` | `title`-family sized from the token scale, `radius::MD` |
| Plugin health dots (`plugins_view.rs`) | 3 `from_rgb` | `positive` / `warning` / `danger` (existing threshold logic unchanged) |
| Peak-meter over-ceiling (`widgets/peak_meter.rs`) | `from_rgb(220,60,60)` | `danger` |
| Host warning glyph mark (`theme::paint_host_glyph`) | `Color32::WHITE` | `text_on_accent` of the active theme |
| Prose blocks (welcome, privacy, getting started, settings help, empty states) | full width | `ui.set_max_width(min(available, body_measure))` |
| Panel separators (`now_playing.rs`, `settings/mod.rs`) | `ui.separator()` | `ui.add_space(space::XL)` |
| In-panel group rules (`settings/{account,controls,plugins}.rs`) | `ui.separator()` | `theme::divider(ui)` |

---

## 7. Lifecycle

1. `App::new` → `theme::apply_tokens(&cc.egui_ctx)` then the existing
   `theme::apply(ctx, controller.theme())`. Tokens are installed before the
   first paint.
2. Every frame, first statement of `App::update` → `theme::apply_tokens(ctx)`
   (idempotent; two `Arc` clones — research R7).
3. Theme switch (Appearance settings, or the OS under `Theme::System`) →
   egui picks the other already-installed `Style`. **No view code runs, no
   view holds a colour** (FR-002, US5 AC3, SC-007).
4. Plugin panels draw inside the same `Style`/`Visuals` and inherit
   everything (`contracts/ui-panels.md` A4) — no plugin API change
   (FR-015b).

## 8. Validation rules (all compile-time constants, all test-enforced)

| Rule | Test |
|---|---|
| Every text role ≥ its floor vs both surfaces in both themes | `contrast::every_text_role_clears_its_floor` |
| `surface_raised` ≥ 1.2:1 vs `surface_base`, both themes | `contrast::raised_surface_is_separated` |
| `text_on_accent` ≥ 4.5:1 vs `accent`, both themes | `contrast::on_accent_is_readable` |
| `accent`/`positive`/`warning`/`danger` ≥ 4.5:1 vs `surface_base` | `contrast::status_roles_clear_the_text_floor` |
| Disabled composite ≥ 3:1 vs both surfaces, both themes | `contrast::disabled_composite_clears_the_non_text_floor` |
| All 8 `MARKER_PALETTE` entries ≥ 3:1 vs both `surface_base` values | `contrast::marker_palette_clears_three_to_one` |
| 8 palette entries distinct | `markers::marker_palette_has_eight_distinct_colours` (existing) |
| All 7 `text_styles` present, sizes exact | `tokens::text_styles_cover_every_role` |
| Adjacent roles differ in size (`body` ≠ `secondary`) | `tokens::body_and_secondary_are_visibly_different` |
| Every `mono` digit has one advance width | `tokens::mono_digits_are_tabular` |
| Spacing/radius constants exact and 4-px aligned | `tokens::spacing_scale_is_the_four_pixel_scale` |
| No colour/font-size literal outside `theme/` | `design_token_literals::no_colour_or_font_literals_outside_theme` |

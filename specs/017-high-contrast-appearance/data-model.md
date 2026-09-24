# Phase 1 Data Model: High-Contrast Appearance Option

**Feature**: 017-high-contrast-appearance | **Date**: 2026-09-23
**Inputs**: [spec.md](spec.md), [research.md](research.md)

This feature introduces **no new entity type with identity or lifetime**.
It adds one persisted boolean, two fields on an existing plain-data
struct, two static tables, three constants and two locale keys. Everything
below is a *value*, which is why every assertion in
[quickstart.md](quickstart.md) is a value assertion over a table rather
than a rendering comparison.

---

## §1 — The persisted setting

### 1.1 Domain value

`AudioSettings` (`crates/modplayer-core/src/settings/model.rs:98-…`)
gains one field beside `theme` (`:108`):

| Field | Type | Default | Requirement |
|---|---|---|---|
| `theme` | `Theme` | `Theme::System` | (existing, unchanged) |
| `high_contrast` | `bool` | `false` | FR-003 |

The two are **independent axes**: all six combinations are reachable and
none is normalised away (FR-001, US2 AS-1/AS-4).

| `theme` | `high_contrast` | Rendered appearance |
|---|---|---|
| `System` | `false` | normal light or normal dark, following the OS |
| `System` | `true` | high-contrast light or high-contrast dark, following the OS |
| `Light` | `false` | normal light |
| `Light` | `true` | high-contrast light |
| `Dark` | `false` | normal dark |
| `Dark` | `true` | high-contrast dark |

### 1.2 Serialized shape

`RawAppearance` (`model.rs:489-492`), the `[appearance]` table:

```toml
[appearance]
theme = "system"
high_contrast = false
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAppearance {
    #[serde(default = "default_theme")]
    pub theme: String,
    /// FR-003. `toml::Value`, not `bool`, on purpose: serde's derived
    /// `Deserialize for bool` *errors* on a non-boolean, and
    /// `store.rs:123-131` turns any deserialize error into a total
    /// discard of the settings file (research R10). A permissive raw
    /// type plus late validation is `theme`'s own pattern.
    #[serde(default = "default_high_contrast")]
    pub high_contrast: toml::Value,
}

fn default_high_contrast() -> toml::Value { toml::Value::Boolean(false) }
```

`SCHEMA_VERSION` (`model.rs:69`) **stays `1`**. No migration: an added
field with a serde default inside an existing table is not a schema
change (the `[onboarding]` and `[now_playing_panels]` precedents).

### 1.3 Recovery table (FR-003)

Applied in `RawSettings::into_settings` (`model.rs:653-…`), beside the
`theme` match at `:691`:

| `settings.toml` holds | `as_bool()` | `settings.high_contrast` | `invalid` | Other settings |
|---|---|---|---|---|
| field absent | `Some(false)` | `false` | — | intact |
| `high_contrast = false` | `Some(false)` | `false` | — | intact |
| `high_contrast = true` | `Some(true)` | `true` | — | intact |
| `high_contrast = "yes"` | `None` | `false` | `InvalidField::HighContrast` | **intact** |
| `high_contrast = 1` | `None` | `false` | `InvalidField::HighContrast` | **intact** |
| `high_contrast = []` | `None` | `false` | `InvalidField::HighContrast` | **intact** |

The "intact" column is the point of R10: with a plain `bool` field, every
row after the third would have reset the whole file.

### 1.4 Invalid-field channel

`InvalidField` (`model.rs:198-224`) gains one variant:

```rust
pub enum InvalidField {
    BufferPreset,
    Theme,
    HighContrast,        // NEW
    DeviceName,
    FocusPolicy,
    PluginPanel(String),
}

// field_name():
InvalidField::HighContrast => "appearance.high_contrast",
```

### 1.5 Controller shadow state

`PlaybackController` (`controller.rs`) mirrors the value exactly as it
mirrors `theme` (`:453-456`, populated `:808`, read `:1605`):

| Member | Shape | Modelled on |
|---|---|---|
| `high_contrast: bool` | private field, set from `settings.high_contrast` at launch | `theme: Theme` (`:456`) |
| `pub fn high_contrast(&self) -> bool` | getter | `theme()` (`:1605`) |
| `pub fn set_high_contrast(&mut self, on: bool)` | setter, updates shadow state only | `set_focus_policy` |

Persistence stays where it already is — the reload-mutate-save block in
`settings/appearance.rs:51-61` — so there is exactly one writer.

---

## §2 — The colour-role tables

### 2.1 `Roles` gains two fields

`crates/modplayer-ui/src/theme/tokens.rs:14-27`:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roles {
    pub text_primary: egui::Color32,
    pub text_secondary: egui::Color32,
    pub text_disabled: egui::Color32,
    pub surface_base: egui::Color32,
    pub surface_raised: egui::Color32,
    pub accent: egui::Color32,
    pub text_on_accent: egui::Color32,
    pub positive: egui::Color32,
    pub warning: egui::Color32,
    pub danger: egui::Color32,
    pub disabled_alpha: f32,
    /// FR-007: the multiplier `divider_color_for` applies to
    /// `text_primary`. 0.08 normal, 1.00 high contrast — the divider is
    /// still never a role of its own (014 contract T10).
    pub divider_alpha: f32,        // NEW
    /// FR-008/FR-012: the axis, read only by the width selectors inside
    /// this module. No call site reads it (FR-017).
    pub high_contrast: bool,       // NEW
}
```

### 2.2 The four tables, field by field

`—` means *identical to the normal-mode table for that theme*. Ratios are
the measured values from research R4/R6.

| Field | `LIGHT` | `LIGHT_HIGH_CONTRAST` | `DARK` | `DARK_HIGH_CONTRAST` |
|---|---|---|---|---|
| `text_primary` | `#1c1c1e` | — | `#f2f2f7` | — |
| `text_secondary` | `#5b5b60` | **`#1c1c1e`** (= `text_primary`, FR-005) | `#a8a8b0` | **`#f2f2f7`** (= `text_primary`, FR-005) |
| `text_disabled` | `#828283` | — (FR-006) | `#76767a` | — (FR-006) |
| `surface_base` | `#ffffff` | — (FR-002) | `#141417` | — (FR-002) |
| `surface_raised` | `#e8e8ea` | — (FR-002) | `#26262c` | — (FR-002) |
| `accent` | `#0a63c9` | **`#074a96`** | `#5aa9ff` | **`#74b6ff`** |
| `text_on_accent` | `#ffffff` | — (research R5) | `#141417` | — (research R5) |
| `positive` | `#1f7a44` | **`#165630`** | `#4caf50` | **`#57c95c`** |
| `warning` | `#8a5a00` | **`#694400`** | `#e0a92a` | **— (already ≥ 7:1, R4)** |
| `danger` | `#b3261e` | **`#931f19`** | `#ff6b5e` | **`#ff9a91`** |
| `disabled_alpha` | `0.55` | — (FR-006) | `0.44` | — (FR-006) |
| `divider_alpha` | `0.08` | **`1.00`** (FR-007) | `0.08` | **`1.00`** (FR-007) |
| `high_contrast` | `false` | **`true`** | `false` | **`true`** |

**Do not write a test asserting "every high-contrast role differs from
its normal value."** Dark `warning` is deliberately identical (R4).

### 2.3 Measured floors (what FR-018 asserts)

| Pair | Light HC | Dark HC | Floor |
|---|---|---|---|
| `accent` vs base / raised | 8.65 / 7.07 | 8.65 / 7.08 | ≥ 7.0 |
| `positive` vs base / raised | 8.72 / 7.13 | 8.68 / 7.10 | ≥ 7.0 |
| `warning` vs base / raised | 8.65 / 7.07 | 8.65 / 7.08 | ≥ 7.0 |
| `danger` vs base / raised | 8.57 / **7.00** | 9.00 / 7.37 | ≥ 7.0 |
| `text_primary` vs base / raised | 17.01 / 13.91 | 16.48 / 13.48 | ≥ 7.0 |
| `text_secondary` vs base / raised | = `text_primary` | = `text_primary` | ≥ 7.0 |
| divider vs base / raised | 17.01 / 13.91 | 16.48 / 13.48 | ≥ 3.0 |
| `text_on_accent` vs HC `accent` | 8.65 | 8.65 | ≥ 4.5 |

Light `danger` vs `surface.raised` = **7.00** is the tightest value in the
feature.

### 2.4 Selection

```rust
/// FR-017: the one place the high-contrast table is chosen.
pub fn for_theme(dark_mode: bool, high_contrast: bool) -> &'static Roles {
    match (dark_mode, high_contrast) {
        (false, false) => &LIGHT,
        (false, true)  => &LIGHT_HIGH_CONTRAST,
        (true,  false) => &DARK,
        (true,  true)  => &DARK_HIGH_CONTRAST,
    }
}

/// Unchanged signature, unchanged meaning: the normal-mode table.
/// Kept because `style.rs`, the contrast suite and several unit tests
/// name a table directly.
pub fn for_dark_mode(dark_mode: bool) -> &'static Roles {
    for_theme(dark_mode, false)
}

/// FR-017 read-back. See research R2: this is FR-005 restated, not a
/// heuristic. Precondition (`text_secondary != text_primary` in both
/// normal tables) is itself asserted by a test.
pub fn is_high_contrast(visuals: &Visuals) -> bool {
    visuals.weak_text_color == Some(visuals.widgets.noninteractive.fg_stroke.color)
}

/// Unchanged signature — every existing call site keeps compiling.
pub fn roles(visuals: &Visuals) -> &'static Roles {
    for_theme(visuals.dark_mode, is_high_contrast(visuals))
}
```

### 2.5 The divider

```rust
pub fn divider_color_for(roles: &Roles) -> egui::Color32 {
    roles.text_primary.gamma_multiply(roles.divider_alpha)   // was: 0.08
}
```

One line. Its consumers, all unchanged:

| Consumer | Site | What it becomes in high contrast |
|---|---|---|
| `window_stroke` | `style.rs:167` | 1 px full-alpha `text.primary` |
| `widgets.{noninteractive,inactive,hovered,active,open}.bg_stroke` | `style.rs:188` via `recolor_widget` ×5 | 1 px full-alpha `text.primary` |
| `theme::divider()` in-panel rule | `mod.rs:71-81` | 1 px full-alpha `text.primary` |
| `Variant::Default`'s outline | `controls.rs:53` | 1 px full-alpha `text.primary` |
| Switch off-state outline | `controls.rs` §5 | 1 px full-alpha `text.primary` |
| **Markers panel colour swatch** | `src/markers.rs:601` (`egui::Button`) | **1 px full-alpha `text.primary` — FR-011 satisfied for free (research R7)** |

---

## §3 — The two widths

`crates/modplayer-ui/src/theme/controls.rs`:

```rust
/// The focus ring's normal-mode width (015 FR-010).
pub const FOCUS_RING_WIDTH: f32 = 2.0;                    // unchanged
/// FR-008: high contrast thickens the ring.
pub const FOCUS_RING_WIDTH_HIGH_CONTRAST: f32 = 3.0;      // NEW

pub fn focus_ring(roles: &Roles) -> Stroke {
    Stroke::new(focus_ring_width(roles), roles.accent)
}

pub const fn focus_ring_width(roles: &Roles) -> f32 {
    if roles.high_contrast { FOCUS_RING_WIDTH_HIGH_CONTRAST } else { FOCUS_RING_WIDTH }
}
```

`FOCUS_RING_GAP` (`controls.rs:106`) is **unchanged** at `1.0` — FR-008
names width only, and the gap is geometry
(`no_geometry_or_interaction_field_changes` must keep passing).

`crates/modplayer-ui/src/theme/markers.rs`:

```rust
/// FR-012: the palette-outline stroke width.
pub const MARKER_OUTLINE_WIDTH: f32 = 1.0;                // NEW

/// FR-011/FR-012: `Some` only in high contrast. The colour is the
/// active theme's `text_primary` — its extreme luminance end, so it
/// holds >= 7:1 against both waveform surfaces whichever of the eight
/// palette entries it encircles (research R6).
pub fn marker_outline(roles: &Roles) -> Option<Stroke> {
    roles.high_contrast
        .then(|| Stroke::new(MARKER_OUTLINE_WIDTH, roles.text_primary))
}

/// FR-011: `Some` only where the resolved overlay colour is palette
/// *data*. `Accent`/`Secondary`/`Neutral` resolve through the recoloured
/// roles and are excluded unconditionally (research R9).
pub fn overlay_outline(token: OverlayColor, visuals: &Visuals) -> Option<Stroke> {
    match token {
        OverlayColor::Positive | OverlayColor::Warning => marker_outline(roles(visuals)),
        OverlayColor::Accent | OverlayColor::Secondary | OverlayColor::Neutral => None,
    }
}

/// The casing width for a stroked shape (research R8): the palette
/// stroke drawn `2 * MARKER_OUTLINE_WIDTH` wider, underneath.
pub fn casing_width(base: f32) -> f32 { base + 2.0 * MARKER_OUTLINE_WIDTH }
```

---

## §4 — Outline form per shape (FR-011, research R8)

Every entry is drawn **only** when the corresponding `Option<Stroke>` is
`Some`. The palette fill/stroke colour is never changed (FR-011).

| # | Surface | Site | Shape today | Outline form |
|---|---|---|---|---|
| O1 | Detail-lane region bracket | `src/markers.rs::paint_bracket` (`:234`) | stroke `2.0` focused / `1.5` unfocused | casing: same polyline at `casing_width(w)` in outline colour, painted first |
| O2 | Detail-lane point glyph | `paint_point_glyph` | filled triangle | `Shape::convex_polygon` with `stroke = outline` |
| O3 | Detail-lane cue glyph | `paint_cue_glyph` | filled square + digit | `rect_stroke` in outline colour; **digit unchanged** (`text_on_accent`, FR-020/Edge Cases) |
| O4 | Overview-lane marker line | `src/markers.rs:399-407` | line, `2.0` focused / `1.0` unfocused | casing at `casing_width(w)` |
| O5 | Loop-region span shading | `src/markers.rs:~425` (`rect_filled(.., gamma_multiply(0.25))`) and `paint_hatched` | translucent fill / hatch | `rect_stroke` around `span_rect` in outline colour |
| O6 | Clamped-marker warning | `paint_clamped_warning` (`markers.rs:~410`) | palette-coloured mark | casing, same rule as O4 |
| O7 | Plugin overlay `Line` | `plugin_overlays.rs:97` | `Stroke::new(1.0, resolved)` | casing at `casing_width(1.0)` |
| O8 | Plugin overlay `Region` | `plugin_overlays.rs:118` | `rect_filled(.., ×0.25)` | `rect_stroke` around `region_rect` |
| O9 | Plugin overlay `Label` | `plugin_overlays.rs:151` | `painter.text(..)` | halo: the same text drawn at the four ±1 px offsets in outline colour, then the coloured text on top |
| O10 | Plugin overlay `Glyph` (host) | `plugin_overlays.rs:163-166` | `paint_host_glyph` | outline passed through to `paint_host_glyph`, which casings each of its six vector forms |
| O11 | Plugin overlay `Glyph` (package fallback) | `plugin_overlays.rs:191` | `HostGlyph::Dot` in `weak_text_color()` | **none** — `weak_text_color` is a role, not palette data (FR-011's exclusion) |
| O12 | Markers panel colour swatch | `src/markers.rs:601` | `egui::Button::new("").fill(color)` | **none written** — already outlined by FR-007's divider (research R7) |

### 4.1 What is deliberately *not* outlined

| Surface | Why |
|---|---|
| Plugin overlay coloured `Accent` / `Secondary` / `Neutral` | resolves through the recoloured roles — already at the high-contrast value; an outline would only obscure it (FR-011 last sentence) |
| `Package` glyph fallback (O11) | painted in `weak_text_color()`, a role |
| The cue glyph's digit | `text_on_accent` over an unchanged fill; a pre-existing palette property, not this feature's (FR-020, Edge Cases) |
| Any surface at all when high contrast is off | `marker_outline` returns `None` (FR-002, US3 AS-1 "not present in normal mode") |

---

## §5 — Style application

`crates/modplayer-ui/src/theme/mod.rs`:

```rust
static STYLES: OnceLock<[Arc<Style>; 4]> = OnceLock::new();   // [L, D, L-HC, D-HC]

pub fn apply_tokens(ctx: &Context) { apply_tokens_for(ctx, false) }

pub fn apply_tokens_for(ctx: &Context, high_contrast: bool) {
    let styles = STYLES.get_or_init(|| [ /* build all four, once */ ]);
    let base = if high_contrast { 2 } else { 0 };
    ctx.set_style_of(EguiTheme::Light, Arc::clone(&styles[base]));
    ctx.set_style_of(EguiTheme::Dark,  Arc::clone(&styles[base + 1]));
}
```

`style::build_style(theme)` becomes
`style::build_style(theme, high_contrast)`, whose only change is that its
internal `tokens::for_dark_mode(dark_mode)` (`style.rs:98`) becomes
`tokens::for_theme(dark_mode, high_contrast)`. **Every other line of
`visuals()` is untouched** — the whole recolouring falls out of the table
swap, which is what makes FR-013 and FR-017 structurally true.

`build_style` is `pub` and named by `style.rs`'s own tests; the
one-argument form is not preserved (it has no non-test caller), and those
three tests pass `false` explicitly.

### 5.1 Live application (FR-004)

| Moment | Call |
|---|---|
| `App::new`, before the first paint | `theme::apply_tokens_for(&cc.egui_ctx, controller.high_contrast())` (`app.rs:125`) |
| First statement of every frame | `theme::apply_tokens_for(ui.ctx(), self.controller.high_contrast())` (`app.rs:223`) |
| Checkbox toggled | `controller.set_high_contrast(on)` + reload-mutate-save; the next frame's `App::ui` applies it |

No `theme::apply` change: the light/dark axis is still
`ctx.set_theme(ThemePreference)` and is untouched (FR-002).

---

## §6 — The control and its registry row

| Item | Value | Site |
|---|---|---|
| Widget | `switch(ui, SwitchKind::Checkbox, &mut on, &tr("setting-high-contrast"))` | `src/settings/appearance.rs`, below the Theme combo |
| Focus | `if focus == Some("appearance.high_contrast") { response.request_focus(); }` | same, mirroring `:47-49` |
| Descriptor | `SettingDescriptor { category: Appearance, id: "appearance.high_contrast", title_key: "setting-high-contrast", description_key: "setting-high-contrast-desc" }` | `settings_registry.rs`, after `appearance.theme` (`:158-163`) |

### 6.1 Locale keys (en-US only, research R13)

```ftl
setting-high-contrast = High contrast
setting-high-contrast-desc = Raise text, border and focus contrast. Works with any theme.
```

Added to `locales/en-US/settings.ftl` after `setting-theme-dark` (`:50`)
and to `tests/fluent_keys.rs`'s Appearance block (`:530-535`).

---

## §7 — Complete file map

`+` new, `~` modified, `=` unchanged-but-load-bearing as a gate.

```text
crates/
├── modplayer-core/
│   └── src/
│       ├── settings/model.rs      ~ AudioSettings.high_contrast (:108 area);
│       │                            RawAppearance.high_contrast: toml::Value (:489);
│       │                            default_high_contrast(); InvalidField::HighContrast
│       │                            + field_name() arm (:214-224); into_settings()
│       │                            recovery beside the theme match (:691);
│       │                            to_raw() emits Value::Boolean (:564);
│       │                            SCHEMA_VERSION unchanged at 1 (:69)
│       ├── settings/store.rs      = the total-discard path (:123-131) is why
│       │                            §1.2 uses toml::Value — not modified
│       ├── settings_registry.rs   ~ one SettingDescriptor after :163
│       └── controller.rs          ~ high_contrast shadow field (:453 area),
│                                    populated at :808; high_contrast() /
│                                    set_high_contrast()
└── modplayer-ui/
    ├── Cargo.toml                 = unchanged — no new dependency
    ├── src/
    │   ├── theme/
    │   │   ├── tokens.rs          ~ Roles.divider_alpha + .high_contrast;
    │   │   │                        LIGHT_HIGH_CONTRAST / DARK_HIGH_CONTRAST;
    │   │   │                        for_theme(); is_high_contrast(); roles();
    │   │   │                        divider_color_for() one-line change
    │   │   ├── controls.rs        ~ FOCUS_RING_WIDTH_HIGH_CONTRAST;
    │   │   │                        focus_ring_width(); focus_ring() body
    │   │   ├── markers.rs         ~ MARKER_OUTLINE_WIDTH; marker_outline();
    │   │   │                        overlay_outline(); casing_width();
    │   │   │                        paint_host_glyph gains an outline param
    │   │   ├── style.rs           ~ build_style(theme, high_contrast); one line
    │   │   │                        inside visuals() (:98)
    │   │   ├── mod.rs             ~ STYLES -> [Arc<Style>; 4]; apply_tokens_for();
    │   │   │                        re-exports for the new items
    │   │   └── contrast.rs        = unchanged — the ratio() the new tests use
    │   ├── markers.rs             ~ O1-O6: outline at the six palette paint sites
    │   ├── plugin_overlays.rs     ~ O7-O10: outline at the four primitive arms
    │   ├── settings/appearance.rs ~ the checkbox, its focus, its persist branch
    │   ├── app.rs                 ~ :125 and :223 -> apply_tokens_for
    │   ├── shell.rs               = its two apply_tokens calls are #[cfg(test)];
    │   │                            unchanged (research R1)
    │   └── widgets/controls.rs    = switch() reused as-is; its five test-module
    │                                apply_tokens calls unchanged
    ├── tests/
    │   ├── design_token_contrast.rs  ~ NEW high-contrast block; the five existing
    │   │                               tests kept VERBATIM (research R14)
    │   ├── design_token_literals.rs   = EXPECTED_BASELINE_HITS must stay 0 (R15)
    │   ├── design_token_roles.rs      = must pass unmodified
    │   ├── interaction_states.rs      = must pass unmodified (FR-020)
    │   ├── control_variants.rs        = must pass unmodified
    │   ├── accessibility.rs           = must pass unmodified (FR-014)
    │   ├── fluent_keys.rs             ~ two keys added to the Appearance block
    │   ├── markers.rs                 ~ outline presence/absence assertions
    │   ├── plugin_overlays.rs         ~ O7-O11 assertions
    │   └── high_contrast.rs        + NEW: the FR-005/007/008/013/017 value suite
    └── ../modplayer-core/tests/settings.rs  ~ §1.3 recovery cases + the
                                               proptest round-trip (Constitution VIII)
locales/en-US/settings.ftl          ~ two keys after :50
```

**Counts**: new crates **0**; new dependencies **0**; new feature flags
**0**; new traits **0**; new semantic colour roles **0**; new `Theme`
variants **0**; plugin API changes **0**; new locale keys **2**; new
persisted fields **1**; new `Roles` fields **2**; new static tables **2**;
new constants **2**.

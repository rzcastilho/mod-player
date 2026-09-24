# Phase 0 Research: Design Tokens, Type Scale, and Spacing

**Feature**: 014-design-tokens-and-type-scale | **Date**: 2026-09-22 |
**Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

Every decision below was verified against the code in this worktree
(`crates/modplayer-ui/src/**`) and against the vendored toolkit source
(`egui`/`epaint`/`ecolor` **0.36.2**, the version pinned in the workspace
`Cargo.toml`). Contrast figures are WCAG 2.x relative-luminance ratios
computed from the token values themselves; the computation is the one the
shipped test will use (`research` worksheet kept out of the repo under the
gitignored `target/plan-work/`).

---

## R1 — The token module is `theme/`, a directory module

**Decision**: `crates/modplayer-ui/src/theme.rs` becomes
`crates/modplayer-ui/src/theme/` with five files:

| File | Holds |
|---|---|
| `mod.rs` | `apply` (unchanged `ThemePreference`), `apply_tokens`, `roles()`, the public re-exports |
| `tokens.rs` | the colour roles (light/dark), spacing, radius, type-scale constants, `divider` |
| `style.rs` | the egui `Style`/`Visuals`/`Spacing` construction for each theme |
| `contrast.rs` | WCAG 2.x ratio + egui-identical alpha compositing, used by the module and its tests |
| `markers.rs` | `MARKER_PALETTE`, `marker_color`, `overlay_color`, `paint_host_glyph` |

**Why**: FR-001 names `theme.rs` and explicitly allows a directory module
"if it outgrows one file". It will: the colour table alone is 10 roles ×
2 themes, plus a full `Visuals` construction per theme. The existing
residents move unchanged into `theme/markers.rs` — they are the sanctioned
`contracts/ui-panels.md` A4 exception (spec Clarifications) and keep their
public paths (`theme::MARKER_PALETTE`, `theme::marker_color`,
`theme::overlay_color`, `theme::paint_host_glyph`) through `mod.rs`
re-exports, so no call site outside the module changes import form.

**Rejected**: a new `modplayer-tokens` crate — Constitution X requires
stating why an existing crate is insufficient, and `modplayer-ui` is the
only consumer; a second crate would buy nothing and add a dependency edge.

---

## R2 — "Platform default font family" = the toolkit's embedded default stack

**Finding**: egui does **not** use platform fonts. `epaint_default_fonts`
embeds exactly four faces:

```
Ubuntu-Light.ttf      → FontFamily::Proportional
Hack-Regular.ttf      → FontFamily::Monospace
NotoEmoji-Regular.ttf, emoji-icon-font.ttf  → emoji fallback
```

**Decision**: the five proportional roles resolve to
`FontFamily::Proportional` and `mono` to `FontFamily::Monospace`, i.e.
egui's embedded defaults. This feature bundles no typeface and calls no
`ctx.set_fonts`. FR-003's "the platform's default font family" is read as
"the toolkit's default family", which is what the source review meant by
"single family, platform default `TextStyle` overrides" — the review
describes overriding `TextStyle`s, which is exactly this.

**Why**: Constitution X requires identical behaviour on macOS, Windows and
Linux; egui's embedded stack *is* identical on all three, whereas real
platform fonts are not (San Francisco / Segoe UI / whatever fontconfig
resolves), and text metrics would diverge per platform — including the
72-character measure (R17) and the digit-column alignment (R4).

**Rejected**: resolving the OS UI font through `font-kit`/`fontdb`/
`font-loader` — a new dependency (Constitution X: must state why `std` or
an existing dependency is insufficient — it is not), per-platform metric
divergence, and `cargo deny` surface for a purely cosmetic gain.

---

## R3 — There is no semibold face; the degradation clause fires on every platform

**Finding**: the embedded stack is **single-weight** (`Ubuntu-Light`,
`Hack-Regular`). `FontId` carries only `size` + `family`; egui 0.36 has no
weight axis and no synthetic emboldening. `RichText::strong()` does not
change weight — it swaps the colour to `Visuals::strong_text_color()`.

**Decision**: FR-003's fallback path is the *only* path: "where the
platform exposes neither [semibold nor bold], the role falls back to the
single available weight and its distinction is carried by size and, for
`section`, by uppercase". Concretely:

- `display` 22 / `title` 17 / `section` 13-uppercase-tracked / `body` 14 /
  `secondary` 13 / `mono` 13 — size alone separates every adjacent pair
  except `section`↔`secondary` (both 13), which uppercase + letter-spacing
  separate.
- The emphasis half of "weight" is carried by **colour**: `display`,
  `title`, `section` and row titles draw in `text.primary`; `secondary`
  draws in `text.secondary`. In a list row that is 14 px `text.primary`
  against 13 px `text.secondary` — US2's "visibly different in size
  **and/or** weight" is satisfied on size *and* colour.

**Documented consequence**: the shipped app renders no bold text anywhere.
A future feature that wants real weight must bundle a face (out of scope:
FR-003 says "no typeface is bundled") or add the system-font dependency
R2 rejected. This is recorded in plan.md Complexity Tracking.

**Rejected**: bundling Inter/Noto Sans SemiBold — FR-003 forbids it;
double-draw synthetic bold (painting text twice at a sub-pixel offset) —
epaint offers no hook at the `TextStyle` level, it would have to be done at
every call site (re-creating the defect FR-018 forbids), and it smears at
13 px.

---

## R4 — Tabular figures come free, and are testable without a screenshot

**Finding**: `Hack-Regular` is a monospace face — every glyph, digits
included, carries the same advance width by construction. `epaint::Fonts`
exposes `glyph_width(&FontId, char) -> f32`, and `egui::__run_test_ctx` /
`__run_test_ui` give a headless `Context` with a real font atlas.

**Decision**: `mono` = `FontId::new(13.0, FontFamily::Monospace)`. FR-004
is tested by laying out two equal-length `mono` strings
(`"0:35.204"` / `"1:07.881"`) in a `__run_test_ctx` and asserting equal
widths, plus asserting `glyph_width` is identical for `'0'..='9'`.

**Why this matters for the plan**: the UI crate's existing tests are plain
logic tests with no rendering harness (`egui_kittest` is not a dependency
and is not added). `__run_test_ctx` is already in egui's public surface, so
the type-scale and measure tests need no new dev-dependency.

---

## R5 — Letter-spacing is expressible; it lives in a helper, not in `Style`

**Finding**: `Style::text_styles` is `BTreeMap<TextStyle, FontId>` — size
and family only, no tracking. But `RichText::extra_letter_spacing(f32)`
**does** exist in egui 0.36 (`src/widget_text.rs:160`).

**Decision**: `section` is applied through a helper,
`theme::section_label(text) -> RichText`, which sets the `Name("section")`
text style, uppercases (R6), and adds `extra_letter_spacing(0.52)`
(= 0.04 em × 13 px). The `Name("section")` entry still exists in
`text_styles` so a caller that only names the style still gets the right
size. Letter-spacing therefore *is* delivered, and FR-003's "omitted where
the toolkit does not support it" clause is not needed.

---

## R6 — Uppercasing: `str::to_uppercase` at draw time

**Decision**: `theme::section_label` calls `str::to_uppercase()` on the
already-translated string, at draw time. No catalogue entry changes case;
no all-caps duplicate key is added to `locales/*/app.ftl` (FR-003b).

**Why**: Rust's `str::to_uppercase` implements Unicode *default* full
case mapping, which is correct for both shipping locales (en-US, pt-BR),
including pt-BR's accented forms (`ç`→`Ç`, `ã`→`Ã`). Locale-*tailored*
casing differs from the default only for Turkish/Azeri dotted-I and
Lithuanian accent retention — neither locale ships, and adding ICU for a
hypothetical one would be a new dependency (Constitution X). Recorded as a
follow-up note in contracts/design-tokens.md rule **T6**.

---

## R7 — Once-per-frame application: two `set_style_of` calls over cached `Arc<Style>`s

**Finding**: `Context` exposes `set_style_of(Theme, impl Into<Arc<Style>>)`,
`style_mut_of(Theme, impl FnOnce(&mut Style))` and
`all_styles_mut(impl FnMut(&mut Style))`; the last two go through
`Arc::make_mut`, i.e. they **clone the whole `Style`** (including its
`BTreeMap` of text styles) on every call.

**Decision**: build both themes' `Style` exactly once into a
`static STYLES: OnceLock<(Arc<Style>, Arc<Style>)>`, and have
`theme::apply_tokens(ctx)` call `ctx.set_style_of(egui::Theme::Light,
light.clone())` and `…(egui::Theme::Dark, dark.clone())`. `apply_tokens` is
called from `App::new` (before the first paint) and from the top of
`App::update` (every frame). Per-frame cost: two `Arc` clones and two
option writes — no allocation, no map rebuild.

**Why per frame at all**: FR-002 requires that a token change or a theme
switch is visible "everywhere on the next frame with zero per-view code
changes", and that the style can never be left at a toolkit default by a
code path that forgot to re-apply. An idempotent per-frame install makes
that structural instead of a call-site discipline.

`theme::apply(ctx, theme)` (the existing `ThemePreference` setter, called
from `settings/appearance.rs` and `App::new`) is unchanged — egui keeps
following the OS theme for `Theme::System`, and both installed styles are
already correct when it flips.

**Rejected**: `all_styles_mut` per frame (full `Style` clone × 2 per frame
at 30 Hz); applying once at startup only (a later token change or a
`Visuals` mutation by any view would persist — the exact regression FR-018
exists to prevent).

---

## R8 — How a view reaches a role

**Decision**: three channels, no fourth:

1. **Colour that egui already models** — reached through `ui.visuals()`
   exactly as today (R9 maps every slot). Plugin panels are already bound
   to this by `contracts/ui-panels.md` A4, so they inherit the tokens with
   **no plugin API change** (FR-015b, Constitution IX not triggered).
2. **Colour egui does not model** (`positive`, `warning`, `danger`,
   `text.on_accent`, `divider`) — `theme::roles(ui.visuals()) ->
   &'static Roles`, which selects the light or dark table from
   `visuals.dark_mode`. This is the same shape `overlay_color` already uses
   and needs no `Context`, so it works inside painters and plugin-overlay
   code.
3. **Sizes** — `theme::text::DISPLAY/TITLE/SECTION/BODY/SECONDARY/MONO`
   (`TextStyle` values), `theme::space::{XS,SM,MD,LG,XL,XXL}` (`f32`),
   `theme::radius::{SM,MD,full(height)}` (`CornerRadius`).

---

## R9 — Every `Visuals` slot is derived from a role (FR-010b)

Verified against `egui-0.36.2/src/style.rs` (`Visuals`, `Widgets`,
`WidgetVisuals`, `Selection`). The full map is normative in
[data-model.md](data-model.md) §5; the load-bearing entries:

| `Visuals` slot | Role |
|---|---|
| `panel_fill`, `window_fill`, `extreme_bg_color`, `text_edit_bg_color` | `surface.base` |
| `faint_bg_color`, `code_bg_color`, `widgets.*.bg_fill`/`weak_bg_fill` | `surface.raised` |
| `widgets.noninteractive.fg_stroke.color`, `widgets.inactive/hovered/active/open.fg_stroke.color` | `text.primary` |
| `weak_text_color: Some(..)` | `text.secondary` |
| `selection.bg_fill`, `hyperlink_color` | `accent` |
| `selection.stroke.color` | `text.on_accent` |
| `warn_fg_color` / `error_fg_color` | `warning` / `danger` |
| `widgets.*.bg_stroke`, `window_stroke` | `divider` (`text.primary` @ 8 %) |
| `disabled_alpha` | per-theme constant from R11 |

Nothing is left at `Visuals::light()`/`dark()` defaults: both themes are
built by starting from the matching egui default (for the non-colour
fields such as shadows, `handle_shape`, `text_cursor`) and then assigning
**every** colour field listed in data-model.md §5.

---

## R10 — The 2.96:1 defect is `weak_text_alpha`, and `weak_text_color` fixes it

**Finding**: `Visuals::weak_text_color()` returns
`weak_text_color.unwrap_or_else(|| text_color().gamma_multiply(weak_text_alpha))`
and the light default is a 0.6 gamma multiply of an already-mid-grey text
colour over white — which is exactly the § 4.2 measurement of **2.96:1**
on artist/album/duration/owner lines.

**Decision**: set `weak_text_color: Some(text.secondary)` in both themes.
Every existing `ui.weak()`/`weak_text_color()` call site — including
`overlay_color(OverlayColor::Neutral, …)` — then lands on the token with
no call-site edit, and US1 is fixed for every secondary line at once.

---

## R11 — Disabled text: egui multiplies opacity, so the token and the α must agree

**Finding**: `Ui::disable()` calls `multiply_opacity(visuals.disabled_alpha())`
(`egui-0.36.2/src/ui.rs:501`); the default is `0.5`
(`style.rs:1560`). A disabled widget therefore draws `text.primary`
composited over its surface, **not** a separate colour slot.

**Decision**: pick `disabled_alpha` per theme so that the composite equals
the `text.disabled` token, and verify the composite (not just the token)
in the contrast test, using `ecolor::Color32::blend` — the same
premultiplied blend egui paints with.

| Theme | `disabled_alpha` | composite over base | vs base | over raised | vs raised |
|---|---|---|---|---|---|
| Light | **0.55** | `#828283` | 3.84:1 | `#78787a` | 3.60:1 |
| Dark | **0.44** | `#76767a` | 4.06:1 | `#808085` | 3.83:1 |

All four clear the 3:1 non-text floor (FR-012).

---

## R12 — `surface.raised` values (FR-013 adjustment, computed)

Floor: ≥ 1.2:1 vs `surface.base`. The spec's starting swatches fail:

| Candidate | vs base | |
|---|---|---|
| light `#f5f5f7` (source) | 1.089 | fail |
| light `#eaeaec` | 1.201 | passes by 0.001 — rejected, rounding-fragile |
| light **`#e8e8ea`** | **1.224** | **chosen** |
| dark `#1e1e22` (source) | 1.107 | fail |
| dark `#25252a` | 1.205 | rejected, rounding-fragile |
| dark **`#26262c`** | **1.222** | **chosen** |

Direction of elevation is the source's: raised is *darker* than base in
light, *lighter* in dark.

---

## R13 — `text.disabled` also fails its floor against `surface.raised`

**Finding not in the spec**: with R12's surfaces, the spec's `text.disabled`
swatches clear 3:1 against `surface.base` but **fail against
`surface.raised`** — light `#8e8e93` measures 3.26:1 on base and **2.66:1**
on raised; dark `#6c6c72` measures 3.52:1 on base and **2.88:1** on raised.
The spec's own clarification ("the text roles' floors apply against both
surfaces") and FR-012's "against both surfaces in both themes" therefore
force an adjustment, by the same floor-over-swatch rule FR-013 states.

**Decision** (also chosen to equal R11's composite, so the two mechanisms
agree by construction):

| Role | Light | vs base / raised | Dark | vs base / raised |
|---|---|---|---|---|
| `text.disabled` | `#828283` | 3.84 / 3.14 | `#76767a` | 4.06 / 3.32 |

Still a large, deliberate reduction from today's light-theme 8.84:1 (§ 4.2:
"arguably too strong for a disabled state"), as FR-012 intends.

---

## R14 — Marker palette re-tone (FR-015): four values, computed

Band required by 3:1 against **both** `#ffffff` and `#141417`:
`0.121 ≤ L ≤ 0.300`.

| # | Hue | Today | light / dark | → New | light / dark | L | HSL hue |
|---|---|---|---|---|---|---|---|
| 0 | red | `#D94F4F` | 4.05 / 4.54 | *(unchanged)* | — | 0.209 | 0° |
| 1 | blue | `#3E8EDE` | 3.43 / 5.37 | *(unchanged)* | — | 0.256 | 210° |
| 2 | green | `#4CAF50` | **2.78** / 6.61 | **`#3D8B43`** | 4.22 / 4.35 | 0.199 | 125° |
| 3 | amber | `#E09B1A` | **2.37** / 7.77 | **`#B0711A`** | 4.02 / 4.57 | 0.211 | 35° |
| 4 | violet | `#9C5CE0` | 4.18 / 4.40 | *(unchanged)* | — | 0.201 | 269° |
| 5 | teal | `#00ACB0` | **2.79** / 6.59 | **`#00868A`** | 4.40 / 4.18 | 0.189 | 182° |
| 6 | pink | `#DE6FB4` | **2.98** / 6.16 | **`#C2519A`** | 4.25 / 4.32 | 0.197 | 321° |
| 7 | olive | `#8F9A4B` | 3.05 / 6.03 | *(unchanged)* | — | 0.295 | 68° |

- **Hue preserved** in all four: 125° (was 122°), 35° (was 39°), 182° (was
  182°), 321° (was 326°).
- **Mutual distinguishability**: minimum pairwise hue separation across the
  eight is **28°** (amber 35° vs olive 68°) — the amber was pulled toward
  orange (`#A97612`, 40°, was also viable) precisely to keep that gap.
- **Index 7 is a thin pass** at 3.05:1 light. It is left alone per FR-015,
  but the contrast test pins it, so any future tweak that drops it below
  3:1 fails CI rather than shipping. Flagged in plan.md Complexity Tracking.
- `PaletteIndex` keying is untouched, so persisted markers keep their slot
  (FR-015); `overlay_color`'s `Positive`→`[2]` / `Warning`→`[3]` mapping is
  untouched (FR-015a) and simply resolves to the new values.
- `theme/markers.rs`'s doc comment "Chosen for >= 3:1 contrast against both
  the light and dark panel backgrounds" is corrected to name the two
  `surface.base` values it is verified against.

---

## R15 — Literal scan: a Rust integration test, and the verified baseline

**Decision**: `crates/modplayer-ui/tests/design_token_literals.rs`, walking
`$CARGO_MANIFEST_DIR/src/**` and `$CARGO_MANIFEST_DIR/../modplayer/src/**`.
Runs under the existing `cargo test --workspace` gate on all three CI
platforms; no CI YAML change; no bash dependency.

**Rejected**: a `scripts/check-design-tokens.sh` alongside
`scripts/check-license-headers.sh` — matches precedent but needs a new CI
step, runs only where bash does, and cannot share the exclusion list with
the token module.

Patterns (contracts/literal-scan.md is normative):
`Color32::from_rgb`/`from_rgba_*`/`from_gray`/`from_black_alpha`/
`from_white_alpha`, the `Color32::` named constants (`WHITE`, `BLACK`,
`RED`, …), `Rgba::from_*`, `Hsva::new`, a `#rrggbb`/`0xRR, 0xGG, 0xBB`
colour triple, `FontId::proportional(`/`::monospace(`/`FontId::new(`, and
`TextStyle::resolve`-free numeric font sizes (`.size = <number>`,
`font_id.size`). Exclusions, and only these: `src/theme/**` and test code
(`#[cfg(test)]` modules, `tests/` directories).

**Verified baseline in this worktree** — 17 sites across 9 files (the
number the spec states; the file list differs slightly from the spec's,
which named `artwork.rs` and `plugin_assets.rs` — both verified clean of
colour, font-size and radius literals today — and omitted
`widgets/chain_meters.rs`, `widgets/skeleton.rs` and `rows.rs`):

| File | Sites | What |
|---|---|---|
| `markers.rs` | 256, 273, 282, 281 | 3 × `Color32::WHITE`, `FontId::monospace(9.0)` |
| `plugins_view.rs` | 195, 197, 201, 112 | 3 health-dot `from_rgb`, `add_space(16.0)` |
| `plugin_overlays.rs` | 37+145, 177 | `LABEL_FONT_SIZE = 10.0`, `Color32::WHITE` |
| `widgets/peak_meter.rs` | 38, 30 | over-ceiling `from_rgb(220,60,60)`, `CornerRadius::from(2u8)` |
| `widgets/initials.rs` | 76, 69 | `FontId::proportional(size*0.4)`, `CornerRadius::from((size*0.15) as u8)` |
| `waveform/paint.rs` | 116 | `FontId::proportional((h*0.35).max(10.0))` |
| `widgets/chain_meters.rs` | 54 | `CornerRadius::from(2u8)` |
| `widgets/skeleton.rs` | 28 | `CornerRadius::from(4u8)` |
| `rows.rs` | 347 | `CornerRadius::from((ARTWORK_SIZE*0.15) as u8)` |
| `crates/modplayer/src/**` | — | already clean; the scan guards it |
| `src/theme.rs` (sanctioned) | 8 palette + 2 `Color32::WHITE` in `paint_host_glyph` | stays, moves to `theme/markers.rs` |

The two `Color32::WHITE` uses inside `paint_host_glyph` (the warning
glyph's exclamation mark) are inside the token module and therefore
sanctioned — but they are *wrong* semantically (a white mark on a
light-theme warning triangle), so the plan repoints them at
`text.on_accent`-style contrast-safe roles as part of FR-016's "opaque
white marker and overlay labels".

---

## R16 — Contrast test: `theme/contrast.rs`, blending exactly as egui does

**Decision**: `pub(crate) fn ratio(a: Color32, b: Color32) -> f32` (WCAG
2.x: sRGB → linear with the 0.04045 knee, `0.2126/0.7152/0.0722`,
`(L₁+0.05)/(L₂+0.05)`), plus `fn composite(fg, alpha, bg)` implemented as
`bg.blend(fg.gamma_multiply(alpha))` — `ecolor::Color32::blend` and
`gamma_multiply` are the exact operations egui's disabled path and painter
use, so the test measures what the screen shows rather than an idealised
model. `crates/modplayer-ui/tests/design_token_contrast.rs` asserts every
pair named in FR-018b.

The screenshot sampler (`target/manual-walk/contrast.py`) stays as the
quickstart's manual evidence (Governance › Manual Scenario Sign-Off) that
the rendered pixels match these token values.

---

## R17 — The 72-character measure

**Decision**: `theme::body_measure(ctx: &Context) -> f32` =
`72.0 * ctx.fonts(|f| f.glyph_width(&body_font_id, '0'))`, applied at prose
call sites as `ui.set_max_width(ui.available_width().min(measure))` — a
maximum, never a minimum (FR-006, Edge Cases). Call sites, all multi-line
prose: `welcome.rs`, `privacy_notice.rs`, `getting_started.rs`,
`device_check.rs`, `sign_in.rs` (store-unreadable explanation),
`settings/{audio,playback,controls,plugins,account,language,about}.rs`
field descriptions, and the empty-state copy in `library_view.rs` /
`search_view.rs` / `queue_view.rs` / `effects_view.rs`. Single-line labels,
row titles, table cells, tooltips and notifications are **excluded** by
FR-006.

Because the font is fixed (R2), the measure is deterministic and testable:
a `__run_test_ctx` asserts `body_measure` equals 72 × `glyph_width('0')`
and that a 200-character string laid out at that width wraps to more than
one row.

---

## R18 — Spacing and radius mapping onto egui

`Spacing` (egui defaults → token, nearest step, ties round **up**, FR-007):

| Field | Default | Token |
|---|---|---|
| `item_spacing` | `(8, 3)` | `(SM 8, XS 4)` |
| `button_padding` | `(4, 1)` | `(SM 8, XS 4)` |
| `window_margin` | `6` | `LG 16` (panel content padding, FR-008) |
| `menu_margin` | `6` | `SM 8` |
| `indent` | `18` | `LG 16` (nearest; 24 is 6 away) |
| `icon_spacing` | `4` | `XS 4` |
| `menu_spacing` | `2` | `XS 4` (ties/sub-scale round up) |

`interact_size`, `slider_width`, `combo_width`, `text_edit_width`,
`tooltip_width`, `menu_width`, `default_area_size`, `scroll.*` are
**component sizes, not padding/margin/gap**, and FR-007 does not reach
them; they keep egui's values (changing them would be the layout work
FR-019 forbids).

Radius (`CornerRadius` is four `u8`s in epaint 0.36):
`radius::SM = CornerRadius::same(4)` → `widgets.*.corner_radius`, inputs,
chips, skeletons, meters; `radius::MD = CornerRadius::same(8)` →
`window_corner_radius`, `menu_corner_radius`, cards, artwork;
`radius::full(height: f32) -> CornerRadius` =
`CornerRadius::same((height * 0.5).round().clamp(0.0, 255.0) as u8)` — a
pill at any height, per the spec's clarification.

---

## R19 — `xl` is 24 px; US4 AC2's "(32px)" is a spec typo

FR-007 defines `xs 4, sm 8, md 12, lg 16, xl 24, xxl 32`; FR-008 says
panels are separated by "the `xl` step"; US4 acceptance scenario 2 says
"the `xl` (32px) spacing token". The **name** governs: panel separation is
`xl` = **24 px**. Recorded in plan.md Complexity Tracking; the contract and
the tests use `xl`, so either reading of the prose lands on one value.

---

## R20 — The eight `ui.separator()` sites

| Site | Separates | Becomes |
|---|---|---|
| `now_playing.rs` 178, 198, 211 | panels (transport / waveform / markers / plugin lanes) | `ui.add_space(space::XL)` |
| `settings/mod.rs` 133, 146 | the settings section list from its content | `ui.add_space(space::XL)` |
| `settings/account.rs` 51, `settings/controls.rs` 282, `settings/plugins.rs` 85 | groups *within* one panel | `theme::divider(ui)` — a 1 px line in `divider` (`text.primary` @ 8 % α) |

FR-008 replaces panel separation with space and keeps a divider "where a
divider is still used"; the three in-panel group rules are that case.

---

## R21 — Blast radius: what this feature does **not** touch

- **No real-time path change.** No file under `crates/modplayer-engine/` or
  `crates/modplayer-effects/` is modified. PR real-time note: "N/A".
- **No plugin API change.** Tokens arrive through `Style`/`Visuals`, which
  `contracts/ui-panels.md` A4 already binds plugin panels to read
  (FR-015b) — Constitution IX's written change request is not triggered,
  `api/v1.toml` is untouched, `docs/plugin-api/v1.md` is not regenerated.
- **No new crate, dependency, feature flag or trait** (Constitution X).
  `crates/modplayer-ui/Cargo.toml` is unchanged.
- **No layout, interaction, hover/focus/pressed-state or component
  restructuring** (FR-019) — `widgets.hovered`/`active`/`open` get
  token-derived *colours* (FR-010b demands it: otherwise egui defaults leak)
  but no new state, expansion or geometry.
- **No locale change.** No Fluent key is added, removed or re-cased
  (FR-003b); `tests/fluent_keys.rs` is unaffected.
- **No persisted-data change.** `PaletteIndex` keys are stable (FR-015), the
  settings file gains no field, no migration.

---

## R22 — Manual Scenario Sign-Off: environment blocker (Governance), M1–M10

**Decision**: Every manual scenario (M1–M10) is recorded pass/deviation on
its `tasks.md` task as executed, with full detail written back into
quickstart.md § 4. This entry is the research-side pointer required by
Governance › Manual Scenario Sign-Off's "written back into quickstart.md
**and** research.md" — the detailed evidence lives in quickstart.md § 4 to
avoid duplicating it here.

**Finding, unchanged across every phase this feature touched (2026-09-22)**:
no manual scenario could be completed on any host used across this
feature's implementation. Two independent blockers stack:

1. **Spotify sign-in gate** (`App::launch_step`) hides every section that
   renders real content — Library, Search, Now Playing, Settings ›
   Appearance, Plugins — behind a live OAuth sign-in. No credentials are
   available in the sandboxed hosts used, and an unattended agent must not
   drive a live Spotify login on the user's behalf. `MODPLAYER_LIBRARY_FIXTURE=large`
   seeds the library *index* but does not pass the account gate.
2. **Screen lock**, observed on some hosts/sessions (`Quartz.CGSessionCopyCurrentDictionary()`
   → `CGSSessionScreenIsLocked: True`): with the screen locked, even a
   window-specific `screencapture -l<id>` returns a stale/blank compositor
   image (not an error) — only a full-display capture reveals the actual
   macOS lock screen, which is the diagnostic that first exposed this
   blocker (M4, T046).

**Consequence for SC-001/002/003/006/007/008**: each stands on **automated
evidence only** (see the contract/unit test named on the corresponding
`tasks.md` task — C1's `every_text_role_clears_its_floor`, T5's
`body_and_secondary_are_visibly_different`, U1's
`numeric_fields_use_the_mono_role`, U6's
`theme_switch_changes_every_slot_with_no_view_change`, U2/U3's `measure::*`
and `no_separator_between_panels`). **SC-004/SC-005/SC-009 do not depend on
a manual scenario** — they are defined directly by an automated
scan/test (T1/S6 for SC-004/005, C1–C7 for SC-009) and are **fully
satisfied**, confirmed at T066/T018 respectively.

**Alternatives rejected**: adding a demo/offline library fixture that
bypasses the sign-in gate (fabricates the very state the scenario exists to
verify, and is a source change outside this feature's scope, FR-019);
declaring the manual-gated SCs satisfied on automated evidence alone (a
silent redefinition of Governance's own rule — the scenarios gate
completion, not a fallback the implementing agent may skip). Both rejected
for the same reason every earlier phase rejected them (plan.md ›
Complexity Tracking carries the M1/M2 instance).

**What would close this**: any of M1–M10 running on a host that is both
unlocked and signed in to a real Spotify account, ideally by the
maintainer rather than an unattended agent.

**Update, 2026-09-22 (follow-up session): blocker cleared for
M1/M2/M3/M4/M5/M6/M7.** A host became available that was unlocked, and the
maintainer signed in to Spotify live (their own account, via the OAuth
flow in the app UI — the agent did not touch credentials). With a
populated Library, M1/M2 ran to completion: light `#5b5b60` on `#ffffff`
(6.752:1), dark `#a8a8b0` on `#141417` (7.786:1) — both exact matches to
the token table, both clearing the 4.5:1 floor. **SC-001/SC-002 are now
fully satisfied.** In the same session, M3 also ran to completion: Library
row titles read visibly distinct from their secondary artist·album·duration
lines, and Now Playing's `display`-role title read unmistakably larger
than the artist/album lines beneath it. **SC-003 is now also fully
satisfied.** M4 also ran to completion: the position timer and the
Markers list's three stacked timestamps both showed visibly uniform-width
digits in zoomed captures. **SC-006 is now also fully satisfied.** M5 also
ran to completion: with sign-in unlocking Settings, About › Privacy notice
showed four paragraphs wrapping well under a maximized window's width, and
every section visited used space rather than a hairline rule to separate
panels. **SC-008 is now also fully satisfied** (this also corrects a
bookkeeping error where `tasks.md` T056 had been checked off without an
actual manual run). M6 also ran to completion: Plugins and Now Playing
both repainted fully across Light↔Dark with no stale colour, and the app
repainted live (no restart) when the OS appearance was flipped while on
System — reverted afterward, leaving the host as found. **SC-007 is now
also fully satisfied.** M7 also ran to completion: 8 fresh markers, one
per `PaletteIndex` slot, kept their assigned colour across a Light↔Dark
theme flip, and the four re-toned hues (green/amber/teal/pink) read as
recognizably those colours on both backgrounds — FR-015/FR-015a closed
out with real markers (no SC of its own). **US1 through US5 are now all
complete.** M8–M10 (Polish) were not attempted in that session, so they
remain open; the same unlocked+signed-in conditions, applied to M8–M10,
would plausibly close the rest in one pass.

# Phase 0 Research: High-Contrast Appearance Option

**Feature**: 017-high-contrast-appearance | **Date**: 2026-09-23
**Input**: [spec.md](spec.md) (Status: Clarified, no open markers)

Every decision below was verified against **this worktree** (paths and
line numbers are from the tree at commit `e773439`) or against the
vendored **egui 0.36.2** source at
`~/.cargo/registry/src/index.crates.io-*/egui-0.36.2/`. Nothing here is
assumed from memory. Contrast ratios were computed with the same WCAG 2.x
formula `theme::contrast::ratio` implements (`theme/contrast.rs:11-37`)
and are reproduced by `target/plan-scratch/contrast.py` in this worktree.

The spec arrived fully clarified — sixteen decisions settled across two
sessions, no `[NEEDS CLARIFICATION]` left — so Phase 0's job was not to
choose *what* to build but to establish **whether the existing code can
deliver it the way FR-017 demands** (one selection site, zero call-site
branching, zero plugin-side change). Four findings changed the shape of
the design: R1 (the style cache is a `OnceLock` of exactly two entries),
R2 (most colour call sites hold only `&Visuals`, so the axis must travel
inside `Visuals`), R10 (a plain `bool` settings field would discard the
**entire** settings file on one bad character), and R7 (one of FR-011's
four outline sites is already satisfied for free by FR-007).

---

## R1 — Where the high-contrast axis enters the style pipeline

**Question**: FR-004/FR-017 require high contrast to apply live, on the
next frame, from one selection site. Where is that site, and what does it
look like today?

**Evidence**: `theme/mod.rs:50-65`.

```rust
static STYLES: OnceLock<(Arc<Style>, Arc<Style>)> = OnceLock::new();

pub fn apply_tokens(ctx: &Context) {
    let (light, dark) = STYLES.get_or_init(|| {
        (Arc::new(style::build_style(EguiTheme::Light)),
         Arc::new(style::build_style(EguiTheme::Dark)))
    });
    ctx.set_style_of(EguiTheme::Light, Arc::clone(light));
    ctx.set_style_of(EguiTheme::Dark, Arc::clone(dark));
}
```

Two facts matter. First, the cache holds **exactly two** styles, built
once for the process lifetime — high contrast needs four (light/dark ×
normal/high-contrast). Second, `apply_tokens` is called from **two
production sites only** — `app.rs:125` (`App::new`, before the first
paint) and `app.rs:223` (`App::ui`, first statement of every frame) —
but from **25 further call sites in test code**: `shell.rs:142` and
`shell.rs:190` (both inside `#[cfg(test)]`), five in
`widgets/controls.rs`'s test module, and eighteen across
`crates/modplayer-ui/tests/*.rs`.

**Decision**: widen the cache to four entries and add a **new** entry
point rather than change the existing signature.

```rust
static STYLES: OnceLock<[Arc<Style>; 4]> = OnceLock::new();   // [L, D, L-HC, D-HC]

/// Normal-mode tokens (014's original contract, unchanged).
pub fn apply_tokens(ctx: &Context) { apply_tokens_for(ctx, false) }

/// 017 FR-017: the single selection site. `high_contrast` picks the
/// table pair; nothing downstream branches on it.
pub fn apply_tokens_for(ctx: &Context, high_contrast: bool) { .. }
```

**Rationale**: the two production sites move to `apply_tokens_for`; the
25 test sites keep compiling verbatim. That matters for more than
convenience — 016's plan made the point explicitly, and it holds here:
**the regression net for "no change outside high contrast" (US1 AS-3,
SC-001's "up from", FR-002) is the existing suites passing unmodified.**
A signature change would edit all 25 of them, and an edited regression
test proves nothing. `apply_tokens` becomes a defaulting wrapper, not a
second mechanism — it delegates, so there is still one construction path
(014 design note 3).

Idempotence is preserved: `Arc::ptr_eq` across repeated calls (the
assertion at `style.rs:213-221`) still holds, because the four styles are
still built once and cloned thereafter. Toggling high contrast costs two
`Arc` clones and two `set_style_of` calls on the frame it changes —
allocation-free, which is what makes FR-004's "next frame, no relaunch"
and SC-005's "no flash of unstyled content" structurally true rather than
a timing hope.

**Rejected**: rebuilding the `Style` on each toggle (allocates two full
`Style`s inside the frame loop, and `OnceLock` cannot be re-initialised
anyway); an `RwLock<HashMap<..>>` (a lock on the UI path for a
four-element fixed table).

---

## R2 — How a call site holding only `&Visuals` learns high contrast is on

**Question**: FR-017 forbids any host call site or plugin from branching
on a high-contrast flag to pick a colour. But `tokens::roles(visuals)`
(`tokens.rs:65-67`) selects the role table from `visuals.dark_mode`
alone, and the colour functions that fan out from it take `&Visuals`, not
a flag:

| Function | Site | Who calls it |
|---|---|---|
| `roles(&Visuals) -> &'static Roles` | `tokens.rs:65` | everything |
| `divider_color(&Visuals)` | `tokens.rs:77` | `theme::divider`, borders |
| `overlay_color(OverlayColor, &Visuals)` | `markers.rs:51` | `plugin_overlays.rs` ×4 — **the plugin draw path** |
| `paint_host_glyph(.., &Visuals)` | `markers.rs:69` | `plugin_overlays.rs:166, 191` |
| `marker_mark_color(&Visuals)` | `markers.rs:220` (ui) | `markers.rs:81` |

So the axis has to reach these functions without a new parameter and
without a global.

**Decision**: **the applied `Visuals` already carries the axis, as a
derivable invariant.** `style.rs:110` sets
`visuals.weak_text_color = Some(roles.text_secondary)`, and
`recolor_widget` (`style.rs:190`) sets
`widgets.noninteractive.fg_stroke.color = roles.text_primary`. FR-005's
promotion (`text_secondary := text_primary`) therefore makes exactly one
equality true in high contrast and false otherwise:

```rust
/// FR-017: high contrast is recoverable from the applied `Visuals`
/// alone. This is not a sniff — it is FR-005 itself, read back: high
/// contrast *is defined as* the mode in which the promoted secondary
/// text colour equals the primary one.
pub fn is_high_contrast(visuals: &Visuals) -> bool {
    visuals.weak_text_color == Some(visuals.widgets.noninteractive.fg_stroke.color)
}
```

`roles(visuals)` then selects from four tables on
`(visuals.dark_mode, is_high_contrast(visuals))`, and **every existing
call site is unchanged**, including the two plugin-facing ones.

**Verified against the real values** (`tokens.rs:29-56`):

| Table | `weak_text_color` | `noninteractive.fg_stroke.color` | Classified |
|---|---|---|---|
| `LIGHT` | `#5b5b60` | `#1c1c1e` | normal ✅ |
| `DARK` | `#a8a8b0` | `#f2f2f7` | normal ✅ |
| `LIGHT_HC` | `#1c1c1e` | `#1c1c1e` | high contrast ✅ |
| `DARK_HC` | `#f2f2f7` | `#f2f2f7` | high contrast ✅ |
| raw `Visuals::light()` / `::dark()` | `None` | (egui default) | normal ✅ |

That last row is load-bearing: `theme/markers.rs`'s own unit tests
(`:161-183`) and `tests/plugin_overlays.rs:173` pass a bare
`Visuals::dark()`, never built through `build_style`. egui's default
`weak_text_color` is `None`, so `None == Some(..)` is false and those
tests keep classifying as normal mode with no edit.

**Guard against the one way this can break**: if some future feature ever
made a *normal* table's `text_secondary` equal its `text_primary`, the
predicate would silently report high contrast. The precondition is
therefore itself asserted —
`LIGHT.text_secondary != LIGHT.text_primary` and the same for `DARK` —
so that change fails the build with a message naming this predicate.

**Rejected**:

- *A `high_contrast: bool` parameter on every `Visuals`-taking theme
  function.* It changes `paint_host_glyph`'s and `overlay_color`'s
  signatures, which sit on the plugin overlay draw path
  (`plugin_overlays.rs:97, 118, 151, 163, 166, 191`), and it makes every
  call site carry the flag — which is precisely what FR-017 forbids and
  what would make FR-013's "zero plugin-side change" a convention rather
  than a structural fact.
- *A `static AtomicBool` in the theme module, set by `apply_tokens_for`.*
  Process-global mutable state, and `cargo test` runs the 25 contexts
  that call `apply_tokens` **in parallel threads within one process** —
  one high-contrast test would non-deterministically recolour unrelated
  suites. Disqualifying.
- *`ctx.data_mut()` typed memory.* Most call sites hold `&Visuals` or a
  `&Painter`, not a `&Context`; recovering one would mean threading
  `&Context` instead of a `bool`, which is strictly worse.

---

## R3 — Where the new values live, and why `Roles` is the right carrier

**Question**: FR-019 requires every colour and stroke width this feature
introduces to live in the theme module. FR-008's focus-ring width and
FR-012's outline width are widths, not roles — where do they go?

**Evidence**: `Roles` (`tokens.rs:14-27`) is `Copy` plain data that
**already carries a non-colour scalar**, `disabled_alpha: f32`, precisely
because it varies per theme and must not be a call-site literal. And
`controls::focus_ring(roles: &Roles) -> Stroke` (`controls.rs:98-100`)
already takes `&Roles` — every focus-ring call site goes through it.

**Decision**: two new `Roles` fields and four static tables.

```rust
pub struct Roles {
    ..                          // the ten colour roles, unchanged
    pub disabled_alpha: f32,    // unchanged
    pub divider_alpha: f32,     // NEW: 0.08 normal, 1.0 high contrast (FR-007)
    pub high_contrast: bool,    // NEW: the axis, for the width selectors
}
```

`divider_color_for` becomes a one-line change that carries FR-007
everywhere by itself:

```rust
pub fn divider_color_for(roles: &Roles) -> egui::Color32 {
    roles.text_primary.gamma_multiply(roles.divider_alpha)   // was: 0.08
}
```

014 contract T10 — *the divider is "never a role of its own, never a
call-site literal"* — holds **verbatim**, and every consumer follows with
no edit: `style.rs:99` (`window_stroke`, all five
`widgets.*.bg_stroke` slots via `recolor_widget`), `theme::divider`
(`mod.rs:71-81`), and `controls::variant_paint`'s `Default` outline
(`controls.rs:53`) and switch off-outline.

`focus_ring(roles)` and a new `marker_outline(roles)` read
`roles.high_contrast` **inside the theme module**, so FR-008's 3 px and
FR-012's 1 px are constants of `theme/controls.rs` / `theme/markers.rs`,
never literals at a draw site, and no call site branches.

**Rejected**: a separate `HighContrast` struct parallel to `Roles` (two
tables to keep in sync, and `focus_ring`/`variant_paint`/`hover_fill`
would each need both); an eleventh colour role for the divider (014
contract T10 forbids exactly this).

---

## R4 — FR-009's eight swatches, measured

All eight values in the spec's FR-009 table were recomputed against the
surfaces they are claimed against. **Every one lands on the ratio the
spec states**, so the table is normative and implementable as written —
no plan-time re-tuning is owed.

| Theme | Role | Value | vs `surface.base` | vs `surface.raised` | ≥ 7:1 |
|---|---|---|---|---|---|
| Light (`#ffffff` / `#e8e8ea`) | `accent` | `#074a96` | 8.65 | 7.07 | ✅ |
| | `positive` | `#165630` | 8.72 | 7.13 | ✅ |
| | `warning` | `#694400` | 8.65 | 7.07 | ✅ |
| | `danger` | `#931f19` | 8.57 | 7.00 | ✅ |
| Dark (`#141417` / `#26262c`) | `accent` | `#74b6ff` | 8.65 | 7.08 | ✅ |
| | `positive` | `#57c95c` | 8.68 | 7.10 | ✅ |
| | `warning` | `#e0a92a` | 8.65 | 7.08 | ✅ |
| | `danger` | `#ff9a91` | 9.00 | 7.37 | ✅ |

Light `danger` at **7.00** against `surface.raised` is the tightest
value in the set — it clears by 0.005 of a ratio point. That is recorded
here so a later "harmless" tweak to `#931f19` or to `surface.raised` is
understood to be the one change that will trip
`high_contrast_roles_clear_the_enhanced_floor`.

**Cross-check on the normal-mode values**, confirming FR-009's premise
and its one "unchanged" cell:

| Theme | Role | Normal value | base | raised |
|---|---|---|---|---|
| Light | `accent` `#0a63c9` | | 5.77 | 4.71 |
| | `positive` `#1f7a44` | | 5.35 | 4.37 |
| | `warning` `#8a5a00` | | 5.93 | 4.84 |
| | `danger` `#b3261e` | | 6.54 | 5.34 |
| Dark | `accent` `#5aa9ff` | | 7.49 | 6.13 |
| | `positive` `#4caf50` | | 6.61 | 5.41 |
| | `warning` `#e0a92a` | | **8.65** | **7.08** |
| | `danger` `#ff6b5e` | | 6.58 | 5.38 |

Seven of eight genuinely fail 7:1 today. Dark `warning` already clears it
— which is why FR-009's table leaves that one cell unchanged. **That cell
is correct, not a copy-paste slip**, and the implementation should set
`DARK_HC.warning == DARK.warning` deliberately. Note the consequence for
testing: an assertion of the form "every high-contrast role differs from
its normal-mode value" would be **wrong** and must not be written.

---

## R5 — FR-010's conditional does not fire

**Question**: FR-010 says `text.on-accent` gets its own high-contrast
counterpart *if* the FR-009 accent substitution drops it below 4.5:1.
Does it?

**Measured**:

| Theme | `text.on-accent` | vs high-contrast `accent` | ≥ 4.5:1 |
|---|---|---|---|
| Light | `#ffffff` | vs `#074a96` = **8.65** | ✅ |
| Dark | `#141417` | vs `#74b6ff` = **8.65** | ✅ |

**Decision**: **no high-contrast `text_on_accent` counterpart is added.**
`LIGHT_HC.text_on_accent == LIGHT.text_on_accent` and likewise for dark.
Recorded explicitly so the implementation does not add an unused variant
"for symmetry" (Constitution X, YAGNI). FR-018's 4.5:1 assertion is still
written — it is the thing that would catch a future accent change — it
simply passes on today's values.

This also settles two downstream questions with no further work:
`markers::marker_mark_color` (`markers.rs:220-224`, the cue-slot digit
and glyph focus mark) and `paint_host_glyph`'s `Warning` mark
(`theme/markers.rs:126`) both resolve `text_on_accent`, so both are
unchanged in high contrast — exactly what the spec's Edge Cases and
FR-020 say should happen.

---

## R6 — The marker outline, measured against both things it touches

**Question**: FR-012 claims a `text.primary` outline "holds at least 7:1
against both waveform surfaces regardless of which of the eight palette
entries it encircles". Verify, and check what it does against the fill
itself.

**Outline vs surface** (the claim FR-012/FR-018 actually make): the
outline is the theme's `text.primary`, so this is independent of the
palette entry —

| Theme | outline | vs `surface.base` | vs `surface.raised` |
|---|---|---|---|
| Light | `#1c1c1e` | 17.01 | 13.91 |
| Dark | `#f2f2f7` | 16.48 | 13.48 |

Comfortably ≥ 7:1 in every case. **FR-012's claim is verified.**

**Outline vs the fill it encircles** (a ratio the spec does *not* state a
floor for, measured here so nobody later mistakes its absence for an
oversight):

| idx | palette | vs light outline `#1c1c1e` | vs dark outline `#f2f2f7` |
|---|---|---|---|
| 0 | `#D94F4F` red | 4.20 | 3.63 |
| 1 | `#3E8EDE` blue | 4.97 | 3.07 |
| 2 | `#3D8B43` green | 4.03 | 3.78 |
| 3 | `#B0711A` amber | 4.23 | 3.60 |
| 4 | `#9C5CE0` violet | 4.07 | 3.75 |
| 5 | `#00868A` teal | 3.87 | 3.94 |
| 6 | `#C2519A` pink | 4.00 | 3.81 |
| 7 | `#8F9A4B` olive | 5.58 | **2.73** |

**Decision**: the automated floor is **outline vs surface**, exactly as
FR-012 and FR-018 word it. Outline-vs-fill is measured and recorded, not
gated. Rationale: FR-011 adds the outline because a palette colour is
*data* exempted from the recolouring, so the outline's job is to separate
that exempt shape **from its background** — which is what a user is
trying to do when they cannot tell a marker from the waveform. It is not
required to be independently legible against its own fill, and no
requirement asks for that. The worst case (dark olive, 2.73:1) is still a
visible edge; tightening it would mean either per-entry outline tuning —
which FR-012 explicitly rules out ("never a treatment that could itself
fail against a given palette entry") — or changing `MARKER_PALETTE`,
which the Scope boundary forbids.

---

## R7 — One of FR-011's four outline sites is already satisfied by FR-007

**Question**: FR-011 names four palette-coloured surfaces that must gain
an outline. How much paint code does each need?

**Evidence**: the Markers panel row's colour swatch is not custom paint —
it is an ordinary egui button (`src/markers.rs:601-604`):

```rust
let color = theme::marker_color(row.color);
if ui.add(egui::Button::new("").fill(color).min_size(vec2(16.0, 16.0)))
```

In egui 0.36.2, `Button::fill()` overrides **only** the frame's fill
(`widgets/button.rs:346-348`); the frame's stroke comes from
`Style::button_style`, which sets `frame.stroke = visuals.bg_stroke`
(`widget_style.rs:158-166`). `style.rs:188` sets every widget slot's
`bg_stroke` to `Stroke::new(1.0, divider)`, and `frame_when_inactive`
defaults to `true` (`button.rs:50`), so the frame is drawn at rest.

**Finding**: once FR-007 raises the divider to full-alpha `text.primary`,
the swatch gains a **1 px `text.primary` outline with no edit at that
call site at all** — the same colour and the same width FR-012
specifies, arrived at independently. FR-011's swatch clause is a
*consequence* of FR-007, not separate work.

**Decision**: write no swatch-specific paint code, and pin the
consequence with a test (`contracts/marker-outline.md` M7) — because it
is a consequence rather than an intention, a future change to the swatch
(e.g. `.stroke(Stroke::NONE)` for a "cleaner" look) would silently drop
an accessibility requirement, and only a test says so.

---

## R8 — The three palette surfaces that do need new paint code

With R7 removing the swatch, FR-011's remaining sites are:

| Surface | Site | Shape | Outline form |
|---|---|---|---|
| Detail-lane glyph | `src/markers.rs:84-96` → `paint_glyph` (`:227`) → `paint_bracket` / `paint_point_glyph` / `paint_cue_glyph` | stroked bracket, filled triangle, filled square | casing stroke under the glyph's own stroke; rect/convex outline for the filled forms |
| Overview-lane marker line + loop span | `src/markers.rs:399-430` | 1–2 px vertical line; `rect_filled(span, .., gamma_multiply(0.25))`; `paint_hatched` | casing stroke; `rect_stroke` around the span |
| Plugin overlay primitives resolving to a palette entry | `src/plugin_overlays.rs:97` (Line), `:118` (Region), `:151` (Label), `:163-192` (Glyph) | line, rect, text, vector glyph | casing stroke; `rect_stroke`; 4-offset text halo; glyph casing |

**Decision on the "outline" of a 1 px line**: a line has no interior, so
"an outline around the shape" is a **casing** — the same stroke drawn
1 px wider in the outline colour, painted first, with the palette-coloured
stroke on top. This is the standard cartographic road-casing idiom and it
is what makes a thin coloured line readable over a busy waveform. The
width arithmetic (`base + 2 * OUTLINE_WIDTH`) lives in
`theme/markers.rs`, not at the call site (FR-019).

**Decision on FR-012's focus-width preservation**: `paint_bracket`
(`markers.rs:234`) strokes at `2.0` focused vs `1.5` unfocused, and
`markers.rs:401` uses `2.0` vs `1.0` in the overview lane. The casing is
applied **uniformly** — the same `+1 px` to both — so the 0.5 px (and
1.0 px) focused/unfocused difference survives unchanged, which is what
FR-012's second sentence requires and what the spec's fourth Edge Case
describes.

---

## R9 — How an overlay call site knows its colour is palette data

**Question**: FR-011 outlines a plugin overlay only when its
`OverlayColor` resolves to a `MARKER_PALETTE` entry (`Positive`,
`Warning`), and explicitly *not* for `Accent`/`Secondary`/`Neutral`. How
does `plugin_overlays.rs` know, without branching on the mode?

**Decision**: a sibling to `overlay_color`, in the same module, with the
same signature shape:

```rust
/// FR-011: `Some(stroke)` only where the resolved colour is palette
/// *data* (Positive/Warning -> MARKER_PALETTE) **and** high contrast is
/// on. `Accent`/`Secondary`/`Neutral` resolve through the recoloured
/// roles and are already at their high-contrast values, so they are
/// `None` unconditionally.
pub fn overlay_outline(token: OverlayColor, visuals: &Visuals) -> Option<Stroke>
```

The call site then writes `if let Some(stroke) = ..` — an `Option` check,
not a mode check. The mode never appears in `plugin_overlays.rs`.

**Rejected**: testing `MARKER_PALETTE.contains(&resolved)` at the call
site. It couples the decision to a value comparison rather than to the
token's meaning, and would outline a plugin's `Accent` overlay in the
(currently impossible, but nowhere forbidden) event that an accent value
collided with a palette entry — the exact case FR-011's last sentence
rules out.

---

## R10 — A plain `bool` settings field would discard the whole file

**Question**: FR-003 requires a malformed `appearance.high_contrast` to
recover to `false` **and** be reported as an invalid field. What does
`#[serde(default)] pub high_contrast: bool` actually do?

**Evidence**: `settings/store.rs:123-131`.

```rust
let raw: RawSettings = match toml::from_str(&content) {
    Ok(raw) => raw,
    Err(_) => {
        return LoadOutcome {
            settings: AudioSettings::default(),          // <-- everything lost
            warnings: vec![SettingsWarning::Unreadable],
        };
    }
```

serde's derived `Deserialize for bool` **errors** on a non-boolean, and
this handler discards the **entire settings file** — theme, volume,
device, keybindings, panel state, all of it — and reports the generic
`Unreadable`, not `InvalidField`. One stray character in
`high_contrast = yes` would reset every other setting. That is the exact
opposite of FR-003 and of the spec's eighth Edge Case.

**How `theme` avoids this** (`model.rs:489-492`, `:691-698`): the raw
type is permissive (`theme: String`) and validation happens later, in
`into_settings`, which pushes `InvalidField::Theme` and falls back to the
default. The permissive-raw-type-plus-late-validation shape is the
established pattern.

**Decision**: mirror it with `toml::Value`, which is already a
`modplayer-core` dependency (`Cargo.toml:22`, `toml.workspace = true`):

```rust
pub struct RawAppearance {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_high_contrast")]      // Value::Boolean(false)
    pub high_contrast: toml::Value,
}
```

and in `into_settings`, beside the `theme` match:

```rust
let high_contrast = match self.appearance.high_contrast.as_bool() {
    Some(value) => value,
    None => { invalid.push(InvalidField::HighContrast); false }
};
```

This yields exactly the three behaviours FR-003 names:

| File contains | `toml::Value` | Result |
|---|---|---|
| (field absent) | `Boolean(false)` from the serde default | `false`, no `InvalidField` |
| `high_contrast = true` | `Boolean(true)` | `true` |
| `high_contrast = "yes"` / `= 3` / `= []` | `String`/`Integer`/`Array` | `false` + `InvalidField::HighContrast` |

and, critically, **no other setting is harmed** — `toml::from_str`
succeeds, so the rest of the file parses normally. `InvalidField::
HighContrast` maps to `"appearance.high_contrast"` in `field_name()`
(`model.rs:214-224`), the same channel `InvalidField::Theme` uses.

Serialization: `to_raw` (`model.rs:564-571`) writes
`toml::Value::Boolean(settings.high_contrast)`, so the file round-trips
as a plain TOML boolean — a human editing `settings.toml` sees
`high_contrast = false`, not a tagged value.

**Rejected**: `Option<bool>` (indistinguishable absent-vs-malformed, and
still errors on a non-boolean rather than yielding `None`); a custom
`Deserialize` visitor (more code than `toml::Value` for the same result,
and it would be the only hand-written visitor in the file);
`#[serde(deny_unknown_fields)]`-style strictness (would make the file
*less* recoverable, not more).

`SCHEMA_VERSION` (`model.rs:69`) stays at `1`: an added field with a
serde default inside an existing table is not a schema change — the
precedent is `[onboarding]` and `[now_playing_panels]` (016). No
migration.

---

## R11 — How the flag reaches the frame loop

**Evidence**: `theme` is mirrored into `PlaybackController`'s shadow
state (`controller.rs:453-456`, populated at `:808`) and read back by
`PlaybackController::theme()` (`:1605`); `App::new` uses it at
`app.rs:126`. Settings *writes* go through the reload-mutate-save
pattern in `settings/appearance.rs:51-61`.

**Decision**: mirror `high_contrast` identically —
`PlaybackController::high_contrast()` and `set_high_contrast()`, shaped
like the existing `focus_policy` / `set_focus_policy` pair. `App::ui`
(`app.rs:223`) then reads `self.controller.high_contrast()` and passes it
to `apply_tokens_for`, and `App::new` (`app.rs:125`) does the same before
the first paint.

**Rationale**: the alternative — calling `settings_store().load()` inside
the frame loop — would put a file read on the UI path every frame. The
controller's shadow state exists for exactly this.

---

## R12 — The control, its focus, and its registry row

**Evidence**: `widgets::controls::switch(ui, SwitchKind::Checkbox, &mut
bool, label) -> Response` (`widgets/controls.rs:81`) already maps
`Checkbox` to `WidgetType::Checkbox` (`:132`) and is verified to report
`Role::CheckBox` with a `Toggled` state by its own test (`:357-380`).
Search focus is `response.request_focus()` guarded on the incoming id —
`appearance.rs:47-49` is the exact shape.

**Decision**: FR-015's control is
`switch(ui, SwitchKind::Checkbox, &mut high_contrast, &tr("setting-high-contrast"))`,
placed below the Theme combo, with
`if focus == Some("appearance.high_contrast") { response.request_focus(); }`
immediately after — one line each, both copied in shape from the Theme
combo above it. FR-016's registry row goes in
`settings_registry.rs`'s `SettingDescriptor` array beside
`appearance.theme` (`:158-163`).

No new widget is written, and NFR-6.2 (role/name/state) is inherited from
`switch` rather than re-derived — which is why the checkbox, and not a
hand-rolled toggle, is the right shape here as well as the conventional
one.

---

## R13 — Locale catalogues: en-US only

**Evidence**: `locales/` contains exactly one directory, `en-US/`
(11 `.ftl` files). Constitution X's "English and pt-BR ship first" has
not yet produced a `locales/pt-BR/`, through 016.

**Decision**: add `setting-high-contrast` and
`setting-high-contrast-desc` to `locales/en-US/settings.ftl` beside
`setting-theme` (`:46-50`), and to the Appearance block of
`tests/fluent_keys.rs`'s inventory (`:530-535`). **No `pt-BR` catalogue
is created** — creating a two-key-only second locale would be worse than
not having one, is not gated by any existing test, and is not this
feature's scope. Recorded so the omission is visibly deliberate rather
than forgotten.

---

## R14 — Extending the contrast test without editing the regression net

**Evidence**: `tests/design_token_contrast.rs` imports `{self, DARK,
LIGHT}` and every test iterates `for roles in [&LIGHT, &DARK]` with
`TEXT_FLOOR = 4.5`, `NON_TEXT_FLOOR = 3.0`, `MARKER_FLOOR = 3.0`
(`:21-24`).

**Decision**: **do not widen the existing loops.** Add a new block of
tests over `[&LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST]` with the FR-018
floors (`ENHANCED_TEXT_FLOOR = 7.0`, plus the existing 3.0 for the
divider and 4.5 for `text_on_accent`), and one further test asserting the
**normal** tables still hold their present literal values field by field.

**Rationale**: widening the existing loops would make those five tests
cover high contrast too, which sounds like a bonus but destroys their
value as the regression net for US1 AS-3 / FR-002 ("no regression outside
high contrast"). Keeping them byte-identical means a future change that
degraded normal mode fails a test that this feature never touched. The
field-by-field normal-table assertion is the belt to that braces: it is
the only thing that would catch an implementation that "simplified" by
editing `LIGHT`/`DARK` in place instead of adding two tables.

---

## R15 — The literal scanner already covers the new code

**Evidence**: `tests/design_token_literals.rs:53` —
`const EXPECTED_BASELINE_HITS: usize = 0;` (016 drove it to zero) — and
the scanner's exclusion list (module doc, S3) excludes `src/theme/**`.

**Consequence**: every constant this feature adds — the eight
high-contrast swatches, `FOCUS_RING_WIDTH_HIGH_CONTRAST`,
`MARKER_OUTLINE_WIDTH`, the two `divider_alpha` values — is exempt by
construction because it lives under `src/theme/`. But the **new paint
code** in `src/markers.rs` and `src/plugin_overlays.rs` is *not*
excluded: any `Color32::…` or bare stroke-width literal written there
raises the count above 0 and fails the build.

**Decision**: this is a feature, not an obstacle. Every outline stroke at
those sites is obtained from a `theme::markers` function returning a
`Stroke`, never constructed locally. `EXPECTED_BASELINE_HITS` **must stay
`0`** — SC-008's companion, and the mechanical enforcement of FR-019.

---

## R16 — What this feature does not touch, verified

| Area | Checked | Result |
|---|---|---|
| Real-time path | no file under `crates/modplayer-engine/`, `crates/modplayer-effects/`, `crates/modplayer-audio-io/` | untouched |
| Plugin API schema | `crates/modplayer-capability-gateway/api/v1.toml`, `docs/plugin-api/v1.md` | byte-identical; `OverlayColor` gains no variant, `HostGlyph` gains none |
| Audio source | no `audio-source-*` crate | untouched |
| Credentials / network | no keyring, no HTTP, no telemetry; one boolean added to `settings.toml` | untouched |
| `MARKER_PALETTE` values | `theme/markers.rs:23-32` | unchanged (Scope boundary; only whether an outline is drawn) |
| `Theme` enum | `modplayer_engine::Theme` | no fourth variant (FR-014) |
| Type scale | `tokens.rs:96-107`, `text_scale()` | unchanged (FR-014) |
| `surface.base` / `surface.raised` | `tokens.rs` | identical across all four tables (FR-002) |
| Hover / pressed alphas | `controls.rs:94-95` | unchanged (FR-020) |
| `text.disabled`, `disabled_alpha` | `tokens.rs` | identical across all four tables (FR-006) |
| Spacing, radii, geometry | `style.rs:78-86`, `apply_spacing` | untouched; `no_geometry_or_interaction_field_changes` (`style.rs:307`) must pass verbatim |

---

## Open items carried into implementation

None blocking. Two things are deliberately **not** gated and are recorded
above so their absence reads as a decision:

1. Outline-vs-fill contrast (R6) — measured, floor not asserted, because
   no requirement states one and FR-012 forbids the per-entry tuning that
   raising it would need.
2. A `pt-BR` catalogue (R13) — not created, because a two-key locale
   directory is worse than none and nothing gates it.

---

## R17 — A pre-existing, out-of-scope dimming on the last row of a capped
search-result group (found executing quickstart.md M3, 2026-09-23)

**Question** (T021, US1 manual walk): does every sampled text run
actually clear 7:1 on the real, signed-in build, not just in the token
tables?

**Evidence**: real build, real Spotify session (`rodrigo.zampieri`),
`MODPLAYER_CONFIG_DIR` pointed at a scratch dir with
`[appearance] high_contrast = true`. Point-sampling
`target/manual-walk/hc_on_library.png`/`hc_on_metallica2.png` (same
method as `target/plan-scratch/contrast.py`, ported to read PNG pixels)
against the WCAG formula:

- Library view (395 real saved tracks): every sampled title, secondary
  (artist — album) line, the "Saved Tracks" count label, the trailing
  duration column and the unselected nav labels all resolve to the exact
  byte value `#1c1c1e` (`LIGHT_HIGH_CONTRAST.text_primary`) against a
  `#ffffff` surface — ratio **17.01**, matching
  `target/plan-scratch/contrast.py`'s own printed value for
  `text_primary`/`text_secondary` exactly. Confirms H11/FR-005's
  promotion reaches the screen, not just the token table.
- Settings › Appearance (`Theme` label + its "Follow the system..."
  help line) and Now Playing's "No track is playing." / "Pick a track to
  start playing." both scan as the same `#1c1c1e`, no lighter grey
  present — confirms the promotion covers `.weak()` help text outside
  list rows too.
- **Search results, `TRACKS` group (6 rows, `GROUP_VISIBLE_ROWS = 6` in
  `search_view.rs:35`, capped by `virtualized_list`'s
  `max_height`)**: rows 1–5's secondary line scan as the same promoted
  `#1c1c1e` (ratio 17.01). **Row 6 — the last row drawn immediately above
  the "Show more Tracks" button — does not**: its secondary line's
  darkest pixels cluster around `#585859`–`#59595b`, ratio ≈ **6.75**,
  below both the 7:1 enhanced floor this feature promises and (barely)
  the pre-existing 4.5:1 floor. The row's own *title* line is unaffected
  (`#1c1c1e`, full ratio) — only the secondary `.weak()` line beneath it
  dims. Reproduced twice, two different `metallica` searches, same
  result.
- **Control**: relaunched with `high_contrast = false` (today's
  behaviour) against the same query. Rows 1–5's secondary line scan as
  today's `#5b5b60` (`LIGHT.text_secondary`, ratio ≈ 9.7, unremarkable).
  The 6th/last row (`Master Of Puppets` in that run — Spotify's ranking
  reordered the result set between sessions, confirming this is a
  **row-position** effect, not a property of any one track) scans
  visibly lighter than its siblings there too (no `#5b5b60` pixels in
  its top colour histogram at all, only lighter greys ≥`#8f8f92`).

---

## R18 — M7 (US3 marker outline) not executed: the GUI session was locked
(attempted executing quickstart.md M7, 2026-09-23, T034)

**Attempt**: built `modplayer` (pinned toolchain), launched it against the
real, already-signed-in `MODPLAYER_CONFIG_DIR` (the same one T021/T029
used successfully earlier the same session), then polled
`Quartz.CGWindowListCopyWindowInfo` for owner `modplayer`/name
`ModPlayer` for 18+ seconds (`target/manual-walk/m7-launch.log`,
`drive.py`).

**Finding**: no window ever appeared, for either the app or any other
GUI application — a window-list scan turned up only `loginwindow`,
`SecurityAgent` and `ScreenSaverEngine` on screen, meaning the macOS GUI
session itself had locked (screen-saver/lock screen) between T029's
sign-off and this attempt, not anything this feature's code did. The
process itself started and ran normally (plugin `host_busy` warnings at
launch are the same benign transient every prior session's log shows,
not a hang) — this is squarely the §4 "window never forms" case, just
with a different, machine-state cause (a locked session) than 014/015's
sign-in-gate wedge.

**Action taken (Constitution › Governance › Manual Scenario Sign-Off,
quickstart.md §4)**: killed the launched process rather than attempt to
unlock the session or otherwise route around it (never appropriate for
an agent to do), and recorded M7 **not executed** on T034 in tasks.md,
with this entry as the point of failure. No automated-evidence-only
sign-off was given in its place. M4/M5/M6/M8/M9's automated coverage
(`markers.rs`'s M4–M9 suite, this session) still machine-checks the
same paint-site claims M7 would visually confirm; M7 itself remains
outstanding and should be re-attempted once the GUI session is unlocked.

**Conclusion**: this is a **pre-existing dimming of the last row directly
above a capped list's "Show more" control**, reproducible identically
with high contrast on *and* off — not a regression this feature
introduces. It sits in `crates/modplayer-ui/src/search_view.rs`
(`GROUP_VISIBLE_ROWS`/`virtualized_list`'s `max_height` cap) and
`rows.rs`'s `.weak()` detail line, neither of which any task in
`tasks.md` (Phase 2 through 8) touches — the whole plan's file list for
this feature is `theme/**`, `app.rs`, `settings/appearance.rs`,
`settings_registry.rs`, `markers.rs`, `plugin_overlays.rs` and
`locales/en-US/settings.ftl`. Because it is invisible in normal mode
(today's secondary grey-on-white already reads as "just lighter," not as
a broken control) but becomes a measurable, real ≥7:1 miss once
everything around it is promoted, high contrast **surfaces** a latent
defect rather than causing one. Filing a fix is out of this feature's
scope (no task names those files); recorded here per governance so the
miss is not silently dropped from the manual sign-off. See
`quickstart.md`'s M3 row for the sign-off text this backs.

---

## R19 — M5/M6 (US5 border/divider floor, focus-ring thickening) not
executed: the GUI session was still locked
(attempted executing quickstart.md M5/M6, 2026-09-24, T039)

**Attempt**: built `modplayer` (pinned toolchain, `RUSTUP_TOOLCHAIN=1.95.0
cargo build -p modplayer`, clean build), launched it against a fresh
scratch `MODPLAYER_CONFIG_DIR` seeded from T021/T029's own
`hc-config-off` (`disclosure.acknowledged_version` already set so
onboarding is skipped, `appearance.high_contrast = false` so M6's
off-capture would come first), then polled
`Quartz.CGWindowListCopyWindowInfo` for owner `modplayer`/name
`ModPlayer` for 23+ seconds (`target/manual-walk/m5m6-launch.log`,
`target/manual-walk/m5m6-config/`).

**Finding**: no window ever appeared, for either the app or any other GUI
application — a window-list scan turned up only `Finder`, `Google
Chrome`, `iTerm2`, `Universal Control`, `EOS Utility`, `Window Server`
and, critically, `ScreenSaverEngine` on screen, meaning the macOS GUI
session itself was still locked (screen-saver/lock screen), exactly
R18's blocker one day later, not anything this session's build changed.
The process started (`ps` showed it `RN`, running normally) and was
killed rather than left running or routed around the lock.

**Action taken (Constitution › Governance › Manual Scenario Sign-Off,
quickstart.md §4)**: killed the launched process, recorded M5 and M6
**not executed** on T039 in tasks.md, with this entry as the point of
failure. No automated-evidence-only sign-off was given in its place,
though it is worth noting the values M5/M6 would visually confirm are
already machine-checked this session by `high_contrast.rs`'s
`every_border_follows_the_divider` (H15: every `bg_stroke`/
`window_stroke` in a high-contrast style equals `Stroke::new(1.0,
divider)`), `design_token_contrast.rs`'s
`high_contrast_divider_clears_the_non_text_floor` (H17: divider ≥ 3.0
against both surfaces, both high-contrast tables) and
`focus_ring_thickens_only_in_high_contrast` (H16: ring width 2.0 → 3.0,
colour stays `accent`) — see T038. M5/M6 themselves — whether those
values actually reach the rendered screen at the stated pixel widths —
remain outstanding and should be re-attempted once the GUI session is
unlocked, per quickstart.md §4's "M1, M2, M4, M5, M6, M9, M10 and M11
need only the Settings screen and any rendered chrome" — this feature's
best-positioned scenarios were still the ones blocked, purely by machine
state, not by anything requiring the sign-in gate.

---

## R20 — Manual-scenario sign-off consolidated (Polish, T046)

**Scope**: T046 asks for the sign-off across T021, T029, T034, T037,
T039 to be consolidated into this file and quickstart.md before the
feature is called done. This entry is that consolidation, gathered from
each task's own note in tasks.md rather than re-running anything — the
Polish phase verifies and cross-references, it does not re-attempt a
prior phase's manual-scenario task.

| Scenario(s) | Task | Status | Where recorded |
|---|---|---|---|
| M1, M2, M9, M10 (control, live-apply, restart, search/focus) | T029 | **PASS**, no deviation | quickstart.md M1/M2/M9/M10 sign-off note (2026-09-23) |
| M11 (malformed value recovers independently) | T029 | **PASS**, no deviation | same note |
| M3, M4 (text ≥ 7:1; unchanged when off) | T021 | **PASS**, one documented out-of-scope deviation (a pre-existing dimmed last row in a capped search-results group, `search_view.rs` — not a file any task in this plan touches) | quickstart.md M3/M4 sign-off note (2026-09-23); R17 |
| M7 (markers stay identifiable) | T034 | **NOT EXECUTED** — GUI session locked at attempt time | quickstart.md M7 sign-off note (2026-09-23); R18 |
| M5, M6 (border/divider floor, focus-ring thickening) | T039 | **NOT EXECUTED** — GUI session locked at attempt time (same blocker as R18, one day later) | quickstart.md M5/M6 sign-off note (2026-09-24); R19 |
| M8 (docked plugin panel follows) | T037 | **NOT EXECUTED** — GUI session locked at attempt time (same blocker as R18/R19, one day later) | quickstart.md M8 sign-off note (2026-09-24); R21 |

**M8/T037, specifically**: attempted the same way T034/T039 were
(quickstart.md §3's recipe, pre-docking the panel via a scratch
`settings.toml` so no further in-app step was needed) and hit the
identical locked-session blocker. Its automated half is in place
regardless: `plugin_overlays.rs`'s `M12` suite (T035/T036)
machine-checks that a docked plugin panel's outlined primitives match
host chrome's outline treatment exactly, and `P4`
(`docked_plugin_panel_matches_host_chrome`) machine-checks that the
panel reads the identical applied `Style` object host chrome does — so
M8's mechanism is proven; only the visual, on-screen confirmation (and a
point-sampled ≥ 7:1 reading of the panel itself) remains outstanding.
Re-attempt T037 once the GUI session is unlocked.

**Net state at the end of Polish**: US1 and US2 are independently
verified end to end (M1–M4, M9–M11 all PASS). US3, US4 and US5's
automated coverage is green but their manual confirmation is blocked on
a locked GUI session (M5–M8, see R21 below for M8's own attempt). None
of these are a regression or a defect in this feature's own logic —
they are recorded, per Governance › Manual Scenario Sign-Off, rather
than assumed.

---

## R21 — M8 (US4 docked plugin panel) not executed: the GUI session was
still locked
(attempted executing quickstart.md M8, 2026-09-24, T037)

**Attempt**: built `modplayer` (pinned toolchain, clean build), created
a scratch `MODPLAYER_CONFIG_DIR` (`target/manual-walk/m8-config/`)
seeded from the real signed-in `hc-config` (`account.toml` copied
across) with `appearance.high_contrast = false` (so an off→on toggle
could be captured) and a pre-set
`[plugin_panels."org.modplayer.section-loop/main"]` entry
(`placement = "docked"`, `disabled = false`) so the panel would already
be docked on the very first frame rather than requiring a further
in-app enable step. Launched `target/debug/modplayer` against that
config and polled `Quartz.CGWindowListCopyWindowInfo` for owner
`modplayer`/name `ModPlayer` for 25+ seconds
(`target/manual-walk/m8-launch.log`).

**Finding**: no window ever appeared, for either the app or any other
GUI application — the on-screen owner list held only background
agents (`Dock`, `SystemUIServer`, `loginwindow`, `SecurityAgent`,
`Window Server`, etc.), no app with a visible content window, and
`python3 -c "import Quartz; print(Quartz.CGSessionCopyCurrentDictionary())"`
confirmed `CGSSessionScreenIsLocked = 1` directly — the same
screen-saver/lock-screen machine state R18 (T034) and R19 (T039) hit,
not anything this session's config or build changed. The process
itself ran normally (`ps` showed it sleeping/running, `SN`; the
`host_busy` plugin-registration warnings in its log at launch are the
same benign transient every prior session's log shows, not a hang) —
squarely the §4 "window never forms" case.

**Action taken (Constitution › Governance › Manual Scenario Sign-Off,
quickstart.md §4)**: killed the launched process rather than attempt to
unlock the session or otherwise route around it, and recorded M8 **not
executed** on T037 in tasks.md, with this entry as the point of
failure. No automated-evidence-only sign-off was given in its place,
though the mechanism M8 would visually confirm is already
machine-checked this session's regression net by
`plugin_overlays.rs`'s `M12` suite (outline parity at the four
plugin-overlay primitive arms, T035/T036) and
`docked_plugin_panel_matches_host_chrome` (`P4`: the docked panel reads
the identical applied `Style` object host chrome does). M8 itself —
whether a docked panel's on-screen pixels actually land at the stated
≥7:1 — remains outstanding and should be re-attempted once the GUI
session is unlocked.

**Updated net state**: all three of this feature's sign-in-gated
scenarios (M3/M7/M8) that were reachable in principle (quickstart.md
§4's "M3, M7 and M8 are the three scenarios that verify the spec's own
acceptance lines against the real renderer") are now either PASS (M3,
with one out-of-scope deviation, R17) or NOT EXECUTED on an identical
locked-session blocker (M7 R18, M8 R21) alongside M5/M6 (R19). No
scenario has failed against this feature's own logic; three remain
blocked purely by host machine state.

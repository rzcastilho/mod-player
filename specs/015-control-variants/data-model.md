# Phase 1 Data Model: Button, Toggle, and Meter Variants

**Feature**: 015-control-variants | **Spec**: [spec.md](spec.md) |
**Plan**: [plan.md](plan.md) | **Research**: [research.md](research.md)

This feature persists nothing and defines no domain entity. Its "data
model" is the **presentation value set** it adds to the 014 token module,
plus the fixed call-site mapping that consumes it. Every value below is
derived inside `crates/modplayer-ui/src/theme/` from the ten semantic
roles 014 already ships — this feature adds **no eleventh role**
(FR-019).

---

## 1. Module surface

```text
crates/modplayer-ui/src/theme/
├── mod.rs        ~ re-export the controls surface
├── tokens.rs       (unchanged — the ten roles, the scales)
├── style.rs      ~ recolor_widget re-differentiated (§4)
├── contrast.rs     (unchanged — ratio/relative_luminance/composite, reused by §3)
├── controls.rs   + THIS FEATURE'S VALUES (§2, §3, §5, §6)
└── markers.rs      (unchanged)

crates/modplayer-ui/src/widgets/
└── controls.rs   + the three host widgets + the focus pass (§7)
```

`theme/controls.rs` holds **every number**. `widgets/controls.rs` holds
**no number** — it reads them (research R7), which is what keeps 014's
literal scan at zero with its exclusion list unchanged.

---

## 2. Button variants (FR-001)

```rust
pub enum Variant { Primary, Default, Quiet, Destructive }

pub struct VariantPaint {
    pub fill: Color32,        // TRANSPARENT where the variant has none
    pub outline: Stroke,      // Stroke::NONE where the variant has none
    pub label: Color32,
}

pub fn variant_paint(roles: &Roles, variant: Variant) -> VariantPaint
```

| Variant | `fill` | `outline` | `label` | Applied to |
|---|---|---|---|---|
| `Primary` | `accent` | `Stroke::NONE` | `text_on_accent` | FR-002 — `welcome-acknowledge`, once per view |
| `Default` | `surface_raised` | 1 px `divider` | `text_primary` | FR-005 — everything unnamed; identical to what `recolor_widget` installs, so no call site edits |
| `Quiet` | `TRANSPARENT` | `Stroke::NONE` | `text_primary` | FR-004 — the four Queue row actions |
| `Destructive` | `TRANSPARENT` | 1 px `danger` | `danger` | FR-003 — the six named sites |

**Validation V1**: the four `(fill, outline.color, outline.width, label)`
tuples are pairwise distinct in **both** themes.
**Validation V2**: every component of every tuple is a 014 role, a derived
value from §3, or `TRANSPARENT`/`Stroke::NONE`.

`Default` is the do-nothing variant on purpose: FR-005's ~60 call sites
keep calling `ui.button(...)` and inherit it from the shared style, so
they cannot drift out of the variant system by omission.

---

## 3. Interaction-state values (FR-009, FR-010, FR-011)

```rust
pub fn hover_fill(roles: &Roles)   -> Color32 // text_primary @ 4 %
pub fn pressed_fill(roles: &Roles) -> Color32 // text_primary @ 8 %
pub fn focus_ring(roles: &Roles)   -> Stroke  // 2 px accent
pub const FOCUS_RING_WIDTH: f32 = 2.0;
pub const FOCUS_RING_GAP:   f32 = 1.0;
```

| Value | Derivation | Why this value |
|---|---|---|
| hover | `roles.text_primary.gamma_multiply(0.04)` | § 5.4 "full-row hover fill at 4 % foreground" |
| pressed | `roles.text_primary.gamma_multiply(0.08)` | FR-011 — double hover; "more committed" |
| focus ring | `Stroke::new(FOCUS_RING_WIDTH, roles.accent)` | § 5.4 "focus ring = 2 px `accent`" |
| ring offset | `rect.expand(FOCUS_RING_GAP)` + `StrokeKind::Outside` | FR-010 — 1 px of underlying surface between control and ring |

**Composition rule**: a control's rendered fill in state *s* is
`contrast::composite(resting_fill, overlay(s))`, where `overlay(Resting)`
is nothing, `overlay(Hover)` is `hover_fill` and `overlay(Pressed)` is
`pressed_fill`. For `Quiet`/`Destructive` (transparent resting) the
composite is taken over `surface_base`, which is what a viewer sees.

**Validation V3 (FR-011, normative)**: for every variant × theme, the
three composited fills are pairwise distinct and the overlay alphas are
strictly increasing (`0 < 0.04 < 0.08`). The *ordering* is the contract;
the two percentages are not.
**Validation V4 (FR-010)**: the ring is exactly `FOCUS_RING_WIDTH` px, its
colour is `accent`, and it is painted on `rect.expand(FOCUS_RING_GAP)`
with `StrokeKind::Outside` — so ring and an `accent` selection fill stay
separable structurally, not chromatically (US3-3).

**Deliberate coincidence**: `pressed_fill` equals 014's `divider`
numerically (both `text_primary` @ 8 %). Different names, different jobs;
no test asserts they differ.

---

## 4. Widget-state slot map (FR-011a)

`theme/style.rs::visuals` today paints all five slots identically
(`style.rs:120-128`). The new map:

| `Visuals::widgets` slot | `bg_fill` / `weak_bg_fill` | `bg_stroke` | `fg_stroke` | `corner_radius` | `expansion` |
|---|---|---|---|---|---|
| `noninteractive` | `surface_raised` | 1 px `divider` | `text_primary` | `radius::SM` | untouched |
| `inactive` | `surface_raised` | 1 px `divider` | `text_primary` | `radius::SM` | untouched |
| `hovered` | `composite(surface_raised, hover_fill)` | 1 px `divider` | `text_primary` | `radius::SM` | untouched |
| `active` | `composite(surface_raised, pressed_fill)` | 1 px `divider` | `text_primary` | `radius::SM` | untouched |
| `open` | `composite(surface_raised, pressed_fill)` | 1 px `divider` | `text_primary` | `radius::SM` | untouched |

**Validation V5**: `inactive.bg_fill != hovered.bg_fill != active.bg_fill`
in both themes.
**Validation V6 (regression)**: 014's
`style::tests::no_geometry_or_interaction_field_changes` passes verbatim —
`expansion`, `striped`, `handle_shape`, `interact_cursor`,
`animation_time` and every `Spacing` component size are still equal to the
toolkit default (FR-018, FR-021).

---

## 5. The switch (FR-007)

```rust
pub struct SwitchMetrics { pub track: Vec2, pub thumb: f32, pub inset: f32 }
pub fn switch_metrics() -> SwitchMetrics;
pub fn switch_track(roles: &Roles, on: bool) -> (Color32, Stroke);
pub fn switch_thumb(roles: &Roles, on: bool) -> Color32;
pub const SWITCH_OUTLINE_WIDTH: f32 = 1.0;
```

| Metric | Value | From the 014 scale |
|---|---|---|
| track height | 16.0 | `space::LG` |
| track width | 32.0 | `space::XXL` (a 2:1 pill) |
| track radius | `radius::full(track.y)` | FR-007 "pill-shaped (`radius.full`)" |
| thumb diameter | 12.0 | `space::MD` |
| thumb inset | `(track.y − thumb) / 2` = 2.0 | derived, never written |

| State | Track fill | Track outline | Thumb colour | Thumb position |
|---|---|---|---|---|
| **Off** | `surface_raised` | 1 px `divider` | `text_secondary` | leading end (`track.left() + inset`) |
| **On** | `accent` | `Stroke::NONE` | `text_on_accent` | trailing end (`track.right() − inset − thumb`) |

FR-007 does not name the off-thumb colour; `text_secondary` is chosen
because 014 FR-011 already guarantees it ≥ 4.5:1 against
`surface_raised`, so the thumb is visible in both themes with no new floor
to verify (assumption, recorded in plan.md § Complexity Tracking).

**Validation V7 (NFR-6.4)**: the thumb's centre x differs between off and
on by `track.x − thumb − 2 × inset` > 0 — state is carried by **position**
as well as colour.
**Validation V8**: the switch's `(track fill, outline, thumb)` triple in
either state equals no button variant's `(fill, outline, label)` triple,
and its corner radius is `radius::full`, which no variant uses — so a
switch cannot be mistaken for a button (FR-007).

---

## 6. Meters (FR-012, FR-012a, FR-013)

```rust
pub enum Band { Positive, Warning, Danger }
pub const BAND_WARNING_DB: f32 = -6.0;
pub fn band(db: f32, danger_boundary_db: f32) -> Band;
pub fn band_color(roles: &Roles, band: Band) -> Color32;
pub const SCALE_MARK_WIDTH:  f32 = 1.0;   // −6 dB and 0 dB
pub const CEILING_MARK_WIDTH: f32 = 2.0;  // the peak meter's ceiling
pub fn mark_color(roles: &Roles, filled: bool) -> Color32;
```

**Band selection — the fixed order of FR-012**, so a boundary belongs to
the higher band and a boundary at or below −6 dBFS simply empties the
warning band:

```rust
if db >= danger_boundary_db   { Danger }
else if db >= BAND_WARNING_DB { Warning }
else                          { Positive }
```

| Meter | File | Danger boundary | Scale marks |
|---|---|---|---|
| Now Playing peak | `widgets/peak_meter.rs` | the active limiter ceiling (preserves today's `peak_db >= ceiling_db`) | −6 dB, 0 dB (1 px) + ceiling (2 px) |
| Effect Chain pre/post level pair (peak **and** RMS sub-bars) | `widgets/chain_meters.rs::level_pair` | fixed `0.0` dBFS (no ceiling input) | −6 dB, 0 dB (1 px) |

**Fill model (segmented, research R10)**: the filled span
`[x(SCALE_MIN_DB), x(level)]` is drawn once in `positive` with
`radius::SM`, then the warning span `[x(−6), min(x(level), x(boundary))]`
and the danger span `[x(boundary), x(level)]` are overdrawn as square
rects clipped to the filled span. Interior seams are vertical cuts, where
a square edge is correct; the left end keeps its radius.

**Mark colour rule (one rule, both meters, both themes)**:
`mark_color(roles, filled)` = `surface_base` where the fill has reached
the mark (a gap cut through the band), `text_secondary` where it has not.
The 0 dBFS mark sits at the scale maximum and is inset left by its own
width so the border stroke does not swallow it.

**Removed by this feature**: `peak_meter.rs:54`'s `warn_fg_color` ceiling
tick (invisible over a `warning` band after FR-012) and
`chain_meters.rs:73`'s `gamma_multiply(0.7)` RMS dim (FR-012a). Both
sub-bars stop reading `selection.bg_fill`.

**Unchanged and guarded**: every meter's `mono` readout
(`peak_meter.rs:76`, `chain_meters.rs:91-92`) and its accessible value
(`WidgetInfo::labeled(ProgressIndicator, …)`) — FR-014, NFR-6.4.

**Validation V9**: `band` returns the expected role for inputs below, at
and above each boundary, including `danger_boundary_db <= -6.0`.
**Validation V10**: at `level >= boundary` the rightmost filled column is
`danger` (SC-005).
**Validation V11**: mark positions are `fraction_of(-6.0)` and
`fraction_of(0.0)` of the track width, the 0 dB mark inset by
`SCALE_MARK_WIDTH`, and `mark_color` flips exactly at
`x_mark <= x_fill_right`.

---

## 7. Host widgets (`widgets/controls.rs`)

```rust
pub fn button(ui: &mut Ui, variant: Variant, text: impl Into<RichText>) -> Response;
pub enum SwitchKind { Checkbox, Toggle }
pub fn switch(ui: &mut Ui, kind: SwitchKind, on: &mut bool, label: &str) -> Response;
pub fn row_frame<R>(ui: &mut Ui, id: Id, add: impl FnOnce(&mut Ui) -> R) -> (Rect, R);
pub fn destructive_gap(ui: &mut Ui);          // ui.add_space(DESTRUCTIVE_GAP)
pub fn paint_focus_ring(ctx: &Context);       // once, last, per frame
```

| Helper | State source | Notes |
|---|---|---|
| `button` | `ctx.read_response(ui.next_auto_id())` from last pass (research R4) — `hovered()`, `is_pointer_button_down_on()`; **never** `widget_state()`, which folds focus into `Active` | composites the overlay into the fill it hands `egui::Button`, so the label is never tinted |
| `switch` | same | emits `WidgetInfo::selected(WidgetType::Checkbox \| SelectableLabel, …)` per `kind` — §8 |
| `row_frame` | the row's own `Response` | reserves a shape index before content, `set`s it after (research R5) — zero layout change |
| `paint_focus_ring` | `ctx.memory(\|m\| m.focused())` + `ctx.read_response` | one foreground-layer ring; returns silently when nothing is focused or the widget is not visible |

**`DESTRUCTIVE_GAP`** = `space::LG` (16 px).
**Validation V12 (FR-006)**: `DESTRUCTIVE_GAP >= 2.0 * style.spacing.item_spacing.x`
(today 2 × 8 px) — stated as the ratio so a later spacing-scale change
cannot silently void the rule.

---

## 8. Accessible role preservation (FR-016, SC-008)

| `SwitchKind` | `WidgetInfo` | AccessKit role | Call sites |
|---|---|---|---|
| `Checkbox` | `selected(WidgetType::Checkbox, …)` | `Role::CheckBox` + `Toggled` | `plugins_view.rs` enable toggle, `markers.rs:757` loop-arm, `settings/audio.rs:166` safe volume, `settings/plugins.rs:167` boolean fields, `plugin_panels.rs:363` plugin-contributed |
| `Toggle` | `selected(WidgetType::SelectableLabel, …)` | `Role::Button` + `Toggled` | `now_playing.rs:145/152/159`, `effects_view.rs:140/290/343/549/558/567`, `queue_view.rs:27` |

Mapping verified in `egui-0.36.2/src/response.rs:936-957`. The existing
suites pin both halves and are **not edited**: `accessibility.rs`
(`:278`, `:317-342`, `:385-409`, `:1434/1447`, `:1966`, `:2141`),
`plugins_view.rs` (`:290/313/458/506`), `plugin_panels.rs`
(`:503/1062/1066`), `settings_plugins.rs` (`:318/360`).

---

## 9. Call-site map (the complete set of view edits)

### 9.1 Button variants

| Variant | Control | Site |
|---|---|---|
| `Primary` | `welcome-acknowledge` | `welcome.rs:108` |
| `Destructive` | `markers-clear-all` | `markers.rs:853` |
| `Destructive` | `markers-clear-yes` (+ gap before `markers-clear-no`) | `markers.rs:843` |
| `Destructive` | `effects-remove` | `effects_view.rs:163` |
| `Destructive` | `plugin-panel-disable` **only while the label reads Disable** | `plugins_view.rs:161` **and** `plugin_panels.rs:291` |
| `Destructive` | `account-sign-out` | `settings/account.rs:60` |
| `Destructive` | `signout-confirm` (modal) | `settings/account.rs:146` — *removes* an existing `error_fg_color` call-site colour |
| `Quiet` | `queue-move-up` / `-move-down` / `-play-next` / `-remove` | `queue_view.rs:84 / 87 / 90 / 93` |

`queue-remove` is **`Quiet`, not `Destructive`** (FR-004): a queue removal
is trivially reversible; the destructive variant's value is its scarcity.

### 9.2 `DESTRUCTIVE_GAP` insertions (FR-006)

Markers panel header (before `markers-clear-all`), Effect Chain row
(before `effects-remove`), plugin panel row (before the Enable/Disable
button), marker-clear confirmation (between `markers-clear-yes` and
`markers-clear-no`). Where the destructive control has no neighbour, the
helper is simply not called — the spacer rule is a no-op (Edge Cases).

### 9.3 Switch conversions

**FR-008 (acceptance set)**: `now_playing.rs:145/152/159`
(`queue-toggle`/`effects-toggle`/`transport-toggle`);
`plugins_view.rs:183` (`plugins-enable-toggle`);
`effects_view.rs:140` (`effects-bypass`).

**FR-008a (app-wide boolean set)**: `effects_view.rs:290` (formant),
`:343` (mute), `:549` (mono-sum), `:558` (phase-invert), `:567`
(channel-swap); `queue_view.rs:27` (shuffle); `markers.rs:757`
(loop-arm); `settings/audio.rs:166` (safe volume);
`settings/plugins.rs:167` (boolean plugin settings);
`plugin_panels.rs:363` (plugin-contributed).

**FR-008b (NOT converted — one-of-N selection)**: `library_view.rs:117`
(tabs), `shell.rs:109` (nav rail), `settings/mod.rs:111/126/148`
(categories, search hits), `settings/audio.rs:93` and
`settings/plugins.rs:103/323` (combo options),
`plugin_panels.rs:540` (list-item selection),
`settings/language.rs:27`. They keep their appearance and still receive
hover/focus/pressed from §3–§4.

### 9.4 Row hover (FR-009)

`rows.rs:502-510` (search/library/detail — rect and response already
exist), `queue_view.rs` rows, `plugins_view.rs` rows, `markers.rs` rows.

### 9.5 Meters

`widgets/peak_meter.rs` (bands, three marks, ceiling-tick rule),
`widgets/chain_meters.rs::level_pair` (bands on both sub-bars, dim
removed, two marks). `chain_meters::spectrum` is **not** in scope — it is
not a level meter and FR-012 names neither it nor a spectrum band rule.

### 9.6 App wiring

`app.rs` — `widgets::controls::paint_focus_ring(ui.ctx())` as the last
statement of `App::ui`.

---

## 10. What this model does NOT contain

No persisted field, no settings key, no migration, no Fluent key, no
plugin-API item, no engine or effects value, no animation duration, no
new semantic colour role, and no geometry beyond §5's switch box and
§7's `DESTRUCTIVE_GAP` (FR-017, FR-018, FR-019, FR-021).

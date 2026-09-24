# Phase 0 Research: Button, Toggle, and Meter Variants

**Feature**: 015-control-variants | **Spec**: [spec.md](spec.md) |
**Plan**: [plan.md](plan.md)

Every decision below was verified against **this worktree** (branch
`015-control-variants`, 2026-09-22) and against the pinned toolkit source
in `~/.cargo/registry/src/index.crates.io-*/egui-0.36.2/`. Where a finding
contradicts the spec, the contradiction is stated, resolved, and carried
into [plan.md](plan.md) § Complexity Tracking.

The spec's Clarifications session already resolved the *value* questions
(what each variant paints, the 4 %/8 % fills, the 2 px offset ring, the
switch form, the band boundaries). This document answers the *mechanism*
questions those values raise in egui 0.36.2, which is where the surprises
are.

---

## R1 — There is no `Style` hook that repaints a `Checkbox`. FR-008c's mechanism must change; its guarantee does not

**Decision**: the switch is a **host widget**
(`widgets/controls.rs::switch`), used at every host call site that today
draws a persistent boolean — including `plugin_panels.rs:363`, the single
host line that draws every **plugin-contributed** checkbox. FR-008c's
outcome (plugin booleans inherit the switch, no plugin API surface
changes, Principle IX untriggered) is delivered unchanged; only its stated
mechanism ("installed in the shared style once per frame") is not
achievable.

**Evidence**: egui 0.36.2 introduced a widget-style layer
(`src/widget_style.rs`) with `CheckboxStyle`, `ButtonStyle`, `Classes` and
`Style::checkbox_style(&classes, state)` — but `checkbox_style` is an
**inherent method on `Style`**, not a field, closure, trait object or
registry (`widget_style.rs:174-190`). Nothing in `Style` can be assigned
to change *how* a `Checkbox` paints; a caller can only change the colours
it reads. `Checkbox::ui` then hard-codes its own geometry — a square
`big_icon_rect` of `spacing.icon_width` plus a three-point check-mark
polyline (`widgets/checkbox.rs:120-160`). A pill track with a positioned
thumb cannot be expressed through those fields at any value.

**Why the guarantee survives**: `contracts/ui-panels.md` A4 binds plugin
panels to the host's rendering of their declared controls; a plugin
declares a boolean field and the **host** draws it. The grep is
unambiguous — plugin-contributed checkboxes reach the screen through
`plugin_panels.rs:363` (`ui.checkbox(&mut checked, &spec.label)`) and
`settings/plugins.rs:167` (`Checkbox::new(&mut checked, field.label…)`),
both host files. Swapping those two lines for `switch` gives every plugin
the new visual with **zero** change to `crates/modplayer-capability-gateway/api/v1.toml`, to any DTO, to any
permission and to any bundled package. Principle IX stays untriggered for
exactly the reason 014 recorded as its FR-015b.

**Rejected**: forking `egui::Checkbox` into the repo (a vendored widget to
maintain against every toolkit bump, for a shape we can draw in 40 lines);
waiting for an upstream style hook (blocks the feature on a third party);
`Classes`-based styling (`with_class`/`SELECTED_CLASS`) — classes only
*select* among values `checkbox_style` already computes; they cannot
replace its painting.

---

## R2 — `Response::widget_state()` folds focus into `Active`, so focus and pressed cannot both come from the widget-state slots

**Decision**: split the two mechanisms.

- **Pressed** lives in the `Visuals::widgets` slots, as FR-011a requires:
  `hovered` gets the 4 % composite, `active`/`open` the 8 % composite.
- **Focus** is drawn **outside** the toolkit's slot system, by one
  app-level pass (R3), so it is a ring and never a fill.
- Host-drawn controls (button variants, the switch, row frames) compute
  their own state from the `Response`'s fields
  (`is_pointer_button_down_on()`, `hovered()`), **never** through
  `widget_state()`, so for them pressed means pressed.

**Evidence**: `widget_style.rs:104-116` —

```rust
} else if self.is_pointer_button_down_on() || self.has_focus() || self.clicked() {
    WidgetState::Active
```

Focus, click and press all collapse to one slot. This is why FR-010's
"distinct from the selected state" cannot be satisfied by assigning a
colour to a slot: there is no focus slot to assign.

**Residual, accepted**: egui's own built-in widgets that this feature does
*not* replace (the FR-008b selection controls, `ComboBox`, `Slider`,
`DragValue`, plain `ui.button` default-variant call sites) will render the
**pressed** fill while merely focused, because they route through
`widget_state()` internally. They also get the R3 focus ring, so a focused
control is never ambiguous *about being focused* — the cost is that its
fill is one step stronger than resting while focused. This is a toolkit
property of 0.36.2, not a choice; it is recorded in plan.md § Complexity
Tracking and is invisible to FR-011's normative test, which is stated over
the three fills' alphas, not over which input produced them.

**Rejected**: leaving `active` equal to `inactive` to avoid the
conflation — that deletes FR-011/FR-011a outright, which is the larger
loss.

---

## R3 — The focus ring is one app-level pass, not ~120 call-site edits

**Decision**: `widgets::controls::paint_focus_ring(ctx)`, called as the
**last statement** of `App::ui` (`crates/modplayer-ui/src/app.rs`,
`impl eframe::App for App::ui`, which already opens with
`theme::apply_tokens`). It reads the focused id, reads that widget's rect,
and strokes one ring into a foreground layer:

```rust
let Some(id)   = ctx.memory(|m| m.focused())   else { return };
let Some(resp) = ctx.read_response(id)          else { return };
let painter    = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("focus-ring")));
painter.rect_stroke(resp.rect.expand(GAP), radius, ring_stroke, StrokeKind::Outside);
```

**Evidence (every API verified in 0.36.2)**: `Memory::focused()`
(`src/memory/mod.rs:566`), `Context::read_response`
(`src/context.rs:1355`), `Context::layer_painter`
(`src/context.rs:1587`), `Painter::rect_stroke` with
`StrokeKind::Outside`.

**Why this shape**:

- **FR-019 compliance by construction** — one site, in shared code, so no
  view file gains a colour, an alpha or a stroke width.
- **FR-010's offset is exact** — `expand(GAP)` where `GAP` is 1 px leaves
  one pixel of the underlying surface, then `StrokeKind::Outside` lays the
  2 px ring entirely beyond it. On an `accent`-filled selection the ring
  reads as a separate band, which is the structural separability US3-3
  demands.
- **FR-020 is free** — a disabled widget cannot hold focus, so
  `memory.focused()` never names one.
- **Scrolled-away widgets are free** — `read_response` returns `None` for
  a widget not visible this pass or last (`context.rs:1348-1354` doc),
  so no ring is stranded over a scrolled list.

**Known nuance to verify manually (quickstart M4/M5)**: the ring is
painted into `Order::Foreground`, the same order the sign-out `Modal`
uses. Painted last, it lands above the modal's own layer; since egui's
modal takes focus, the focused widget is inside the modal anyway, so the
ring is over the right control. M4 confirms this on the real build.

**Rejected**: `Style::debug.show_focused_widget` — a debug affordance
(`style.rs:1383-1388`), not a themed ring, and it is not token-coloured;
a per-call-site `focus_ring(ui, &response)` helper — ~120 edits, each one
an opportunity to forget, and FR-010 says *every* focusable control.

---

## R4 — Host controls read last pass's response rather than over-painting the label

**Decision**: `button()` and `switch()` resolve their interaction state
**before** drawing, via `ui.ctx().read_response(ui.next_auto_id())`, and
composite the hover/pressed overlay **into the fill they pass to the
widget**. Nothing is painted over the label.

**Evidence**: this is egui's own idiom for exactly this problem —
`widgets/checkbox.rs:69-72` does

```rust
let id = ui.next_auto_id();
let response: Option<Response> = ui.ctx().read_response(id);
let state = response.map(|r| r.widget_state()).unwrap_or_default();
```

and `read_response` explicitly supports being "called _before_ creating
the widget (!)" because "widget interaction happens at the start of the
pass, using the widget rects from the previous pass"
(`context.rs:1348-1352`).

**Constraint this places on the helper**: it must call `next_auto_id()`
and then allocate the widget with **nothing allocated in between**, or the
id it read is not the id the widget gets. The helpers are written so the
`next_auto_id()` call and the `ui.add(...)` are adjacent statements; a
unit test asserts the state actually tracks a simulated hover across two
passes.

**Rejected**: painting the 4 %/8 % overlay on top of the finished widget —
simpler, but it tints the label (up to 8 % of `text.primary` over the
glyphs) and, on the `primary` variant, over `text.on-accent`; it also
double-darkens the outline. Reserving a shape index *under* the widget
(`Painter::add(Shape::Noop)` + `Painter::set`, `painter.rs:213/242`) —
correct for a **transparent-resting** variant but invisible under
`primary`'s and `default`'s opaque fill, so it cannot be the single
mechanism. (That same reserve/set pair *is* the right mechanism for row
hover — see R5.)

---

## R5 — Row hover is a reserved shape index, so no row layout moves

**Decision**: `widgets::controls::row_frame(ui, id, add_contents)` reserves
a shape slot before the row's content runs, draws the content unchanged,
then `set`s the reserved slot to a `RectShape` of the row's own rect in
the hover/pressed fill. Because the fill is written into a slot that was
already reserved, it paints **beneath** the content while costing zero
layout — FR-018's "no geometry" line is kept literally.

**Evidence**: `Painter::add -> ShapeIdx` and `Painter::set(idx, shape)`
(`painter.rs:213`, `:242`) — the mechanism `egui::Frame` itself uses.

**Per-row-type application** (verified call sites):

| Row | File | Today | Hook |
|---|---|---|---|
| Search / Library / Detail | `rows.rs:502-510` | `allocate_space` + `ui.interact(rect, row_id, Sense::click())` | rect and response **already exist** — reserve before `draw_artwork`, set after |
| Queue | `queue_view.rs:~55-95` | `ui.horizontal(…)` per row, no row response | wrap in `row_frame`; rect from the inner response |
| Plugin | `plugins_view.rs` rows | `ui.horizontal(…)` | same |
| Marker | `markers.rs` rows | `ui.horizontal(…)` | same |
| Plugin-panel list item | `plugin_panels.rs:540` | `selectable_label` | FR-008b keeps its appearance; it gets hover/pressed from the R2 slots |

`rows.rs` is the cheap case and the one SC-003 samples first: the row rect
is computed at line 507 *before* any content is drawn, so the reserve/set
pair brackets `draw_artwork`/`draw_content` with no restructuring at all.

---

## R6 — Four variants are four `egui::Button` configurations, not four widgets

**Decision**: one helper,
`widgets::controls::button(ui, Variant, text) -> Response`, returning a
configured `egui::Button`. `Button` exposes `fill()` (`button.rs:143`),
`stroke()` (`:151`), `frame(bool)` (`:166`) and `corner_radius()`
(`:200`); the label colour rides on the `RichText` the helper builds. All
four variants are reachable:

| Variant | `fill` | `stroke` | label |
|---|---|---|---|
| `primary` | `accent` | `Stroke::NONE` | `text.on-accent` |
| `default` | `surface.raised` | 1 px `divider` | `text.primary` |
| `quiet` | `TRANSPARENT` | `Stroke::NONE` | `text.primary` |
| `destructive` | `TRANSPARENT` | 1 px `danger` | `danger` |

`fill()`/`stroke()` are **static** overrides in egui — they apply to every
state, which is exactly why the helper must composite the R4 state overlay
into the value it passes rather than relying on the slot system. The
`default` variant is also the **do-nothing** variant: it reproduces what
`recolor_widget` already installs, so FR-005's ~60 untouched `ui.button`
call sites need no edit and cannot drift.

**Verified call-site inventory** (the only lines FR-002/003/004 move):

| FR | Control | Site |
|---|---|---|
| FR-002 `primary` | `welcome-acknowledge` | `welcome.rs:108` |
| FR-003 `destructive` | `markers-clear-yes` | `markers.rs:843` |
| FR-003 | `markers-clear-all` | `markers.rs:853` |
| FR-003 | `effects-remove` | `effects_view.rs:163` |
| FR-003 | `plugin-panel-disable` (label-dependent) | `plugins_view.rs:161` **and** `plugin_panels.rs:291` |
| FR-003 | `account-sign-out` | `settings/account.rs:60` |
| FR-003 | `signout-confirm` (modal) | `settings/account.rs:146` |
| FR-004 `quiet` | `queue-move-up` / `-move-down` / `-play-next` / `-remove` | `queue_view.rs:84/87/90/93` |

Two findings worth carrying into tasks: the spec names only
`plugins_view.rs` for `plugin-panel-disable`, but **two** host files draw
that label (`plugin_panels.rs:291` draws the dock's own copy) — both take
the variant, or SC-001 fails on the dock; and `settings/account.rs:146`
already hand-colours its modal label
`RichText::new(tr("signout-confirm")).color(ui.visuals().error_fg_color)`,
so converting it to `destructive` *removes* a call-site colour rather than
adding one.

---

## R7 — Every new number lives in the token module, including stroke widths and geometry

**Decision**: a new `crates/modplayer-ui/src/theme/controls.rs` holds the
variant table, the derived state fills, the ring stroke and gap, the
switch metrics, the destructive gap, the band boundary and the scale-mark
widths. `widgets/controls.rs` and the meters **consume** them and contain
no numeric visual literal of their own.

**Why**: FR-019 forbids "a colour, an alpha, or a stroke width at any call
site", and 014's literal scan
(`crates/modplayer-ui/tests/design_token_literals.rs`) excludes exactly
`src/theme/**`, test code, `use` statements and doc comments
(contracts/literal-scan.md S3). A switch drawn in `widgets/` with a
hard-coded `Stroke::new(2.0, …)` would sit in a scanned file. Keeping the
numbers in `theme/controls.rs` keeps the scan at **0** with its exclusion
list untouched, which FR-019 requires in as many words.

**Scan scope is unchanged** (no S2 pattern is added): 014 deliberately
scans colour and font-size literals only, because radius/spacing/stroke
patterns cannot be told from legitimate geometry maths without false
failures (014 Complexity Tracking). This feature does not reopen that; it
achieves the stronger FR-019 property by *construction* (no number to
find) and pins it with a contract test that asserts the widget module's
values are the token module's values, rather than by widening a scanner
that would start failing on `rect.height() * 0.5`.

**Derived values added** (all computed inside the module from the ten
014 roles — **no eleventh role**, per FR-019):

| Name | Derivation | Source |
|---|---|---|
| `hover_fill` | `text_primary.gamma_multiply(0.04)` | FR-009, § 5.4 |
| `pressed_fill` | `text_primary.gamma_multiply(0.08)` | FR-011 |
| `focus_ring` | `Stroke::new(2.0, accent)` | FR-010, § 5.4 |
| `FOCUS_RING_GAP` | `1.0` | FR-010 |
| `DESTRUCTIVE_GAP` | `space::LG` (16) | FR-006 |
| switch metrics | `space::LG` track height, `space::XXL` track width, `space::MD` thumb, inset `(track_h − thumb) / 2` | FR-007 |
| `BAND_WARNING_DB` | `-6.0` | FR-012 |

**Note, deliberate**: `pressed_fill` is numerically identical to 014's
`divider` (both are `text_primary` at 8 %). They are different names for
different jobs — a 1 px rule versus a control fill — and 014's own
precedent is that a derived value is named for its job, not for its
formula. No test asserts they differ.

---

## R8 — The three state fills are distinguishable by construction, and the test says so in alphas

**Decision**: the normative test is FR-011's ordering — for each variant ×
theme, the composited resting, hover and pressed fills are **pairwise
distinct**, and the overlay alphas are strictly increasing
(`0 < 0.04 < 0.08`).

**Mechanism**: `theme::contrast::composite` (already in the module from
014, used there for the disabled composite) blends the overlay over the
variant's resting fill; for the two transparent-resting variants the
composite over `surface.base` is what a viewer sees, so the test composites
over `surface.base` for `quiet`/`destructive` and over the variant's own
fill otherwise. `Color32::gamma_multiply(0.04)` yields α≈10/255 —
measurably non-zero in both themes, on both surfaces.

**Rejected**: asserting specific RGB outputs — brittle against any future
role re-tone, and it would re-pin 014's values in a second place.

---

## R9 — The switch keeps whatever accessible role its call site reports today

**Decision**: `switch` takes a `SwitchKind`:

- `SwitchKind::Checkbox` → `WidgetInfo::selected(WidgetType::Checkbox, …)`
  → AccessKit `Role::CheckBox` + `Toggled`.
- `SwitchKind::Toggle` → `WidgetInfo::selected(WidgetType::SelectableLabel, …)`
  → AccessKit `Role::Button` + `Toggled`.

**Evidence**: `response.rs:936-957` maps
`WidgetType::{Button, CollapsingHeader, SelectableLabel} => Role::Button`
and `WidgetType::Checkbox => Role::CheckBox`. The existing suites pin both
halves and would fail on a single mis-typed call site:

- `Role::Button` + `Toggled` for `selectable_label`/`toggle_value`
  controls — `accessibility.rs:317-342` (`queue-shuffle`),
  `:385-409` (`effects-bypass`), `:278` (`queue-toggle`).
- `Role::CheckBox` for checkbox controls — `accessibility.rs:1434/1447`
  (`loop-arm`/`loop-disarm`), `:1966` (plugin rows),
  `plugins_view.rs:290/313/458/506`, `plugin_panels.rs:503/1062/1066`,
  `settings_plugins.rs:318/360`.

`plugins_view.rs:290` **counts** `Role::CheckBox` nodes and
`plugins_view.rs:313` asserts their **absence** in another state — so a
switch that reported `Role::Button` everywhere would fail two ways at
once. FR-016 and SC-008 are therefore gated by tests that already exist;
this feature adds none for them and must change none.

---

## R10 — Segmented meter fill: lowest band rounded, higher bands square on top

**Decision**: draw the filled span once in `positive` with the existing
`radius::SM` corner, then overdraw the warning span (from x(−6 dB) to
min(x(level), x(boundary))) and the danger span (from x(boundary) to
x(level)) as **square** rects. Each overdraw is clipped to the filled
span.

**Why**: egui has no rounded clip rect, and painting three rounded rects
leaves 4 px notches at the interior seams. The band seams are vertical
cuts mid-bar, where a square edge is the *correct* rendering; only the two
ends of the bar want the radius, and the left end is always the lowest
band. The right end loses its rounding once the fill passes −6 dB — at a
14–18 px bar with a 4 px radius this is imperceptible, and it is the price
of a seam that is not notched.

**The band selector is pure and is the unit under test** (FR-012's fixed
order, so a ceiling at or below −6 dBFS simply empties the warning band):

```rust
fn band(db: f32, danger_boundary_db: f32) -> Band {
    if db >= danger_boundary_db      { Band::Danger }
    else if db >= BAND_WARNING_DB    { Band::Warning }
    else                              { Band::Positive }
}
```

Boundaries belong to the higher band, matching today's
`peak_db >= ceiling_db` test (`widgets/peak_meter.rs:38`).

**Level-pair consequence, recorded**: `chain_meters::level_pair` clamps to
`SCALE_MAX_DB = 0.0` (`chain_meters.rs:41-42`) and FR-012 fixes its danger
boundary at 0 dBFS, so its danger band is the single rightmost column,
reached only at exactly 0.0 dBFS. That is what FR-012 says and what SC-005
tests ("the rightmost filled column … renders `danger`"); it is noted here
so nobody later reads an all-but-invisible red sliver as a bug.

---

## R11 — Scale marks: one colour rule, and the 0 dB mark is inset

**Decision**: per FR-013 —

- Ticks at −6 dBFS and 0 dBFS, 1 px; the peak meter's ceiling tick stays
  2 px so the two kinds stay distinguishable.
- A mark is drawn in `surface.base` where the fill has reached its
  position (`x_mark <= x_fill_right`) and in `text.secondary` where it has
  not.
- The 0 dBFS mark is inset left by its own width so it is not swallowed by
  the meter's border stroke at the scale maximum.

**Why the colour rule needs no new contrast work**: 014 already verifies
`positive`/`warning`/`danger` ≥ 4.5:1 against `surface.base` (014 FR-014)
and `text.secondary` ≥ 4.5:1 against it (014 FR-011). A gap cut in
`surface.base` through a band, and a `text.secondary` tick on the empty
`extreme_bg_color` track, are each already-verified pairs — one rule, both
meters, both themes, no floor to re-derive.

**This deletes a literal and a defect**: `peak_meter.rs:54` currently
strokes the ceiling tick in `ui.visuals().warn_fg_color`, which after
FR-012 is also a *band fill* colour — a warning tick over a warning band
is invisible. The rule above replaces it.

---

## R12 — The RMS sub-bar loses its dim

`chain_meters.rs:73` draws RMS as
`ui.visuals().selection.bg_fill.gamma_multiply(0.7)`. FR-012a removes the
0.7: dimming a `danger` band defeats the signal the bands exist to give.
The two sub-bars stay distinguishable by occupying separate halves of the
track (`chain_meters.rs:58-69`) and by their own labelled `mono` readouts
(`:91-92`), both untouched. Both sub-bars also stop reading
`selection.bg_fill` — after FR-012 their colour comes from the band
selector, not from the selection role.

---

## R13 — `recolor_widget` re-differentiation is a five-line change in one function

`theme/style.rs:120-159` currently loops all five slots through one
`recolor_widget` call with identical arguments — which is precisely why
the app has had no hover or pressed difference since 014 (014 FR-019
deferred them here). The change is to give the loop a per-slot fill:

| Slot | `bg_fill` / `weak_bg_fill` |
|---|---|
| `noninteractive` | `surface.raised` (unchanged) |
| `inactive` | `surface.raised` (resting, unchanged) |
| `hovered` | `composite(surface.raised, hover_fill)` |
| `active` | `composite(surface.raised, pressed_fill)` |
| `open` | `composite(surface.raised, pressed_fill)` |

`bg_stroke` stays 1 px `divider`, `fg_stroke` stays `text.primary`,
`corner_radius` stays `radius::SM`, and **`expansion` is still not
touched** — 014's `no_geometry_or_interaction_field_changes` test
(`style.rs:267-306`) must keep passing verbatim, which it does: it asserts
`expansion`, `striped`, `handle_shape`, `interact_cursor`,
`animation_time` and the `Spacing` component sizes, none of which move.
This is the whole of FR-011a: one construction site, once per frame, no
view edit (014 FR-002).

---

## R14 — No animation, and `animation_time` stays where 014 left it

FR-021 forbids easing or transition timing; egui's `animation_time` and
`interaction` settings are already asserted unchanged by 014's test above,
so honouring FR-021 is a matter of adding nothing. State changes land on
the frame their input changes. (The R4 read-last-pass mechanism means a
host control's *fill* settles one frame after a hover begins — that is a
sampling artefact of egui's two-phase interaction, identical to what its
own `Checkbox` does, not an animation, and it is invisible because egui
repaints on hover change.)

---

## R15 — Nothing here touches the real-time path, the plugin API, locales, or persisted data

- **Engine/effects**: no file under `crates/modplayer-engine/` or
  `crates/modplayer-effects/` is modified. The meters *display* values the
  caller already reads per frame (`chain_meters.rs` module doc,
  `peak_meter.rs:20-23`); no new read, no new queue, no atomic.
- **Plugin API**: no `crates/modplayer-capability-gateway/api/v1.toml` line, no DTO, no permission, no
  manifest field. The capability gateway's API-reference regeneration diff
  test is the gate (Principle IX, 014 A9).
- **Locales**: zero Fluent keys added, removed or re-cased. Every label
  this feature restyles keeps its existing `tr(...)` key —
  `fluent_keys.rs` must stay green. A switch renders the *same* string its
  checkbox did.
- **Persisted data**: none. Variants are a static property of a call site
  (spec § Key Entities); no settings field, no migration.
- **Behaviour**: FR-017. Every converted call site keeps its click
  handler, its `changed()` branch and its keyboard route verbatim; the
  existing view suites (`queue_view.rs`, `effects_view.rs`,
  `plugins_view.rs`, `markers.rs`, `settings_plugins.rs`,
  `plugin_panels.rs`, `controls.rs`, `actions.rs`) are the regression net
  and are not edited.

---

## R16 — Verification split, and what is honestly not testable headlessly

Per the spec's own Clarifications ("both, at different times") and
Principle VIII:

**Automated** (values, headless, no rendering harness — the same
`egui::__run_test_ctx` route 014 used, no new dev-dependency):

- four variants → four distinct `(fill, outline, label)` triples, both
  themes;
- resting/hover/pressed → three distinct composited fills, alphas strictly
  increasing;
- focus ring → 2 px, `accent`, `FOCUS_RING_GAP` = 1 px, drawn outside;
- band selector → correct role below, at and above each boundary,
  including a danger boundary at or below −6 dBFS;
- scale-mark positions and the filled/unfilled colour rule;
- the switch's reported role/state per `SwitchKind`;
- `DESTRUCTIVE_GAP >= 2 × spacing.item_spacing.x`;
- a **source inventory** (SC-010) asserting no `toggle_value`/`Checkbox`/
  boolean `selectable_label` survives at the FR-008/FR-008a sites and none
  of the FR-008b selection sites was converted;
- 014's `design_token_literals.rs` still reporting **0**;
- every existing suite, unmodified, still green (SC-008, FR-016, FR-017).

**Manual** (quickstart M1–M10, executed by the implementing agent per
Governance): that the rendered pixels match those values — a destructive
button reading as distinct on a real screen, a hover fill actually
appearing, a ring and a selection remaining two signals, a meter walking
positive → warning → danger.

**Not testable headlessly, stated plainly**: the app has no pixel-capture
harness (014 rejected `egui_kittest`, and this feature adds no
dependency), so "visually distinguishable" is verified as *values* by
tests and as *pixels* by the manual scenarios. A screenshot-diff gate
would be a new dependency and a new CI surface for a feature whose entire
payload is already expressible as values — rejected on Constitution X.

**Addendum, 2026-09-23 (tasks.md T058's executed walk)**: on that day's
host, none of M1–M10 could even be attempted — the process never reached
`eframe::run_native`. `sample <pid> 1` pinned the main thread inside
`modplayer_account::service::AccountService::launch_resolve_session` →
`modplayer_secure_store::keyring_store::KeyringSecureStore::get` →
`SecKeychainFindGenericPassword` → `mach_msg_trap`, blocked for 60+ s of
active (not idle) CPU across two launch attempts (a plain background job
and one via `launchctl asuser 501`, to rule out a foreign security-session
artifact of the launching shell). `Quartz.CGWindowListCopyWindowInfo`
found no window for the process at any point. The equivalent lookup run
directly (`security find-generic-password -s ModPlayer -a
session-credential -w`) returns instantly with "item not found" (exit
44), so the in-process call is wedging on something the bare CLI path
does not — most likely a keychain-access-consent prompt the ad-hoc-signed
dev binary cannot get answered with no interactive session to show it in.
This sits entirely inside `modplayer_account`/`modplayer_secure_store`,
neither of which this feature touches (plan.md § Project Structure); it
is a pre-existing host/toolchain precondition, the same class of risk
014 already carried forward as "the sign-in gate", just tripped one step
earlier this time (before any window forms, rather than after Welcome).
Recorded per Governance as M1–M10 not executed, no keychain state
touched, no session fabricated. See plan.md § Complexity Tracking D11 and
quickstart.md § 4's 2026-09-23 update.

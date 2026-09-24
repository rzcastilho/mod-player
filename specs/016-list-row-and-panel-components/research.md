# Phase 0 Research: List Row, Tab Strip, and Panel Card Components

**Feature**: 016-list-row-and-panel-components | **Date**: 2026-09-23

Every decision below was verified against **this worktree** and the
**vendored egui 0.36.2 / accesskit 0.24.1 sources**
(`~/.cargo/registry/src/index.crates.io-*/egui-0.36.2`,
`.../accesskit-0.24.1`), not assumed. Citations are `file:line`.

The spec arrived **Clarified** with sixteen decisions already settled
(spec.md § Clarifications 1–16) and **no open `[NEEDS CLARIFICATION]`
markers**. Phase 0 therefore had no spec ambiguity to resolve; its job was
the *other* half — verifying that the toolkit and the existing code can
actually deliver what those sixteen decisions promise, and pricing the ones
that turn out to cost more than the spec assumed. Three findings changed
the design (R3, R6, R9); one changed a success criterion's evidence route
(R16).

---

## R1 — The row widget's signature must change, and `RowEvent` gains a variant

**Decision**: `rows::list_row` becomes
`list_row(ui, artwork, entity, selected: bool) -> Option<RowEvent>`, and
`RowEvent` gains a third variant, `Select`. The widget stays
`PlaybackController`-free and still *reports* rather than *applies*
(`rows.rs:13-16`, design note 6).

**Rationale**: FR-006 needs two new pieces of information to cross the
widget boundary in opposite directions — "am I the selected row?" (in, to
drive FR-008's fill and FR-031's text colour) and "I was single-clicked"
(out, so the view can update its own state per FR-007). The existing
`Option<RowEvent>` return is the established channel for the outbound half;
`Action`/`Open` already ride it (`rows.rs:128-132`). A `bool` in is the
minimum inbound surface: the widget does not need the index, the list salt
or the view's identity — only the caller does, when it builds the
`(key, index)` pair. Keeping the index out of the widget is what lets
`list_row` stay ignorant of virtualization.

**Call-site cost, fully enumerated** (every `list_row` call in the tree):
`library_view.rs:262, 294, 343, 381, 435`; `search_view.rs` (one, in
`show_group`); `detail_view.rs:94`. Seven sites, each currently matching
`Some(RowEvent::Action(action))` or a three-arm `match`. The three-arm
matches gain a `Select` arm; the `if let` sites become matches.

**Alternatives considered**:
- *A builder struct (`ListRow::new(entity).selected(true).show(ui))`* —
  the idiomatic egui shape, and where this widget probably ends up if it
  grows a fourth or fifth knob. Rejected for now under Constitution X
  (YAGNI): it is a larger diff across seven call sites for one `bool`, and
  it would obscure the `Option<RowEvent>` return the call sites already
  pattern-match.
- *A separate `rows::row_clicked(...)` query alongside `list_row`* — two
  functions that must be called in lockstep with the same `entity`, which
  is a correctness trap the type system would not catch.
- *Returning `Vec<RowEvent>`* so a frame could report both `Select` and
  `Open` — unnecessary: the spec's own Edge Case says the intermediate
  selected frame need not be suppressed, so `Open` simply wins the frame it
  happens on and the `Select` from the first click already landed a frame
  earlier.

---

## R2 — Selection identity: `(list salt, entity key, display index)`, reconciled in O(1)

**Decision**: a view's selection is
`RowSelection { list: String, key: String, index: usize }`. `list` is the
**id salt the view already passes to `rows::virtualized_list`**
(`"library-saved-tracks"`, `"library-saved-albums"`,
`"library-followed-artists"`, `"library-playlists"`,
`"library-recently-played"`, the four Search group salts, `"detail-tracks"`
— `rows.rs:613-620`, `library_view.rs:240,254,289,333,371,420`,
`detail_view.rs:86-88`). `key` is `rows::entity_key(entity)`, which is
today's private `entity_salt` (`rows.rs:294-301`) made `pub` — it already
returns each entity's id as `&str`.

**Rationale**: Clarification 3 fixes the pair as (entity id, display
index); the *list salt* is the third component this plan adds, and it is
load-bearing for Search. Clarification 2 makes selection exclusive across
Search's four simultaneously-visible groups, so one `RowSelection` per view
holds a row that lives in one of four different index spaces — without the
salt, FR-012's reconciliation cannot know *which* list to re-check index
`3` against. Reusing the `virtualized_list` salt rather than inventing a
parallel enum means the discriminator is a string the call site already
types, and a typo shows up as a selection that never reconciles rather than
as a compile error — so R2's contract test asserts the salt set.

**The reconciliation must be O(1), not O(n)** — this is the finding.
FR-012 clears the selection "on any later frame where that index no longer
holds that id", and the naive shape (`ids.iter().position(...)`, or
building a `Vec<String>` of keys per frame) is O(rows) *every frame*, which
silently destroys the virtualization property `virtualized_list` exists to
provide (`rows.rs:597-607`: "a 50 000-row list costs O(visible rows) per
frame, not O(rows)"). The API is therefore:

```rust
pub fn reconcile(&mut self, list: &str, key_at: impl FnOnce(usize) -> Option<String>)
```

— `FnOnce`, called at most once, at exactly the stored index, and only
when `list` matches the stored salt. A list that shrank past the index
returns `None` and the selection clears.

**Alternatives considered**:
- *A bare entity id* — Clarification 3 rejects it directly: a playlist may
  hold the same track twice, so two rows would highlight.
- *A `ListId` enum over the ten lists* — more type safety, but it must be
  kept in sync with the salts by hand anyway, and it puts Search's group
  kinds and Library's tabs into one enum that belongs to neither module.
- *Reconciling by scanning the rendered range* — wrong by construction: the
  selected row is usually *not* in the rendered range (that is the
  Edge Case FR-012 names), so scanning what was drawn would clear a
  perfectly valid selection the moment the user scrolls.
- *Storing the selection in `ui.memory`* — the state FR-019 is currently
  moving *out* of `ui.memory` for exactly the reason that it is invisible
  to tests and to the owning view. Selection goes into App-owned view state
  (`LibraryViewState`'s established shape, `library_view.rs:78-89`).

---

## R3 — FR-029 is satisfied by egui's hit test, but only for the "…" button

**Decision**: clicking the "…" button does **not** also select the row, by
construction — no suppression code is needed for that path. Secondary click
and `Shift+F10` need no suppression either, because neither produces
`Response::clicked()`. **But** a click on any *future* control inside a row
would need it, so the selection is computed from `row_response.clicked()`
**only**, and the contract test pins the "…" case rather than trusting it.

**Rationale (verified, not assumed)**: `hit_test.rs:76-80` — *"In tie, pick
last = topmost"* — and `hit_test_on_close` (`hit_test.rs:206-217`) picks a
**single** `hit_click` widget via `find_closest_within` over everything
that `senses_click()`. The row registers its interaction first
(`rows.rs:514`, `ui.interact(rect, row_id, Sense::click())`) and the "…"
button is created later, inside the child `Ui` (`rows.rs:576-578` →
`actions_menu` → `rows.rs:457`, `ui.button("…")`). Both sense click and
both contain the pointer, so the tie goes to the later-registered button:
exactly one of the two gets the click, and it is the button.
`row_response.clicked()` is `false`. FR-029's "…" clause holds for free.

`secondary_clicked()` (`rows.rs:531`) and the `Shift+F10` branch
(`rows.rs:532-535`) are separate `Response` predicates from `clicked()`, so
neither can produce a `Select` — FR-029's other two clauses also hold for
free.

**Why the test exists anyway**: this is a behaviour *inherited from a
toolkit tie-break rule*, not one this code states. An egui bump that
changed the tie-break, or a later feature adding a second control inside
the row (a heart, an inline play button), would silently reintroduce the
invisible side effect Clarification 4 exists to prevent. Contract L7 pins
it.

**Alternatives considered**:
- *Defensively suppressing selection whenever the actions menu is open or
  was clicked this frame* — dead code today (verified above), and dead
  defensive code rots into a false sense of coverage. Rejected in favour of
  the test.
- *Moving the row's `interact` after the content* — would invert the
  tie-break and break the existing hover fill, which needs the response
  before it paints (`rows.rs:513-514`).

---

## R4 — The trailing column's width: `mono` `'0'` advance × 7, measured at runtime

**Decision**: a new `theme::duration_measure(ctx) -> f32`, defined as
`7.0 * ctx.fonts_mut(|f| f.glyph_width(&mono_font_id(), '0'))`, living in
`theme/tokens.rs` beside `body_measure` (`tokens.rs:160-165`) and named by
a `DURATION_FIGURES: f32 = 7.0` constant in the same module.

**Rationale**: FR-001 mandates "the `mono` role's `'0'` advance × 7,
mirroring `theme::body_measure`'s derivation, never a hand-written pixel
literal". Seven is the character count of the widest string FR-033 permits
(`h:mm:ss` → `1:04:15`). Runtime measurement rather than a constant is
forced by `text_scale()` (`tokens.rs:84-90`): a future text-scale setting
must widen the column with the figures, and `body_measure` already
establishes that a measure is a function of `ctx`, not a `const`.
`tokens.rs:329-343`'s existing `mono_digits_are_tabular` test already
proves every digit shares the `'0'` advance, so measuring `'0'` bounds
every digit string.

**Why the colon is not measured separately**: in a monospace family every
glyph — including `:` — shares one advance, which the existing tabular test
covers for digits and which contract L3 extends to `:` for this feature.

**Alternatives considered**:
- *Measuring the widest actually-present duration per frame* — O(rows) per
  frame (see R2) and makes the column's width data-dependent, which is the
  precise defect FR-001 exists to fix.
- *A `const DURATION_COLUMN_WIDTH: f32 = 56.0`* — a pixel literal, banned
  by FR-024 and caught by `design_token_literals.rs`.
- *Reusing `ACTIONS_RESERVED_WIDTH`'s 40.0 style of constant*
  (`rows.rs:46`) — that constant is itself a survivor from before 014's
  token discipline; this feature adds no second one. (It is left alone:
  FR-024 binds files this feature *modifies* for colour/spacing/radius, and
  `ACTIONS_RESERVED_WIDTH` is a reserved-width geometry value the literal
  scan deliberately does not pattern-match — see R15.)

---

## R5 — `format_duration`'s rollover, and the truncation backstop

**Decision**: `rows::format_duration` (`rows.rs:303-306`) gains an
`h:mm:ss` branch at ≥ 3 600 000 ms and keeps `m:ss` below it. The
`Label::truncate()` already on every line (`rows.rs:360-362`) is the
backstop FR-033 requires for a malformed figure wider than the column — the
duration label is created through the same `line` helper, so a figure that
somehow exceeds the fixed column truncates instead of widening it.

**Rationale**: `total_seconds / 60` on a 124-minute mix yields `124:15`
today — six characters where the column budgets seven, but the *next* step
(`1000:00`) is not bounded at all, and `duration_ms: u32` permits ~49 days.
Rolling over at exactly 60 minutes both matches the convention
Clarification 8 cites and makes `h:mm:ss` the provable maximum for any
`u32` under ~10 hours; beyond that (`10:00:00`, eight characters) the
truncate backstop is what holds the column, which is why FR-033 names it.

**Exact boundary**: 3 600 000 ms renders `1:00:00`, not `60:00` — the
spec's own Edge Case, and contract L4's first assertion.

**Alternatives considered**:
- *`hh:mm:ss` zero-padded* — eight characters, widening the column by one
  figure for a case no catalog row hits.
- *Clamping the value* — displaying a wrong duration to protect a layout.

---

## R6 — The tab strip must become a host widget, because the fill is app-wide

**Decision**: `widgets::controls` gains `tab(ui, selected, label) ->
Response`, drawing the label plus (when selected) an underline stroked at
`theme::controls::TAB_UNDERLINE_WIDTH` in the `accent` role. Library's five
`ui.selectable_label` calls (`library_view.rs:117`) are replaced by it. The
count is a **separate sibling** `ui.label(theme::mono_text(..))` drawn
beside the tab inside the same `ui.horizontal`.

**Rationale (this is the finding)**: the obvious cheap route — keep
`selectable_label` and just repaint its selected background — is
**unreachable**. `selectable_label`'s fill comes from
`Visuals::selection.bg_fill`, which is a single app-wide field built once
in `theme/style.rs`'s one construction site (014 design note 3). Changing
it to "no fill" would silently delete the selection fill from **every other
`selectable_label` in the app** — the nav rail (`shell.rs`) and the
Settings category list (`settings/mod.rs`), which spec.md's Scope boundary
explicitly puts *out of scope* and 015's design note 10 explicitly left
alone. A host widget is the only way to restyle the Library tabs without
reaching those two.

**The accessibility state comes free, and must be preserved deliberately**:
`response.rs:976` shows egui already maps `WidgetInfo::selected` to
`accesskit`'s `set_toggled(Toggled::True/False)`, which is why today's
`selectable_label` tabs already report a toggled state. A hand-rolled
`ui.allocate` + `interact` widget reports **nothing** unless it says so.
FR-034's "additionally expose an AccessKit selected/toggled state" is
therefore not an addition at all — it is a *preservation* requirement that
the widget must actively re-implement. The `tab` widget sets both
`set_selected(bool)` (accesskit-0.24.1 `lib.rs:2133`) and
`set_toggled(Toggled)` (`lib.rs:2138`), plus `Role::Tab` and the exact
un-suffixed `tr("library-tab-…")` label.

**Why the count is a sibling node and not part of the tab**: FR-016 pins
`tests/accessibility.rs:900`'s `find_one(&nodes, Role::Tab, &tr(key))` and
`tests/library_view.rs:380`'s `labels_in_tree_order(&update, Role::Tab)` —
both compare the Tab node's label to the **exact** Fluent string. Any
suffix fails both, verbatim. A sibling label node keeps the count reachable
as adjacent text (Clarification 10) while leaving those two assertions
byte-identical.

**Alternatives considered**:
- *A per-widget `Visuals` scope around the tab row* — `ui.scope` with a
  mutated `selection.bg_fill` would work, but it leaves the tab a
  *button-shaped* widget with an invisible fill, and gives no place to
  stroke the underline. It also hides an app-wide field mutation inside a
  view file, which is what 014 design note 3 forbids.
- *`Button::fill(Color32::TRANSPARENT)` + a manual underline* — `Button`
  reports `Role::Button`, not `Role::Tab`, failing FR-016.
- *Appending the count to the tab's Fluent label* — FR-025 names FR-016 as
  "the binding case" and forbids it explicitly.

---

## R7 — The Panel card: one helper, `Frame`-based, and it must not re-inset twice

**Decision**: `widgets::controls` gains
`panel_card(ui, header: &str, add_contents: impl FnOnce(&mut Ui))`, built
on `egui::Frame` with `.fill(roles.surface_raised)`,
`.inner_margin(theme::space::LG)`, `.corner_radius(theme::radius::MD)` and
**no stroke** (§ 5.4 names none, FR-017). It draws
`theme::section_label(header)` with the accessible name pinned to the
exact un-uppercased string, then calls `add_contents`. All four panels —
`markers::panel` (`markers.rs:530`), `effects_view::show`
(`effects_view.rs:71`), `transport_view::show` (`transport_view.rs:42`),
`queue_view::show` (`queue_view.rs:21`) — render through it.

**Rationale**: FR-036 requires exactly one implementation. `Frame` is
egui's own fill+margin+radius container, so the helper is ~20 lines and
adds no layout container of its own beyond the one `Frame` needs. The
header treatment is already proven three times over
(`markers.rs:544-548`, `effects_view.rs:99-103`, and
`settings::controls::section_heading`), including the accessible-name pin
that FR-018 carries forward; the helper's job is to stop it being proven a
fourth and fifth time.

**Two panels need their header *moved*, not added**: Markers and Effect
Chain already draw a `section_label` header *inside their own first
`ui.horizontal`*, alongside other controls (`markers.rs:535-553` puts the
header beside "New loop" and the clear-all controls;
`effects_view.rs:92-115` puts it beside the CPU and overload readouts).
Naïvely wrapping those functions in `panel_card(header)` would render the
header **twice**. Each panel's own header label is deleted from its
horizontal row and the row keeps its remaining controls. Transport's
`ui.heading()` (`transport_view.rs:48`) is deleted and replaced by the
helper's (FR-018's stated correction). Queue has none today and simply
gains one.

**Alternatives considered**:
- *A `PanelCard` builder struct* — YAGNI (Constitution X) for four call
  sites with identical parameters.
- *Passing the header as an `Option`* to let Markers/Effects keep their
  in-row headers — defeats FR-036's whole point, which is that the four
  cannot drift.
- *Adding a stroke for definition* — § 5.4 names fill, padding and radius
  only; FR-017 says "no outline/stroke".

---

## R8 — `EFFECTS_PANEL_RESERVED_HEIGHT` must be re-derived, and it is currently a literal

**Decision**: `now_playing.rs:39`'s `EFFECTS_PANEL_RESERVED_HEIGHT: f32 =
140.0` is replaced by a function that sums named `theme::space` tokens and
the measured heights beneath the Effect Chain panel, plus **two** card
paddings' worth of new inset (`2 × theme::space::LG` per card that now sits
in the reserved region).

**Rationale**: the constant exists because of a real, recorded defect — the
2026-09-19 manual walk (M8/M10) found the "Add node…" row, master volume
and Queue panel falling off the bottom with no way to reach them
(`now_playing.rs:36-39, 174-189`). FR-023 exists because FR-017's padding
makes every panel taller by `2 × LG = 32 px`, and the reserved height is
computed as `ui.available_height() - EFFECTS_PANEL_RESERVED_HEIGHT`
(`now_playing.rs:181`) — an under-count re-creates exactly the clipping the
constant was added to fix. `140.0` is also a bare pixel literal in a file
this feature modifies, which FR-024 forbids independently.

**What sits below the Effect Chain panel** (read off `show`, in order):
the Transport panel when open (`:194-197`), `volume::master_volume`
(`:199`), `peak_meter` (`:205`), and the Queue panel when open (`:207-210`)
— plus the `theme::space::XL` gaps at `:175`, `:195` and `:208`. Of those,
Transport and Queue are now cards and each gains `2 × LG`.

**Alternatives considered**:
- *Re-guessing a larger literal (`180.0`)* — FR-023 forbids it in terms
  ("expressed as a sum of `theme::space` tokens and measured widget
  heights rather than a re-guessed pixel literal").
- *Dropping the cap and letting the panel scroll the whole window* — the
  2026-09-19 finding is precisely that this loses the controls beneath.

---

## R9 — The three keyboard toggles move off `ctx` and onto `controller`

**Decision**: `now_playing::toggle_queue_panel`,
`effects_view::toggle_effect_chain_panel` and
`transport_view::toggle_transport_panel` change signature from
`(ctx: &egui::Context)` to `(controller: &mut PlaybackController<B, H>)`.
Their three `panel_open_id()` helpers (`now_playing.rs:44-46`,
`effects_view.rs:36-38`, `transport_view.rs:22-24`) are **deleted**, as
FR-019 requires.

**Rationale (this is the second finding)**: FR-019 moves the flag to
`settings.toml`, and only `PlaybackController` can reach
`persist_settings` (`controller.rs:4684`, private). The keyboard path
currently runs through `actions::invoke`'s three arms
(`actions.rs:518, 531, 532`), which pass `ctx` — but `invoke` **already
holds `controller: &mut PlaybackController<B, H>`** and uses it in a dozen
neighbouring arms (`actions.rs:480-506`). So the change is three call sites
inside one `match`, and **`invoke`'s own signature does not change**. This
was worth verifying: had the dispatcher been controller-free, FR-019 would
have forced a much larger refactor of 007's action plumbing.

**Alternatives considered**:
- *Keeping a `ui.memory` mirror written by the shortcut and read by the
  view* — FR-019 forbids mirroring in terms ("removed rather than
  mirrored, so a click and a shortcut cannot diverge").
- *Threading a `&mut bool` out of the dispatcher* — reintroduces the two
  sources of truth by another name.

---

## R10 — The settings section follows `[onboarding]`'s precedent exactly

**Decision**: a new optional `[now_playing_panels]` table with three
`#[serde(default)]` booleans (`effect_chain_open`, `transport_open`,
`queue_open`), a `RawNowPlayingPanels` wire struct beside `RawOnboarding`
(`settings/model.rs:494-500`), a `NowPlayingPanels` field on
`AudioSettings` beside `getting_started_dismissed` (`:135`), and
`SCHEMA_VERSION` left at `1` (`:69`).

**Rationale**: `[onboarding]` is the exact precedent Clarification 15
names, and it is a one-field optional table added without a schema bump —
`model.rs:300-301` (`#[serde(default)] pub onboarding: RawOnboarding`),
`:320`, `:595`, `:753`. An absent table deserializes to
`RawNowPlayingPanels::default()` → all three `false` → every panel closed,
which is byte-for-byte the `unwrap_or(false)` default the three
`ui.memory` lookups use today (`now_playing.rs:94-104`). The round-trip
test mirrors `plugin_panels_round_trip` (spec FR-027).

**Controller shape**: three cached `bool` fields loaded at construction
(mirroring `getting_started_dismissed`, `controller.rs:2463-2464`) with an
accessor/setter pair each, the setter calling
`persist_settings` exactly as `set_focus_policy` (`:2455-2458`) and
`dismiss_getting_started` (`:2471-2474`) do. Per Clarification 15, the
setter persists on **every** toggle from **either** input path.

**One shape question this plan settles**: three separate pairs
(`queue_panel_open`/`set_queue_panel_open`, …) versus one pair taking a
`NowPlayingPanel` enum. **Decision: one pair with an enum.** Three panels
with byte-identical logic is the case an enum exists for, it keeps the
persisted-field mapping in one `match` instead of three copy-pasted
`persist_settings` closures, and it gives the contract test one surface to
enumerate. Recorded because the spec leaves it open and `tasks` should not
re-litigate it.

**Alternatives considered**:
- *Reusing `[plugin_panels]`* — spec.md § Assumptions rejects it: that
  table is keyed by plugin identifier and already means something else.
- *Bumping `SCHEMA_VERSION` to 2* — would make every older `settings.toml`
  report a version mismatch for a change that is, by the file's own
  precedent, not a schema change.

---

## R11 — Selected-row text: `text_on_accent` replaces `.weak()`, threaded as a parameter

**Decision**: `draw_content` and `line` take the selection state; on a
selected row every run — title, secondary line, duration, the "E" badge
(`rows.rs:392`), the availability reason (`:398`) — is explicitly coloured
`roles.text_on_accent` and **not** `.weak()`.

**Rationale**: `.weak()` resolves to `Visuals::weak_text_color()`, i.e.
`text_secondary` (`rows.rs:407, 417, 424, 431, 441`), and FR-031 plus
Clarification 6 establish that `text_secondary` over an `accent` fill fails
NFR-6.5's 4.5:1 floor. 014 already ships the `contrast` module
(`theme/contrast.rs`) that SC-011 reuses, so the check is a value test, not
a judgement. The secondary line stays distinguishable by the type scale —
`SIZE_BODY 14.0` vs `SIZE_SECONDARY 13.0` (`tokens.rs:104-105`), already
pinned by `body_and_secondary_are_visibly_different` (`:346-350`) — which
is exactly what NFR-6.4 asks for when colour is removed as the carrier.

**Note on the "E" badge**: it is drawn with a bare `ui.label("E")`
(`rows.rs:392`) and so inherits the default text colour, not `.weak()`. It
still needs the explicit `text_on_accent` on a selected row, because the
default is `text_primary`, which also fails against `accent`. FR-031 names
it; this is why.

**Alternatives considered**:
- *Pushing `visuals.override_text_color` for the row's child `Ui`* —
  tempting and shorter, but it would also recolour the "…" button's glyph
  and any future in-row control, and it cannot express "title full strength
  / secondary line same colour" any better than passing the colour does.
- *Keeping `.weak()` and darkening the selection fill* — changes a token
  role for one widget's benefit; FR-024 forbids new values and 014 forbids
  an eleventh role.

---

## R12 — Hover and pressed must blend *over* the selection fill

**Decision**: the base colour that `rows.rs:544-554`'s existing fill
computation blends into becomes `accent` when the row is selected, and
stays `surface_base` otherwise. The 4 %/8 % `text_primary` overlays
(`theme::controls::hover_fill`/`pressed_fill`, `controls.rs:82-95`) are
unchanged.

**Rationale**: FR-008 requires a selected row to "still visibly react to
the pointer", and the existing code already does
`roles.surface_base.blend(hover_fill(roles))` — a one-line base swap
delivers it. `Color32::blend` is already in use at that site, so no new
mechanism is needed.

**Interaction with the focus ring**: 015's ring is painted by one app-level
pass at the end of `App::ui` into a foreground layer, offset outside the
widget rect (015 research R3, FR-010). Because it is *outside* the fill and
in a different layer, it cannot be hidden by the selection fill — which is
what US2 Scenario 6 asks. Nothing in this feature touches it; contract L6
asserts the two remain separable.

---

## R13 — Tooltips: two Fluent keys, `Response::on_hover_text`, untestable headlessly

**Decision**: two new keys in `locales/en-US/library.ftl` (beside
`row-explicit`/`row-actions`, `library.ftl:18-19`) — `row-open-hint-track`
and `row-open-hint-entity` — attached with
`row_response.on_hover_text(...)`. A third key, `queue-panel-title`, goes
in `playback.ftl` beside `queue-toggle` (`playback.ftl:38`). All three are
added to `tests/fluent_keys.rs`'s inventory, which already asserts the
catalogues define no key the test does not exercise.

**Rationale**: Clarification 9 fixes the two-key split and the reason
(a Track row plays, it does not open). `library.ftl` is where every other
row string already lives; `playback.ftl` is where every other Queue string
lives (`queue-toggle`, `queue-shuffle`, `:38-39`).

**Verification limit, stated plainly**: a tooltip renders in a deferred
popup layer on hover and **cannot be driven by
`Context::run_ui`/`__run_test_ctx`** without synthetic pointer state the
existing suites do not construct. The *keys*' existence and resolution are
automated (`fluent_keys.rs`); the *rendered tooltip text* is SC-004, and
its evidence is manual scenario M4. This is the same values-plus-pixels
split 014 and 015 both used (015 research R16) — not a gap.

**Alternatives considered**:
- *One generic key ("a second click or Enter activates this row")* —
  Clarification 9 rejects it: the hint must never be wrong about what
  happens.
- *Adding pt-BR now* — spec.md § Assumptions: `en-US` is the only bundle
  in the tree (`locales/` has exactly one directory), and closing that gap
  is a dedicated feature's job.

---

## R14 — Skeleton rows are already non-selectable, structurally

**Decision**: no change to `widgets::skeleton::skeleton_row`. FR-032 is
satisfied because skeleton rows are drawn by a *different function* that
never touches `list_row`, has no `RowEntity`, and therefore cannot emit
`RowEvent::Select` or carry the tooltip.

**Rationale**: verified at all five skeleton call sites —
`library_view.rs:129, 299, 349, 387` and `detail_view.rs:106, 125, 141,
169`. Each is an `else`/`None` branch that renders `skeleton_row(ui,
height)` *instead of* `list_row`. FR-032 is a property of the existing
structure, so the task is a **regression test**, not an implementation.
Recorded so `tasks` does not budget implementation work for it.

---

## R15 — The literal scan needs no widening, and must still report 0

**Decision**: `design_token_literals.rs`'s `EXPECTED_BASELINE_HITS` stays
`0` (`design_token_literals.rs:55`) and its patterns and exclusion list are
unchanged. Every value this feature introduces lands in `theme/`, which the
scan excludes.

**Rationale**: the scan covers colour constructors/constants, font-size
literals and panel-level `ui.separator()` calls, excluding `src/theme/**`
(module doc, `:1-30`). This feature's new values are
`TAB_UNDERLINE_WIDTH` (→ `theme/controls.rs`, beside `FOCUS_RING_WIDTH`
`controls.rs:103`), `DURATION_FIGURES`/`duration_measure` (→
`theme/tokens.rs`), and the card's fill/padding/radius (existing
`surface_raised`/`space::LG`/`radius::MD` tokens, no new value at all). SC-008
is therefore met by placement. 015 rejected widening the scan to stroke and
spacing patterns for a stated reason — false positives on legitimate
geometry maths push implementers toward `#[allow]`-shaped escapes (015
Complexity Tracking D8) — and this feature inherits that judgement rather
than reversing it.

**One deliberate non-target**: `ARTWORK_SIZE` (`rows.rs:43`) and
`ACTIONS_RESERVED_WIDTH` (`rows.rs:46`) are pre-existing reserved-geometry
constants in a file this feature modifies. FR-001 explicitly keeps
`ARTWORK_SIZE` "unchanged from today", and neither is a colour, font size
or radius — the three things FR-024 names. They stay.

---

## R16 — Window sizes in SC-006/SC-012 are spec figures, not code figures

**Decision**: the "initial 1200×820" and "960×640 minimum" that SC-006 and
SC-012 sample at are **not set anywhere in this repository**. The manual
scenarios resize the window to those dimensions explicitly rather than
assuming a launch default.

**Rationale**: `crates/modplayer/src/main.rs:129` is
`eframe::NativeOptions::default()` — no `ViewportBuilder`, no
`inner_size`, no `min_inner_size`; a tree-wide search for those symbols
returns that one line and nothing else. The figures come from the source
specification document, and the app currently inherits whatever eframe's
default and the window manager give it.

**Consequence for the plan**: quickstart's M6/M9 must include an explicit
resize step (Quartz `CGWindowListCopyWindowInfo` to read bounds, then a
resize) before capturing, or the evidence does not answer the criterion
asked. Recorded here rather than discovered mid-walk. Setting those sizes
in `main.rs` would be a real fix — and is **out of scope**: FR-026 and the
Scope boundary keep window chrome to the Now Playing workbench feature.

---

## Resolved: no NEEDS CLARIFICATION remained

spec.md's Status line records the clarify session of 2026-09-23 as closed,
and a scan of spec.md finds no `[NEEDS CLARIFICATION]` marker. The two
places where this plan had to choose a shape the spec left open —
R10's enum-vs-three-pairs and R2's list-salt discriminator — are recorded
above and carried into Complexity Tracking in plan.md rather than left
implicit.

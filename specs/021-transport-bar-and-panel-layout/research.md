# Research: Sticky Transport Bar and Panel Layout

**Feature**: 021-transport-bar-and-panel-layout | **Date**: 2026-09-28 | **Spec**: [spec.md](./spec.md)

The Technical Context had no open `NEEDS CLARIFICATION` items after the spec's
clarify session. The items below are the design unknowns found while reading the
current code (`crates/modplayer-ui/src/now_playing.rs`, `queue_view.rs`,
`rows.rs`, `widgets/controls.rs`, `actions.rs`, `section_memory.rs`,
`crates/modplayer-core/src/controller.rs`, `settings/model.rs`) and the local
egui 0.36.2 source. Each one is resolved here.

---

## R1 — How to pin the transport bar inside the Now Playing content `Ui`

**Decision**: Draw the bar with `egui::Panel::top("now-playing-transport-bar")
.resizable(false).show_inside(ui, …)`, **after** the 018 docked column
(`plugin_panels::show_dock`, itself a `Panel::right` drawn with `show_inside`) and
**before** the scroll region. Everything below it goes into one
`egui::ScrollArea::vertical()` filling the remaining rect.

**Rationale**: `show_inside` already works for the docked column (018 contract
D1): it takes space from the parent `Ui` and leaves the rest for later content. A
top panel drawn this way sits outside the scroll area, so it cannot scroll away
(FR-001, FR-010). Its height comes only from its own content (FR-002). Drawing it
after the dock gives it the dock-narrowed width, so the bar never runs under the
docked column. The overlay (`Area`) presentation still needs the existing
pre-measure wrap (R4), because an `Area` never reflows `ui`.

**Alternatives considered**:
- *A `ui.vertical` block followed by a `ScrollArea` with `max_height(ui.available_height())`.*
  This works, but it hand-writes the partitioning that `Panel` already gives, and
  the bar's rect is harder to read back in tests. `PanelState::load` reads a
  panel's rect back from egui memory.
- *A viewport-level `Panel::top` in `App::ui`.* Rejected. It would sit above the
  nav rail and the 018 dock and span the whole window, and it would show on every
  section unless each section gated it.
- *A floating `Area` pinned to the top.* Rejected. An `Area` doesn't reserve
  layout space, so content would scroll underneath it.

---

## R2 — One scroll region: which `ScrollArea` settings

**Decision**: `ScrollArea::vertical()` built by
`SectionMemory::scroll_area(&ViewKey::NowPlaying)` (a new `ViewKey` variant),
then `.animated(false)` and
`.scroll_source(ScrollSource { drag: false, ..ScrollSource::ALL })`. In egui
0.36, the old `drag_to_scroll(bool)` is replaced by `scroll_source`. The
`scroll_bar` and `mouse_wheel` sources stay on. The Effect Chain's inner
`ScrollArea` and `effects_panel_reserved_height` are deleted (spec Clarification
1, FR-003).

**Rationale**:
- **`ViewKey::NowPlaying`**: 020 research R5 left Now Playing out of
  `SectionMemory` only because it had no section-level scroll area. It has one
  now. Going through `SectionMemory` gives the same retention on section switch
  and the same sign-out `epoch` reset as every other section, so 020 FR-007 keeps
  holding without a special case. The `memory_epoch` parameter of
  `now_playing::show` stops being needed and is replaced by the section-memory
  scroll area, which `app.rs` records and returns as `Some(key)` like Search does.
- **`animated(false)`**: egui's own doc says the `animated` flag affects only
  `scroll_to_*`. Turning it off applies the bring-into-view offset in the same
  pass (FR-007 "same interaction"), makes it deterministic in headless
  `ctx.run_ui` tests, and matches the spec's "no animation requirement".
- **No drag source**: the waveform overview and detail, the marker lane
  handles, the effect-chain knobs and the volume slider are all drag targets.
  This is a desktop app, where wheel and trackpad scroll already work. Turning off
  drag-to-scroll removes any chance of a waveform drag also dragging the page.

**Alternatives considered**: a raw `ScrollArea` with an `id_salt` that folds in
`memory_epoch` (today's inner-area pattern). Rejected: that resets on sign-out
but doesn't keep the scroll position when the user switches sections, so Now
Playing would be the only scrollable section that forgets its offset.

---

## R3 — Waveform height source (018 D8) inside a scroll region

**Decision**: Keep `let waveform_h = ui.max_rect().height()` as the first
statement of `now_playing::show`, before the dock, bar or scroll area touch `ui`.
`layout::waveform_heights(waveform_h)` is unchanged.

**Rationale**: Spec Clarification 2 says H is "Now Playing available content
height, independent of scroll position" and that the formula is not changed. The
`CentralPanel` inner height meets both conditions. Inside a `ScrollArea`,
`max_rect().height()` would be unbounded or would depend on content height, so it
must not be read there. Taking H from the whole content rect, including the bar,
keeps SC-003 and 018's D10.9 test values exactly as they are.

**Alternatives considered**: H = scroll viewport height (content minus bar).
Rejected. It changes 018's numbers, and it would make the waveform depend on the
bar's height, which can wrap with window width.

---

## R4 — Bar layout, height invariance and 40 % text expansion

**Decision**: The bar is one `horizontal_wrapped` row of **atomic groups**:

1. **Identity**: 40 px artwork (`rows::ARTWORK_SIZE`) plus a vertical title/artist
   column with a **fixed width budget** of `clamp(0.25 × bar_width, 160, 320)`.
   Title and artist are each one `Label::truncate()` line. The full text goes in
   `on_hover_text` and in the group's AccessKit label.
2. **Transport**: skip back, play/pause, stop, skip forward. `Button`s use
   `TextWrapMode::Extend`, as today (018 D4).
3. **Time**: elapsed / remaining, `theme::mono_text`.
4. **Level**: master volume slider (`widgets::volume::master_volume`) over the
   peak meter (`widgets::peak_meter::peak_meter`), in a `ui.vertical`.
5. **Panel toggles**: Queue, Effects, Transport, preceded by a `space::XL` gap
   (FR-019).
6. **Panels**: 018's toggle, only under 018's conditions.

Before each group is drawn, `wrap_group_before(ui, boundary, width)` checks it.
This generalises today's `wrap_switch_before`, and every group gets the same
treatment. The check pre-measures the group's width. If the group would cross
the wrap boundary (the overlay's left edge, the docked column's edge, or the
row's own right edge; logic unchanged from 018 R8/R12), it calls `ui.end_row()`.

**Rationale**:
- A fixed identity width makes the bar's height independent of title and artist
  length (FR-002, edge case "long title"). Truncating with an ellipsis is allowed
  only for title and artist (spec Clarification 3). Every control label stays
  `Extend`, so wrapping moves whole controls to the next row and never elides a
  control label (FR-017, 018 D4).
- The identity column is two text lines and the level group is two rows, so both
  are about the height of the 40 px artwork. The row height therefore doesn't
  change with playback state or with whether a track is loaded. With no track
  loaded, the identity group draws the placeholder square plus the existing
  `now-playing-empty` text on one truncated line.
- Pre-measuring each group is needed for the same reason 018's M8 walk found:
  `horizontal_wrapped` places a nested `ui.horizontal` or `ui.vertical` before
  its size is known, so it never wraps a group by itself.

**Alternatives considered**:
- *Let the title column take the remaining width.* Rejected. Remaining width
  depends on which groups wrapped, which creates a feedback loop between title
  width and wrap decisions, and the bar's height could then change with title
  length.
- *A compact, single-row peak meter variant for the bar.* Deferred. The existing
  widget's label-over-bar layout fits the identity group's two-line height, and
  changing `peak_meter` would ripple into 015's `meter_bands` tests. If
  pseudo-localisation shows it doesn't fit, a follow-up can add a horizontal
  layout without changing any of this feature's contracts.

---

## R5 — "Bring into view" semantics with egui 0.36 `scroll_to_rect`

**Finding** (egui-0.36.2 `containers/scroll_area.rs` ≈ l.1105–1135): with
`align = None`, egui scrolls by the minimum amount, plus `item_spacing`, when the
target is partly off one edge. When the target's start is **above** the clip and
its end is **below** it, the target is treated as "already in view" and egui
scrolls nothing. With `align = Some(Align::Min)`, egui aligns the target's top to
the viewport's top.

**Decision**: A pure helper

```text
layout::reveal_align(card: Rangef, viewport: Rangef) -> Option<Option<Align>>
  • card.span() > viewport.span()           → Some(Some(Align::Min))   // header to top
  • card ⊆ viewport                          → None                     // no scroll
  • otherwise                                → Some(None)               // egui minimum scroll
```

After the requested card is drawn inside the scroll area,
`ui.scroll_to_rect(card_rect, align)` runs only when the helper returns
`Some(align)`. The viewport is the scroll area's `ui.clip_rect()`.

**Rationale**: This matches spec Clarification 7 and FR-007 exactly, including
the "taller than the viewport → header to the top" case that egui's `None`
branch would otherwise skip. Keeping the decision in a pure function makes it
unit-testable and property-testable without a UI (Principle VIII).

**Alternatives considered**: `Response::scroll_to_me(Some(Align::Min))` always.
Rejected. It scrolls even when the card is already fully visible, and it isn't
the minimum scroll the spec requires.

---

## R6 — Carrying the reveal request from a click or a shortcut to the card

**Decision**: Add a one-shot request stored in egui temp memory under a fixed
id: `now_playing::request_reveal(ctx, NowPlayingPanel)`, which records
`(panel, ctx.cumulative_pass_nr())`. The card-drawing code calls
`take_reveal(ctx, panel)`. It honours a request only if the request was made in
this pass or the previous one, and clears it either way. Two paths make
requests:

- the bar's toggle, when its `changed()` result turned a panel **on**; and
- `actions::invoke`, for `ToggleQueue`, `ToggleEffectChain` and
  `ToggleTransportPanel`, when the toggle function returns the new state `true`.
  The three `toggle_*_panel` functions now return `bool`, and `invoke`'s already
  present (currently unused) `_ctx` parameter is renamed `ctx` and used.

**Rationale**:
- FR-009 requires one behaviour for pointer and keyboard. Both paths still write
  the open flag through the single controller setter (016 contract P3/P7). They
  now also share the reveal request.
- The two-pass window covers the fact that `dispatch_and_invoke` may run before
  or after the section body in a pass. If the action fires while another section
  is shown, the request expires unconsumed. Spec Clarification 10 says existing
  behaviour applies there (the flag flips) and bring-into-view applies only when
  Now Playing is shown.
- A reveal request is not panel **state**. 016 contract P1 bans mirroring the
  open flags (`get_temp::<bool>`) in egui memory. This request is a
  `RevealRequest` struct, not a `bool` flag, and it never holds open or closed
  state, so P1's source-level assertion still holds.

**Alternatives considered**:
- *Defer the reveal until the user next navigates to Now Playing.* Rejected. The
  page would jump to a scroll position the user didn't just ask for, possibly
  minutes later.
- *Keep reveal state in `WaveformState` or a new `App` field.* Rejected. `invoke`
  already has `ctx`, and adding a field would thread a new parameter through
  `dispatch_and_invoke`, `App::ui` and every test call site for a single
  transient value.

---

## R7 — Collapsible card and header ↔ toggle same-frame consistency

**Decision**: Add `widgets::controls::collapsible_panel_card(ui, header,
open: &mut bool, add_contents) -> CardResponse { rect, toggled }`.

- **Header row**: the `Role::Heading` label, unchanged from 016 C3/C4, then a
  disclosure button: a `Variant::Quiet` button with a chevron glyph, ⏷ when open
  and ⏵ when closed (originally ▾/▸; changed after the manual walk, see R15). AccessKit gives it `Role::Button`, the label from
  `panel-collapse` or `panel-expand` with `{ $panel }`, and
  `set_expanded(open)`, following the `settings/category_row.rs:378`
  precedent.
- **Collapsed**: a header-only card. The body isn't drawn and the padding stays
  the same.
- **Shared flag**: a click flips `*open`. The caller writes the flag through
  `set_now_playing_panel_open`, so it is the same flag the bar toggle reads
  (FR-006).
- **Same frame**: the bar is drawn before the cards, so a header click is
  processed after the bar was already painted with the old state. On a header
  toggle the caller calls `ui.ctx().request_discard("panel header toggle")`.
  egui 0.36 then discards that pass's output and re-runs it, so the frame that
  reaches the screen shows the bar toggle and the card in the same state (spec
  edge case "same frame"). Toggling from the bar needs no discard, because the
  cards are drawn after the bar in the same pass.

**This supersedes 016 contract C10** ("a collapsed panel draws nothing at all").
Under C10, a collapsed Markers card, which has no bar toggle (spec
Clarification 5), could never be reopened. The spec also gives every card its
own header collapse control (FR-004). The 016 C10 test in `tests/now_playing.rs`
is rewritten: a closed panel contributes a heading node and a collapsed
disclosure button, and none of its body nodes. 016 C11 ("Markers: card, no
toggle") is also superseded: Markers gains a header disclosure and the new
`markers_open` flag, and still has no **bar** toggle.

**Alternatives considered**:
- *Make the whole header row one clickable button.* Rejected. It would replace
  the `Role::Heading` node that 016 C3/C4 tests and screen-reader heading
  navigation rely on.
- *Accept a one-frame lag.* Rejected. The spec's edge case requires the same
  frame, and `request_discard` is the toolkit's own tool for this.

---

## R8 — Queue row: reuse vs. new widget

**Decision**: Add `rows::queue_row(ui, artwork, row: &QueueRow, position: usize)
-> Option<QueueRowAction>` in `rows.rs`, next to `list_row`. It shares the row
layout primitives: `ARTWORK_SIZE`, `ROW_HEIGHT`, the artwork drawing factored out
of the private `draw_artwork` as `draw_artwork_url(ui, cache, url, name)`,
`title_text`, and the `duration_measure` trailing column. It doesn't go through
`RowEntity` or `list_row` itself.

**Rationale**: `list_row` is built around catalog entities. It has a six-item
"…" menu, a Select/Open click model and selection fill, and none of these apply
to queue rows. FR-012 removes the "…" menu, FR-013 forbids the selected fill,
and FR-015 wants four always-visible quiet actions. Changing `list_row` to fit
would add mode flags to a widget used by five lists. A sibling function that
shares the primitives gives the identical 3-column geometry that review §5.4
asks for, without that risk.

**Row geometry** (leading → trailing):
- artwork (40 px, the initials placeholder when there is no URL or the fetch
  failed, as in `rows.rs`);
- for the current row, a leading `accent` bar (`theme::controls::nav_indicator`
  stroke, the same token 020's nav rail uses) on the row's left edge;
- the text column: for the current row, a ▶ glyph (`queue-playing-glyph`) and
  then the title; below that the artist, weak; the `play next` and `unavailable`
  badges stay on the artist line;
- four quiet action buttons (`button(ui, Variant::Quiet, …)`), each its own tab
  stop, with the same per-row presence rules as today;
- the 1-based position, right-aligned in a `duration_measure`-wide column, in
  `theme::mono_text`.

When text column + actions + position can't fit on one line at the current width
(for example 960 px at +40 % text), the actions move to a second line under the
text column. They aren't elided or hidden (FR-017).

**Accessibility**: the row's container node gets `Role::ListItem` and the label
`queue-row-name` ("{ $title }, { $artist }") or `queue-row-name-current`
("Now playing, { $title }, { $artist }"), both externalised (FR-013, FR-016).
The visible "Now playing:" prefix (`queue-current`) is removed. Its fluent key is
deleted, and `tests/fluent_keys.rs` and `tests/queue_view.rs:221` are updated.

**Alternatives considered**: adding `RowEntity::QueueItem` to `list_row`.
Rejected for the reasons above.

---

## R9 — Queue row data: artwork URL

**Finding**: `modplayer_core::QueueRow` has `uid, title, artist, origin,
unavailable, is_current`. It has no artwork URL.

**Decision**: Add two fields to `QueueRow`: `artwork_url: Option<String>` from
`item.track.artwork_url`, and `artwork_name: String`, which is the album, or the
title when there is no album. `rows.rs` and `now_playing.rs` already use that
rule to derive the name for the initials placeholder. `queue_view()` fills both.
The 1-based position isn't stored. It is `index + 1` over `view.items`, whose
order is already the display order (spec Assumption "Queue position").

**Rationale**: This is a small additive change to a projection struct. It lives
in `modplayer-core`, the crate that owns the queue (Principle III), and it adds
no dependency.

---

## R10 — Markers open flag persistence

**Decision**: Add `markers_open` to both `NowPlayingPanels` (domain) and
`RawNowPlayingPanels` (wire), with `#[serde(default = "default_true")]`. Replace
`NowPlayingPanels`' derived `Default` with a hand-written one that sets
`markers_open: true` and the other three to `false`. Add
`NowPlayingPanel::Markers`, and extend `now_playing_panel_open` and
`set_now_playing_panel_open` to cover it. `SCHEMA_VERSION` stays `1` (016 P5).

**Rationale**: When the key is absent, `true` keeps today's behaviour of markers
always shown (spec Clarification 5, FR-006). A derived `Default` would give
`false` and silently hide Markers for every existing user. 016's P4 promise that
an absent section means all panels are closed still holds for the three original
flags. Markers is new and has its own documented default.

**Alternatives considered**: storing it outside `[now_playing_panels]`. Rejected,
because FR-006 names that section.

---

## R11 — Shift+wheel over the waveform detail inside a vertical scroll area

**Finding**: `waveform/input.rs::pointer_zoom_or_pan` pans the detail view on a
horizontal scroll, or on Shift plus a vertical scroll (`raw.y` when
`raw.x == 0`). A vertical `ScrollArea` wrapping the waveform would also consume
that same `y` delta and scroll the page while the user pans.

**Decision**: When `pointer_zoom_or_pan` produces a `Pan` from the Shift+vertical
branch, it zeroes the consumed delta with
`ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO)`. The enclosing
`ScrollArea` reads its scroll input at the end of `show`, after the content, so
it sees nothing. A plain vertical wheel over the waveform still scrolls the page.
A pinch or Ctrl+wheel zoom produces `zoom_delta`, not a scroll delta, so it isn't
affected.

**Rationale**: This keeps 005's waveform gesture contract working. Before this
feature the waveform had no enclosing scroll area, so the problem is new.

---

## R12 — What moves out of the scroll region

**Decision**: Remove the following from the old column:
- `show_heading`: the 96 px artwork, the `display`-role title, and the artist
  and album lines.
- The control row.
- The standalone master volume and peak meter.

The identity group (R4) replaces them. The `display` text role has nothing left
to style on Now Playing: the bar uses `BODY` for the title, strong, and
`.weak()` for the artist, matching 016 list-row typography. The Now Playing
screen's accessible name is unchanged; the shell nav item and heading stay as
they are (spec Clarification 4). `show_status_line` and `show_transfer_banner`
move to the top of the scroll region (FR-003, FR-018).

**Consequence for 014**: 014 data-model §6 named the Now Playing title as the
app's one `display` surface. That surface is gone, so the `DISPLAY` style stays
defined but unused on this screen. Any test asserting a `display`-sized title on
Now Playing is updated to assert the bar's title and its accessible name
instead. This is recorded in plan Complexity Tracking.

---

## R13 — Testing approach

**Decision**: Headless `egui::Context::run_ui` tests, following the existing
pattern in `crates/modplayer-ui/tests/now_playing.rs` (`fresh_ctx`,
`active_controller`, `tall_input`). Tests assert on AccessKit node bounds and
roles, painted shapes, and `PanelState::load` for the bar rect. Scroll effects
are checked by running two or three passes and comparing node bounds against the
screen rect. There are no pixel snapshots.
- Pure helpers get unit tests and proptests in `layout.rs`: `reveal_align` and
  `identity_width`.
- Serialisation of `NowPlayingPanels` with four flags gets a proptest round-trip
  in `settings/model.rs` (Principle VIII, state serialisation).
- Manual scenarios in quickstart.md are run by the implementing agent under the
  constitution's Manual Scenario Sign-Off recipe.

## R15 — Manual walk findings (2026-09-28)

Deviations and defects found while executing quickstart M1–M14 against the real
build, and what was done about each:

- **Tofu disclosure chevrons.** `▾`/`▸` exist only in egui's monospace Hack
  font; the proportional family (Ubuntu-Light → NotoEmoji → emoji-icon-font)
  has neither, so every card header drew a replacement box. Fixed: the glyphs
  are now `⏷`/`⏵` (emoji-icon-font), exposed as `DISCLOSURE_OPEN_GLYPH` /
  `DISCLOSURE_CLOSED_GLYPH`, with a unit test asserting both resolve in the
  proportional font. The effect-row drag handle `⋮` has the same problem but
  predates this feature (it's on `main`); left alone.
- **Effect Chain card painted over the plugin dock.** A vertical-only
  `ScrollArea` widens its clip on the horizontal axis to its parent's clip, so
  card content wider than the centre column drew on top of the docked panels.
  Rows were too wide because `level_pair`, switches and mode combos are atomic
  `horizontal`s that a wrapping row can't break, and one overflowing item
  widened the card's max rect for every row after it. Fixed in three parts:
  the scroll region's clip is pinned to the area left of the dock; the pre-
  and post-chain level pairs sit on separate lines; and each node row wraps,
  with its parameters starting on their own line.
- **Duplicate title/artist tooltip.** `Label::truncate()` shows egui's own
  elision tooltip, and the bar also adds `on_hover_text`, so hovering a
  truncated title showed two identical tooltips. Fixed with
  `show_tooltip_when_elided(false)`.
- **M7: shortcut while on Library.** `ToggleQueue`, `ToggleEffectChain` and
  `ToggleTransportPanel` are `Scope::NowPlaying` (unchanged from `main`), so
  pressing Q on Library does nothing. The spec requires preserving the
  existing off-screen behaviour, which this does; the quickstart's "the flag
  flipped" expectation was wrong and is corrected.
- **M8: no runtime pseudo-localisation override.** 018's +40 % expansion is the
  test-only `i18n::with_pseudo_expansion`; the app has no environment switch
  for it. The +40 % case stays covered by `responsive_dock` (T-B5). Manually,
  M8 was run at 960 × 640 with real strings.
- **Harness note (M14).** A synthetic Shift+wheel `CGEvent` leaves Shift
  latched in the HID state, so later unflagged synthetic wheel events still
  read as Shift. The helper must set flags explicitly (0 for a plain wheel).
  This isn't an app defect.
- **M12: transport buttons with no track.** Contract B7 ties them to
  `transport_enabled()` (active device and source health), not to having a
  track, so they're enabled on a fresh signed-in session. The quickstart said
  "disabled" and has been corrected.
- **Harness note (M10/M11).** Queue rows are laid out only while visible, so
  off-screen rows aren't in the Tab cycle. Reveal the Queue before tabbing to
  row actions. The Library's "Play now" queues the whole saved-tracks context
  (395 items), so use a search result to get a short queue.
- **Outside this feature: in-session re-sign-in.** After Settings › Account ›
  Sign out and signing in again without a restart, the bar showed "Sign in to
  play from your account" (transport disabled) and Library showed 0 items
  until the app was relaunched. The relaunched app was healthy. This is the
  account/session flow, not the layout; worth a follow-up.

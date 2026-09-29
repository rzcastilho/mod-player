# Data Model: Sticky Transport Bar and Panel Layout

**Feature**: 021-transport-bar-and-panel-layout | **Date**: 2026-09-28 | **Research**: [research.md](./research.md)

This feature is mostly layout. It changes four data shapes: one persisted
settings section, one core enum, one core projection struct, and one UI-side
transient request. There are also two pure layout functions, whose inputs and
outputs are specified here so they can be property-tested.

---

## §1 `NowPlayingPanel` (modplayer-core, `controller.rs`) — extended

| Variant | Status | Bar toggle | Header disclosure | Persisted flag |
|---|---|---|---|---|
| `Markers` | **new** | none (spec Clarification 5) | yes | `markers_open` |
| `EffectChain` | unchanged | "Effects" | yes (new) | `effect_chain_open` |
| `Transport` | unchanged | "Transport" | yes (new) | `transport_open` |
| `Queue` | unchanged | "Queue" | yes (new) | `queue_open` |

- The render order below the waveform follows `NowPlayingPanel::RENDER_ORDER`,
  a new associated `const`: `[Markers, EffectChain, Transport, Queue]`
  (FR-004).
- `Markers` renders only while `controller.current_track().is_some()`. The other
  three always render, either open or as a header-only collapsed card.
- The doc comment that says "Markers has no toggle (FR-021) and is not a variant
  here" is replaced. Markers has no **bar** toggle, but it now has a persisted
  open flag.

## §2 `NowPlayingPanels` / `RawNowPlayingPanels` (modplayer-core, `settings/model.rs`) — extended

```text
NowPlayingPanels {                    RawNowPlayingPanels  ([now_playing_panels])
  effect_chain_open: bool  (false)      effect_chain_open: bool  #[serde(default)]            → false
  transport_open:    bool  (false)      transport_open:    bool  #[serde(default)]            → false
  queue_open:        bool  (false)      queue_open:        bool  #[serde(default)]            → false
  markers_open:      bool  (TRUE)  NEW  markers_open:      bool  #[serde(default = "true_")]  → true   NEW
}
```

**Validation and rules**
- **V1**: `Default` is written by hand, not derived, so that `markers_open`
  defaults to `true` (research R10). The same applies to `RawNowPlayingPanels`.
- **V2**: if the key or the whole section is absent, `markers_open` is `true`
  and the other three are `false`. This keeps 016 P4 for the original three and
  today's behaviour of Markers always being visible.
- **V3**: a non-boolean value is a TOML deserialisation error handled by the
  existing whole-file fallback. The section keeps plain `bool` fields, the same
  as its three siblings, and there is no new permissive `Option<toml::Value>`
  path.
- **V4**: `SCHEMA_VERSION` stays `1` (016 P5).
- **V5**: flags are independent. Setting one never changes another (016 P6,
  extended to four flags).

**State transitions** (per flag): `open ⇄ closed`. The only writer is
`PlaybackController::set_now_playing_panel_open(panel, open)`, which updates the
shadow state and then calls `persist_settings`. It is reached from three places:
the bar toggle, the card's header disclosure, and the `ToggleQueue`,
`ToggleEffectChain` and `ToggleTransportPanel` actions.

## §3 `QueueRow` (modplayer-core, `controller.rs`) — extended

| Field | Type | Status | Source in `queue_view()` |
|---|---|---|---|
| `uid` | `QueueItemId` | unchanged | `item.uid` |
| `title` | `String` | unchanged | `item.track.title` |
| `artist` | `String` | unchanged | `item.track.artists.join(", ")` |
| `origin` | `Origin` | unchanged | `item.origin` |
| `unavailable` | `bool` | unchanged | `item.unavailable` |
| `is_current` | `bool` | unchanged | `uid == current uid` |
| `artwork_url` | `Option<String>` | **new** | `item.track.artwork_url.clone()` |
| `artwork_name` | `String` | **new** | `item.track.album` or else `item.track.title` (the initials placeholder source, same rule as `rows.rs` and `now_playing.rs`) |

- **Derived, not stored**: `position = index + 1`, where `index` is the row's
  place in `QueueView::items`. That order is already the display order: the
  current item, then play-next, then upcoming context, with history excluded
  (spec Assumption).
- **Invariant**: at most one row has `is_current == true`. This is unchanged.

## §4 `RevealRequest` (modplayer-ui, `now_playing.rs`) — new, transient

```text
struct RevealRequest { panel: NowPlayingPanel, pass: u64 }   // egui temp memory, Id::new("now-playing-reveal")
```

- **Written by**:
  - `now_playing::request_reveal(ctx, panel)` when the bar toggle turns a panel
    on in the current pass;
  - `actions::invoke` when a `Toggle*` action's toggle function returns `true`.
- **Consumed by**: `now_playing::take_reveal(ctx, panel) -> bool`. It returns
  `true` if a request for `panel` exists and
  `ctx.cumulative_pass_nr() - pass ≤ 1`. It always removes the request.
- **Lifetime**: at most two passes. If a request isn't consumed in that window,
  it is dropped, and there is no deferred reveal (research R6).
- It is never persisted, and it never holds open or closed state (016 P1 still
  holds).
- Only one request is kept at a time. A newer request replaces an older one,
  because the most recently opened panel wins (spec Clarification 9).

## §5 `ViewKey::NowPlaying` (modplayer-ui, `section_memory.rs`) — new variant

- Adds a variant to `ViewKey`. Its `Hash` arm is a unit arm, like `Search` and
  `Plugins`.
- It is recorded by `app.rs` after `now_playing::show` returns the scroll
  output's `offset.y`, just as for Search and Plugins, and is reset on sign-out
  through `epoch`.
- The module doc line "Now Playing has no entry (research.md R5)" is replaced.

## §6 Pure layout functions (modplayer-ui, `layout.rs`) — new

### `reveal_align(card: Rangef, viewport: Rangef) -> Option<Option<Align>>`

| Condition (y axis) | Result | Meaning |
|---|---|---|
| `card.span() > viewport.span()` | `Some(Some(Align::Min))` | align the card's header to the top of the viewport |
| `viewport.min ≤ card.min && card.max ≤ viewport.max` | `None` | already fully visible, so no scroll |
| otherwise | `Some(None)` | egui minimum scroll |

- **P-R1**: if the result is `None`, the card is fully inside the viewport.
- **P-R2**: if `card.span() ≤ viewport.span()`, the result is never
  `Some(Some(_))`.
- **P-R3**: the result depends only on the relative position of the two ranges,
  so translating both by the same `dy` doesn't change it.

### `identity_width(bar_width: f32) -> f32`

`clamp(0.25 × bar_width, 160.0, 320.0)` (research R4).
- **P-I1**: the result is always in `[160, 320]`.
- **P-I2**: the result never decreases as `bar_width` grows.
- **P-I3**: the result doesn't depend on title or artist length. This is
  structural, because the function has no text parameter.

## §7 Widget output: `CardResponse` (modplayer-ui, `widgets/controls.rs`) — new

```text
pub struct CardResponse { pub rect: Rect, pub toggled: bool }
```

`collapsible_panel_card(ui, header, &mut open, add_contents) -> CardResponse`.
`rect` is the card's outer rect this pass, which feeds the reveal check.
`toggled` is `true` if the header disclosure flipped `open` this pass. In that
case the caller persists the flag and calls `ctx.request_discard(..)`
(research R7).

## §8 Queue row action (modplayer-ui, `rows.rs`) — new

```text
pub enum QueueRowAction { MoveUp, MoveDown, PlayNext, Remove }
```

`rows::queue_row` returns `Option<QueueRowAction>`, and `queue_view::show`
applies it to the controller: `queue_move_up`, `queue_move_down`,
`queue_play_next` and `queue_remove`, which are unchanged. The presence rules
are unchanged from today: `PlayNext` is absent on the current row, and the other
three are always present.

## Entity ↔ spec map

| Spec Key Entity | Model |
|---|---|
| Transport Bar | `Panel::top("now-playing-transport-bar")` plus the §6 `identity_width` rule. There is no data type; its state is the controller's playback state plus §2 flags |
| Waveform Region | unchanged `WaveformState`; heights from `layout::waveform_heights(H)` (018) |
| Panel Card | §1 plus §2 plus §7 |
| Queue Row | §3 plus §8 |

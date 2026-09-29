# Contract: Now Playing layout (transport bar, scroll region, panel cards, reveal)

**Feature**: 021-transport-bar-and-panel-layout | Requirements: FR-001–FR-011, FR-016–FR-019; NFR-6.1, NFR-6.2, NFR-6.4, NFR-7.4 | Model: [data-model.md](../data-model.md) §1, §4–§7

This contract supersedes the following:
- 016 `contracts/panel-card.md` **C10** (a collapsed card now draws a
  header-only card), **C11** (Markers gains a header disclosure, but still no
  bar toggle) and **C13** (the reserved-height calculation is deleted);
- the layout part of 018 `contracts/ui-responsive-dock.md` **D4**, where the
  Panels toggle is now the last group of the transport bar.

All other 016 and 018 rules still apply, including 018 D1–D3, D5–D10.

## Region structure

```text
Now Playing content Ui (CentralPanel inner rect)
├─ [018 D1] docked plugin column   Panel::right  (only when Docked)
├─ TRANSPORT BAR                   Panel::top("now-playing-transport-bar")   — pinned
└─ SCROLL REGION                   ScrollArea::vertical (ViewKey::NowPlaying) — the only vertical scroll
   ├─ status line / transfer banner
   ├─ waveform region (overview lane+view, time labels, detail lane+view)   [only with a track]
   ├─ XL
   ├─ Markers card                 [only with a track]
   ├─ XL
   ├─ Effect Chain card
   ├─ XL
   ├─ Transport focus card
   ├─ XL
   └─ Queue card
[018] overlay Area / floated windows — position-independent, unchanged
```

## Transport bar (B)

**B1 — Pinned.** The bar sits outside the scroll region. For every scroll
offset, including the maximum, every bar control's AccessKit bounds are inside
the window's content rect, and each control still responds to a click and to
keyboard activation (FR-001, FR-010, SC-001).

**B2 — Order, leading to trailing.** The groups are:
1. identity: 40 px artwork, then title over artist;
2. transport: skip back, play/pause, stop, skip forward;
3. time: elapsed and remaining;
4. level: master volume over the peak meter;
5. a `space::XL` gap, then the Queue, Effects and Transport toggles;
6. Panels, shown only under 018 D1's Hidden or Overlay presentation.

There is no Markers toggle. The old `show_heading` row is gone (FR-018).

**B3 — Height invariance.** Over one test run at a fixed width, the bar's outer
height stays within ±1 px across all of the following (FR-002, FR-011, SC-003):
- window heights {640, 820, 1200};
- scroll offsets {0, max};
- panel flag combinations: all closed, all open;
- `Intent::Playing` and `Intent::Paused`;
- a track title of 5 characters and of 200 characters;
- no track loaded.

The bar's outer height is read from `PanelState::load(ctx, bar_id)`.

**B4 — Identity width.** The title and artist column is exactly
`layout::identity_width(bar_width)` wide. Title and artist are one line each,
truncated with an ellipsis. The full title and artist appear in the hover text
and in the identity group's AccessKit label, which is
`now-playing-bar-identity` → "{ $title }, { $artist }" (FR-002, edge case
"long title").

**B5 — Wrapping.** Each group is atomic. A group moves to a new row only when
drawing it would cross the wrap boundary, which is the overlay's left edge, the
docked column's edge, or the row's right edge (018 R8/R12). No control label is
elided at 960 × 640 with +40 % pseudo-localisation, and no two interactive rects
in the bar overlap (FR-017, 018 D10.8 extended to the bar).

**B6 — Toggles.** Each panel toggle is `switch(ui, SwitchKind::Toggle, …)` from
015:
- `Role::Button`;
- `Toggled::True` exactly when the panel's flag is open;
- the on and off states differ in thumb position as well as colour
  (NFR-6.4).

The toggle group is separated from the transport group by at least
`space::XL` (FR-019).

**B7 — No-track state.** The bar renders with the same groups. Transport
buttons follow `controller.transport_enabled()`, which is disabled when
transport isn't available, the same as today. The identity group shows the
placeholder square and the `now-playing-empty` text (edge case "no track").

## Scroll region (S)

**S1 — One scroll area.** Exactly one vertical `ScrollArea` exists in Now
Playing. No card body contains a nested vertical `ScrollArea`. The Effect Chain
inner area and `effects_panel_reserved_height` are deleted (FR-003).

**S2 — Waveform heights.** The overview and detail rect heights equal
`layout::waveform_heights(H)`, where H is `ui.max_rect().height()` of the
Now Playing content `Ui`, captured before the dock or bar is drawn. H doesn't
depend on the scroll offset or on the bar's height. 018 D8/D10.9 still hold
(FR-003, FR-011). At a content height below bar plus waveform, the heights stay
at their minimums (64 and 120) and the region becomes scrollable
(US1 Scenario 2).

**S3 — Spacing.** Adjacent cards, and the waveform and the first card, are
separated by `space::XL`. No `Separator` or hairline shape is painted in the
scroll region (FR-005).

**S4 — Retention.** The offset is kept per `ViewKey::NowPlaying` across section
switches and reset on sign-out (020 FR-007 via `SectionMemory`).

**S5 — Input.** The scroll area has no drag scroll source. When a Shift+vertical
wheel pans the waveform detail, that wheel delta doesn't also scroll the region
(research R11).

## Panel cards (C)

**C1 — Collapsible card.** `collapsible_panel_card` has:
- the 016 card chrome, which is unchanged (C2–C4: `surface_raised`, `LG`
  padding, `MD` radius, uppercase `section` header, and a `Role::Heading` with
  the exact label);
- a disclosure button in the header: `Role::Button`, with the accessible name
  `panel-collapse` or `panel-expand` ("Collapse { $panel }" / "Expand { $panel }"),
  and `expanded = open`.

**C2 — Collapsed card.** A collapsed card draws its header and disclosure only,
and none of its body nodes. (This supersedes 016 C10.)

**C3 — One state.** The header disclosure and the bar toggle read and write the
same `[now_playing_panels]` flag through the controller. After a header click,
the output that reaches the screen shows the bar toggle's `Toggled` state equal
to the card's expanded state. Implementation: `request_discard`. The test runs
the pass and asserts both nodes in the resulting AccessKit update (FR-006, edge
case "same frame").

**C4 — Order and presence.** The cards are Markers, then Effect Chain, then
Transport focus, then Queue. Markers is present only with a track. The other
three are always present (FR-004).

**C5 — Keyboard.** Every disclosure button can be reached with Tab and
activated with Space or Enter (NFR-6.1).

**C6 — Master volume and peak meter placement.** They appear only in the bar.
No master-volume slider node or peak-meter node exists inside the scroll region
(FR-004).

## Reveal (R)

**R1 — Opening a panel reveals it.** When the scroll region is scrolled to the
top and a closed panel's bar toggle is clicked, then after that pass the card's
heading bounds and bottom edge are inside the scroll viewport. If the card is
taller than the viewport, the heading's top is within `item_spacing.y` of the
viewport's top, directly under the bar. No second input is needed (FR-007,
SC-002). This holds for Queue, Effects and Transport.

**R2 — Minimum scroll.** If the card is already fully visible when it opens, the
scroll offset doesn't change. The decision is made by `layout::reveal_align`
(data-model §6).

**R3 — Focus.** After a reveal, keyboard focus is on the widget that had it
before: the toggle for a click, or unchanged for a shortcut. Focus never moves
into the card (FR-007).

**R4 — Collapse never reveals.** Pressing an open panel's toggle collapses it,
even when the card is off screen. The offset changes only by the clamp caused by
the shorter content (FR-008).

**R5 — Keyboard parity.** Invoking `HostAction::ToggleQueue`,
`ToggleEffectChain` or `ToggleTransportPanel` through `actions::invoke`, with
their current bindings, produces the same R1, R2 and R4 behaviour as the bar
toggle (FR-009). If Now Playing isn't the shown section, the flag flips (this is
the existing behaviour), no reveal happens, and no reveal happens later on
navigation.

**R6 — Newest wins.** With another panel already open, the newly opened card is
revealed even if that scrolls the other card out of view. The other card stays
open (spec Clarification 9).

## Test obligations

| ID | Assertion | Location |
|---|---|---|
| T-B1 | B1: all four panels open, 960 × 640, scroll to max, then each bar control's bounds are inside the content rect; a click on play/pause changes `Intent` | `crates/modplayer-ui/tests/now_playing.rs` |
| T-B3 | B3 matrix, bar height ±1 px | same |
| T-B4 | B4: 200-char title, one line, ellipsis painted, full text in the AccessKit label | same |
| T-B5 | B5 at 960 × 640 with +40 % pseudo-localisation and at dock 240: no elided labels, no overlapping interactive rects | `crates/modplayer-ui/tests/responsive_dock.rs` |
| T-B6 | B6 toggles report `Toggled` and are separated by at least `XL` from skip forward's rect | `tests/now_playing.rs` |
| T-S1 | S1: no nested scroll area; `effects_panel_reserved_height` removed (source-level grep) | `tests/now_playing.rs` |
| T-S2 | S2: overview and detail heights at H ∈ {640, 820, 1200} equal `waveform_heights(H)`, independent of scroll | `tests/waveform.rs` (extends D10.9) |
| T-S3 | S3: no `Separator` or hline shape inside the scroll region's clip rect; card gaps equal `XL` | `tests/now_playing.rs` |
| T-C2 | C2 (replaces the 016 C10 test) | same |
| T-C3 | C3: header click, then in the same output the bar toggle is `Toggled::False` | same |
| T-C4 | C4 order and presence with and without a track | same |
| T-C6 | C6 no volume or meter nodes below the bar | same |
| T-R1 | R1 for each of Queue, Effects, Transport, including a tall-card case (16-node chain) where the header aligns to the top | same |
| T-R2 | R2 already visible, so the offset is unchanged | same |
| T-R4 | R4 collapse while off screen | same |
| T-R5 | R5 through `actions::dispatch_and_invoke` with the bound keys; the not-shown section case | `tests/actions.rs` |
| T-L1 | `reveal_align` P-R1–P-R3 proptest; `identity_width` P-I1–P-I2 proptest | `crates/modplayer-ui/src/layout.rs` unit tests |
| T-K | new fluent keys resolve; `queue-current` removed from the key list | `tests/fluent_keys.rs` |

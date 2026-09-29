# Contract: Queue panel rows

**Feature**: 021-transport-bar-and-panel-layout | Requirements: FR-012–FR-016, FR-3.2.2; NFR-6.1, NFR-6.2, NFR-6.4, NFR-7.4 | Model: [data-model.md](../data-model.md) §3, §8

The Queue card's header controls stay as they are: the shuffle switch and the
repeat cycle button. The empty-state copy (`queue-empty`) also stays. The rows
change.

## Geometry (Q1–Q4)

**Q1 — Three columns.** From leading to trailing edge, each row has:
- the 40 px artwork (`rows::ARTWORK_SIZE`);
- the text column: title (`body`, `text_primary`) on line 1 and artist
  (`.weak()`) on line 2, with the `queue-badge-play-next` and
  `queue-badge-unavailable` badges on line 2;
- the quiet row actions;
- the position column: a `theme::duration_measure`-wide, right-aligned column
  holding `position` (1-based) in `theme::mono_text`.

There is no "…" menu. The title and the artist are on separate lines (FR-012,
US4 Scenario 2).

**Q2 — Missing artwork.** If `artwork_url` is `None` or the fetch fails, the row
shows the initials placeholder built from `artwork_name`. This is the same
treatment Library and Search rows use.

**Q3 — Row height.** The row is `ROW_HEIGHT` when everything fits on one line.
When the text column would drop below its minimum width (research R8) because of
the actions and position, the actions move to a second line under the text
column. The row grows; nothing is elided or hidden (FR-017).

**Q4 — Text truncation.** Title and artist each truncate to one line with an
ellipsis inside the text column. The full text is in the row's accessible name.

## Current-row mark (Q5–Q7)

**Q5 — Two signals.** The current row (`is_current`) has both:
- a leading ▶ glyph (`queue-playing-glyph`) before the title; and
- a vertical `accent` bar on the row's leading edge, spanning the row's full
  height (`theme::controls::nav_indicator` stroke).

Neither signal depends on colour alone. The glyph is a shape, and the bar is a
position and a shape (FR-013, FR-014, SC-004).

**Q6 — Not a selection.** The current row isn't filled with `accent`, which is
016's selected fill. Its background follows the same hover and pressed rules as
any other row (`row_frame`) (FR-013).

**Q7 — No visible prefix.** No row paints the text "Now playing:". The
`queue-current` fluent key is deleted (FR-013, US4 Scenario 1).

## Actions (Q8–Q10)

**Q8 — Quiet and always visible.** Move up, Move down, Play next and Remove use
`button(ui, Variant::Quiet, tr(key))` with their existing keys
(`queue-move-up`, `queue-move-down`, `queue-play-next`, `queue-remove`). They
are painted on every row whether or not the row is hovered (FR-015).

**Q9 — Presence.** Play next is absent on the current row. The other actions
are present on every row. Each click runs exactly one of `queue_move_up`,
`queue_move_down`, `queue_play_next` or `queue_remove` with the row's `uid`.
This is unchanged behaviour (FR-3.2.2).

**Q10 — Keyboard.** Each action is its own Tab stop, in row order and then in
action order, and activates with Space or Enter. Its accessible name is the
action's label (NFR-6.1, NFR-6.2, FR-016).

## Accessibility (Q11)

**Q11 — Row node.** Each row has one `Role::ListItem` node. Its label is:
- `queue-row-name` → `{ $title }, { $artist }`; or
- for the current row, `queue-row-name-current` →
  `Now playing, { $title }, { $artist }`.

When the artist is empty, the key's `$artist` falls back to omitting the
`, { $artist }` part. This uses a Fluent select on
`$has_artist`.

## Test obligations

| ID | Assertion | Location |
|---|---|---|
| T-Q1 | Every row: one artwork texture or placeholder rect about 40 px square; the title galley's top is above the artist galley's top; a position label is right-aligned inside the trailing measure | `crates/modplayer-ui/tests/queue_view.rs` |
| T-Q2 | Missing artwork gives the initials placeholder | same |
| T-Q3 | 960 px width and +40 % pseudo-localisation: every action label is unelided and no interactive rects overlap | same |
| T-Q5 | The current row paints the ▶ glyph and a line segment whose colour is `nav_indicator`; no other row does | same |
| T-Q6 | No `accent`-filled rect behind the current row | same |
| T-Q7 | No painted text equals the old `queue-current` string (replaces the assertion at `tests/queue_view.rs:221`) | same |
| T-Q8 | With the pointer away from all rows, every row paints its action buttons | same |
| T-Q9 | Each action applies to its row's uid; Play next is absent on the current row | same (existing tests retargeted) |
| T-Q10 | Tab order visits each row's actions in order; Enter on a focused Remove removes that row | same |
| T-Q11 | `ListItem` label equals the resolved `queue-row-name-current` for the current row | same |
| T-Q12 | A single-item queue where the one row is current renders the mark correctly | same |
| T-QC | `queue_view()` fills `artwork_url` and `artwork_name` (album, else title) | `crates/modplayer-core/src/controller.rs` unit |

# Contract: Effect Chain Rows, Handle and Empty State (024)

Supersedes the row layout parts of 008's `contracts/ui-effect-chain.md`
§2/§4 (row = one run-on `horizontal_wrapped`). Everything not named here
(controller calls, DnD payload type, `↑`/`↓` claims, capacity refusal) is
unchanged from 008. Surface: `crates/modplayer-ui/src/effects_view.rs`.

Public API of the module is unchanged: `show`, `handle_focused_handle_keys`,
`handle_id`, `toggle_effect_chain_panel` keep their signatures.

## R1 — Four zones per row

| # | Zone | Content (in order) |
|---|---|---|
| 1 | Identity | drag handle · `"{index+1}."` · kind label · owner label |
| 2 | State | `Bypass` toggle · `effects-cpu` figure (mono) · `effects-auto-bypassed` note if `auto_bypassed` · `effects-mode-note` note if `mode_note` |
| 3 | Parameters | the kind's controls exactly as 008 (ranges, steps, suffixes, combos, `add_enabled` for phase invert) |
| 4 | Actions | `destructive_gap` · `Remove` (`Variant::Destructive`) |

- R1.1 Adjacent zones on one line are separated by a vertical
  `ui.separator()` and `theme::space::LG` spacing.
- R1.2 A zone is never split across lines; line breaks happen only
  between zones, decided by `zone_breaks` (data-model §4.1).
- R1.3 First frame (no remembered widths): Parameters starts a new line.
- R1.4 When Actions shares a line with Parameters it is right-aligned.
- R1.5 The notes no longer appear in the Parameters zone.
- R1.6 Kind and owner labels are truncating `Label`s (max identity text
  width 180 px) and show full text on hover when elided.
- R1.7 An empty Parameters zone still keeps its separator slot.

## R2 — Drag handle

- R2.1 A six-dot 2×3 grip painted in `roles.text_secondary` (not a font glyph: egui's bundled fonts lack U+283F "⠿" — T043 D1), visible without hover.
- R2.2 Pointer hover: `CursorIcon::Grab` (egui built-in); while the
  handle's own drag is active: `CursorIcon::Grabbing`.
- R2.3 Focusable; focus ring from the app-level `paint_focus_ring` only.
- R2.4 AccessKit: role `Button`, name
  `tr_args("effects-reorder-handle-node", kind, position)` =
  "Reorder {kind}, position {n}" — starts with `tr("effects-reorder-handle")`.
- R2.5 Hover tooltip: `tr("effects-reorder-handle-hint")`.
- R2.6 `↑`/`↓` while focused move the node by ∓1 (unchanged) and focus
  stays on the same node's handle; every row's position number and handle
  name reflect the new order in the next frame.
- R2.7 Pointer drop onto another row calls `chain_move_node` (unchanged).

## R3 — Focus order

Tab order within a row: handle → Bypass → parameter controls (existing
order) → Remove; then the next row's handle. After the last row: the add
row (or the empty-state button).

## R4 — Empty state (`view.nodes.is_empty()`)

- R4.1 Header figures, level pairs and spectrum render first (as with
  nodes).
- R4.2 Collapsed: a wrapped label `tr("effects-empty-explanation")` and
  one `button(Variant::Primary, tr("effects-empty-add"))`. No kind combo
  is visible.
- R4.3 Activating it (click / Enter / Space) reveals, from the next frame,
  the existing add row (kind combo + `effects-add-node` label + "Add"
  confirm drawn as `Variant::Primary`), explanation still shown, and
  focuses the kind combo once.
- R4.4 Exactly one `Primary` control is visible in the panel in every
  state.
- R4.5 With ≥ 1 node: no explanation; ordinary add row below the rows,
  "Add" drawn as today (default button); reveal/focus flags cleared, so
  a later empty chain starts collapsed.
- R4.6 Adding still goes through `chain_add_node(kind)`; the at-capacity
  refusal text is unchanged.

## R5 — Narrow widths

At a 960 × 640 window with the panel open: no two controls' rects
overlap, and every control rect lies inside the panel card's rect.

## Test hooks

Tests locate controls via AccessKit (existing `render_nodes_on` /
`find_all` helpers in `tests/effects_view.rs`): handle by name prefix
`tr("effects-reorder-handle")`, empty-state button by
`(Role::Button, tr("effects-empty-add"))`.

# Contract: Markers Panel Structure (UI)

**Feature**: 023-markers-panel-structure | **Supersedes**: [006 contracts/ui-markers.md §4](../../006-markers-loops-and-cues/contracts/ui-markers.md#4-markers-panel-fr-021) (panel layout only; §1–§3, §5–§9 of that contract stay in force) | **Types**: [data-model.md](../data-model.md)

This is the externally observable contract of `markers::panel` (`crates/modplayer-ui/src/markers.rs`): what is rendered, in what order, with which roles, names and keyboard behaviour. The same automated tests (§9) pin both the AccessKit tree and the visible behaviour.

The public signature is unchanged:

```rust
pub fn panel<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
    open: &mut bool,
) -> CardResponse
```

`pub fn handle_focused_marker_keys` keeps its signature. Its only behavioural change is the Enter gate in P7.

---

## P1. Top-level layout (inside `collapsible_panel_card("markers-panel")`)

**Populated** (`TrackMarkers::count() > 0`), top to bottom:

```text
[markers-status line]                  ← only if waveform.marker_status is Some (FR-021)
LOOP REGION  n                         ← heading, always
  [New loop region]                    ← Default variant, first line of the group (FR-017)
  ┌ block per region (research R3 order) ─────────────────────────┐
  │ A row             (if present)                                │
  │ B row             (if present)                                │
  │ arm / armed-inactive / ⚠ line                                 │
  │ repeat · crossfade line                                       │
  │ wraps-remaining | wraps-infinite line (armed-active only)     │
  └───────────────────────────────────────────────────────────────┘  space::SM between blocks
POINTS  n                              ← only if n > 0
  point rows, by position
CUES  k                                ← always; k = occupied slots
  slot 1 … slot 8 (occupied row | empty-slot row)
                                   [Clear all markers]   ← footer, right-aligned, Destructive (FR-018)
```

**Empty** (`count() == 0` or no `TrackMarkers`):

```text
[markers-status line]                  ← only if Some
No markers — press I to set A          ← markers-empty
[New loop region]
```

There are no headings, no cue rows and no Clear all (FR-020, SC-007).

The spacing between groups is `theme::space::MD`, between a heading and its first line `space::XS`, and between loop blocks `space::SM`. No new spacing literal is introduced (FR-023).

## P2. Group heading

| Aspect | Value |
|---|---|
| Visible | `section_label(tr_args(key, count))` + `mono_text(count)` in `text_secondary` |
| Keys | `markers-group-loop`, `markers-group-points`, `markers-group-cues` (each `{ $count }`; en-US value is the bare label) |
| AccessKit | `Role::Heading`, label `markers-group-heading { $label } { $count }` → e.g. "Points, 2" |
| Interaction | none; not focusable; not collapsible |
| Count | Loop = number of regions (incomplete counts 1); Points = number of points; Cues = number of occupied slots (FR-002) |

## P3. Populated row (loop boundary, point, occupied cue)

Children are created left to right in the order below. That is both the visual order and the Tab order (Clarification 6, research R5).

| # | Cell | Widget | AccessKit | Behaviour |
|---|---|---|---|---|
| 1 | swatch | 16×16 filled button (`theme::marker_color`) | `Role::Button`, label `markers-color { $index }` (1-based) | click / Enter / Space → palette popover (P4) |
| 2 | role | label `marker-role-a`/`-b`/`-point`/`-cue { $slot }` | static text | none |
| 3 | name | Quiet button with the name, or `markers-name-placeholder` ("Add name") in `text_secondary` when empty; truncated to available width | `Role::Button`, label = name or placeholder | click / Enter / F2 → inline rename (P5) |
| 4 | position | `mono_text(format_mmss_millis(pos))`; the live drag position while dragging | static text | none |
| 5 | clamped ⚠ | as 006 (`warn_fg_color`, `marker-clamped-desc`) | static text + tooltip | only when `clamped` |
| 6 | jump | Quiet button, glyph `theme::markers::ROW_ACTION_JUMP_GLYPH` | `Role::Button`, label `markers-jump` | P6 |
| 7 | nudge earlier | Quiet, `ROW_ACTION_NUDGE_EARLIER_GLYPH` | label `markers-nudge-earlier` | P6 |
| 8 | nudge later | Quiet, `ROW_ACTION_NUDGE_LATER_GLYPH` | label `markers-nudge-later` | P6 |
| 9 | remove | Quiet (**not** Destructive), `ROW_ACTION_REMOVE_GLYPH` | label `markers-remove` | P6 |

- Cells 4–9 form a trailing right-aligned column that is never pushed out of view by a long name.
- Each action has a tooltip equal to its accessible name.
- The actions are always rendered, never hover-revealed (FR-008).
- The row container keeps its 006/016 `markers-row` node: `Role::ListItem`, label `marker-glyph { $role } { $name } { $time }`, plus the 016 hover and pressed fill.
- Tab stops per row are 1, 3, 6, 7, 8, 9 (six in all).

## P4. Palette popover (FR-013)

| Aspect | Value |
|---|---|
| Opener | the row's swatch (P3 #1) |
| Container | `egui::Popup` below the swatch, `CloseOnClickOutside`; AccessKit `Role::RadioGroup`, label `markers-palette` ("Marker colour") |
| Items | 8 swatch buttons, `PaletteIndex` 0..8 in palette order; `Role::RadioButton`, label `markers-color { $index }` (1-based), `toggled = true` on the current index |
| Current marker | `text_primary` outline + "✓" in `marker_mark_color`, a non-colour indicator (NFR-6.4) |
| Pick (click / Enter / Space) | `recolor_marker(id, index)`, popover closes, focus returns to the swatch |
| `Esc` / click outside | closes with no change; the `Esc` is consumed (it does not also run the key table's `ReturnFocus`) |
| Keyboard open | focus lands on the current swatch; ←/→/Tab move among swatches |
| `C` key (focused marker) | unchanged: cycles colour and does not open the popover |
| Reflection | the swatch and both lane glyphs show the new colour on the next rendered frame (repaint requested; research R9) |

## P5. Inline rename (FR-012)

| Trigger | Result |
|---|---|
| Single primary click on the name cell (or its placeholder) | `TextEdit` replaces the name cell and takes focus once. The draft is the current name (empty for an unnamed marker, never the placeholder text). |
| Enter / F2 while the name cell or a lane glyph has focus | same as click (006 key table) |
| Enter in the field | commit → `rename_marker(id, draft)` (006 trim/64-char/empty-Point rules) |
| Focus loss (click elsewhere, Tab) | commit, as Enter |
| `Esc` in the field | cancel with no change; checked before focus loss |
| Another row's action clicked while the rename is open | the rename commits first, then that action runs (intent pass order) |
| Reflection | lane glyph accessible name / hover tooltip and the row show the new name on the next rendered frame |

The field keeps `Claim::TextLike` and the `markers-rename` accessible label (006).

## P6. Row actions (FR-009–FR-011, FR-014)

| Action | Effect | Refusals / edge cases |
|---|---|---|
| jump | `seek_frames(marker.position)`; play/pause state is unchanged (006 FR-014) | no-op while this marker is being dragged; does not change the current region or arm state |
| nudge earlier / later | `nudge_marker(id, ∓1, 1)`: exactly 1× `nudge_step_ms` | clamps at 0 / `len_frames` (006 FR-019); an armed region is re-committed without resetting wraps (006) |
| remove | `delete_marker(id)` immediately, with no confirmation | a region boundary leaves the region incomplete and disarmed; the last boundary deletes the region (006 I9) |

**Focus after remove**: the next populated row in visual order (its swatch), else the previous one, else "New loop region" (Clarification 8).

## P7. Keyboard and focus

- **Tab order in the card**:
  1. "New loop region"
  2. per loop block: A row (6 stops), B row (6 stops), arm, repeat, crossfade
  3. point rows
  4. occupied cue rows (empty cue rows are skipped)
  5. footer controls
- **Focused marker**: any row control gaining focus sets `waveform.focused_marker` to that row's marker and selects it. The 006 §3 key table (←/→ and Shift variants via 007 `Scope::MarkerFocused`, Delete/Backspace, F2, C, Esc) then applies from any row control. Row controls register `marker_claims()` and the horizontal-arrow focus-lock filter, as lane glyphs do.
- **Enter gate**: the key table's `Enter` → rename alias fires only when a lane glyph or a row's name cell has egui focus. On the swatch or an action button, Enter/Space activate that control and nothing else.
- **Unchanged**: no global shortcut is added, removed or remapped (FR-022).

## P8. Empty cue-slot row (FR-015, FR-016)

| Aspect | Value |
|---|---|
| Text | `markers-cue-empty { $slot }`: "Cue { $slot } — empty · Shift+{ $slot } to set" |
| Style | `text_secondary`, same row height as a populated row |
| Interaction | none: no swatch, no name field, no actions, no hover fill, not a tab stop, clicks ignored |
| AccessKit | static text with the same string; no `ListItem` container |

## P9. Footer: Clear all markers (FR-018, FR-019)

| State | Rendered (right-aligned, below the Cues group) |
|---|---|
| idle | Destructive `markers-clear-all` |
| confirming | `markers-clear-confirm { $count }` label, Destructive `markers-clear-yes`, `destructive_gap`, `markers-clear-no`; `Esc` = No |
| after Yes | `clear_all_markers()` → the panel renders the Empty layout (P1) |

It is never rendered on the Empty layout. It is never on the same line as, or immediately next to, "New loop region": the Loop, Points and Cues groups always lie between them.

## P10. Strings (`locales/en-US/playback.ftl`, new section)

```ftl
## Markers panel — structure (023)

markers-group-loop = Loop region
markers-group-points = Points
markers-group-cues = Cues
markers-group-heading = { $label }, { $count }
markers-name-placeholder = Add name
markers-cue-empty = Cue { $slot } — empty · Shift+{ $slot } to set
markers-jump = Jump to marker
markers-nudge-earlier = Nudge earlier
markers-nudge-later = Nudge later
markers-remove = Remove marker
markers-palette = Marker colour
```

`markers-group-loop/points/cues` are called with `$count` (unused in en-US; reserved for plural-aware locales). Existing keys are reused unchanged: `markers-panel`, `markers-empty`, `markers-status` and the refusal keys, `markers-new-loop`, `markers-clear-*`, `marker-role-*`, `markers-rename`, `markers-color`, `marker-clamped-desc` and `loop-*`.

## P11. Unchanged surfaces (FR-022)

- Lane glyph shapes and overlay painting (006/022)
- The view-level shortcuts `I`/`O`/`L`/`M`/`1–8`/`Shift+1–8`
- Persistence and the 64-marker limit
- Crossfade and repeat ranges
- The panel's position and collapse flag (021)
- `handle_focused_marker_keys` actions (P7's Enter gate narrows only *where* the Enter alias applies)

## §9. Tests pinning this contract

In `crates/modplayer-ui/tests/markers.rs` unless noted.

| Test | Pins |
|---|---|
| `panel_model_partitions_by_kind` (unit, `markers.rs`) + `proptest panel_model_invariants` | P1, data-model P1–P4, FR-001–FR-006 |
| `panel_groups_in_order_with_counts` | P1, P2, US1 AS1/AS3/AS4, SC-002 |
| `loop_block_cells_follow_last_boundary` (incl. B-only region) | P1, FR-003, Clarification 12 |
| `points_group_omitted_when_empty` / `loop_group_always_when_populated` | FR-001, FR-004, US4 AS4 |
| `empty_panel_shows_message_and_new_loop_only` | P1 Empty, FR-020, SC-007 |
| `cues_group_shows_eight_slots_muted_empty_rows` | P8, FR-015/016, SC-008 |
| `empty_cue_rows_are_not_tab_stops` | P7, P8 |
| `row_actions_present_quiet_and_named` | P3, FR-008 |
| `row_tab_order_swatch_name_actions` | P3, P7, Clarification 6, SC-009 |
| `row_jump_seeks_preserving_play_state` (Playing and Paused, each kind) | P6, FR-009 |
| `row_jump_noop_while_dragging` | P6 |
| `row_nudge_matches_keyboard_step` (incl. clamp at 0) | P6, FR-010 |
| `row_remove_matches_keyboard_delete` (region incomplete, last boundary, armed disarm) | P6, FR-011 |
| `row_remove_moves_focus_next_prev_new_loop` | P6, Clarification 8 |
| `swatch_opens_palette_popover_and_picks` (incl. the 8 RadioButton labels are `markers-color` 1..8, the 1-based regression check) / `palette_esc_and_click_outside_no_change` | P4, FR-013 |
| `click_name_opens_rename_placeholder_when_unnamed` / `rename_commits_on_focus_loss_esc_cancels` | P5, FR-012 |
| `rename_reflected_in_lane_glyph_name_next_frame` | P5, SC-004 |
| `rename_commits_before_other_row_action` | P5, Edge Case |
| `enter_on_row_action_does_not_open_rename` | P7 |
| `clear_all_in_footer_destructive_not_adjacent` / `clear_all_two_step_unchanged` | P9, FR-018/019, SC-005/006 |
| `status_line_above_loop_group` | P1, FR-021 |
| `row_action_glyphs_covered_by_fonts` | research R6 |
| `fluent_keys.rs` (extended) | P10, FR-024 |
| `design_token_literals.rs` (baseline unchanged) | FR-023 |

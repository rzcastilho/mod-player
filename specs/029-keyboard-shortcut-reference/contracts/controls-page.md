# Contract: Settings › Controls page (C1–C16)

Scope: `modplayer-ui::settings::controls` and `modplayer-ui::shortcut_groups`. Covers FR-002 to FR-006, FR-013, SC-002 and SC-005. Capture, add, remove, reset and reset-all semantics from 007 and 011 are kept; only presentation changes.

## Layout

| # | Rule | Test (`tests/controls.rs` unless noted) |
|---|---|---|
| C1 | Order: heading, filter box, "Reset all to defaults", divider, then the groups. The filter box and Reset all span the full width, above the columns | `layout_order_filter_and_reset_all_above_columns` |
| C2 | Groups appear in `ActionCategory::ALL` order (Transport, Markers, Loop, Cues, Navigation, Effects), then plugin groups ("{plugin} actions") in `rows()` order. Panel toggles stay under Navigation | `groups_follow_catalog_then_plugin_order` |
| C3 | Content width ≥ 720 px (`TWO_COLUMN_MIN_WIDTH`) gives two columns; < 720 px gives one. Measured with `ui.available_width()` at the point where the groups start | `two_columns_at_or_above_720`, `one_column_below_720` (headless at 1280 and 700 px) |
| C4 | No group is split across columns. The break is `column_break(1 + rows)` | `shortcut_groups` unit tests + proptest `column_break_is_optimal_boundary` |
| C5 | Chips wrap inside their row. No chip text is elided or clipped | `many_bindings_wrap_without_elision` (an action with 5 bindings at 700 px) |

## Row anatomy at rest

| # | Rule | Test |
|---|---|---|
| C6 | A row at rest shows the action name, "(inactive)" (weak text) when disabled, one chip per binding, a `⚠` plus the conflict note when the binding conflicts, and "Not bound" (secondary text) when the action has no bindings | `row_at_rest_shows_name_chips_conflict_only`, `unbound_row_shows_not_bound` |
| C7 | The "trigger" kind label is not drawn. A kind label is drawn only for `ActionKind::Continuous` | `trigger_kind_label_removed` |
| C8 | Chip painted text = `KeyLabel::visible` only (no "Binding" prefix). Chip accessible name = `controls-binding-chip { $binding = spoken }` | `chip_text_is_key_label`, `accessibility.rs` sweep |
| C9 | At rest, `×`, "Add binding" and "Reset {action} to default" are **not present**: no AccessKit node and zero width | `controls_hidden_at_rest_take_zero_width` (row rect width equals its rest width; nodes absent) |

## Reveal

| # | Rule | Test |
|---|---|---|
| C10 | Pointer over the row's rect reveals `×` (per chip), Add binding and Reset in the next frame, and they are operable. A repaint is requested when the reveal state changes | `hover_reveals_controls` |
| C11 | Each row has one focusable row element in Tab order. Its accessible name is `controls-row { $action } { $bindings }` (spoken bindings, or "Not bound") | `row_element_is_focusable_and_named` |
| C12 | Focusing the row element reveals the controls. The next Tab moves to the first revealed control, and Tab order is: row element, chip `×`s in order, Add binding, Reset. When focus leaves the row, the controls hide | `keyboard_reveal_tab_walk` |
| C13 | While `capture == Some(row)` or a capture rejection is shown for the row, the controls stay visible whatever the pointer or focus does | `capture_keeps_controls_visible_when_pointer_leaves` |

## Filter

| # | Rule | Test |
|---|---|---|
| C14 | The query is trimmed and lower-cased. Substring match on the group heading or the action label works as before | existing filter tests (unchanged) |
| C15 | A row also matches when a binding's alias (canonical name, visible key label, or whole visible chord) equals the query. Key matching is exact: `e` does not match Enter or Escape. "L", "l", "space", "Space", "←" (mac), "left", "ctrl+shift+l" (other) each list the bound actions | `filter_by_key_alias_*` (one per alias kind), `filter_key_match_is_exact` |
| C16 | Chips matched by key are drawn with the accent focus-style outline (`theme` stroke width/role, not a fill) and stay visible in high contrast. No match shows `controls-no-match` | `key_match_chip_outlined`, `high_contrast.rs::key_match_outline_visible_in_hc` |

## Size (SC-005)

| # | Rule | Test |
|---|---|---|
| C17 | Headless 1280×800, full host catalog, default bindings, empty filter: content height ≤ 0.6 × `PRE_029_CONTROLS_HEIGHT` (baseline measured on pre-feature code, research R12) | `controls_height_at_most_60_percent_of_baseline` |

## Cross-cutting

- All colours come from `tokens::roles`, the accent and the focus stroke. `design_token_literals.rs` scans `key_label.rs`, `shortcut_groups.rs` and `shortcut_sheet.rs`.
- The `i18n::with_pseudo_expansion(40, …)` variant of C5 and C9 shows no overlap or truncation.

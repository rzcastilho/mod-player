# Research: Markers Panel Structure

**Feature**: 023-markers-panel-structure | **Date**: 2026-09-29 | **Spec**: [spec.md](./spec.md)

Phase 0 of `/speckit-plan`. The Technical Context in [plan.md](./plan.md) has no open `NEEDS CLARIFICATION`. The spec's Clarifications 1–15 already settle product behaviour. This file records the *implementation* decisions those clarifications leave open. Each decision was checked against the current code on this branch: `crates/modplayer-ui/src/markers.rs` (1 097 lines), `crates/modplayer-core/src/controller.rs`, `crates/modplayer-core/src/markers/model.rs`, `crates/modplayer-ui/src/widgets/controls.rs`, `crates/modplayer-ui/src/theme/**`, `crates/modplayer-ui/src/settings/category_row.rs` (the popup precedent) and `locales/en-US/playback.ftl`.

## Baseline observed in the code (what changes)

| Aspect | Today (`markers::panel`) | After this feature |
|---|---|---|
| Top line | `ui.horizontal`: "New loop region" (`ui.button`) **then** `destructive_gap` + "Clear all markers" (Destructive) | "New loop region" is the first line of the Loop Region group. "Clear all markers" sits in its own footer at the bottom of the card. |
| Status | `markers-status` label under the top line | Directly under the card header, above everything else (FR-021) |
| Empty | "New loop region" + "Clear all" + `markers-empty` | `markers-empty`, then "New loop region" only (FR-020) |
| Rows | One flat loop over `TrackMarkers::markers()` | Partitioned into Loop Region blocks, Points and 8 Cue slots |
| Loop cells | After the **A** row only; a B-only region shows **no** cells | After the block's last boundary row, so a B-only region shows its cells too (Clarification 12) |
| Swatch | `egui::Button` with fill; click → `cycle_marker_color` | Popover of 8 swatches; pick → `recolor_marker` (already exists in core) |
| Name | Label only; rename opens via F2/Enter from the key table | Name cell is a Quiet button (or "Add name" placeholder); one click opens the rename; focus loss commits |
| Row actions | none | jump / nudge earlier / nudge later / remove, Quiet icon buttons |
| Row focus | Row controls never set `waveform.focused_marker`. The key table works only from lane glyphs. | Any row control that gains focus sets `focused_marker` (R8) |

---

## R1 — Where the new structure lives: same module, split into private helpers

- **Decision**: All of the panel's structure stays in `crates/modplayer-ui/src/markers.rs`. The one `panel` body becomes a small pipeline:
  1. `PanelModel::snapshot(&TrackMarkers, sample_rate)` (pure, see data-model §1)
  2. `show_status` / `show_empty` / `show_loop_group` / `show_points_group` / `show_cues_group` / `show_footer` (render and collect `PanelIntent`s)
  3. `apply_intents` (the only place that mutates `controller`)
- **Rationale**: `markers.rs` already owns the panel, the lane glyphs and the key table, and they share private helpers (`role_label`, `row_accessible_name`, `MarkerRowData`). The row work that exists today (`show_marker_row`, `show_region_cells`, `clear_all_controls`) is refactored in place, not duplicated. A pure `PanelModel` makes the partition and counts (FR-001–FR-006, FR-016) unit-testable without an egui context.
- **Alternatives considered**: (a) A new `markers_panel.rs` module. Rejected: it would need to re-export or duplicate the private helpers the lane shares, for no boundary gain. The file grows by ~350 lines, which is within the size of `now_playing.rs`. (b) Reusing `rows.rs::list_row`. Rejected: that row grammar (thumbnail, title/subtitle, duration, "…") does not fit a swatch/role/name/position/actions row, and 016 already records that `markers.rs` keeps its own row rect (design note 7).

## R2 — Snapshot, then render, then apply (intent queue)

- **Decision**: Rendering never calls `&mut controller`. Every widget interaction pushes a `PanelIntent` (data-model §4) into a `Vec` local to the frame. After the last group has rendered, `apply_intents` runs them in two passes: first any `CommitRename`/`CancelRename`, then everything else in emission order.
- **Rationale**:
  - Removes today's borrow juggling (`MarkerRowData` exists only to avoid holding `&TrackMarkers` across `&mut controller`).
  - Satisfies the Edge Case "rename open, another row's action clicked → rename commits before the other action runs", deterministically, even if egui delivers the TextEdit's `lost_focus` and the other button's `clicked` in the same frame.
  - Keeps the rendered frame self-consistent. A mid-frame `delete_marker` today can make later rows render against a stale snapshot.
- **Alternatives considered**: Immediate mutation, as today, plus a "commit rename first" special case at the top of each action. Rejected: the ordering guarantee would be spread over five call sites, and the stale-snapshot hazard would remain.

## R3 — Grouping and ordering rules (pure)

- **Decision**:
  - **Loop Region**: one block per `LoopRegion` in `TrackMarkers::regions()`. Blocks are sorted by the position of the region's earliest *present* boundary (`a` if present, else `b`), tie-broken by `RegionId`. Inside a block the rows are A (if present), then B (if present), then the loop cells.
  - **Points**: every `MarkerKind::Point`, sorted by `(position, id)`.
  - **Cues**: exactly 8 slots, `CueSlot::new(1..=8)`, each `Occupied(row)` via `TrackMarkers::cue(slot)` or `Empty(slot)`.
  - **Counts**: `regions().len()`, `points.len()`, and the number of occupied slots.
  - **Visibility**: `is_empty` ⇔ `TrackMarkers::count() == 0` (or no `TrackMarkers`). When not empty, the Loop and Cues groups always render. The Points group renders only when `points.len() > 0` (FR-001, FR-004, FR-016).
- **Rationale**: This is a direct encoding of FR-001–FR-006 and FR-016. Sorting explicitly makes FR-005 hold even if `markers()` order ever changes. Membership comes only from `MarkerKind` (FR-006). Because 006's I3 swap keeps a region's A before its B, "A then B" is also position order.
- **Alternatives considered**: Ordering blocks by `RegionId` (creation order). Rejected: FR-003 says blocks are ordered by boundary position.

## R4 — Group header rendering and accessibility

- **Decision**:
  - **Visual**: the header is one `ui.horizontal` holding the group label (`theme::section_label(&tr_args("markers-group-…", count))`, the existing section style, uppercase at draw time) and the count (`theme::mono_text(count)` in `text_secondary`).
  - **Behaviour**: not interactive, not focusable.
  - **Accessibility**: an `accesskit_node_builder` node with `Role::Heading` and a label from the new key `markers-group-heading = { $label }, { $count }` ("Points, 2").
  - **Spacing**: `space::MD` above each header except the first, and `space::XS` between the header and its first line.
- **Rationale**: This follows Clarification 4. The `$count` argument on the three group keys is accepted even though en-US ignores it, so a later locale can inflect the label ("1 Cue" / "2 Cues") without a code change. The accessible name is externalized as a whole rather than concatenated in Rust (NFR-7.1).
- **Alternatives considered**: Hard-coding `format!("{label}, {count}")`. Rejected: that is an unexternalized punctuation pattern. `CollapsingHeader` per group. Rejected: Clarification 4 says headers are not collapsible.

## R5 — Row layout and truncation

- **Decision**: A populated row is `ui.horizontal` with a fixed reading order:
  1. **Swatch**: 16×16 `egui::Button` filled with `theme::marker_color`, as today.
  2. **Role label**: `role_label(kind)`.
  3. **Name cell**: a Quiet `widgets::controls::button` showing the name, or the placeholder `markers-name-placeholder` in `text_secondary` when the name is empty. Inside `ui.add_sized` it takes the remaining width minus the trailing measure, and its text is truncated (`egui::Label::truncate` semantics via `RichText` in a width-bounded layout).
  4. **Trailing column**: a `right_to_left` sub-layout holding, right to left, remove, nudge later, nudge earlier, jump, the clamped ⚠ (as today), and the position (`theme::mono_text(format_mmss_millis)`, the live drag position while dragging).

  The row keeps its existing `markers-row` `Role::ListItem` overlay node and hover fill (016 FR-009).
- **Rationale**:
  - FR-007 and FR-008 fix the order.
  - The right-to-left sub-layout keeps the position and actions in view whatever the name length (Edge Case "long name").
  - `mono_text` is 014's single tabular-figure path. Today the position uses a plain `ui.label`, which `type_roles::numeric_fields_use_the_mono_role` would flag once it is covered.
  - Visual order equals tab order (Clarification 6) because egui assigns focus order by widget creation order. Widgets are created in the order swatch → name → jump → nudge− → nudge+ → remove, and are then *placed* by the right-to-left layout. To make that hold, the trailing column is built with a pre-allocated rect and a left-to-right child UI (`ui.allocate_ui_with_layout(trailing_size, Layout::left_to_right(..))`), so creation order and visual order are both left to right.
- **Alternatives considered**: Using `right_to_left` directly. Rejected: widgets would be created remove-first, which reverses the Tab order and breaks Clarification 6.

## R6 — Row-action controls: Quiet variant, glyph constants, and tooltips

- **Decision**:
  - **Control**: each action is `widgets::controls::button(ui, Variant::Quiet, glyph)` (015's lowest emphasis) with `.on_hover_text(tr(key))`, and an AccessKit label set to the same `tr(key)`, so the accessible name is the words, not the glyph.
  - **Glyphs**: new constants in `crates/modplayer-ui/src/theme/markers.rs`: `ROW_ACTION_JUMP_GLYPH = "⌖"`, `ROW_ACTION_NUDGE_EARLIER_GLYPH = "⏴"`, `ROW_ACTION_NUDGE_LATER_GLYPH = "⏵"`, `ROW_ACTION_REMOVE_GLYPH = "×"`.
  - **Glyph test**: a new test asserts that each glyph is present in the app's configured fonts (`ctx.fonts(|f| f.has_glyphs(..))` after `theme::apply`). If any fails, the implementer substitutes a covered glyph from the same family and records the change here.
  - **Substitution recorded (T006/T004, `row_action_glyphs_covered_by_fonts`)**: none of the four originally-proposed glyphs are covered by the app's proportional font (each drew as tofu) — the app bundles a narrow symbol set (media-transport glyphs, a handful of geometric/dingbat shapes, no arrow block). Replaced, one family member for another, with glyphs the font does cover:
    - jump `↦` (RIGHTWARDS ARROW FROM BAR) → `⌖` (U+2316 POSITION INDICATOR) — "location/target" family.
    - nudge earlier `◂` → `⏴` (U+23F4 LEFT-POINTING SMALL TRIANGLE) — media-transport family.
    - nudge later `▸` → `⏵` (U+23F5 BLACK RIGHT-POINTING SMALL TRIANGLE) — the same glyph `widgets::controls::DISCLOSURE_CLOSED_GLYPH` already proves covered; a different control, reused only for its shape.
    - remove `✕` → `×` (U+00D7 MULTIPLICATION SIGN) — "close/remove" family.
  - **Sizing**: the interact height is kept at the row's height, as `settings/category_row.rs` does for its Quiet "…" opener.
  - **Remove**: Quiet, not Destructive (Clarification 5).
- **Rationale**:
  - Glyph constants live in `theme/**`, the same home as `DISCLOSURE_*_GLYPH`, so every icon choice has one place to change.
  - The font-coverage test prevents a tofu box from shipping. That risk is real: egui's bundled fonts cover arrows and geometric shapes, but not every symbol.
- **Alternatives considered**:
  - Text labels ("Jump", "−", "+", "Remove"). Rejected: the spec asks for quiet icon actions, and four words per row would compete with the name (US2 AS6).
  - Painting vector icons. Rejected: more code and a new painting path for no user-visible gain over font glyphs.

## R7 — Swatch popover (recolour)

- **Decision**:
  - **Opening**: the swatch button is the opener of an `egui::Popup::new(palette_popup_id(marker), ctx, &swatch, swatch.layer_id)`, with `PopupKind::Popup`, `.close_behavior(PopupCloseBehavior::CloseOnClickOutside)`, and `.open_memory(swatch.clicked().then_some(SetOpenCommand::Toggle))`. This is the same memory-driven pattern `settings/category_row.rs` uses.
  - **Contents**: a horizontal run of 8 swatch buttons for `PaletteIndex::new(0..8)`. The current index is marked by a `text_primary` outline stroke plus a "✓" glyph painted in `marker_mark_color` (non-colour indicator, NFR-6.4). **Substituted (T043 follow-up):** `✓` (U+2713) is absent from the monospace font it is painted in and drew as tofu in the manual walk; it is now `theme::markers::PALETTE_CURRENT_GLYPH = "✔"` (U+2714 HEAVY CHECK MARK, same family), pinned by `palette_current_glyph_covered_by_mono_font`.
  - **Choosing**: selecting a swatch pushes `PanelIntent::Recolor(id, index)` and calls `Popup::close_id`.
  - **Keyboard**: focus moves to the current swatch when the popover opens by keyboard (the `opened_by_keyboard` pattern from category_row). ←/→ and Tab move between swatches. Enter/Space pick. `Esc` closes via egui's built-in popup Esc handling; the panel also consumes the `Esc` so the focused-marker key table does not also treat it as `ReturnFocus`. On close, focus returns to the swatch.
  - **Accessibility**: the popover container is `Role::RadioGroup` labelled `markers-palette` ("Marker colour"). Each swatch is `Role::RadioButton` with `toggled` = current, labelled with the existing `markers-color { $index }` key using the 1-based index (the key already exists; today it is fed the raw 0-based index, which is fixed here).
- **Rationale**:
  - Clarification 2 fixes the behaviour, and 006 contracts/ui-markers.md §4 already specified a popup.
  - `PlaybackController::recolor_marker(id, PaletteIndex)` already exists ("The colour-swatch popup's direct pick"), so no core change is needed.
  - The category_row precedent already handles keyboard-open focus, Esc and click-outside in this codebase on egui 0.36.2.
- **Alternatives considered**: (a) An inline expanding row of 8 swatches. Rejected: it shifts every row below and is not the contract's "popup". (b) Tracking open state in `WaveformState`. Rejected: egui's popup memory already guarantees that at most one popup is open, and it closes on click-outside for free. The test harness asserts on the AccessKit tree, not on state fields.

## R8 — Row focus drives the focused-marker key table (Clarification 6)

- **Decision**:
  - **Focus tracking**: when any focusable row control (swatch, name cell, jump, nudge−, nudge+, remove) *gains* focus, the row sets `waveform.focused_marker = Some(id)` and calls `select_marker(id)` (as an intent). This mirrors the lane glyph's `has_focus` branch.
  - **Claims and arrows**: each of these controls registers `actions::register_claim(.., Claim::Keys(actions::marker_claims()))` and the horizontal-arrow `EventFilter`, as the glyph does. Arrows then reach `HostAction::NudgeEarlier/Later` under `Scope::MarkerFocused` instead of egui's focus traversal.
  - **Enter guard**: `handle_focused_marker_keys` honours its `Enter` → `OpenRename` alias only when egui's focused widget is a lane glyph or the row's name cell. On the swatch and the four action buttons, Enter/Space activate that control (egui default) and do **not** also open a rename. F2, Delete/Backspace, C and Esc keep working from any row control.
  - **Implementation of the guard**: a per-frame `RenameOnEnter` id set, stored in egui temp memory and written by the glyphs and name cells, is read by `handle_focused_marker_keys`.
- **Rationale**:
  - The spec keeps the key table working from rows. Today it silently works only from glyphs, because rows never set `focused_marker`.
  - Without the Enter guard, pressing Enter on "remove" would both remove the marker and open a rename on a marker that no longer exists. On the swatch it would open the popover and a rename at once.
- **Alternatives considered**: Making the whole row one focus stop with an inner roving tabindex. Rejected: Clarification 6 names six tab stops per row, and egui has no roving-tabindex primitive.

## R9 — In-place rename by click, focus-loss commit

- **Decision**:
  - **Opening**: clicking the name cell (or activating it with Enter/F2, R8) pushes `OpenRename(id)`, which sets `waveform.rename = Some((id, name))` and `waveform.rename_focus_pending = true` (new field). `name` is the current name; the placeholder text never becomes the draft.
  - **Focus**: the `TextEdit` calls `request_focus()` only while `rename_focus_pending`, then clears it. Today's code calls it every frame, which makes click-away impossible.
  - **Esc**: `consume_key(Esc)` → `CancelRename`.
  - **Commit**: Enter, or `response.lost_focus()` without Esc → `CommitRename(id, draft)`. The Esc check comes first, because egui's TextEdit also surrenders focus on Esc.
  - **Name rules**: unchanged; `rename_marker` trims, truncates to 64 and keeps an empty Point name.
  - **Same-frame reflection**: `apply_intents` mutates `TrackMarkers` inside the `panel` call. `now_playing` draws the lanes *before* the panel, so the lanes read the new name or colour on the first frame after the commit. `apply_intents` calls `ctx.request_repaint()` whenever it applied any intent, so that frame follows immediately and not on the next input event. This is the frame-level guarantee every existing 006 panel edit already has, and it is what FR-012/FR-013 "on the same frame the commit lands" and SC-004 "within the same interaction" are tested against. The test commits, settles one frame, then reads the glyph's accessible name. Reordering the lanes after the panel was rejected: 021 fixes the layout order.
- **Rationale**: This implements Clarification 3 and FR-012 exactly and fixes the always-refocus bug the click-away rule would otherwise expose.
- **Alternatives considered**: Double-click to rename. Rejected: Clarification 3 chose a single click.

## R10 — Jump, nudge and remove semantics (no core changes)

- **Decision**:
  - **Jump**: `controller.seek_frames(position)`, the same path `jump_to_cue` uses, so play/pause state is preserved (006 FR-014). The panel skips it when `waveform.marker_drag.is_some_and(|d| d.marker == id)`. It does not touch `focused_marker`, the current region or arm state; the button taking focus does set `focused_marker` per R8, which is focus, not a state change of the marker model.
  - **Nudge**: `controller.nudge_marker(id, ∓1, 1)`. Clamping, armed-region re-commit and I3 swap are all inherited.
  - **Remove**: `controller.delete_marker(id)`. The disarm-if-armed-endpoint rule and "last boundary deletes the region" (`TrackMarkers::delete`) are inherited. Then `waveform.focused_marker` is cleared if it was `id`, and a focus target is computed (R11).
- **Rationale**: Every effect FR-009 to FR-011 names already exists behind the keyboard path. Reusing the same controller methods is what makes "same result as keyboard" (SC-009) true by construction. No new `PlaybackController` method means no engine or core surface change (Constitution I and III).
- **Alternatives considered**: A new `PlaybackController::jump_to_marker(id)`. Rejected: it would be a one-line wrapper over `seek_frames`, and the drag gate is UI state that core cannot see. YAGNI (Principle X).

## R11 — Focus after remove (Clarification 8)

- **Decision**:
  - **Visual order**: `PanelModel::visual_order() -> Vec<MarkerId>` lists populated rows in render order: blocks' A/B, then points, then occupied cues.
  - **Target**: on `Remove(id)`, the target is the next id after `id` in that order, else the previous one, else `PanelFocus::NewLoopRegion`. It is stored in `waveform.panel_focus` (new field) and applied on the next frame. The target row's swatch (its first tab stop) calls `request_focus()`, which through R8 also sets `focused_marker`; or the "New loop region" button requests focus. The field is then cleared.
  - **Stale target**: if the target id no longer exists next frame (for example it was the same region's other boundary, deleted with it), fall through to the first populated row, else to "New loop region".
- **Rationale**: Focus needs the next frame's widget ids. A one-shot state field is the pattern `category_row.rs` uses (`focus_after_close`).
- **Alternatives considered**: Leaving focus on nothing, which is today's keyboard-delete behaviour. Rejected: Clarification 8. Keyboard `Delete` keeps its 006 behaviour (focus returns to none) because FR-022 forbids shortcut changes. A later feature may align the two.

## R12 — Empty cue-slot rows

- **Decision**: An empty cue-slot row is a plain `ui.label(RichText::new(tr_args("markers-cue-empty", slot)).color(roles.text_secondary))` inside the same row height. It has no `interact`, no hover fill, no focusable widget and no ListItem overlay. Its AccessKit node is the label's own static-text node, which carries the same string.
- **Rationale**: FR-015 and Clarification 10. `text_secondary` is 014's muted role and passes 4.5:1 in all four appearances (014/017 contrast suites), so muted does not mean illegible.
- **Alternatives considered**: `text_disabled`. Rejected: disabled text is not held to 4.5:1 and would imply the row is broken rather than free.

## R13 — "New loop region" and "Clear all markers" placement

- **Decision**:
  - **New loop region**: `widgets::controls::button(ui, Variant::Default, tr("markers-new-loop"))`. Today it is a bare `ui.button`, which already resolves to Default paint through 015's style; the explicit variant pins it. It is the first line under the Loop Region header, or directly under `markers-empty` on an empty panel.
  - **Clear all markers**: `show_footer` renders a `space::MD` gap, then a `Layout::right_to_left` row holding either the Destructive "Clear all markers" or the unchanged confirm cluster (`markers-clear-confirm` label, Destructive "Yes", `destructive_gap`, "No", and Esc cancels). The footer is not rendered on an empty panel. The existing `clear_all_controls` body moves as-is.
- **Rationale**: This follows FR-017–FR-019 and Clarification 11. The Loop group, Points group and eight Cues rows always lie between the two buttons, so "not adjacent" (SC-005) holds by construction and is testable by comparing AccessKit bounds.
- **Alternatives considered**: Putting Clear all in the card header's trailing slot. Rejected: `collapsible_panel_card` owns the header (021), and the header would sit visually adjacent to the Loop group's first line.

## R14 — Strings

- **Decision**: The new keys are added to `locales/en-US/playback.ftl` under a new `## Markers panel — structure (023)` section:
  - `markers-group-loop`, `markers-group-points`, `markers-group-cues` (each takes `$count`)
  - `markers-group-heading` (`$label`, `$count`)
  - `markers-name-placeholder`
  - `markers-cue-empty` (`$slot`)
  - `markers-jump`, `markers-nudge-earlier`, `markers-nudge-later`, `markers-remove`
  - `markers-palette`

  `crates/modplayer-ui/tests/fluent_keys.rs` is extended because it asserts that `playback.ftl` defines no key the test does not exercise.
- **Rationale**: FR-024 and Clarification 13. Two keys beyond FR-024's minimum (`markers-group-heading`, `markers-palette`) exist so that the accessible names are externalized, not built from Rust literals.
- **Alternatives considered**: None; pt-BR is out of scope (Clarification 13).

## R15 — Test strategy (FR-025)

- **Decision**:
  - **Pure tests**: `PanelModel` unit tests in `markers.rs` `#[cfg(test)]`. There is also a `proptest` over random marker sets (any mix of regions, including incomplete ones, points and cues ≤ 64) asserting three things: the partition is exact, the counts equal the kinds, and `visual_order` is a permutation of the populated rows. The constitution requires proptest for marker arithmetic.
  - **Harness tests**: integration tests extend `crates/modplayer-ui/tests/markers.rs` with the existing `active_controller`/`settle_frame`/`press_key`/`find_node_bounds`/`rendered_texts` helpers. They assert by AccessKit role and name:
    - group headings and counts
    - row action nodes and Tab order (Tab sequence of focused node labels)
    - jump preserving `Playing`/`Paused`
    - nudge equal to `nudge_marker(.., 1)`
    - remove equal to keyboard Delete, including the region-incomplete rule
    - the popover RadioGroup with a toggled current swatch
    - click-to-rename, the placeholder, and commit on click-away
    - Esc cancel
    - post-remove focus
    - Clear-all footer below the Cues group, not adjacent to "New loop region", unchanged two-step confirm
    - the empty state (exactly `markers-empty` and "New loop region"; no headings, cue rows or Clear all)
    - 8 cue rows with muted empty rows that are not tab stops
  - **Scans and suites**: `design_token_literals.rs` must stay at its current baseline (no new colour, font or spacing literal outside `theme/**`), and `fluent_keys.rs` must pass.
- **Rationale**: Every FR-025 clause maps to at least one named test ([contracts/ui-markers-panel.md](./contracts/ui-markers-panel.md) §9). Tests are written first, per Principle VIII.
- **Alternatives considered**: Screenshot or golden-image tests. Rejected: the repo has no image-diff infrastructure, and the AccessKit tree already exposes every asserted property.

# Contract: The List Row (`rows::list_row`)

**Feature**: 016 | Covers FR-001–FR-012, FR-024, FR-028–FR-033

The UI contract for the one widget every catalog list renders through.
Each rule names the test that pins it. `L*` = layout/format, `S*` =
selection, `A*` = accessibility.

---

## Layout and format

**L1 — Three columns, fixed trailing width.** `list_row` lays out
artwork (`ARTWORK_SIZE`), a middle text column, then a trailing region of
exactly `theme::duration_measure(ctx) + ACTIONS_RESERVED_WIDTH`. The
middle column's width is
`(available - ACTIONS_RESERVED_WIDTH - duration_measure(ctx)).max(0.0)`.
*Test*: `rows.rs` unit — the computed text width for a given available
width matches the formula, for both `ROW_HEIGHT` and `WIDE_ROW_HEIGHT`
kinds. (FR-001)

**L2 — The trailing width is identical across every row kind.** The
subtraction above is applied for `Track`, `Album`, `Artist` **and**
`Playlist`; only `Track` draws a figure into it. *Test*: `rows.rs` unit —
the text width for all four kinds at one available width is equal.
(FR-003)

**L3 — The column width is a named derivation, never a literal.**
`duration_measure(ctx) == DURATION_FIGURES * glyph_width(mono_font_id(),
'0')`, `DURATION_FIGURES == 7.0`, and the `mono` family's `:` shares the
digit advance. *Test*: `theme/tokens.rs` unit, under
`egui::__run_test_ctx`; extends the existing
`mono_digits_are_tabular`. (FR-001, FR-024)

**L4 — `format_duration` rolls over at exactly 60 minutes.**
`3_599_000 → "59:59"`, `3_600_000 → "1:00:00"`, `3_855_000 → "1:04:15"`,
`0 → "0:00"`. No output for any `u32` below ten hours exceeds
`DURATION_FIGURES` characters. *Test*: `rows.rs` unit, table-driven —
plus, because Constitution VII's doc example only runs when rustdoc can
see the item, `format_duration` is made `pub` and carries that table as a
doc example executed by `cargo test -p modplayer-ui --doc`. (FR-033,
Constitution VII)

**L5 — The duration uses the `mono` role and lives in the trailing
column, not the detail line.** A Track row's secondary line contains its
artists and album only — no `" — m:ss"` suffix. The duration label is
built through `theme::mono_text`. *Test*: `rows.rs` unit on the
detail-string builder + a `tests/rows.rs` accesskit sweep asserting no
node's label ends with the formatted duration on the secondary line.
(FR-002)

**L6 — Titles truncate, never wrap.** Every text run still goes through
`rows::line`'s `Label::new(..).truncate()`. *Test*: `tests/rows.rs` — a
title longer than any plausible column renders one line whose height is
`ROW_HEIGHT`/`WIDE_ROW_HEIGHT`, unchanged from today. (FR-004)

**L7 — The full-row hover fill survives the narrower text column.** The
fill still covers the whole `rect`, and the "…" button's rect is inside
it. *Test*: `tests/rows.rs` — the button's rect is contained by the row's
rect. (FR-005)

---

## Selection

**S1 — A primary single click reports `RowEvent::Select` and nothing
else.** No `Action`, no `Open`, no controller call. *Test*:
`tests/rows.rs` — a synthetic primary click yields exactly
`Some(RowEvent::Select)`. (FR-006)

**S2 — Selection is exclusive per view.** Two `select` calls on one
`RowSelection` leave exactly one selected row, including when the two
rows are in different lists of the same view. *Test*: `rows.rs` unit on
`RowSelection`; `tests/search_view.rs` for the four-group case.
(FR-007, Clarification 2)

**S3 — Selection survives scrolling out of and back into the rendered
range.** `RowSelection` is owned by the view, not by the widget's
per-frame call, and `reconcile` is keyed on the stored index. *Test*:
`rows.rs` unit — `reconcile` at an index outside any rendered range keeps
the selection when the key still matches. (FR-012)

**S4 — Selection clears when the pair no longer holds.** `reconcile`
clears on a different key at the stored index (reorder), and on `None`
(removal or a shrunk list). *Test*: `rows.rs` unit, three cases.
(FR-012)

**S5 — `reconcile` is O(1).** `key_at` is `FnOnce`, invoked at most once
per call, and only when the salt matches. *Test*: `rows.rs` unit — a
counting closure records exactly one invocation for a matching salt and
zero for a non-matching one. (research R2; guards the virtualization
property of `rows.rs:597-607`)

**S6 — The same entity twice in one list selects only the clicked row.**
Two rows sharing a key but differing in index are distinguishable.
*Test*: `rows.rs` unit — `is_selected(L, k, 3)` is true while
`is_selected(L, k, 7)` is false. (FR-007, Edge Case)

**S7 — The actions menu never changes the selection.** Clicking "…",
secondary-clicking, and `Shift+F10` each leave the view's selection
exactly as it was. *Test*: `tests/rows.rs` — a synthetic click at the
"…" button's centre yields no `RowEvent::Select`; pins egui's
"in tie, pick last = topmost" hit-test rule this depends on
(`hit_test.rs:76-80`). (FR-029, research R3)

**S8 — A selection is cleared on list replacement.** Library tab change,
detail target change, search query change. *Test*: `tests/library_view.rs`,
`tests/search_view.rs` — drive the change, assert `RowSelection::default()`.
(FR-028)

**S9 — Selection is never persisted.** No `settings.toml` field, no
`ui.memory` write, no controller call. *Test*: the settings round-trip
test's field inventory (contract P4) contains no selection field.
(FR-028, Assumptions)

**S10 — A skeleton row is not selectable and carries no tooltip.**
`skeleton_row` is a different function with no `RowEntity`. *Test*:
`tests/library_view.rs` — an un-hydrated id's frame produces no
`Role::ListItem` node and no `RowEvent`. (FR-032, research R14)

**S11 — Double-click/Enter behaviour is byte-for-byte today's.**
`RowEvent::Open` for Album/Artist/Playlist; `RowEvent::Action(PlayNow)`
for Track. *Test*: the existing `tests/rows.rs` activation assertions
must pass **unmodified**. (FR-009)

**S12 — A primary click also takes keyboard focus.** Enter immediately
after a click activates that row; after focus moves away, Enter follows
focus, not selection. `rows.rs:585-586`'s
`has_focus() && Enter` branch is unchanged. *Test*: `tests/rows.rs` —
click then Enter activates; `request_focus` elsewhere then Enter does
not. (FR-030, Clarification 5)

---

## Paint and accessibility

**A1 — A selected row fills `accent`.** *Test*: `tests/rows.rs` value
test on the fill selector (data-model §7), both themes. (FR-008)

**A2 — Hover and pressed blend over the selection fill.** For a selected
row, `hovered` ⇒ `accent.blend(hover_fill)`, `pressed` ⇒
`accent.blend(pressed_fill)`, and both differ from plain `accent`.
*Test*: `tests/rows.rs` value test, both themes. (FR-008, Edge Case)

**A3 — Every text run on a selected row is `text_on_accent` at full
strength.** Title, secondary line, duration, "E" badge, availability
reason — none is `.weak()`. *Test*: `tests/rows.rs` value test over the
run table (data-model §7). (FR-031)

**A4 — Every selected-row text run clears 4.5:1 against the fill in both
themes.** Sampled on the widest case: a Track row with an availability
reason and an explicit badge. *Test*:
`tests/design_token_contrast.rs`, reusing 014's `contrast` module.
(FR-031, NFR-6.5, SC-011)

**A5 — A selected row exposes an AccessKit selected state.**
`set_selected(true)` on the row node; `Role::ListItem` and
`rows::accessible_name` are **unchanged**. *Test*: `tests/rows.rs`
accesskit assertion + the existing name assertions passing unmodified.
(FR-011, FR-025)

**A6 — Selection and focus remain separately identifiable.** 015's focus
ring is painted by the app-level pass into a foreground layer, outside
the widget rect; nothing here touches it. *Test*:
`tests/interaction_states.rs` passes unmodified. (US2 Scenario 6,
015 FR-010)

**A7 — Every row carries a kind-correct tooltip.** Track rows get
`row-open-hint-track`; Album/Artist/Playlist rows get
`row-open-hint-entity`. *Test*: `rows.rs` unit on the key selector +
`tests/fluent_keys.rs` for resolution. The **rendered** tooltip is
manual scenario M4 — a tooltip cannot be driven headlessly (research
R13). (FR-010, SC-004)

**A8 — No new colour, alpha, spacing or radius literal.** Every value
above resolves to an existing role or a named constant in `theme/`.
*Test*: `tests/design_token_literals.rs` still reports **0**.
(FR-024, SC-008)

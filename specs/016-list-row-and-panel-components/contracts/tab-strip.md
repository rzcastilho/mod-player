# Contract: The Tab Strip (`widgets::controls::tab`, Library)

**Feature**: 016 | Covers FR-013–FR-016, FR-024, FR-025, FR-034

The Library tab row becomes an underlined navigation row with per-tab
counts. This contract is unusually defensive about *what must not change*,
because FR-016 and FR-025 name two existing assertions that must keep
passing verbatim.

---

## Appearance

**T1 — The active tab is marked by an underline, not a fill.** No
`selectable_label`-style filled/accent background remains on any Library
tab. *Test*: `tests/library_view.rs` value test — the tab widget's paint
decision for `selected == true` is a stroke, and no `rect_filled` in the
`accent` role. (FR-013)

**T2 — The underline's thickness is a named constant.**
`theme::controls::TAB_UNDERLINE_WIDTH`, defined beside
`FOCUS_RING_WIDTH` (`theme/controls.rs:103`), never a literal at the call
site. *Test*: `theme/controls.rs` unit asserting the constant, plus
`tests/design_token_literals.rs` still **0**. (FR-034, FR-024)

**T3 — The underline's colour is the `accent` role.** `tab_underline`
returns a `Stroke` built from `roles.accent` — no new colour, no eleventh
role. *Test*: `theme/controls.rs` unit, both themes. (FR-024)

---

## Counts

**T4 — A count is shown for every tab once loading finishes.**
`library_status().loading == false` ⇒ all five tabs show a count.
*Test*: `tests/library_view.rs` — five count nodes present. (FR-014)

**T5 — No count while loading.** `loading == true` ⇒ zero count nodes.
*Test*: `tests/library_view.rs`. (FR-014)

**T6 — `0` is shown, not hidden.** An empty but loaded list shows `0`. A
hidden count means "not yet known"; it never means "none". *Test*:
`tests/library_view.rs` with an empty set. (FR-014, Edge Case)

**T7 — The count is the tab's own in-memory list length.** Exactly
`saved_tracks().len()` / `saved_albums().len()` /
`followed_artists().len()` / `playlists().len()` /
`recently_played().len()` — never a server-reported total. It updates in
place as a background `refreshing` sync grows the set. *Test*:
`tests/library_view.rs` — assert against the accessor, then merge a page
and assert the new value in the next frame. (FR-014, Clarification 11)

**T8 — Counts render in the `mono` role.** Through
`theme::mono_text`. *Test*: `tests/library_view.rs` /
`tests/design_token_roles.rs`. (FR-015)

---

## Accessibility — the binding constraints

**T9 — Each `Role::Tab` node's accessible name is *exactly*
`tr("library-tab-…")`.** No count, no separator, no suffix of any kind.
*Test*: **`tests/accessibility.rs:900`'s
`find_one(&nodes, Role::Tab, &tr(key))` and
`tests/library_view.rs:380`'s `labels_in_tree_order(&update, Role::Tab)`
must pass verbatim, unmodified.** This is the binding case FR-025 names.
(FR-016, FR-025, SC-009)

**T10 — Exactly five `Role::Tab` nodes, in the fixed
`LibraryTab::ORDER`.** The count is a **sibling** label node and must not
carry `Role::Tab`. *Test*: the same two assertions — a count node typed
as a Tab would change the label list's length and fail both. (FR-016,
Clarification 10)

**T11 — The active tab exposes an AccessKit selected/toggled state.**
Both `set_selected(true)` and `set_toggled(Toggled::True)`; inactive tabs
report the `false` forms. This *preserves* what `selectable_label` gave
for free via `response.rs:976` — a hand-rolled widget reports nothing
unless it says so (research R6). *Test*: `tests/accessibility.rs`
addition — the one accessibility change FR-025 permits here. (FR-034,
NFR-6.4)

**T12 — Click-to-switch, empty-state copy, and every other Library
behaviour are unchanged.** *Test*: every existing `tests/library_view.rs`
assertion other than the new ones passes unmodified. (FR-016, FR-026)

---

## Scope

**T13 — The nav rail and the Settings category list are untouched.**
`shell.rs` and `settings/mod.rs` keep their `selectable_label`s and their
app-wide `Visuals::selection.bg_fill`. A tab widget rather than a style
change is precisely what makes this true (research R6). *Test*: a
source-level assertion that neither file is modified, plus
`tests/accessibility.rs`'s nav/settings assertions passing unmodified.
(Scope boundary, Assumptions, 015 design note 10)

# Feature Specification: List Row, Tab Strip, and Panel Card Components

**Feature Branch**: `feature/016-list-row-and-panel-components`

**Created**: 2026-09-23

**Status**: Clarified (2026-09-23) — no open [NEEDS CLARIFICATION] markers, no escalations outstanding; ready for `/speckit-plan`

**Input**: User description: "Turn the three repeating building blocks of the app — the list row, the tab strip and the panel — into real components, so every list in ModPlayer becomes scannable at a glance. A list row is a fixed three-column grid: artwork or an initials placeholder on the left at a consistent size, a title above a secondary line in the middle, and a right-aligned column holding the item's duration in tabular figures and the overflow menu. Today all of that is concatenated into one wrapped sentence — 'Justin Timberlake — TROLLS (Original Motion Picture Soundtrack) — 3:57' — with the duration buried mid-line and the menu button stranded hundreds of pixels from the title it belongs to. Titles truncate with an ellipsis rather than wrapping, the whole row highlights on hover, and the row exposes how to open it: a single click selects, a double click or Enter opens, and the row's tooltip says so, because today a single click on a playlist is silently discarded and nothing in the interface reveals that a double click is required. A tab strip becomes an underlined tab row that reads as navigation rather than a row of buttons, and shows a count beside each tab when the count is known. A panel becomes a card on the raised surface with a small uppercase header, generous padding and a rounded corner, collapsible with its open or closed state remembered per panel — replacing the identical hairline rules that currently separate every block in the now-playing view. Acceptance: when a library list is displayed, every duration is right-aligned in the same column and every title truncates at the same edge. When the pointer moves over a row, the row highlights and its overflow control is reachable without leaving the row. When a playlist row is clicked once, it becomes selected; when it is clicked twice or Enter is pressed, the playlist opens. When a panel is collapsed and the app is restarted, the panel is still collapsed."

**Source**: [ModPlayer-UI-UX-Review.md § 3.3 Library](../autonomous/ModPlayer-UI-UX-Review.md#33-library) (`UX-13`, `UX-14`, `UX-15`, `UX-16`), [§ 3.5 Now Playing](../autonomous/ModPlayer-UI-UX-Review.md#35-now-playing) (`UX-21`, `UX-22`), [§ 4.5 Feedback and affordance](../autonomous/ModPlayer-UI-UX-Review.md#45-feedback-and-affordance), [§ 5.4 Component rules](../autonomous/ModPlayer-UI-UX-Review.md#54-component-rules); [ModPlayer-Software-Specification.md](../autonomous/ModPlayer-Software-Specification.md) NFR-6.1 (keyboard operable), NFR-6.2 (accessible names/roles/states), NFR-6.4 (colour never the sole carrier of meaning), NFR-6.5 (contrast minimums in both themes). **Origin**: [specs/autonomous/breakdown/008-design-foundation/003-list-row-and-panel-components.md](../autonomous/breakdown/008-design-foundation/003-list-row-and-panel-components.md). **Prerequisites**: builds on the token roles and type scale shipped by [014-design-tokens-and-type-scale](../014-design-tokens-and-type-scale/spec.md) and the button/toggle/hover/focus/pressed grammar shipped by [015-control-variants](../015-control-variants/spec.md) (`crates/modplayer-ui/src/theme/`, `crates/modplayer-ui/src/widgets/controls.rs`) — both already applied host-wide. The row component this feature reshapes (`crates/modplayer-ui/src/rows.rs`) is already consumed by the Library, Search, and Detail views; a future Now Playing workbench feature and a future Search/Library polish feature adopt it further without needing to change it again.

**Scope boundary**: Delivers the components and adopts them in the existing lists, tab strips and panels; it does not change which lists exist, how search groups are arranged (011-browse-and-manage-polish/002), or how the now-playing view is ordered (010-now-playing-workbench/001). The app's nav rail (`shell.rs`) and the Settings category list (`settings/mod.rs`) are one-of-N *selection* controls, not the tab strip this feature defines (§ 5.4's "Tabs vs. chips" names only the Library tab row and Settings categories as the two conversions; Settings categories belong to a separate shell/navigation feature per the source review's own roadmap) — this feature touches neither.

## Clarifications

### Session 2026-09-23 (clarify review)

Every decision below is encoded in the numbered requirement named at the end
of its entry; nothing here lives only in this section. Sources: `C` =
constitution, `R` = `ModPlayer-UI-UX-Review.md`, `P` = the feature's
breakdown prompt, `X` = existing code/tests in this repository, `D` =
assumed default (no source settles it; the conventional, constitution-
consistent choice, recorded here so `plan`/`tasks` do not re-litigate it).

1. **"Single click selects" vs. the audit table's "single click opens"** —
   `R` § 6's one-line `UX-15` row reads "Single click opens (or a visible
   'Open' affordance)", while `R` § 5.4's component rule and the prompt's
   own acceptance line both say "single click selects, double click or Enter
   opens, with the hint shown in the row's tooltip". **Decision**: § 5.4 +
   the prompt win — single click selects only; activation stays on
   double-click/Enter. The § 6 row is the same finding stated loosely, and
   `P`'s acceptance line is testable where § 6's parenthetical is not.
   *(`R`/`P`; FR-006, FR-009)*
2. **What a "list" is, for selection exclusivity** — `X` gives three
   call sites but four shapes: Library (five tabs, one visible at a time),
   Search (four groups visible at once, `search_view.rs::GROUP_ORDER`),
   and each detail view's track list. **Decision** (`D`): the selection
   scope is one per *view*, not one per rendered group — at most one row is
   selected anywhere on the Library screen, at most one anywhere on the
   Search screen (a click in Albums clears a selected Tracks row), and at
   most one in a detail view. Conventional (a music client highlights one
   row per screen), and it keeps "at most one selected row" true of what the
   user sees rather than of an invisible grouping. *(`D`; FR-007)*
3. **Selection identity and lifetime** — `X`'s lists are virtualized
   (`rows::virtualized_list` draws only the visible range) and a playlist
   may hold the same track twice, so "the selected row" cannot be a
   per-frame response or a bare entity id. **Decision** (`D`): selection is
   the pair (entity id, that row's index in the list's current display
   order); it survives scrolling out of and back into the rendered range,
   clears when that index no longer holds that id (removal, reorder,
   shrink), clears when the Library's active tab changes, and clears when a
   detail view navigates to a different entity. Session-local, never
   persisted. *(`D`; FR-007, FR-012, FR-028)*
4. **Which clicks select** — `X`'s row already consumes secondary click and
   `Shift+F10` (actions menu) and contains its own "…" button.
   **Decision** (`D`): only a primary single click on the row itself
   selects. Opening the actions menu (by "…" button, secondary click, or
   `Shift+F10`) and clicking any control inside a row leave the selection
   unchanged — the menu already names its own target row, so making it also
   move the selection would be an invisible side effect. *(`D`; FR-029)*
5. **Selection vs. keyboard focus when the two diverge** — `P` describes
   "clicked once to hold keyboard focus and then Enter". **Decision** (`D`):
   a primary click both selects the row and gives it keyboard focus, so
   Enter straight after a click opens/plays that row; if focus later moves
   away (Tab, or a click in another list), Enter follows *focus*, not
   selection — `X`'s existing `row_response.has_focus() && Enter` branch is
   unchanged, and this feature adds no second "current row" notion and no
   new arrow-key list navigation. *(`D`/`X`; FR-009, FR-030)*
6. **A selected row's text colours** — `X` draws a row's secondary line,
   duration, availability reason and "E" badge `.weak()`
   (`text.secondary`); `text.secondary` over an `accent` fill fails
   NFR-6.5's contrast minimum. **Decision** (`C` NFR-6.5, NFR-6.4): on a
   selected row *every* text run — title, secondary line, duration, "E",
   availability reason — renders `text.on-accent` at full strength, and the
   secondary line stays distinguishable by the type scale (`secondary`
   role), not by colour. Hover (4%) and pressed (8%) `text.primary` fills
   blend *over* the selection fill, so a selected row still reacts to the
   pointer. *(`C`/`R` § 5.4; FR-008, FR-031)*
7. **Rows with no entity yet** — `X`'s Library renders a `skeleton_row` for
   an un-hydrated id. **Decision** (`D`): a skeleton row is not selectable
   and never carries the tooltip — there is nothing to open, and
   `list_row`/`RowEntity` is not involved. *(`D`; FR-032)*
8. **The trailing column's width, and durations past an hour** — `X`'s
   `format_duration` emits bare `m:ss`, so a 124-minute DJ mix renders
   `124:15` and would widen the one column FR-001 exists to keep fixed.
   **Decision** (`D`, bounding a value by its formatter rather than trusting
   catalog data): the trailing duration column is a fixed width derived at
   runtime from the `mono` role's `'0'` advance × 7 (mirroring
   `theme::body_measure`'s 72 × `body` `'0'` advance — a named constant, not
   a literal), `format_duration` rolls over to `h:mm:ss` at 60 minutes
   (`1:04:15`, the convention every music client uses), and any figure still
   wider than the column truncates rather than widening it, so the "…"
   menu's x-position is identical in 100% of rows. *(`D`; FR-001, FR-033)*
9. **Tooltip copy for a Track row** — `P` says the tooltip states a second
   click or Enter "opens" the row, but on a Track row `X` plays it.
   **Decision** (`D`): two Fluent strings — one for Track rows (a second
   click or Enter plays it) and one for Album/Artist/Playlist rows (a second
   click or Enter opens it) — so the hint is never wrong about what happens.
   New keys land in `locales/en-US` (the only locale shipped in `X` today)
   and in `tests/fluent_keys.rs`'s inventory. *(`D`/`X`; FR-010)*
10. **Where a tab's count goes in the accessibility tree** — `X`'s
    `tests/accessibility.rs` and `tests/library_view.rs` assert each
    `Role::Tab` node's accessible name is *exactly*
    `tr("library-tab-…")`, which appending a count would break (FR-025
    forbids that). **Decision** (`C` NFR-6.2 + `X`): the count renders as
    its own sibling label node beside the tab, in the `mono` role; the Tab
    node's name, role and Fluent label are untouched, and assistive
    technology still reaches the count as the tab's adjacent text. *(`X`;
    FR-014, FR-016)*
11. **What a tab's count counts, and whether `0` shows** — `X`'s
    `library_status().loading` flag covers only the local index load, and a
    background `refreshing` sync can grow a set afterwards.
    **Decision** (`D`): the count is exactly the number of rows that tab
    would render right now (its own in-memory list's length), shown once
    `loading == false`, including `0`, and updating live as a refresh lands
    — the number always matches what is on screen and is never a
    server-reported total the list cannot show. *(`D`; FR-014)*
12. **The underline must not be the only signal** — `R` § 5.4 asks for an
    underlined active tab; `C` NFR-6.4 forbids colour (or any single visual
    cue) carrying meaning alone for assistive technology.
    **Decision**: the active tab additionally exposes an AccessKit
    selected/toggled state, and the underline's thickness is a named
    constant in `theme::controls` (like `FOCUS_RING_WIDTH`), not a literal
    at the call site. *(`C`/`R`; FR-013, FR-024, FR-034)*
13. **Where a panel's collapse control lives** — `R` § 5.4 calls panels
    "collapsible", which could mean a disclosure control in the card's own
    header. `X` puts the Queue/Effects/Transport toggles in Now Playing's
    control row (`now_playing.rs`'s three `switch` calls), and `R` § 5.5
    keeps "panel toggles" in the transport bar that a later Now Playing
    workbench feature makes sticky. **Decision**: this feature adds no new
    collapse control anywhere — the existing three switches and the `Q`/`E`/
    `T` shortcuts remain the collapse affordance, and a collapsed panel
    renders nothing at all (as today), rather than a header-only card.
    *(`R` § 5.5/`X`; FR-035, and FR-021 for Markers)*
14. **One card helper, not four card call sites** — the feature's whole
    point is that these are components. **Decision** (`D`): the card
    (fill, padding, radius, uppercase header, accessible-name pin) is one
    shared helper in `widgets/`, and all four panels render through it, so
    the treatment cannot drift panel to panel; no outline/stroke is added
    (§ 5.4 names none). *(`D`/`R` § 5.4; FR-017, FR-036)*
15. **The persisted flag's source of truth, its home, and its writer** —
    `X` holds each flag in `ui.memory` temp storage, and
    `PlaybackController::persist_settings` is *private*
    (`controller.rs:4684`), so the UI cannot call it; the settings model
    lives in `crates/modplayer-core/src/settings/model.rs` (not the UI
    crate). **Decision**: `settings.toml` becomes the single source of truth
    — the three `ui.memory` flags and their `panel_open_id` helpers are
    removed, not mirrored — reached through new public
    accessor/setter pairs on `PlaybackController` (the shape
    `set_focus_policy`/`focus_policy` already uses), each setter persisting
    immediately on every toggle from either input path (switch or shortcut).
    `SCHEMA_VERSION` stays `1`, exactly as `[onboarding]`'s addition did:
    an absent optional section is not a schema change. *(`X`; FR-019,
    FR-020, FR-037)*
16. **Tests that seed the old memory flag** — `X`'s `tests/now_playing.rs`
    and `tests/effects_view.rs` open/close panels by writing the egui memory
    id. **Decision**: those tests are migrated to seed the persisted setting
    instead; this is the one place FR-025's "existing assertions pass
    unmodified" is explicitly relaxed, because FR-019 moves the state they
    reach for. *(`X`; FR-019, FR-025, FR-037)*

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A list is scannable at a glance (Priority: P1)

A user looking at any catalog list — a Library tab, a Search results group, or an album/playlist/artist's detail view — can read every row's duration from one aligned column and every title from one consistent truncation edge, instead of today's single run-on line ("Justin Timberlake — TROLLS (Original Motion Picture Soundtrack) — 3:57") that buries the duration mid-sentence and gives the row no rhythm.

**Why this priority**: This is the feature's lead acceptance line and the source audit's top-rated library finding (`UX-13`, P1) — every row in the app's three busiest screens (Library, Search, Detail) renders through the one shared row widget, so this fix reaches the whole catalog at once.

**Independent Test**: Display a Library tab, a Search results group, and an album's detail view, each with several rows visible; confirm every row's duration renders in the same right-aligned column in tabular (`mono`) figures, every title truncates at the same trailing edge, and resting the pointer on any row highlights the full row while the "…" menu stays reachable without the pointer leaving the row.

**Acceptance Scenarios**:

1. **Given** a Library tab (e.g. Saved Tracks) with several rows visible, **When** the tab renders, **Then** every row's duration renders right-aligned in the same column, in `mono` figures, to the left of the "…" menu — not embedded in the artist/album detail line.
2. **Given** a Search results group or a detail view's track list, **When** it renders, **Then** its rows share the identical column layout, alignment, and truncation edge as the Library tab (same shared row widget, `rows.rs`).
3. **Given** any row whose title is longer than the title column's width, **When** it renders, **Then** it truncates with an ellipsis at the same trailing edge every other row in the app uses — it never wraps to a second line.
4. **Given** any row, **When** the pointer rests on it, **Then** the row's full width highlights and the "…" actions menu is reachable without moving the pointer outside the row's bounds.
5. **Given** an Album, Artist, or Playlist row (an entity with no duration), **When** it renders in a mixed-kind list alongside track rows, **Then** its right-hand column reserves the same width as a track row's duration column (left empty) so every row's "…" menu lines up at the same trailing x-position.

---

### User Story 2 - A row tells you how to open it (Priority: P1)

A user looking at any list can tell, before clicking, that a single click selects a row and a second click (or Enter, once the row holds keyboard focus) opens it — instead of today's silent discard, where a single click on a playlist does nothing and nothing in the interface says a double click is required.

**Why this priority**: This is the feature's second acceptance line and the source audit's other P1 library finding (`UX-15`) — today a user can click a playlist once, see nothing happen, and have no way to learn why; this is a discoverability bug on the app's own primary navigation gesture, not a polish item.

**Independent Test**: Display a list of playlists (or any row kind); click a row once and confirm it visibly becomes selected with no navigation; click it a second time (or press Enter while it holds keyboard focus) and confirm it opens (a track plays; an album/artist/playlist opens its detail view); hover the row and confirm its tooltip states that a second click or Enter opens it.

**Acceptance Scenarios**:

1. **Given** any row in any list, **When** it is clicked once, **Then** it becomes the list's selected row (an interior `accent` fill, matching the app's existing selection treatment) and nothing else happens — no navigation, no playback change.
2. **Given** a row that is already selected, **When** a different row in the same view is clicked once — including a row in another of Search's four groups — **Then** the previous row's selected state clears and the newly clicked row becomes selected: at most one row per view (Library screen, Search screen, one detail view) is selected at a time (FR-007).
3. **Given** a Playlist (or Album/Artist) row, **When** it is double-clicked, or clicked once to hold keyboard focus and then Enter is pressed, **Then** it opens (navigates to its detail view) exactly as today.
4. **Given** a Track row, **When** it is double-clicked, or clicked once to hold keyboard focus and then Enter is pressed, **Then** it plays now, exactly as today.
5. **Given** any row, **When** the pointer rests on it long enough to show a tooltip, **Then** the tooltip states, in words, that a second click or Enter opens the row.
6. **Given** a row that is both selected and holds keyboard focus, **When** it renders, **Then** the selection fill and the focus ring (015-control-variants FR-010) remain two separately identifiable signals — neither hides the other.

---

### User Story 3 - Now Playing's blocks read as panels, and a collapsed panel stays collapsed (Priority: P2)

A user looking at the Now Playing screen sees the Markers, Effect Chain, Transport, and Queue blocks as distinct cards — a raised surface, a small uppercase header, generous padding, a rounded corner — instead of today's plain content separated only by spacing; collapsing the Effect Chain, Transport, or Queue panel and restarting the app leaves it collapsed, instead of today's panels reopening closed every launch regardless of what the user last chose.

**Why this priority**: This is the feature's fourth acceptance line and its only cross-session correctness bug — today's panel open/closed flags live only in the UI toolkit's per-session memory (`now_playing.rs`'s `queue_panel_open_id`/`effects_view::panel_open_id`/`transport_view::panel_open_id`) and are silently discarded on every restart, which is the literal scenario the feature's acceptance line names.

**Independent Test**: Collapse the Effect Chain panel, restart the app, and confirm it is still collapsed; expand it, restart again, and confirm it is still expanded; repeat for the Transport and Queue panels independently; separately, display the Now Playing screen and confirm the Markers, Effect Chain, Transport, and Queue blocks each render as a card (raised surface, padded, rounded) with a small uppercase header.

**Acceptance Scenarios**:

1. **Given** the Effect Chain panel is open, **When** it is collapsed and the app is restarted, **Then** it renders collapsed on the next launch.
2. **Given** the Transport panel is collapsed, **When** it is expanded and the app is restarted, **Then** it renders expanded on the next launch.
3. **Given** the Queue panel's open/closed state, **When** it is changed and the app is restarted, **Then** the new state survives independently of the Effect Chain and Transport panels' own states.
4. **Given** the Now Playing screen with a track loaded, **When** it renders, **Then** the Markers, Effect Chain, Transport, and Queue blocks each show a raised-surface card background, a rounded corner, generous padding, and a small uppercase header — not the plain unbounded content of today.
5. **Given** the `Q`/`E`/`T` keyboard shortcuts that already toggle the Queue/Effect Chain/Transport panels, **When** one is pressed, **Then** it still toggles the same panel the matching switch in Now Playing's control row toggles (there is one flag, now living in `settings.toml`, not two), and the new open/closed value is the one persisted.

---

### User Story 4 - The Library tab strip reads as navigation (Priority: P2)

A user looking at the Library screen sees its five tabs as an underlined navigation row, with each tab showing how many items it holds, instead of today's five identical `selectable_label`s where the active tab is a filled, button-like rounded rectangle carrying no count.

**Why this priority**: This is the feature's third named component and a real but lower-severity finding (`UX-16`, P2 in the source audit) — the tab strip is already fully functional (every tab switches correctly, every empty state is honest); this fixes how it *reads*, not what it does.

**Independent Test**: Display the Library screen with the account's saved-tracks/albums/artists/playlists/recently-played data loaded; confirm the active tab renders with an underline rather than a filled background, and every tab shows its own item count once the library has finished loading.

**Acceptance Scenarios**:

1. **Given** the Library screen, **When** it renders, **Then** the currently active tab is marked by an underline rather than a filled/accent background, and every tab keeps its existing `Role::Tab` accessible role and label.
2. **Given** the Library screen has finished its initial load (`library_status().loading == false`), **When** the tab row renders, **Then** every tab shows its own item count (the length of that tab's already-loaded list) beside its label, in `mono` figures.
3. **Given** the Library screen is still loading, **When** the tab row renders, **Then** no tab shows a count (the count is not yet known).
4. **Given** a tab is clicked, **When** the active tab changes, **Then** switching behavior, the empty-state copy, and every other existing Library behavior are unchanged.

---

### Edge Cases

- An Album/Artist/Playlist row (no duration) sits in the same virtualized list as Track rows — its right-hand column reserves the same width as a duration figure would, left empty, so every row's "…" menu still lines up (User Story 1, Scenario 5).
- A double-click's first click also satisfies User Story 2's single-click selection — the row may become selected and then immediately open in the same interaction; nothing requires suppressing the intermediate selected frame.
- The selected row scrolls out of the virtualized list's rendered range and back into it — its selected state survives (it is tracked as an (entity id, display index) pair in the owning view's own state, not by the row widget's transient per-frame call).
- The selected row is removed from its list (e.g. an unfollowed artist drops out of Followed Artists) between one frame and the next, or the list reorders so a different id now sits at that index — the selection clears rather than silently pointing at nothing or jumping to a neighbour.
- The same track appears twice in one playlist and one of the two is clicked — only the clicked row is selected, because the selection carries the display index alongside the id (FR-007).
- A row is clicked while a different view already holds a selection (e.g. Library selected, then a Search row clicked) — each view keeps its own selection; neither clears the other (FR-007).
- A row is disabled by unavailability (`UnavailableRegion`/`Removed`, `rows.rs::availability_reason`) — it can still be selected (a visual-only state) exactly like an available row; this feature does not change what an unavailable row's activation does.
- The Library screen's item counts (User Story 4) are the length of each tab's own already-loaded, in-memory list (`LibraryIndex::saved_tracks().len()` etc.), the same figures `library_view.rs` already reads — not a separate server-reported total, and not shown until the initial load finishes (`library_status().loading == false`).
- An older `settings.toml` with no `[now_playing_panels]` section loads every Now Playing panel closed — the same default `now_playing.rs`'s `queue_panel_open_id` (etc.) already falls back to today, so this feature changes *where* the flag lives, not its default value.
- The Markers panel receives the same card visual treatment (raised surface, padding, rounded corner, small uppercase header) as Effect Chain/Transport/Queue, but this feature does not add a new collapse control to it, since none exists today and the feature's acceptance line does not name a fourth toggle (Assumptions).
- The Effect Chain panel's own reserved-height calculation (`now_playing.rs::EFFECTS_PANEL_RESERVED_HEIGHT`) already accounts for the header/master-volume/peak-meter/Queue-toggle row beneath it (2026-09-19 manual walk finding) — wrapping panels in a padded card must not silently reintroduce that same off-window clipping.
- A user tabs to a panel's collapse switch in Now Playing's control row by keyboard and toggles it with Space/Enter (not a pointer click) — the same persisted-state write applies; the switch widget (`widgets/controls.rs::switch`) already handles both input paths identically.
- A row is clicked, then the pointer rests on it — the hover fill blends over the selection fill, so the row is visibly both selected and hovered (FR-008).
- A Library tab is still loading, or its list is empty — no count and a `0` count respectively (FR-014): a hidden count means "not yet known", never "none".
- A duration of exactly 60 minutes — the first value formatted `1:00:00` rather than `60:00` (FR-033).

## Requirements *(mandatory)*

### Functional Requirements

**List row — the shared three-column grid (User Story 1)**

- **FR-001**: Every row the shared row widget (`rows::list_row`) draws MUST lay out as a fixed three-column grid: a leading artwork/initials-placeholder column at a consistent size (unchanged from today, `ARTWORK_SIZE`), a middle column holding the row's title above its secondary detail line, and a trailing column, right-aligned, holding the entity's duration (when it has one) in `mono` figures immediately before the "…" actions menu. The trailing duration column's width MUST be a single named constant — derived at runtime from the `mono` role's `'0'` advance × 7, mirroring `theme::body_measure`'s derivation, never a hand-written pixel literal — identical for every row kind, every list, and every duration value (Clarifications 8). *(Prompt; UX-13; § 5.4 "List row"; `rows.rs`; Clarifications 8)*
- **FR-002**: A Track row's duration MUST move out of its secondary detail line ("artists — album — m:ss") into FR-001's trailing column; the secondary line keeps only the artists/album text. Every duration rendered anywhere through this widget MUST use the existing `theme::mono_text` role, unchanged from 014-design-tokens-and-type-scale. *(Prompt; UX-13; `rows::draw_content`)*
- **FR-003**: An Album, Artist, or Playlist row (entities that carry no duration) MUST still reserve FR-001's trailing column's width, left empty, so the "…" menu renders at the same trailing x-position in every row kind a mixed list can show. *(Prompt "every duration is right-aligned in the same column"; Edge Cases)*
- **FR-004**: Every row's title MUST continue to truncate (never wrap) at the same trailing edge across every row kind and every list this widget serves (Library, Search, every detail view) — a regression guard on `rows::line`'s existing `.truncate()` behavior, now re-verified against FR-001's narrower title column. *(Prompt; UX-13)*
- **FR-005**: Every row MUST continue to show a full-row hover fill (015-control-variants FR-009) with its "…" menu reachable without the pointer leaving the row's bounds — a regression guard on `rows.rs`'s existing hover fill and `ACTIONS_RESERVED_WIDTH` reservation, re-verified once FR-001's trailing column also reserves space for the duration figure. *(Prompt; UX-14; 015-control-variants FR-009)*

**List row — click-to-select, double-click/Enter-to-open (User Story 2)**

- **FR-006**: A single click on any row MUST mark that row as the current selection within its own view (FR-007) and MUST NOT trigger any other effect — no navigation, no playback change, no queue mutation. *(Prompt; UX-15)*
- **FR-007**: Selection MUST be exclusive *per view*, not per rendered group: at most one row is selected anywhere on the Library screen, at most one anywhere on the Search screen (selecting an Albums-group row clears a selected Tracks-group row, and vice versa), and at most one in a detail view. Each view's selection MUST be tracked by that view's own state (mirroring `LibraryViewState`'s existing `tab`/`pending_action` fields; Search and Detail gain equivalent App-owned view state, which `search_view.rs`/`detail_view.rs` do not have today) and MUST be independent of every other view's. A view's selection MUST be identified by the pair (entity id, that row's index in the list's current display order), so a list holding the same entity twice never shows two selected rows. *(Prompt; User Story 2, Scenario 2; Clarifications 2, 3)*
- **FR-008**: A selected row MUST render with an interior `accent` fill and `text.on-accent` label text — the same selection treatment 014-design-tokens-and-type-scale's `accent`/`text.on-accent` roles already drive for `selectable_label`-based selection (014 FR-010b) — and MUST remain visually distinct, at the same time, from the row's hover fill (4% `text.primary`), pressed fill (8% `text.primary`), and 2px offset `accent` focus ring (015-control-variants FR-009–FR-011). A selected row's hover and pressed fills MUST blend *over* its selection fill, so a selected row still visibly reacts to the pointer. *(Prompt; § 5.4; 014 FR-010b; 015-control-variants FR-010; Clarifications 6)*
- **FR-009**: Double-clicking a row, or pressing Enter while it holds keyboard focus, MUST continue to produce exactly today's `rows::list_row` behavior — `RowEvent::Open` for Album/Artist/Playlist rows, `RowAction::PlayNow` for Track rows — unchanged by FR-006's new single-click selection. *(Prompt; UX-15; `rows.rs`'s existing `activated` branch)*
- **FR-010**: Every row MUST carry a tooltip stating, in words, what a second click or Enter does to *that* row: one Fluent string for Track rows (a second click or Enter plays it) and one for Album/Artist/Playlist rows (a second click or Enter opens it), so the hint never promises the wrong outcome. Both keys MUST be added to `locales/en-US` — the only locale the repository ships today — and to `tests/fluent_keys.rs`'s key inventory. *(Prompt "the row's tooltip says so"; UX-15; Clarifications 9)*
- **FR-011**: A selected row's accessible state MUST expose that it is selected (an AccessKit selected/toggled state) alongside its existing `Role::ListItem` role and name (`rows::accessible_name`, unchanged), so assistive technology can announce which row is selected. *(NFR-6.2; `rows.rs`'s existing `accesskit_node_builder` call)*
- **FR-012**: A view's selection MUST persist across frames while the selected (entity id, index) pair still holds in that list (including after the row scrolls out of the virtualized range and back in), and MUST clear on any later frame where that index no longer holds that id — removal, reorder, or a list that shrank past it (Edge Cases). *(Prompt; User Story 2; Clarifications 3)*
- **FR-028**: A view's selection MUST also clear when the list it belongs to is replaced rather than mutated: when the Library's active tab changes, when a detail view navigates to a different entity, and when a new search query replaces the results. It MUST NOT be persisted to `settings.toml` or survive a restart (Assumptions). *(Clarifications 3; Assumptions)*
- **FR-029**: Only a primary single click on the row itself MUST change the selection. Opening the row's actions menu — by the "…" button, by secondary click, or by `Shift+F10` — and activating any control inside a row MUST leave the view's selection exactly as it was. *(Clarifications 4; `rows.rs`'s existing `secondary_clicked`/`Shift+F10` branches)*
- **FR-030**: A primary single click MUST both select the row and give it keyboard focus, so Enter immediately afterwards activates that same row (FR-009). Where focus and selection later diverge (Tab moves focus, or a click selects in another view), Enter MUST act on the *focused* row, exactly as today — this feature introduces no second "current row" notion, and no new arrow-key navigation between rows. *(Clarifications 5; `rows.rs`'s existing `has_focus() && Enter` branch; NFR-6.1)*
- **FR-031**: On a selected row, every text run — title, secondary detail line, duration figure, the explicit "E" badge, and any availability reason — MUST render in `text.on-accent` at full strength (never the `.weak()`/`text.secondary` colour today's rows use), meeting NFR-6.5's contrast minimum against the `accent` fill in both themes; the secondary line MUST stay distinguishable from the title by the type scale alone, not by colour. *(NFR-6.4; NFR-6.5; Clarifications 6)*
- **FR-032**: A skeleton row (an un-hydrated id rendered by `widgets::skeleton::skeleton_row`) MUST NOT be selectable and MUST NOT carry FR-010's tooltip — it has no `RowEntity` to open. *(Clarifications 7; `library_view.rs`'s existing skeleton rows)*
- **FR-033**: `rows.rs`'s duration formatter MUST roll over to `h:mm:ss` at 60 minutes (e.g. `1:04:15`) and keep `m:ss` below it, so no duration string can exceed FR-001's fixed column; a figure still wider than the column (only reachable from malformed catalog data) MUST truncate rather than widen the column. *(Clarifications 8; FR-001)*

**Tab strip — Library tabs (User Story 4)**

- **FR-013**: The Library tab strip (`library_view.rs`, `LibraryTab::ORDER`) MUST render its active tab with an underline rather than a filled/accent background, reading as navigation rather than a row of buttons. *(Prompt; UX-16; § 5.4 "Tabs vs. chips")*
- **FR-014**: Once `controller.library_status().loading` is `false`, every tab MUST show its own item count beside its label — the number of rows that tab would render right now, i.e. the length of that tab's own in-memory list (`LibraryIndex::saved_tracks()`/`saved_albums()`/`followed_artists()`/`playlists()`/`controller.recently_played()`, the same accessors `library_view.rs` already reads), never a server-reported total the list cannot show. The count MUST be shown even when it is `0`, and MUST update in place as a background `refreshing` sync grows the set. While `loading` is `true`, no tab shows a count. The count MUST render as its own label node beside the tab, not inside the tab's accessible name (FR-016). *(Prompt "shows a count beside each tab when the count is known"; Edge Cases; Clarifications 10, 11)*
- **FR-015**: Every tab count rendered under FR-014 MUST use the `mono` role, consistent with 014-design-tokens-and-type-scale's "every number a user compares" rule (the same rule FR-002's duration figures already follow). *(014-design-tokens-and-type-scale FR-005; consistency)*
- **FR-016**: The tab strip's existing `Role::Tab` accessible role, its five Fluent labels, its click-to-switch behavior, and every tab's existing empty-state copy MUST remain unchanged by FR-013–FR-015. Each `Role::Tab` node's accessible name MUST stay *exactly* `tr("library-tab-…")`, with no count appended — `tests/accessibility.rs`'s `find_one(&nodes, Role::Tab, &tr(key))` and `tests/library_view.rs`'s `labels_in_tree_order` assertions MUST keep passing verbatim. *(NFR-6.2; Scope boundary — this feature restyles, it does not restructure; Clarifications 10)*
- **FR-034**: The active tab MUST additionally expose an AccessKit selected/toggled state, so the underline is not the only carrier of "this tab is active", and the underline's thickness MUST be a named constant in `theme::controls` (alongside `FOCUS_RING_WIDTH`), never a literal at the call site. *(NFR-6.4; FR-024; Clarifications 12)*

**Panel — Now Playing's Markers, Effect Chain, Transport, and Queue blocks (User Story 3)**

- **FR-017**: The Markers panel (`markers::panel`), the Effect Chain panel (`effects_view::show`), the Transport panel (`transport_view::show`), and the Queue panel (`queue_view::show`) MUST each render as a card: a `surface.raised` background fill, `lg` (16px) padding on every side, and an `md` (8px)-radius rounded corner, using the existing `theme::tokens::space`/`theme::tokens::radius` values — no new literal, and no outline/stroke (§ 5.4 names none). *(Prompt; § 5.4 "Panels"; `theme/tokens.rs`)*
- **FR-036**: The card treatment of FR-017/FR-018 (fill, padding, radius, uppercase `section` header, un-uppercased accessible-name pin) MUST be implemented once, as one shared helper in `crates/modplayer-ui/src/widgets/`, and all four panels MUST render through it — so the four cannot drift apart panel by panel, which is the point of turning a panel into a component. *(Prompt "into real components"; Clarifications 14)*
- **FR-018**: Every one of FR-017's four panels MUST show a small uppercase `section`-role header (`theme::section_label`), consistent with each other: the Transport panel's current `ui.heading()` title (`title` role) becomes `theme::section_label`, matching Markers'/Effects' existing treatment, and the Queue panel — which shows no header today — gains one reading "Queue". Every header's accessible name stays the exact, un-uppercased string (mirrors Markers'/Effects' existing `accesskit_node_builder` pin). *(Prompt; § 5.4 "Panels"; `markers.rs`/`effects_view.rs`'s existing `section_label` precedent)*
- **FR-019**: The Effect Chain, Transport, and Queue panels' existing open/closed toggles (the three `switch` calls in `now_playing.rs`'s control row, and the `E`/`T`/`Q` keyboard shortcuts that flip the same flag) MUST persist their open/closed state to `settings.toml`, instead of today's `ui.memory`-only, per-session storage. `settings.toml` MUST become the single source of truth: the three `ui.memory` flags and their `panel_open_id` helpers (`now_playing.rs::queue_panel_open_id`, `effects_view::panel_open_id`, `transport_view::panel_open_id`) are removed rather than mirrored, so a click and a shortcut cannot diverge. Because `PlaybackController::persist_settings` is private (`controller.rs:4684`), the UI MUST reach it through new public accessor/setter pairs on `PlaybackController` shaped like the existing `focus_policy`/`set_focus_policy`, each setter persisting immediately on every toggle from either input path. Each of the three panels' state persists and restores independently. *(Prompt "collapsible with its open or closed state remembered per panel"; Prompt's Acceptance line; `controller.rs::persist_settings`; Clarifications 15)*
- **FR-020**: The persisted flags FR-019 introduces MUST live in a new, optional `[now_playing_panels]` `settings.toml` section (three booleans: effect-chain-open, transport-open, queue-open), following the same "absent section defaults, `#[serde(default)]` per field" convention already used by `[transport]`/`[onboarding]` (`crates/modplayer-core/src/settings/model.rs`'s `RawTransport`/`RawOnboarding`). An older `settings.toml` with no such section MUST load every panel closed — the same default `now_playing.rs`'s current `ui.memory` lookups already fall back to — and `SCHEMA_VERSION` MUST stay `1`, exactly as `[onboarding]`'s addition left it (an absent optional table is not a schema change). *(Edge Cases; that file's existing optional-table precedent; Clarifications 15)*
- **FR-035**: This feature MUST NOT add a collapse control to any panel's own card header: the three existing control-row switches and the `Q`/`E`/`T` shortcuts remain the only collapse affordance (§ 5.5 keeps panel toggles in the transport bar a later Now Playing workbench feature makes sticky), and a collapsed panel MUST render nothing at all, as today — not a header-only card. *(§ 5.5; Scope boundary; Clarifications 13)*
- **FR-037**: Existing tests that open or close a Now Playing panel by writing the egui memory flag (`tests/now_playing.rs`, `tests/effects_view.rs`) MUST be migrated to seed the persisted `[now_playing_panels]` value instead. This is the one explicit relaxation of FR-025 — the state those tests reach for is what FR-019 moves. *(FR-019; FR-025; Clarifications 16)*
- **FR-021**: The Markers panel MUST receive FR-017/FR-018's card and header treatment but MUST NOT gain a new collapse control in this feature — it has none today, and neither the prompt's acceptance line nor any user story names one for it (Assumptions). *(Scope discipline; Assumptions)*
- **FR-022**: Wrapping FR-017's four panels in a padded card MUST NOT change any panel's existing click targets, keyboard shortcuts, or data — every existing row/control inside a panel keeps its current behavior; only the panel's own chrome (background, padding, corner, header style) changes. *(Scope boundary; mirrors 015-control-variants FR-017/FR-018)*
- **FR-023**: Any fixed-height layout calculation that assumes a panel's content starts at zero inset (e.g. `now_playing.rs::EFFECTS_PANEL_RESERVED_HEIGHT`, sized around the header/master-volume/peak-meter/Queue-toggle row beneath the Effect Chain panel) MUST be re-derived to account for FR-017's own padding, so no panel's content is pushed off-window or clipped at the window's minimum size (960×640) — a regression guard on the 2026-09-19 manual-walk finding `EFFECTS_PANEL_RESERVED_HEIGHT` itself documents. The re-derived value MUST be expressed as a sum of `theme::space` tokens and measured widget heights rather than a re-guessed pixel literal (FR-024). *(Edge Cases; Constitution Principle VIII)*

**Cross-cutting**

- **FR-024**: Every colour, spacing, and radius value this feature introduces (the panel card's fill/padding/radius, the tab underline, the selected-row fill) MUST resolve to an existing role or token in `theme::tokens`/`theme::controls` — this feature MUST NOT write a new colour, alpha, spacing, or radius literal at any call site it touches, extending 014-design-tokens-and-type-scale's and 015-control-variants' zero-literal guarantee to the files this feature modifies. *(014 FR-018/FR-018a; 015-control-variants FR-019)*
- **FR-025**: Every control or row whose accessible name, role, or keyboard operability this feature does not explicitly change (FR-011, FR-034, FR-018) MUST keep it exactly as it is today; the existing `crates/modplayer-ui/tests/accessibility.rs` assertions MUST pass unmodified except where FR-011/FR-018/FR-034 explicitly add a state, and except for FR-037's migration of the two panel-state test helpers. FR-016 is the binding case: the tab nodes' names do not change at all. *(NFR-6.1; NFR-6.2; Constitution Principle X)*
- **FR-026**: This feature MUST NOT change which lists exist, how Search's four groups are arranged, or the order of Now Playing's blocks (heading, transport row, waveform, markers, Effect Chain, Transport, master volume, peak meter, Queue) — those remain 011-browse-and-manage-polish's and 010-now-playing-workbench's scope respectively. *(Scope boundary)*
- **FR-027**: The decisions above MUST be verified by automated tests wherever derivable from values without a rendered frame: FR-001–FR-003's column layout, FR-002's `mono` duration role, and FR-033's `h:mm:ss` rollover (`rows.rs` unit tests, mirroring its existing `accessible_name`/`row_height` tests); FR-006–FR-012 and FR-028–FR-032's selection exclusivity (including across Search's four groups), persistence-across-scroll, clear-on-removal, clear-on-tab-change, menu-does-not-select, and skeleton-not-selectable logic; FR-013–FR-016 and FR-034's tab count/underline/selected-state/role logic, including that each `Role::Tab` node's name is unchanged; and FR-019–FR-020's settings round-trip and absent-section default (mirroring `crates/modplayer-core/src/settings/model.rs`'s existing `plugin_panels_round_trip` test). The quickstart's manual scenarios, executed and captured by the implementing agent per Governance § Manual Scenario Sign-Off, are the evidence that the rendered pixels — the underline, the card, the tooltip text, the selection fill — match those values. *(Constitution Principle VIII; Governance § Manual Scenario Sign-Off; 014 FR-018b/015 FR-022 precedent)*

### Key Entities

- **List Row**: The shared three-column grid one `rows::list_row` call renders for one `RowEntity` (Track/Album/Artist/Playlist) — a leading artwork column, a middle title/secondary-line column, and a trailing right-aligned column holding the entity's duration (when it has one) plus the "…" actions menu. Every catalog list (Library, Search, every detail view) renders through this one widget, so a change to it reaches all of them at once.
- **Row Selection**: Which row, if any, is the current single-click-selected row within one *view* — the (entity id, display index) pair the Library screen, the Search screen, or one detail view currently holds. Owned by that view's own state (mirroring `LibraryViewState`'s existing `tab`/`pending_action` fields; Search and Detail gain equivalent state), exclusive within the view (Search's four groups share one selection), independent across views, session-local, and distinct in kind from keyboard focus (015-control-variants' Interaction State) and from a row's activated (opened/played) outcome.
- **Tab**: One entry in the Library tab strip — a label, an active/inactive state now rendered as an underline rather than a filled background, and an optional item count (the length of that tab's already-loaded list, shown once known).
- **Panel**: A card-styled container (`surface.raised` fill, `lg` padding, `md` radius, a small uppercase `section`-role header), drawn by one shared helper, wrapping one of the Now Playing screen's grouped blocks — Markers, Effect Chain, Transport, or Queue. Three of the four (Effect Chain, Transport, Queue) are collapsible through the existing control-row switch and `Q`/`E`/`T` shortcut — no new in-card control — and their open/closed flag is persisted to `settings.toml` and restored on the next launch, independently per panel; a collapsed panel draws nothing.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In every catalog list sampled (every Library tab, every Search results group, every detail view's track list), 100% of rows with a duration show it right-aligned in one shared column in tabular figures, and every row's title truncates at the same trailing edge.
- **SC-002**: Resting the pointer on any row, in any list sampled, visibly highlights the full row and leaves its "…" menu reachable without the pointer leaving the row's bounds.
- **SC-003**: A single click on any row marks it selected with no other effect, in 100% of rows sampled across every entity kind (Track, Album, Artist, Playlist); a second click or Enter opens it (plays a Track, or navigates to an Album/Artist/Playlist's detail view) in the same 100% of cases.
- **SC-004**: Every row sampled carries a tooltip that states, in words, that a second click or Enter opens the row.
- **SC-005**: The Library tab strip's active tab is marked by an underline rather than a filled background, and every tab shows its own item count once the library has finished loading — sampled across all five tabs.
- **SC-006**: Every one of the Now Playing screen's Markers, Effect Chain, Transport, and Queue blocks renders as a raised-surface, rounded-corner, padded card with a small uppercase header, sampled at both the initial 1200×820 window size and the 960×640 minimum.
- **SC-007**: Collapsing or expanding the Effect Chain, Transport, or Queue panel and restarting the app leaves each panel in the state it was last set to, independently — verified for all three panels and both directions (collapse-then-restart, expand-then-restart).
- **SC-008**: A source-tree scan for colour, spacing, and radius literals outside the token module (the scan established by 014-design-tokens-and-type-scale, extended by 015-control-variants) returns zero new hits across every file this feature modifies.
- **SC-009**: The existing accessibility test suite passes unchanged for every control this feature does not explicitly name (FR-011/FR-018/FR-034), including every `Role::Tab` node's exact accessible name, confirming no regression from FR-025; the only edited test helpers are FR-037's two panel-state seeds.
- **SC-010**: At most one row is selected per view at any time, verified by clicking rows across Search's four groups in sequence and across a Library tab switch: the previously selected row is never still highlighted.
- **SC-011**: A selected row's every text run is legible against the `accent` fill at ≥4.5:1 in both themes (the same `contrast` check 014-design-tokens-and-type-scale established), sampled on a Track row with an availability reason and an explicit badge — the widest set of text runs a row can show.
- **SC-012**: A duration of 60 minutes or more renders `h:mm:ss` and the "…" menu's x-position is unchanged from a `m:ss` row in the same list, sampled at both window sizes.

## Assumptions

- Row selection (User Story 2) is session-local and per-view (FR-007, FR-028), not persisted to `settings.toml` — the prompt's cross-restart persistence requirement names only panels ("when a panel is collapsed and the app is restarted, the panel is still collapsed"), and no source material asks for a remembered row selection.
- Selecting a row has no effect beyond its own visual/accessible state — it does not start playback, does not populate an acting list (`rows::acting_list`), and does not change any other control's enabled state. Only double-click/Enter (already-existing behavior, FR-009) opens or plays a row.
- The nav rail (`shell.rs`) and the Settings category list (`settings/mod.rs`) are one-of-N selection controls, not tab strips, and are out of this feature's scope: § 5.4's "Tabs vs. chips" bullet names only "Library tabs" and "Settings categories" as conversions, and the source review's own roadmap assigns the Settings sidebar to a separate shell/navigation feature, not this one.
- Tab item counts (FR-014) render in the `mono` role even though the source bullet only says "shows a count" — 014-design-tokens-and-type-scale's own rule ("every number a user compares") already covers this figure, and a differently-styled count beside a `mono` duration column would be the same inconsistency this feature is fixing elsewhere.
- The Markers panel receives the Panel component's card visual (FR-017/FR-018) but not a new collapse control (FR-021): it has no open/closed toggle today, and adding one is a new interaction affordance the prompt's acceptance line and user stories do not name — a future feature may add it without conflicting with this one's card styling.
- FR-019's persisted panel state is added as a new `[now_playing_panels]` `settings.toml` section rather than reusing `[plugin_panels]` (011-plugin-ui-contributions' table), because that table is keyed by plugin identifier and panel id and already carries a distinct meaning (docked/floated placement, disabled flag); the Now Playing panels this feature persists are host panels, not plugin panels.
- Fixing the Transport panel's header from `ui.heading()` (`title` role) to `theme::section_label` (FR-018) is treated as a correction within this feature's own "one panel-header treatment" requirement, not a new requirement invented beyond the prompt — § 5.4 defines exactly one panel-header style, and Transport is the one panel that does not yet use it.
- The three new Fluent strings this feature needs (a Track-row open hint, a non-Track-row open hint, the Queue panel's header) land in `locales/en-US` only, because that is the sole locale bundle the repository contains today — every other host string is in the same position, so Principle X's "English and pt-BR ship first" is an app-wide localization gap for a dedicated feature to close, not a decision this feature can make differently from every existing string.
- The underlying toolkit (egui, already used for `selectable_label`'s existing accent-fill selection, tooltips via `Response::on_hover_text`, and AccessKit's selected/toggled state) already supports everything FR-006–FR-012 and FR-010 need; this feature does not evaluate or change that mechanism, only uses it the way `theme::tokens`/`widgets::controls` already does elsewhere in the app.

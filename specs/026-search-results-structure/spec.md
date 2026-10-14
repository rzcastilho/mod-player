# Feature Specification: Search Results Structure and Feedback

**Feature Branch**: `feature/026-search-results-structure`

**Created**: 2026-09-30

**Status**: Draft

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/011-browse-and-manage-polish/002-search-results-structure.md (id 002, search-results-structure)."

## Clarifications

### Session 2026-09-30 (clarify reviewer)

Sources: breakdown `011-browse-and-manage-polish/002-search-results-structure.md` (BD), UI/UX review §3.4 UX-18/19/20 and §3.10 (REV), constitution v1.1.1 (CON), 004-search-and-library-browse spec + clarifications (004), existing code — `crates/modplayer-ui/src/search_view.rs`, `crates/modplayer-core/src/search.rs`, `main.rs` (CODE). Items marked **default** were not settled by a source; the conventional choice was taken.

- Q: What is "the smallest supported window size"? → A (CODE): the minimum inner size 960 × 640 px (`main.rs` `with_min_inner_size`), with the plugin dock in whatever presentation 018 gives at that width. REV's "800 px" predates the 960 px floor; SC-001 is measured at 960 × 640.
- Q: Are groups ever arranged in columns? → A (BD says "may"; **default**, simplest): **no** — this feature uses a single-column stacked layout at every window width. Multi-column arrangement is out of scope. This removes the "resize between stacked and column layouts" edge case.
- Q: Which groups get a header, and what happens to empty ones? → A (004 clarification + CODE): unchanged rule — a group that is `Empty` or `Unsupported` is omitted entirely (no header, no gap); a `RateLimited` group with nothing stale is omitted (004 M12 walk). A `Pending` group shows its header (name only, no count) and 3 skeleton rows. Only when every group is `Empty` does the no-results state appear.
- Q: What does a header's count count? → A (CODE: the service returns no total, only `items` + `next_offset`): the number of rows currently rendered in that group (`items.len()`, including rows appended by "Show more" and stale rows). No "+" suffix; the group's "Show more" control already signals that more exist. Visual: the group name in the existing `theme::section_label` style followed by the count in `text_secondary`; accessible name "<Group>, <n> results" (Fluent, plural-aware).
- Q: What exactly is "pinned"? → A (**default**, standard sticky-header behaviour): while any part of a group's rows is within the results viewport and the group's header has scrolled above the viewport top, the header is drawn at the viewport top over the rows; when the next group's header reaches the top it pushes the previous one out (the previous one never overlaps the next). At most one pinned header is visible at a time.
- Q: What scrolls and what doesn't? → A (**default**): the query field row (field, clear control, progress indicator), the result-count line and the rate-limit status are fixed above one vertical results scroll area; every group, its header, its rows and its "Show more" control live inside that one scroll area. No group owns a scroll area of its own (the `GROUP_VISIBLE_ROWS` cap is removed).
- Q: Must long groups still be virtualised? → A (CODE + **default**): yes — after repeated "Show more" a group may hold hundreds of rows; only rows intersecting the viewport (plus at most one row either side) are laid out per frame, with fixed per-kind row heights reserving the full scroll extent.
- Q: Scroll position on a new query? → A (CODE, 004 walk M2): the single results scroll area returns to the top whenever the effective query changes; "Show more" and stale transitions do not move it.
- Q: How is the field "labelled once"? → A (REV UX-20 + CON X + **default**): the separate visible "Search" label above the field is removed. The field's accessible name is set explicitly to a new key `search-field-label` = "Search the catalog". Its placeholder becomes a hint that is not a repeat of the label: `search-hint` = "Tracks, albums, artists, playlists". The nav item "Search" is unchanged.
- Q: Clear control shape and behaviour? → A (**default**, standard search-field convention): a Quiet-variant (015) button with a "×" glyph at the field's trailing edge, accessible name "Clear search" (`search-clear`), present only while `raw_query` is non-empty (including whitespace-only). Activating it (click, or Enter/Space when focused; it is in the tab order right after the field) sets the query to empty — resetting the session to idle exactly as the existing Escape path does — and returns focus to the field. Escape-in-field keeps its existing behaviour.
- Q: When is a query "in flight" and where is the indicator? → A (CODE + **default**): in flight = the debounce deadline is armed **or** the combined 4-kind request is outstanding (a new `SearchSession` read-only accessor). The indicator is a small egui spinner at the field's trailing edge (before the clear control), with accessible role progress indicator and name "Searching…" (`search-in-flight`). It appears on the same frame the effective query changes (so well within SC-003's 200 ms) and disappears on the frame the combined reply is applied. "Show more" requests do not show it; they keep their existing disabled "Show more" button. While a rate-limit retry is waiting (below) the indicator is not shown — the status says it instead.
- Q: What is the settled result count? → A (**default**): once no request for the current query is in flight and at least one group is `Loaded` or stale, a line below the field reads "<n> results" (`search-result-count`, plural-aware), n = sum of the header counts of the groups shown. It updates after "Show more". It is exposed as a polite live status (accessible role status) and announced once per settled query, not on every "Show more". Not shown while idle, while in flight, offline, or in the no-results state (that message is its own announcement).
- Q: Stale results across a *new* query? → A (BD "keeps showing the previous results" + 004 FR-015 "whatever results are currently on screen" + CODE): "previous results" means rows of the **current** query already on screen when a later request for that query is rate-limited — today a rate-limited "Show more", and the retry below. Results of an earlier, different query are never shown as results of the new one (a new query still resets groups to skeletons, 004 FR-016). A new query whose first request is rate-limited has no previous results → the no-stale edge case.
- Q: Is a refresh really pending? → A (004 FR-015 "until the backed-off retry of the newest query succeeds" + CODE gap): yes — the status may only claim a pending refresh if one is scheduled. A rate-limited search request (the combined request, or a "Show more" page) is retried automatically for the same query generation with the same backoff policy as library sync (`backoff_delay`: honour `retry_after_ms` when given, else exponential from `SYNC_BACKOFF_BASE`, capped at `SYNC_BACKOFF_MAX`). A query edit, clear, or going offline cancels the pending retry (generation poisoning, as today). On success the retried page replaces (combined) or is appended to (Show more) the stale rows, the group returns to `Loaded` with its `next_offset`, and the status disappears. This is completion of 004 FR-015, not a new catalog behaviour.
- Q: How is the rate-limit status styled and worded? → A (019 severity mapping + 004 "never an error" + **default**): an inline status strip above the results scroll area (not a toast; nothing is raised to the notification centre): `surface_raised` fill, `radius::SM`, the Warning severity glyph "⚠" in the `warning` role colour, text in `text_primary`; accessible role status. With stale rows: "Showing earlier results — search is busy, refreshing shortly" (`search-stale`). Without stale rows: "Search is busy — retrying shortly" (`search-rate-limited`). The old bare "Refreshing…" label is removed from Search (the `refreshing` key stays for Library). High contrast (017) needs no special casing (theme roles only).
- Q: Empty state — how is the query named and cleared? → A (CODE + **default**): existing `search-no-results` copy (names the query in quotes) followed by a "Clear search" button (reuses `search-clear`, Secondary variant) that behaves exactly like the field's clear control. The quoted query is truncated to its first 60 characters (Unicode scalar values) plus "…" when longer; the message wraps within the existing 72-character body measure and never widens the view.
- Q: Non-rate-limit failures, offline? → A (CODE, 004 FR-018): unchanged — other errors resolve to `Empty` (and thus the no-results state when all groups are empty); offline shows the existing offline message and no field indicator, count or status.
- Q: Strings? → A (CON X): every new string is a Fluent key shipped in en-US and pt-BR: `search-field-label`, `search-hint`, `search-clear`, `search-in-flight`, `search-result-count`, `search-group-header` (accessible "<Group>, <n> results"), `search-stale`, `search-rate-limited`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Scroll search results as one page (Priority: P1)

A listener searches and gets results in tracks, albums, artists and playlists. Today each group has its own capped scroll area inside the page's scroll area, so at smaller window sizes only tracks are visible and the other groups are hidden until the listener happens to scroll the right container. The results become one page that scrolls with a single gesture. Groups appear in a fixed order (tracks, albums, artists, playlists), each under a section header that stays pinned while its group scrolls past. The header shows the group name and how many results are shown.

**Why this priority**: Hidden result groups are the core defect; without this the search view fails its purpose at small sizes.

**Independent Test**: Run a query returning results in all four groups at the smallest supported window size; scroll once and confirm the playlists group is reached, with each header visible while its results scroll.

**Acceptance Scenarios**:

1. **Given** a query with results in all four groups, **When** the listener scrolls the page with one gesture, **Then** every group, including playlists, is reached without interacting with any other scroll area.
2. **Given** a group taller than the viewport, **When** the listener scrolls within it, **Then** that group's header stays pinned until the group has scrolled past, then the next header takes over.
3. **Given** any group, **When** it is displayed, **Then** its header shows the group name and the count of results shown.
4. **Given** a group with a "show more" control, **When** the listener uses it, **Then** it behaves as before and sits at the end of its group.
5. **Given** any window width from 960 px upward, **When** results show, **Then** groups are stacked in a single column in one results scroll area (no column layout in this feature).
6. **Given** a group with no results (or unsupported), **When** results show, **Then** that group is omitted entirely (no header) and the remaining groups keep their fixed relative order.

---

### User Story 2 - Clear feedback on the query field (Priority: P2)

The search field is labelled once (not three times), shows a clear control while it holds text, shows a progress indicator near the field while a query is in flight, and reports the result count when the query settles.

**Why this priority**: Removes uncertainty about whether a query is running or finished, but results remain usable without it.

**Independent Test**: Type a query, observe the indicator during flight and the count on settle; use clear to empty the field.

**Acceptance Scenarios**:

1. **Given** an empty field, **When** viewed, **Then** there is no separate visible "Search" label above it, its accessible name is "Search the catalog", its placeholder reads "Tracks, albums, artists, playlists", and no clear control is shown.
2. **Given** text in the field, **When** viewed, **Then** a "Clear search" control is shown at the field's trailing edge; activating it (mouse or keyboard) empties the field, resets results to idle and returns focus to the field.
3. **Given** the effective query has just changed (debounce armed or combined request outstanding), **When** viewed, **Then** a spinner named "Searching…" is visible at the field's trailing edge, and it disappears when the reply is applied.
4. **Given** a query has settled with results, **When** viewed, **Then** a line reads "<n> results" where n equals the sum of the header counts, exposed as a live status and announced once per settled query.

---

### User Story 3 - Clear stale and empty states (Priority: P3)

When the service rate-limits a query, the previous results stay on screen under a status that plainly says the results are stale and a refresh is pending, styled as a status rather than plain text. When a query returns nothing, the view names the query and offers to clear it.

**Why this priority**: Improves clarity in less common states.

**Independent Test**: Simulate a rate-limited response and an empty result; confirm the status and the empty-state message with clear action.

**Acceptance Scenarios**:

1. **Given** results for the current query are shown, **When** the service rate-limits a further request for that query (e.g. "Show more"), **Then** those rows remain visible under a status strip (warning glyph, raised surface, role status) reading "Showing earlier results — search is busy, refreshing shortly", and an automatic backed-off retry is scheduled.
2. **Given** a stale status, **When** the scheduled retry succeeds, **Then** the status disappears, the group returns to its normal loaded state (with "Show more" if a further page exists), and the retried page replaces or extends the stale rows.
3. **Given** a query returning no results, **When** viewed, **Then** the existing no-results message names the query (truncated to 60 characters + "…" if longer) followed by a "Clear search" button that empties the field and focuses it.

---

### Edge Cases

- Rate-limited with no previous results (e.g. a new query's first request): the status reads "Search is busy — retrying shortly", no groups are drawn, no stale claim is made; the retry is scheduled. Results of an earlier different query are never shown.
- Query changes while another is in flight or a retry is pending: the indicator persists, the old request's reply and any pending retry are discarded, and only the latest query's outcome is reported.
- Very long query text in the empty-state message: the quoted query is cut to 60 characters + "…" and the message wraps within the 72-character body measure.
- Groups with a single result or very many results: header count stays accurate, including after "show more".
- Window resized at any width: layout stays single-column; the pinned header remains the one for the group at the viewport top.
- Query failing for reasons other than rate limiting: existing handling unchanged (groups resolve to empty → no-results state); no status strip.
- Offline: existing offline message only; no indicator, count or status strip.
- Group still loading (`Pending`): header shows the name without a count, above 3 skeleton rows.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The search view MUST present all result groups within one vertical results scroll area, with no nested independently scrolling group areas; the query field row, result-count line and status strip sit fixed above it. Only rows intersecting the viewport (± one row) MUST be laid out per frame. The scroll area MUST return to the top when the effective query changes.
- **FR-002**: Groups MUST appear in fixed order: tracks, albums, artists, playlists; `Empty`/`Unsupported` groups and rate-limited groups with nothing stale MUST be omitted entirely.
- **FR-003**: Each shown group MUST have a section header that, once scrolled above the viewport top while its rows remain in view, is drawn pinned at the viewport top, and is pushed out by the next group's header; at most one header is pinned at a time.
- **FR-004**: Each header MUST show the group name and the number of rows currently rendered in that group (loaded + appended + stale), with accessible name "<Group>, <n> results"; a `Pending` group's header shows the name only.
- **FR-005**: The per-group "show more" control MUST keep its current behaviour and sit at the end of its group.
- **FR-006**: The view MUST use a single-column stacked layout at every window width; multi-column arrangement is out of scope for this feature.
- **FR-007**: The separate visible label above the search field MUST be removed; the field MUST have exactly one accessible name, "Search the catalog", and a placeholder hint "Tracks, albums, artists, playlists".
- **FR-008**: The field MUST show a Quiet "Clear search" control at its trailing edge only while it holds any text; it MUST be keyboard-reachable directly after the field, and activating it MUST empty the query (resetting results to idle, as Escape does) and return focus to the field.
- **FR-009**: A spinner with accessible name "Searching…" MUST be visible at the field's trailing edge while the debounce is armed or the combined request for the current query is outstanding, and only then (not for "Show more", not while waiting for a rate-limit retry).
- **FR-010**: When a query settles with at least one shown group, the view MUST show "<n> results" (n = sum of header counts, updated after "Show more") as a polite live status announced once per settled query; it MUST NOT show while idle, in flight, offline or in the no-results state.
- **FR-011**: When a request for the current query is rate-limited, the view MUST keep that query's on-screen rows and show an inline status strip (warning glyph in the `warning` role colour, `surface_raised` fill, role status; never a toast or error) reading "Showing earlier results — search is busy, refreshing shortly", or "Search is busy — retrying shortly" when nothing stale exists. Rows of a different, earlier query MUST NOT be shown.
- **FR-011a**: A rate-limited search request MUST be retried automatically for the same query generation using library sync's backoff policy (`retry_after_ms` when given, else exponential base/cap as `backoff_delay`); a query edit, clear or going offline MUST cancel it. On success the status MUST disappear and the group MUST return to its loaded state including its next-page availability (completes 004 FR-015).
- **FR-012**: When a query returns nothing, the view MUST show the existing no-results message naming the query (truncated to 60 characters + "…") followed by a "Clear search" button behaving exactly as FR-008's control.
- **FR-012a**: Every new string MUST be a Fluent key shipped in en-US and pt-BR (constitution NFR-7.1); every new control MUST have an accessible name and be keyboard-operable (constitution NFR-6.1, NFR-6.2).
- **FR-013**: Catalog requests, page size, debounce, "Show more" behaviour, row actions, offline handling and non-rate-limit error handling MUST remain unchanged (FR-011a's retry being the only search-state addition, plus read-only in-flight accessors).

### Key Entities

- **Result group**: One of tracks, albums, artists, playlists; has a name, a shown-result count and an optional "show more" state.
- **Search status**: Idle, in flight (debounce armed or combined request outstanding), settled with count, rate-limited (with or without stale rows of the current query, retry scheduled), empty (named query), or offline.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: At 960 × 640, for a query with results in all four groups, continuous scrolling of the single results area alone reaches the playlists group; no other scroll area exists in the view.
- **SC-002**: 100% of the time a group scrolls, its header remains visible until that group has passed.
- **SC-003**: The in-flight spinner is visible on the first frame after the effective query changes (≤ 200 ms) and gone on the frame the combined reply is applied.
- **SC-004**: In a rate-limited state, the current query's previous rows stay visible, the status strip (role status, warning glyph) is present, and a retry fires after the backoff delay; on success the strip disappears.
- **SC-005**: For an empty result, a listener can clear the query in one action from the empty-state message.
- **SC-006**: Every result count in a header matches the number of rows shown in that group.

## Assumptions

- Search, paging and "show more" from the existing search feature (004) and row/tab/card components from the list-row and panel components feature (016) are reused.
- The four groups and their order are fixed; users cannot reorder them.
- No column layout; smallest window is 960 × 640 (`main.rs`).
- The status strip reuses 019's severity glyph/colour mapping (Warning) inline; nothing is raised to the notification centre.
- Offline search results are out of scope and handled by their own feature.

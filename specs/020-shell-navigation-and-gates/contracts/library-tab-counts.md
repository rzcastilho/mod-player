# Contract: Library Tab Counts (regression only)

**Covers**: FR-013, FR-014 · SC-006 · US4
**Module**: `crates/modplayer-ui/src/library_view.rs` — **no production change** (behaviour
delivered by 016-list-row-and-panel-components FR-014/FR-015, `specs/016-list-row-and-panel-components/contracts/tab-strip.md` T4–T8)
**Tests**: `crates/modplayer-ui/tests/library_view.rs` (existing + additions below),
`crates/modplayer-ui/tests/shell_navigation.rs`

| # | Clause | Status |
|---|---|---|
| L1 | While `library_status().loading`, no tab has a count node. | existing (`no_tab_shows_a_count_while_loading`) — must keep passing |
| L2 | Once loaded, all five tabs show their count in `mono`, including `0`, without selecting the tab; the count is a sibling node, not part of the tab's accessible name. | existing (T4/T6 tests) — must keep passing |
| L3 | After the in-memory list changes (e.g. Saved albums 0 → 2 via a sync page), the next drawn frame shows `2` beside Saved albums. | **new** test if not already asserted |
| L4 | The navigation rail draws no count badge (Clarification 14). | **new**, in `shell_navigation.rs` (C5) |

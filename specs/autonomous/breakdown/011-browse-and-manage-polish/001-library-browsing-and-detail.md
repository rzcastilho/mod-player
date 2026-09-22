# 011-browse-and-manage-polish / 001 — Library Browsing and Detail View

**Source:** [§ 3.3 Library](../../ModPlayer-UI-UX-Review.md#33-library) (UX-14, UX-17), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules), [§ 4.5 Feedback and affordance](../../ModPlayer-UI-UX-Review.md#45-feedback-and-affordance)

**Prerequisites:** Uses the row and tab components from 008-design-foundation/003-list-row-and-panel-components; assumes the library and detail views from 001-mvp/004-search-and-library-browse.

## Prompt

> Finish the browsing experience the shared row component started: make a detail page look like the thing it describes, and make every list action reachable from the row it belongs to.

> An album, playlist or artist page opens with a header that carries the artwork at a generous size, the title in the display style, and the supporting facts — owner, track count, total running time — as one secondary line rather than three equal-weight stacked lines. The header carries the page's primary action, playing the collection from the start, alongside the collection-level actions the overflow menu already offers. Returning to the library is a clearly labelled back control in the header, and the library's previous scroll position is restored when the user goes back.

> In the lists themselves, the row's overflow menu stays anchored to its row at every window width, and the six row actions keep their existing behaviour while adopting quiet styling and a keyboard path. Loading skeletons match the shape of the rows they are replacing, so the list does not reflow when real data arrives, and the existing empty states keep their explanatory sentence and their single primary action.

> Acceptance: when a playlist is opened, its artwork, title and a single line of facts appear in the header along with a play action. When the back control is used, the library is restored at the scroll position it had. When the window is at the minimum width, the overflow control is still inside its own row. When a list is loading, the skeleton rows occupy the same height as the loaded rows.

## Scope boundary

Covers the library lists' remaining affordances and the detail page's layout; library sorting stays with 007-operations-and-polish/005-playback-polish, and offline pinning indicators with 003-offline-and-library/001-offline-cache-and-pinning.

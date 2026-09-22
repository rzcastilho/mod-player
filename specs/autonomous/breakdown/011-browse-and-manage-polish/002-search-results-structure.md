# 011-browse-and-manage-polish / 002 — Search Results Structure and Feedback

**Source:** [§ 3.4 Search](../../ModPlayer-UI-UX-Review.md#34-search) (UX-18, UX-19, UX-20), [§ 3.10 States reviewed from source](../../ModPlayer-UI-UX-Review.md#310-states-reviewed-from-source-not-captured), [§ 4.5 Feedback and affordance](../../ModPlayer-UI-UX-Review.md#45-feedback-and-affordance)

**Prerequisites:** Assumes search from 001-mvp/004-search-and-library-browse and the row, tab and card components from 008-design-foundation/003-list-row-and-panel-components.

## Prompt

> Let a search result page be scrolled like a page. Each of the four result groups currently has its own capped scroll area nested inside the page's scroll area, so at the app's smaller sizes a user sees tracks only and has to discover, by scrolling the right container, that albums, artists and playlists were there all along.

> Replace the nested scrolling with a single page scroll. The four groups — tracks, albums, artists, playlists — appear in a fixed order under section headers that stay pinned while their group scrolls past, each header carrying the group's name and how many results are shown. The per-group "show more" control keeps its behaviour and its position at the end of its group. When the window is wide enough, the groups may share the width in columns rather than stacking.

> Give the query itself the feedback it lacks: the field is labelled once rather than three times over, carries a clear control while it holds text, shows a progress indicator while a query is in flight, and reports the result count when the query settles. The rate-limited state keeps showing the previous results but says plainly that they are stale and a refresh is pending, styled as a status rather than as another anonymous line of text. An empty result set says which query returned nothing and offers to clear it.

> Acceptance: when a query returns results in all four groups, one scroll gesture reaches the playlists group and each group header stays visible while its own results scroll. When a query is in flight, a progress indicator is visible near the field. When the service rate-limits the query, the previous results stay on screen under a clearly marked stale-results status. When a query returns nothing, the view names the query and offers to clear it.

## Scope boundary

Covers the search view's structure and status feedback; catalog behaviour, paging and offline search results remain with 001-mvp/004-search-and-library-browse and 003-offline-and-library/002-offline-session-and-playback.

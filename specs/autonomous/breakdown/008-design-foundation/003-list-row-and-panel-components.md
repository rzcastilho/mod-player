# 008-design-foundation / 003 — List Row, Tab, and Panel Card Components

**Source:** [§ 3.3 Library](../../ModPlayer-UI-UX-Review.md#33-library) (UX-13, UX-14, UX-15, UX-16), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules), [§ 4.5 Feedback and affordance](../../ModPlayer-UI-UX-Review.md#45-feedback-and-affordance), [§ 3.5 Now Playing](../../ModPlayer-UI-UX-Review.md#35-now-playing) (UX-21)

**Prerequisites:** Builds on 008-design-foundation/001-design-tokens-and-type-scale and 002-control-variants. The row component is consumed by the library, search, queue and detail views that 010 and 011 restructure.

## Prompt

> Turn the three repeating building blocks of the app — the list row, the tab strip and the panel — into real components, so every list in ModPlayer becomes scannable at a glance.

> A list row is a fixed three-column grid: artwork or an initials placeholder on the left at a consistent size, a title above a secondary line in the middle, and a right-aligned column holding the item's duration in tabular figures and the overflow menu. Today all of that is concatenated into one wrapped sentence — "Justin Timberlake — TROLLS (Original Motion Picture Soundtrack) — 3:57" — with the duration buried mid-line and the menu button stranded hundreds of pixels from the title it belongs to. Titles truncate with an ellipsis rather than wrapping, the whole row highlights on hover, and the row exposes how to open it: a single click selects, a double click or Enter opens, and the row's tooltip says so, because today a single click on a playlist is silently discarded and nothing in the interface reveals that a double click is required.

> A tab strip becomes an underlined tab row that reads as navigation rather than a row of buttons, and shows a count beside each tab when the count is known. A panel becomes a card on the raised surface with a small uppercase header, generous padding and a rounded corner, collapsible with its open or closed state remembered per panel — replacing the identical hairline rules that currently separate every block in the now-playing view.

> Acceptance: when a library list is displayed, every duration is right-aligned in the same column and every title truncates at the same edge. When the pointer moves over a row, the row highlights and its overflow control is reachable without leaving the row. When a playlist row is clicked once, it becomes selected; when it is clicked twice or Enter is pressed, the playlist opens. When a panel is collapsed and the app is restarted, the panel is still collapsed.

## Scope boundary

Delivers the components and adopts them in the existing lists, tab strips and panels; it does not change which lists exist, how search groups are arranged (011-browse-and-manage-polish/002), or how the now-playing view is ordered (010-now-playing-workbench/001).

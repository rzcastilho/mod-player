# 009-window-and-shell / 003 — Shell Navigation and Launch Gates

**Source:** [§ 3.1 First launch](../../ModPlayer-UI-UX-Review.md#31-first-launch) (UX-06), [§ 3.3 Library](../../ModPlayer-UI-UX-Review.md#33-library) (UX-16), [§ 3.7 Settings](../../ModPlayer-UI-UX-Review.md#37-settings) (UX-31), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules), [§ 4.3 Layout and responsiveness](../../ModPlayer-UI-UX-Review.md#43-layout-and-responsiveness)

**Prerequisites:** Uses the tab and card components from 008-design-foundation/003-list-row-and-panel-components; assumes the launch flow from 001-mvp/002-first-launch-and-sign-in.

## Prompt

> Make the app's navigation honest: never show a control that does nothing, and never let a set of categories reflow into an unreadable pile.

> During the launch gates — the disclosure, sign-in and the audio device check — the five-item navigation rail is currently drawn and clickable, but clicking any item does nothing because the gate re-renders. A first-time user therefore meets five dead controls before meeting a live one. Hide the rail until the user reaches the main application, and give the gates the full window with a simple step indicator so the user knows how many steps remain.

> Inside the main application, give the rail a clear selected state that reads as navigation rather than a button, and keep each section's scroll position when the user returns to it. The settings categories — eleven of them — currently wrap into a second row at the app's smaller sizes, with the wrap point moving as translations lengthen. Keep them as a single horizontal row that never wraps: categories that do not fit collapse into an overflow control that lists the remainder, the selected category is always visible in the row, and the row remains keyboard-navigable end to end. Library tabs gain the count badges the tab component supports, so a user can see there are no saved albums before selecting that tab.

> Acceptance: when the app starts with no account configured, no navigation rail is visible until the main application is reached. When the settings screen is shown at the minimum window width, the category row occupies exactly one line and the categories that do not fit are reachable from an overflow control. When a category name is lengthened by 40 %, the row still occupies one line. When the user leaves the library scrolled halfway and returns from search, the scroll position is preserved.

## Scope boundary

Covers rail visibility, section navigation and the settings category row; the content of each settings category is restructured in 011-browse-and-manage-polish/004, and onboarding copy and hierarchy in 011-browse-and-manage-polish/005.

## Open questions

- The review proposed converting settings categories into a left sidebar; the chosen direction here is to keep a single non-wrapping row with an overflow control (source § 7, open question 3).

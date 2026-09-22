# 009-window-and-shell / 002 — Notification Placement, Severity, and Message Quality

**Source:** [§ 3.8 Notifications](../../ModPlayer-UI-UX-Review.md#38-notifications) (UX-05, UX-37, UX-38), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules), [§ 1 Summary](../../ModPlayer-UI-UX-Review.md#1-summary)

**Prerequisites:** Assumes the notification area from 001-mvp/001-walking-skeleton and plugin-raised notifications from 001-mvp/011-plugin-ui-contributions; uses the severity colours from 008-design-foundation/001-design-tokens-and-type-scale.

## Prompt

> Let notifications inform without hiding the thing the user is working on. The stack currently floats over the top-right of the central area, where it covers the library tab strip, the settings categories and the plugin table's headers, and three stacked messages hide the first rows of any list.

> Move the stack to the bottom-right, out of the path of every header and tab strip in the app, and cap it at three visible notifications with a compact "N more" affordance that expands the rest. Severity becomes legible at a glance: an accent bar in the positive, warning or danger colour plus an icon and the severity word, so a critical plugin failure and a routine informational message are no longer the same grey card. Informational notifications continue to dismiss themselves; warnings and critical messages stay until dismissed or acted on.

> Rewrite the messages themselves so they read as guidance rather than log output. The audio-device notification today reads as a raw system identifier — roughly 120 characters of device string — shown to a musician mid-practice; it becomes a short sentence naming the device in human terms and saying what the app did instead, with the original technical string available behind a "Details" control. Every notification is truncated to two lines in the stack, with the full text available on expansion, and keeps its existing action button where one applies.

> Acceptance: when three notifications are raised while the library is open, the tab strip and the first list row remain fully visible. When a fourth notification arrives, the stack shows three plus a "1 more" control that expands to reveal it. When the output device becomes unavailable, the notification is at most two lines and the raw device identifier appears only after expanding Details. When a critical plugin notification and an informational one are shown together, they are distinguishable without reading the words.

## Scope boundary

Covers the presentation, placement and wording of notifications; it does not change which events raise notifications, nor the Performance Mode suppression rules from 004-performance/003-performance-mode.

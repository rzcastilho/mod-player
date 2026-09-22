# 009-window-and-shell / 001 — Window Sizing and Responsive Plugin Dock

**Source:** [§ 3.5 Now Playing](../../ModPlayer-UI-UX-Review.md#35-now-playing) (UX-01), [§ 4.3 Layout and responsiveness](../../ModPlayer-UI-UX-Review.md#43-layout-and-responsiveness), [§ 5.5 Layout rules](../../ModPlayer-UI-UX-Review.md#55-layout-rules), [§ 1 Summary](../../ModPlayer-UI-UX-Review.md#1-summary), [§ 4.6 Accessibility versus the spec](../../ModPlayer-UI-UX-Review.md#46-accessibility-versus-the-spec) (NFR-7.4 row)

**Prerequisites:** Assumes the plugin dock from 001-mvp/011-plugin-ui-contributions and the tokens from 008-design-foundation/001-design-tokens-and-type-scale.

## Prompt

> Stop the app from opening at a size it cannot render correctly. ModPlayer currently launches into a window small enough that its own plugin dock clips its controls — "Close" renders as "Cl…", a plugin's status line ends mid-word, and the host's transport controls collide with the dock's edge.

> Give the window a deliberate initial size of roughly 1200 × 820 and a minimum of 960 × 640, so no layout is ever asked to render below the width it was designed for. Make the plugin dock responsive instead of a fixed column: a preferred width with a smaller floor, resizable by dragging its inner edge, with the chosen width remembered between sessions. Below a defined window width the dock hides itself automatically and its panels stay reachable through a control in the transport bar, so a narrow window loses no capability. Panel headers inside the dock truncate gracefully with the full text in a tooltip, and no header button label is ever the thing that gets cut.

> Treat the remaining hard-coded dimensions the same way: waveform overview and detail heights become proportional to the available content height with sensible minimums, so a taller window gives more waveform rather than more empty space. The whole layout must survive translated strings roughly 40 % longer than English without truncating a control label.

> Acceptance: when the app is launched for the first time, the window opens at the defined initial size. When the window is dragged to the minimum size with two plugin panels docked, every panel header button is fully readable. When the window is narrowed below the dock's threshold, the dock hides and the transport bar exposes a control that brings the panels back. When the dock is resized and the app is restarted, the dock keeps its width. When the window is made taller, the waveform grows.

## Scope boundary

Covers window geometry, the dock's responsiveness and proportional waveform sizing; it does not reorder the now-playing content (010-now-playing-workbench/001) or change what the waveform draws (010-now-playing-workbench/002).

## Open questions

- Whether the dock should be freely drag-resizable or snap between two fixed widths (source § 7, open question 2). The slice assumes drag-resizable with a remembered width.

# Feature Specification: Window Sizing and Responsive Plugin Dock

**Feature Branch**: `feature/018-window-sizing-and-responsive-dock`

**Created**: 2026-09-24

**Status**: Draft (clarified 2026-09-24)

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/009-window-and-shell/001-window-sizing-and-responsive-dock.md (id 001, window-sizing-and-responsive-dock)."

**Source breakdown**: `specs/autonomous/breakdown/009-window-and-shell/001-window-sizing-and-responsive-dock.md`

**Source review**: `specs/autonomous/ModPlayer-UI-UX-Review.md` §1 (UX-01), §3.5 (UX-01, UX-23), §4.3, §4.6 (NFR-7.4 row), §5.5 Layout rules

**Requirement IDs implemented**: NFR-7.4 (40 % text expansion), NFR-6.1 (keyboard operable), NFR-6.2 (accessible names), NFR-7.1 (externalized strings); review findings UX-01, UX-23 (heights only).

## Clarifications

### Session 2026-09-24

Resolved by the clarify reviewer. "Derived" items cite their source; "Default" items are assumed conventional defaults (no source settles them) and are binding on plan/tasks unless the maintainer overrides.

- Q: What are the exact window sizes? → A: **Derived** (breakdown prompt; review §5.5): initial inner size exactly **1200 × 820** logical points; minimum inner size exactly **960 × 640** logical points. "Approximately" in the breakdown refers to OS rounding only (tolerance ±1 logical point per dimension).
- Q: What are the dock's preferred width, minimum width and auto-hide threshold? → A: **Derived** (review §5.5): preferred (default) width **280** logical points; minimum width **240**; the dock auto-hides when the window's inner width is **< 1024** logical points and shows docked when **≥ 1024**. No hysteresis band (**Default** — simplest, deterministic, testable).
- Q: Drag-resizable or snap between two widths? (breakdown open question; review §7 Q2) → A: **Derived** (breakdown slice assumption + review §5.5 "resizable by drag"): continuously drag-resizable, width remembered. No snap widths.
- Q: What is the dock's maximum width? → A: **Default**: stored width is clamped to **[240, 480]** logical points. At render time the effective width is additionally limited so the Now Playing host content column keeps at least **560** logical points: `effective = clamp(stored, 240, max(240, min(480, windowInnerWidth − navRailWidth − 560)))`. Rationale: the review (§3.5) identifies a ~470 px host column as "crowded"; 560 keeps the host column above that at the 1024 threshold while still allowing a generous dock on wide windows. The render-time limit never rewrites the stored width — widening the window restores the user's chosen width.
- Q: The minimum window (960) is below the auto-hide threshold (1024) — how does the acceptance "at minimum size with two plugin panels docked, every panel header button is fully readable" hold? → A: **Derived** from the two §5.5 numbers together: at 960 × 640 the dock is auto-hidden, so the acceptance is verified by opening the dock through the transport bar "Panels" control at 960 × 640 with two docked panels and checking every header button in that overlay. Independently, header readability is also verified with the dock docked at its **240** minimum width (window ≥ 1024). Both checks are required (SC-002).
- Q: What is the transport-bar control called and when is it shown? → A: **Derived** (review §5.5): a toggle labelled **"Panels"** (new externalized string, NFR-7.1) placed in the Now Playing transport row alongside the existing transport buttons/switches. **Default**: it is shown **only** while the dock is auto-hidden **and** at least one panel is docked (placement = Docked, visible); with zero docked panels it is absent (the dock itself draws nothing when empty today). It is a standard focusable toggle with an accessible name and pressed state (NFR-6.1/6.2); no new global keyboard shortcut is added.
- Q: How does the dock appear when toggled open while the window is still narrow? → A: **Default**: as an **overlay** anchored to the right edge of the Now Playing content area, full content height, at the effective dock width computed against the current window (never narrower than 240), drawn above the host content without reflowing it. It is dismissed by pressing the "Panels" toggle again, pressing **Esc** while focus is inside the overlay, or when the last docked panel is closed/floated. Clicking outside does **not** dismiss it (so the user can operate the transport while panels are shown). Overlay-open state is session-only (never persisted); widening to ≥ 1024 closes the overlay and shows the normal docked dock; narrowing again starts in the hidden state.
- Q: Do floated panels auto-hide too? → A: **Default**: no. Auto-hide applies only to the docked column; floated panel windows are unaffected (they are already position-independent overlays per 011-plugin-ui-contributions L2).
- Q: Is the dock width keyboard-adjustable? → A: **Default** (Constitution X / NFR-6.1 "every host function is keyboard-operable"): the dock's resize edge is a focusable splitter with an accessible name ("Resize plugin dock" — externalized) and value; **←/→** change width by **16** logical points (← widens, since the dock is on the right edge), **Home/End** jump to min/max. Keyboard and drag changes persist identically.
- Q: Is the window size persisted, and what exactly? → A: **Default** (conventional desktop behavior; the breakdown's "first launch" wording implies later launches may differ): only the window's **inner size** (width × height, logical points) is persisted. Position, maximized and fullscreen state are **not** persisted (the OS places the window). On restore the size is clamped up to 960 × 640 (FR-014); the app applies no upper clamp (the OS constrains oversize windows). If the size was last seen while maximized/fullscreen, the last non-maximized size is what is saved.
- Q: Where and when are window size and dock width persisted? → A: **Default** (spec Assumptions: existing mechanism, no new persistence technology; Constitution X): as a new optional `[window]` section in the existing `settings.toml` (fields: inner width, inner height, dock width), additive — a file without it loads the defaults (1200 × 820, dock 280), matching how `[now_playing_panels]` was added. Dock width is saved when a drag or keyboard resize ends; window size is saved when a resize settles (debounced, at most one write per 500 ms) and on clean exit. Unparseable/non-finite/non-positive values are treated as absent (defaults used), never a load failure. State serialization for the new section is covered by property tests (Constitution VIII).
- Q: What exactly is "content height" for waveform sizing, and what are the proportions? → A: **Derived** (review §5.5): overview height = `max(64, round(0.08 × H))`, detail height = `max(120, round(0.22 × H))`, where **H** is the Now Playing screen's available content height (the central content area's inner height, excluding window chrome and the nav rail; independent of scroll position). **Default**: no maximum height. Replaces the fixed 72 / 120 constants (review §3.5 UX-23).
- Q: How do panel headers behave when space is short? (FR-010 and FR-011 previously contradicted each other on whether "Close" may truncate) → A: **Default** (breakdown: "no header button label is ever the thing that gets cut"): header **buttons** (Float/Dock, Close, Disable) are **never** truncated. The header's **title text** (plugin name · panel title, including plugin-supplied titles) is the only element that truncates, single-line with an ellipsis, and exposes the full text as a tooltip on hover and keyboard focus; its accessible name is always the full, untruncated text. When the buttons do not fit on the title's row at the current width, they wrap onto a second header row (and further rows if needed) rather than shrink. Host-drawn status/placeholder text inside the dock (e.g. "coming in a later update", suspended-cause lines) wraps to multiple lines instead of truncating.
- Q: How is the 40 % expansion requirement verified with only en-US shipping? → A: **Default** (NFR-7.4; review §4.6): an automated UI test renders the dock and transport row with a pseudo-localization that expands every host string in scope by 40 % (rounded up, minimum +1 character), at the dock's 240 minimum width and at the 960 × 640 overlay, asserting no button's rendered label is elided and no two interactive controls' rects overlap. Plugin-supplied titles are covered by a test using a 60-character title.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Window opens at a size that renders correctly (Priority: P1)

A user launches ModPlayer for the first time. The window opens large enough that every visible control — including the plugin dock's panel headers and the host transport controls — renders its full label, with no text truncated and no control overlapping another.

**Why this priority**: This is the root problem the feature exists to fix: the app currently launches too small to render its own chrome correctly. Nothing else in this feature matters if the first-run window is still broken.

**Independent Test**: Launch the app with a fresh config directory (no saved `[window]` state). Measure the window's opening inner size and visually confirm the plugin dock's panel headers, host transport controls, and other chrome render without clipped or overlapping text.

**Acceptance Scenarios**:

1. **Given** no previously saved window size, **When** the app is launched for the first time, **Then** the window opens at an inner size of 1200 × 820 logical points (±1 per dimension).
2. **Given** the app is running, **When** the user attempts to shrink the window below 960 × 640 logical points, **Then** the window stops shrinking at 960 wide / 640 tall and does not go smaller in either dimension.
3. **Given** the window is at 960 × 640 with two plugin panels docked (so the dock is auto-hidden), **When** the user opens the dock via the transport bar's "Panels" toggle and inspects each panel header, **Then** every header button label (Float/Dock, Close, Disable) is fully readable, not truncated.
4. **Given** the window is ≥ 1024 wide with two plugin panels docked and the dock at its 240 minimum width, **When** the user inspects each panel header, **Then** every header button label is fully readable, not truncated.

---

### User Story 2 - Plugin dock resizes and remembers its width (Priority: P1)

A user drags the inner edge of the plugin dock to make it narrower or wider to suit their workflow. The chosen width is kept for the rest of the session and is restored the next time the app is launched.

**Why this priority**: The dock is currently a fixed-width column that can clip its own contents regardless of window size; making it user-adjustable and persistent is the direct fix and is inseparable from the window-sizing fix (both are needed for the dock to never clip).

**Independent Test**: With the app running and at least one plugin panel docked, drag the dock's inner edge to a new width, confirm the dock and its panels reflow to that width, restart the app, and confirm the dock reopens at the same width.

**Acceptance Scenarios**:

1. **Given** the plugin dock is visible at its default width of 280, **When** the user drags the dock's inner edge, **Then** the dock resizes continuously between 240 and its effective maximum (see Clarifications: 480, further limited to keep ≥ 560 for host content) and its panels reflow to fit.
2. **Given** the user has resized the dock to a custom width, **When** the app is closed and relaunched, **Then** the dock reopens at that same custom width.
3. **Given** the user attempts to drag the dock narrower than 240, **When** the drag continues past that point, **Then** the dock stops at 240 rather than continuing to shrink.
4. **Given** the dock's resize edge has keyboard focus, **When** the user presses ←/→ or Home/End, **Then** the width changes by 16 points per press or jumps to min/max, and the new width persists as a drag would.

---

### User Story 3 - Narrow window keeps plugin panels reachable (Priority: P2)

A user resizes the window down to a narrow width — for example, to share half a screen with another app. Below 1024 logical points of window width, the plugin dock automatically hides to leave room for the core transport and now-playing content, but a "Panels" toggle in the transport bar lets the user bring the panels back on demand.

**Why this priority**: This preserves full capability at narrow widths without requiring the dock to be squeezed into an unreadable sliver; it depends on the dock resize/persistence behavior in User Story 2 already existing, so it is ordered after it, but it is still a primary flow named explicitly in the acceptance criteria.

**Independent Test**: With a plugin panel docked and visible, narrow the window below 1024, confirm the dock disappears and a "Panels" toggle appears in the transport bar, then use that control to reopen the dock and confirm the same panel is still available.

**Acceptance Scenarios**:

1. **Given** the window inner width is ≥ 1024 and the dock is visible with at least one docked panel, **When** the user narrows the window below 1024, **Then** the dock hides automatically and the "Panels" toggle appears in the transport bar.
2. **Given** the dock is auto-hidden, **When** the user activates the "Panels" toggle, **Then** the dock appears as an overlay over the right edge of the Now Playing content showing the same panels, in the same order, that were docked before it auto-hid.
3. **Given** the overlay is open, **When** the user presses the "Panels" toggle again or presses Esc with focus inside the overlay, **Then** the overlay closes and the dock returns to auto-hidden.
4. **Given** the dock is auto-hidden (overlay open or not), **When** the user widens the window to ≥ 1024, **Then** the overlay (if open) closes, the "Panels" toggle disappears, and the dock returns to its normal docked presentation at its remembered width.

---

### User Story 4 - Waveform height scales with window height (Priority: P3)

A user makes the window taller — for example, maximizing it on a large display. The waveform overview and detail views grow to use the additional vertical space instead of leaving it empty, while never shrinking below a usable minimum height when the window is short.

**Why this priority**: This is a visual-polish extension of the same "no hard-coded dimensions" principle as the window/dock work, but it is scoped separately from what the waveform draws or how its content is organized (explicitly out of scope for this feature), so it is lower priority than the window and dock fixes that address actual clipping.

**Independent Test**: With a track loaded and its waveform visible, resize the window taller and shorter and confirm the waveform overview and detail heights follow `max(64, 8 % of H)` and `max(120, 22 % of H)` without ever clipping or disappearing.

**Acceptance Scenarios**:

1. **Given** a track is loaded and its waveform is visible, **When** the user makes the window taller, **Then** the waveform overview and detail heights increase to `round(0.08 × H)` / `round(0.22 × H)` of the new content height H (once above their minimums).
2. **Given** a track is loaded and its waveform is visible, **When** the user shrinks the window toward 640 tall, **Then** the waveform heights shrink but never go below 64 (overview) and 120 (detail).

---

### Edge Cases

- **40 % longer strings** (translations or pseudo-localization) and long plugin-supplied titles: header buttons never truncate (they wrap to another header row); only the header title text truncates, with ellipsis and a full-text tooltip on hover and focus; host status/placeholder text in the dock wraps. No control overflows its container or overlaps another.
- **Dragging past the maximum**: the dock stops at its effective maximum (480, or less when needed to keep ≥ 560 for the host content column). The stored width is never rewritten by the render-time limit, so widening the window later restores the chosen width.
- **Crossing 1024 while the overlay is open** (e.g. mid window-drag): crossing to ≥ 1024 closes the overlay and shows the docked dock; crossing back below 1024 hides it (overlay closed). Focus that was inside the overlay/dock moves to the "Panels" toggle when the dock hides, or stays on the same widget when the dock becomes docked; no control is left focused but invisible.
- **First launch** (no `[window]` section): window 1200 × 820, dock 280.
- **Saved state below current minimums** (or above the dock's 480 maximum): window size clamped up to 960 × 640; dock width clamped into [240, 480].
- **Corrupt saved values** (non-numeric, non-finite, ≤ 0): treated as absent; defaults used; the rest of `settings.toml` still loads.
- **Zero docked panels while narrow**: the "Panels" toggle is not shown; nothing is lost since no docked panel exists. If the last docked panel is closed or floated while the overlay is open, the overlay closes and the toggle disappears.
- **Floated panels while narrow**: unaffected by auto-hide.
- **Window taller than tall**: waveform heights keep growing with H (no maximum).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The application MUST open its main window at an inner size of 1200 × 820 logical points when no saved window size exists (first launch, or cleared app data).
- **FR-002**: The application MUST enforce a minimum main window inner size of 960 × 640 logical points; the user MUST NOT be able to resize the window smaller in either dimension.
- **FR-003**: The application MUST persist the window's inner size (not position, maximized or fullscreen state) in a new optional `[window]` section of the existing `settings.toml`, saving when a resize settles (debounced, ≤ 1 write per 500 ms) and on clean exit, and MUST restore it on the next launch subject to FR-014. When the window is maximized/fullscreen, the last non-maximized size is what is persisted.
- **FR-004**: The plugin dock MUST have a default width of 280, a minimum width of 240 and a stored maximum of 480 logical points, and MUST be resizable by dragging its inner (left) edge continuously between 240 and its effective maximum, where effective maximum = `max(240, min(480, windowInnerWidth − navRailWidth − 560))`. The render-time limit MUST NOT overwrite the stored width.
- **FR-004a**: The dock's resize edge MUST be keyboard-focusable with an externalized accessible name ("Resize plugin dock") and current value; ←/→ MUST change the width by 16 logical points (← widens), Home/End MUST jump to the minimum/effective maximum.
- **FR-005**: The application MUST persist the dock width in the `[window]` section when a drag or keyboard resize ends, and restore it on the next launch clamped into [240, 480].
- **FR-006**: The docked dock MUST be hidden whenever the window's inner width is < 1024 logical points and shown docked whenever it is ≥ 1024 (and at least one panel is docked), with no hysteresis.
- **FR-007**: While the dock is auto-hidden and at least one panel is docked, the Now Playing transport row MUST show a "Panels" toggle (externalized string, focusable, with accessible name and pressed state). When no panel is docked, or the window is ≥ 1024, the toggle MUST NOT be shown.
- **FR-008**: Activating the "Panels" toggle while the window is < 1024 MUST show the dock as an overlay anchored to the right edge of the Now Playing content area (full content height, effective dock width, never below 240), drawn above host content without reflowing it, containing exactly the docked panels in their existing order and state. The overlay MUST close on: the toggle pressed again; Esc with focus inside the overlay; the last docked panel being closed or floated; the window widening to ≥ 1024. It MUST NOT close on outside clicks. Overlay-open state MUST NOT be persisted.
- **FR-009**: When the window is widened to ≥ 1024, the dock MUST return to its normal docked presentation at its remembered (stored) width, subject to the FR-004 effective maximum.
- **FR-010**: In every panel header, the title text (plugin name · panel title, including plugin-supplied titles) MUST render on a single line and truncate with an ellipsis when space is insufficient, MUST expose the full text as a tooltip on hover and on keyboard focus, and MUST keep the full, untruncated text as its accessible name.
- **FR-011**: Panel header buttons (Float/Dock, Close, Disable) MUST NEVER be truncated. When they do not fit beside the title at the current dock width, they MUST wrap onto additional header rows. This MUST hold at the 240 minimum dock width and in the overlay at a 960 × 640 window.
- **FR-011a**: Host-drawn status and placeholder text inside the dock (e.g. placeholder and suspended-cause lines) MUST wrap to multiple lines rather than truncate.
- **FR-012**: Waveform overview height MUST equal `max(64, round(0.08 × H))` and detail height `max(120, round(0.22 × H))` logical points, where H is the Now Playing central content area's available inner height; the fixed 72 / 120 constants MUST be removed. No maximum height applies.
- **FR-013**: All labels governed by this feature (dock/panel headers, header buttons, the "Panels" toggle, the transport row it joins) MUST remain unelided and non-overlapping when every in-scope host string is expanded by 40 % (NFR-7.4), verified by an automated pseudo-localization test at the 240-wide dock and at the 960 × 640 overlay.
- **FR-014**: On restore, a persisted window size smaller than 960 × 640 MUST be clamped up per dimension; a persisted dock width outside [240, 480] MUST be clamped into it; unparseable, non-finite or non-positive values MUST be treated as absent (defaults used) without failing the settings load.
- **FR-015**: All new user-facing strings ("Panels", "Resize plugin dock", tooltip text sources) MUST be externalized in the locale files (NFR-7.1).

### Key Entities

- **Window state** (`[window]` in `settings.toml`, optional, additive): persisted inner width and height (logical points) and dock width. Absent section or field ⇒ defaults (1200, 820, 280).
- **Plugin dock**: container of docked plugin panels at the right edge of Now Playing. Attributes: stored width (240–480, default 280), effective width (render-time, FR-004), auto-hide threshold (window inner width 1024), presentation mode — `Docked` (≥ 1024), `Hidden` (< 1024, overlay closed), `Overlay` (< 1024, opened via "Panels"; session-only).
- **Docked plugin panel**: a plugin panel with placement Docked; header = icon, truncating title (tooltip), non-truncating wrapping buttons (Float/Dock, Close, Disable); body reflows with dock width.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With a fresh config directory, 100 % of launches open the main window at 1200 × 820 logical points (±1 per dimension), with no dock header text truncated and no control overlapping another.
- **SC-002**: With two plugin panels docked, every header button label is fully readable (not elided) in 100 % of checks in both configurations: (a) window 960 × 640 with the dock opened as overlay; (b) window ≥ 1024 with the dock at 240.
- **SC-003**: The dock can be resized to any width in [240, effective maximum] by a single continuous drag, with its width tracking the pointer every frame.
- **SC-004**: A dock width (and window size) chosen by the user is restored exactly on relaunch, or clamped only per FR-014, in 100 % of relaunches.
- **SC-005**: When the window is narrowed below 1024 with ≥ 1 docked panel, the dock hides and the "Panels" toggle is present in the same frame; activating it shows the overlay with the same panels.
- **SC-006**: For content heights H across the reachable range, measured waveform heights equal `max(64, round(0.08H))` and `max(120, round(0.22H))` (±1), and increase when the window is made taller above the minimums.
- **SC-007**: Under 40 % pseudo-localization (and with a 60-character plugin panel title), no button label is elided and no two interactive controls' rects overlap, in the 240-wide dock and in the 960 × 640 overlay.

## Assumptions

- All sizes are logical points (egui/OS scale-independent units); device-pixel behavior under display scaling is handled by the platform.
- Numeric values for window, dock, threshold and waveform proportions come from the review's §5.5 Layout rules; the dock maximum (480), host-content floor (560), keyboard step (16) and debounce (500 ms) are reviewer defaults recorded in Clarifications.
- "First launch" means no persisted `[window]` state exists for the current profile; reinstalling or clearing app data is treated the same.
- The overlay presentation, its dismissal rules and the "Panels" toggle visibility rules are defaults recorded in Clarifications.
- This feature governs the dock container, panel header layout, window geometry and waveform heights only; it does not change how plugins lay out panel bodies, what the waveform draws (010-now-playing-workbench/002) or how now-playing content is ordered (010-now-playing-workbench/001). The "Panels" toggle is placed in the current transport row; if 010-now-playing-workbench/001 later introduces a dedicated transport bar, the toggle moves with it.
- Persistence reuses the existing `settings.toml` store via an additive optional section; no new persistence technology or crate is introduced (Constitution X).

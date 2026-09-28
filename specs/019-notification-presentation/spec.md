# Feature Specification: Notification Placement, Severity, and Message Quality

**Feature Branch**: `feature/019-notification-presentation`

**Created**: 2026-09-25

**Status**: Clarified

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/009-window-and-shell/002-notification-presentation.md (id 002, notification-presentation): move the notification stack from top-right to bottom-right so it no longer covers the library tab strip, settings categories, or plugin table headers; cap visible notifications at three with a compact 'N more' affordance that expands the rest; make severity legible at a glance via an accent bar, icon, and the severity word; keep informational notifications self-dismissing and warnings/critical notifications persistent until dismissed or acted on; rewrite notification wording (especially the audio-device message) as short, human guidance instead of raw system identifiers, with the original technical string available behind a 'Details' control; truncate every notification to two lines with the full text available on expansion; keep existing action buttons."

**Source**: [specs/autonomous/breakdown/009-window-and-shell/002-notification-presentation.md](../autonomous/breakdown/009-window-and-shell/002-notification-presentation.md) — derived from [ModPlayer-UI-UX-Review.md § 3.8 Notifications](../autonomous/ModPlayer-UI-UX-Review.md#38-notifications) (`UX-05`, `UX-37`, `UX-38`) and [§ 5.4 Component rules](../autonomous/ModPlayer-UI-UX-Review.md#54-component-rules).

## Clarifications

### Session 2026-09-25

Resolutions below are either **derived** (a source settles it — cited) or an **assumed default** (no source settles it; the conventional, constitution-consistent choice was taken and is testable). No material, underivable decision remained, so nothing is escalated.

- Q: Which colour does each severity's accent bar use? → A: **Derived** (breakdown: "an accent bar in the positive, warning or danger colour"; 014 design tokens `Roles`): `Info` → `positive`, `Warning` → `warning`, `Critical` → `danger`, read from the active theme's `Roles` (so light, dark and 017 high-contrast variants apply automatically). No new colour is defined; no call site uses a colour literal.
- Q: What does the accent bar and card look like? → A: **Assumed default** (conventional toast pattern, § 5.4 "panels are a card on `surface.raised`"): each card is an opaque `surface_raised` frame with a solid accent bar on its **left edge**, 4 logical px wide, spanning the card's full height. The severity icon is drawn in the same severity colour. Icons stay the existing glyphs (Critical `⛔`, Warning `⚠`, Info `ℹ`) — each distinct in shape, so severity is distinguishable in greyscale as well as colour.
- Q: Why did the audio-device message show a 120-character identifier, and how is the device named "in human terms"? → A: **Derived from code**: `device-missing-at-launch` interpolates the *persisted device id* (`device_policy::resolve` → `MissingPreferred { device_name: id.as_str() }`), because only the id is persisted (`contracts/settings-file.md` `output_device`). `device-lost` and `device-available-again` already receive the backend's human device name (`OutputDeviceInfo::name`). **Assumed default** for the missing-at-launch case: when the user confirms an output device, the app also persists its display name in a new **optional** settings key `output_device_name` (additive; files without it stay valid and load unchanged). The human name is resolved in order: (1) `output_device_name` if present; (2) the `<name>` part of an id stored in the existing `name:<name>` form; (3) otherwise the generic phrase "Your saved output device". The raw id is **never** used as the display name.
- Q: What does the app say it did instead? → A: **Derived** (breakdown: "saying what the app did instead"). Device notifications name the fallback device when one exists, using the fallback's `OutputDeviceInfo::name` (new Fluent arg `$fallback`). Target en-US wording (exact phrasing may be polished in plan, but each MUST keep the listed facts and contain no raw identifier):
  - `device-lost`: "{ $device } disconnected. Now playing through { $fallback }."
  - `device-missing-at-launch`: "{ $device } isn't connected. Playing through { $fallback } instead."
  - `device-available-again`: "{ $device } is available again. You can switch back in Settings › Audio."
- Q: What exactly goes behind "Details"? → A: **Assumed default**: `Notification` gains an optional, non-localised `detail` string holding the original technical text. It is set for the three device-availability notifications (value: the relevant device's `DeviceId` display form, e.g. `coreaudio:AppleGFXHDAEngineOutputDP:10001:0:{6D1E-7715-00097FED}`) and for `keybindings-invalid-entries` (value: the dropped `$ids` list, which is removed from the collapsed sentence). A "Details" toggle is rendered **only** when `detail` is set; it reveals the string in a selectable, monospace, wrapping label below the message. Notifications without technical payloads (e.g. `plugin-suspended`, whose `$cause` is already localised prose) get no Details control.
- Q: Which three notifications are visible, and in what order? → A: **Assumed default** (keeps the existing newest-first contract in `NotificationCenter`; the spec's own edge case already states the cap does not vary by severity): the three **newest** non-dismissed notifications are visible; older ones are in overflow. No severity-based pinning. Within the stack, reading order top → bottom is newest → oldest; the stack is anchored at the bottom-right and grows upward.
- Q: Where is the "N more" control and how does it behave? → A: **Assumed default**: a single compact button placed directly below the oldest visible card, labelled via Fluent plural "{ $count } more" (N = non-dismissed count − 3). Activating it expands the stack inline to show every non-dismissed notification (breakdown: "expands the rest"); the control then reads "Show fewer" and collapses back to three. The expanded stack's height is capped at 60 % of the window's inner height and scrolls vertically beyond that. The control disappears — and the stack returns to collapsed state — when the non-dismissed count drops to 3 or fewer. Expanded/collapsed state is UI-only and never persisted.
- Q: When a visible card is dismissed while overflow exists, which hidden card is promoted? → A: **Derived** from the newest-three rule: the newest hidden notification takes the freed slot (the visible set is always "the three newest non-dismissed"), and N decreases by one.
- Q: How is "two lines" measured and expanded? → A: **Assumed default**: card width is `min(360, window inner width − 16)` logical px (the 018 minimum window is 960 px, so 360 always fits). The message (including a plugin notification's text after its attribution icon + name) is laid out at the card's text width in the body text style and clipped to two lines with a trailing ellipsis. When — and only when — the full text needs more than two lines, a "Show more" toggle appears; it expands to the full text and then reads "Show less". "Show more" (full message) and "Details" (technical string) are separate controls.
- Q: Does an Info notification auto-dismiss while the user is reading its expanded text? → A: **Assumed default** (WCAG 2.2.1 Timing Adjustable; presentation-only, does not change which events raise notifications or their severity): an `Info` notification whose "Show more" or "Details" is expanded is not auto-dismissed while expanded; on collapse its 10 s `INFO_AUTO_DISMISS` period restarts from the collapse moment. Collapsed Info notifications — visible or in overflow — age out exactly as today (10 s from raise). Warning/Critical behaviour is unchanged.
- Q: Keyboard and screen-reader access to the new controls? → A: **Derived** (Constitution X: every host function keyboard-operable, every UI element has an accessible name; FR-022 of 001): "N more"/"Show fewer", "Show more"/"Show less" and "Details" are real buttons reachable by Tab, activatable by Enter/Space, with externalized accessible names. The accent bar and icon are decorative; the card's accessible text keeps the severity word plus the **full** (untruncated) message, so truncation never hides text from assistive technology.
- Q: What must the bottom-right stack not cover, and is lower content allowed to be overlapped? → A: **Derived** (breakdown: "out of the path of every header and tab strip in the app"): no header, tab strip, category list, table header or nav rail may be overlapped by the collapsed stack at any window size ≥ the 018 minimum (960 × 640). Lower content (e.g. the last visible rows of a scrolling list, the queue in Now Playing) may be overlapped; the stack stays non-modal and never blocks input outside its own cards. Offset from the window corner: 8 logical px on both axes (unchanged from today's top-right offset). Z-order relative to other overlays is unchanged.
- Q: Which languages must the rewritten strings ship in? → A: **Derived** (Constitution X: all host strings externalized; repo ships only `locales/en-US/`): all new/rewritten strings are Fluent keys in `locales/en-US/`; any other locale added later follows the same keys. No raw text in code.
- Q: Performance Mode suppression (FR-015)? → A: **Derived** from repo state: Performance Mode (004-performance/003) is not implemented yet. This feature adds no suppression logic and must not preclude it: the presentation layer renders whatever `NotificationCenter::visible()` yields.
- Q: The draft's usability-study success criteria (SC-003, SC-005, SC-006) can't be run in this pipeline — how are they verified? → A: **Assumed default**: replaced with objective, automatable checks (see Success Criteria), plus the constitution's manual scenario sign-off (screenshot evidence) for the visual ones.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Notifications stop hiding what the user is working on (Priority: P1)

A musician is browsing the library, adjusting settings, or reviewing the plugin list while notifications arrive. Today the stack floats top-right and covers the library's tab strip, the settings category list, and the plugin table's column headers — exactly the controls the user needs while the notification is up. The stack moves to the bottom-right corner, out of the path of every header and tab strip, so the user can keep working while being informed.

**Why this priority**: This is the core complaint driving the feature (`UX-05`, P0) — notifications actively blocking navigation and data the user is looking at. Moving the stack out of the way delivers the primary value even before any other change ships.

**Independent Test**: Open the library with the tab strip and first row of results visible, raise three notifications, and confirm the tab strip and first row stay fully visible and clickable.

**Acceptance Scenarios**:

1. **Given** the library is open with its tab strip and first list row visible, **When** three notifications are raised, **Then** the notification stack appears anchored 8 px from the bottom-right corner of the window and the tab strip and first list row remain fully visible and clickable.
2. **Given** the settings view is open with its category list visible, **When** three notifications are raised, **Then** the category list remains fully visible.
3. **Given** the plugin table is open with its column headers visible, **When** three notifications are raised, **Then** the column headers remain fully visible.
4. **Given** the window is at its 960 × 640 minimum size, **When** three notifications are raised on any screen, **Then** the collapsed stack's rectangle intersects no header, tab strip, category list, table header or nav rail rectangle.

---

### User Story 2 - Notifications don't pile up past a usable limit (Priority: P1)

When several notifications arrive close together, three stacked cards already start to feel like a lot; more than that would push back into covering content lower on screen. The stack shows at most three notifications at once. A fourth (or more) is represented by a single compact "N more" control that expands to reveal the rest on demand, so the user chooses when to see the overflow instead of it being forced into view.

**Why this priority**: Without a cap, restoring space by relocating the stack (User Story 1) would be undone the moment three or more notifications arrive at once. This is the second half of the placement fix and ships in the same increment.

**Independent Test**: Raise three notifications and confirm all three are visible with no overflow control; raise a fourth and confirm exactly three cards plus a "1 more" control are shown, and that activating the control reveals the fourth.

**Acceptance Scenarios**:

1. **Given** no notifications are visible, **When** three notifications are raised in sequence, **Then** all three appear in the stack, newest at the top, and no overflow control is shown.
2. **Given** three notifications are already visible, **When** a fourth notification is raised, **Then** the stack shows the three newest notifications plus a "1 more" control below the oldest visible card; the oldest notification is hidden.
3. **Given** the "1 more" control is showing, **When** the user activates it (click, or Tab to it and press Enter/Space), **Then** all four notifications are visible and the control reads "Show fewer".
4. **Given** the stack is expanded, **When** the user activates "Show fewer", **Then** the stack returns to the three newest plus the "N more" control.
5. **Given** four notifications with the stack collapsed, **When** the user dismisses one of the three visible cards, **Then** the hidden notification becomes visible in its place and the "N more" control disappears.
6. **Given** the stack is expanded, **When** dismissals or auto-dismissals bring the non-dismissed count to 3 or fewer, **Then** the control disappears, the stack is collapsed, and no notification is lost.

---

### User Story 3 - Severity is legible without reading the words (Priority: P2)

A critical plugin failure and a routine "device connected" message currently render as identical grey cards (`UX-37`), so the user must read every word to know whether something needs urgent attention. Each notification now carries an accent bar in its severity's colour, a severity icon, and the severity spelled out as a word, so critical, warning, and informational notifications are visually distinct at a glance, before any text is read.

**Why this priority**: This addresses the "everything looks the same" complaint directly, but a user can still work around it today by reading each message; it matters less than getting the stack out of the way (User Stories 1-2).

**Independent Test**: Raise one critical and one informational notification together and confirm they can be told apart by accent bar colour and icon alone, without reading the message text.

**Acceptance Scenarios**:

1. **Given** a critical notification and an informational notification are both visible, **When** the stack renders, **Then** the critical card's left accent bar and icon use the theme's `danger` colour, the informational card's use `positive`, and the two icons are different glyphs.
2. **Given** a warning notification is visible, **When** the stack renders, **Then** its accent bar and icon use the theme's `warning` colour.
3. **Given** any visible notification, **When** the user inspects it or a screen reader reads it, **Then** the severity word ("Critical", "Warning", "Info") is present as text, not conveyed by colour or icon alone.
4. **Given** the high-contrast appearance (017) is active, **When** the stack renders, **Then** the accent bars use the high-contrast `Roles` values for the same three roles.
5. **Given** a collapsed informational notification is showing, **When** 10 s have elapsed since it was raised, **Then** it disappears from the stack without user action.
6. **Given** a warning or critical notification is showing, **When** no action is taken, **Then** it remains until the user dismisses it or uses its action button.

---

### User Story 4 - Notification text reads as guidance, not a log line (Priority: P2)

The output-device notification today shows the raw system device string — around 120 characters of technical identifier (`UX-38`) — to a musician mid-practice. It is rewritten as a short sentence that names the device in plain terms and states what the app did in response, with the original technical string still reachable behind a "Details" control. Every notification is capped at two lines in the stack, with the full text reachable on expansion, and any existing action button keeps working exactly as before.

**Why this priority**: A content and readability improvement layered on the relocated, capped, severity-legible stack; valuable but independent of where the stack sits or how many cards show.

**Independent Test**: Launch with a confirmed output device that is no longer connected and confirm the visible text is a short, plain-language sentence naming the device and the fallback, at most two lines, with the raw device identifier available only after activating "Details".

**Acceptance Scenarios**:

1. **Given** the confirmed output device is missing at launch and its display name was persisted, **When** the notification is raised, **Then** its collapsed text names the device by that display name and the fallback device by its name, is at most two lines, and contains no part of the raw device id.
2. **Given** the confirmed output device is missing at launch and the settings file predates `output_device_name` (id only, not in `name:<name>` form), **When** the notification is raised, **Then** the text uses the generic phrase "Your saved output device" instead of the id.
3. **Given** the active output device disconnects and the app falls back, **When** the notification is raised, **Then** its collapsed text names the lost device and the fallback device and contains no raw id.
4. **Given** a device-availability notification is visible, **When** the user activates "Details", **Then** the original raw device id is shown in a selectable monospace label; activating it again hides it.
5. **Given** a notification whose full message needs more than two lines at the card width, **When** it is raised, **Then** the card shows two lines ending in an ellipsis plus a "Show more" control, which reveals the full text and then reads "Show less".
6. **Given** a notification whose message fits in two lines, **When** it is raised, **Then** no "Show more" control is shown.
7. **Given** an informational notification with "Show more" or "Details" expanded, **When** 10 s have passed since it was raised, **Then** it remains visible until collapsed, and auto-dismisses 10 s after collapse.
8. **Given** a notification that already has action buttons (e.g. "Retry", "Sign in", "Restart plugin", "Disable plugin"), **When** it is rendered in the new layout, **Then** every action button is present and dispatches the same action as before.

---

### Edge Cases

- **Long device names / other locales**: the two-line cap applies regardless of locale or name length; an over-long device name is clipped with an ellipsis within the two lines and the full text is available via "Show more".
- **Dismissal while overflow is waiting**: the newest hidden notification takes the freed visible slot and N decreases by one; the control disappears once no overflow remains.
- **Critical raised while three lower-severity notifications fill the visible slots**: being newest, it takes a visible slot; the oldest visible card moves to overflow. The cap does not vary by severity and there is no severity pinning.
- **Collapsed Info notification in overflow**: it still auto-dismisses 10 s after being raised, even though hidden; N decreases accordingly.
- **New arrival while a card is expanded**: expanded/collapsed state (Show more, Details) of other notifications is keyed by notification id and is unaffected by a new arrival or by a card moving between visible and overflow.
- **Dedupe replacement** (`raise_keyed`, e.g. a repeat `plugin-suspended`): the replacement is a new, newest notification and appears at the top; the replaced one leaves the stack. No change to dedupe semantics.
- **Expanded stack taller than the cap**: the expanded stack scrolls within 60 % of the window's inner height.
- **Device name unknown**: if the fallback device's name is empty, the sentence falls back to "the system default output"; if the lost/missing device's name cannot be resolved, "Your saved output device".

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST anchor the notification stack to the bottom-right corner of the application window, offset 8 logical px on both axes, growing upward, instead of the top-right. (`UX-05`)
- **FR-002**: At any window size ≥ 960 × 640, the collapsed stack MUST NOT overlap any header, tab strip, settings category list, table column header or the nav rail — specifically the library's tab strip, the settings category list and the plugin table's column headers. Lower content may be overlapped; the stack MUST stay non-modal and not block input outside its own cards.
- **FR-003**: The system MUST show at most three notification cards in the collapsed stack: the three newest non-dismissed notifications, newest at the top. No severity-based pinning.
- **FR-004**: When more than three notifications are non-dismissed, the system MUST show one compact button directly below the oldest visible card labelled "{N} more" (Fluent plural), where N = non-dismissed count − 3.
- **FR-005**: Activating "{N} more" MUST expand the stack inline to show every non-dismissed notification, newest first, and relabel the button "Show fewer"; activating "Show fewer" MUST collapse it. The expanded stack MUST be height-capped at 60 % of the window's inner height and scroll beyond that. Expanded state is UI-only and not persisted.
- **FR-006**: When the non-dismissed count changes while collapsed, the visible set MUST always be recomputed as the three newest non-dismissed notifications (so a dismissal promotes the newest hidden one and N decreases). When the count drops to ≤ 3, the button MUST disappear and the stack MUST return to collapsed state.
- **FR-007**: Each card MUST be an opaque `surface_raised` frame with a 4 px, full-height accent bar on its left edge and a severity icon, both in the active theme's role colour: `Info` → `positive`, `Warning` → `warning`, `Critical` → `danger`. Colours MUST come from the theme `Roles` (no literals). Icons remain `⛔` / `⚠` / `ℹ`. (`UX-37`)
- **FR-008**: The severity word ("Critical", "Warning", "Info", Fluent keys `severity-*`) MUST be present as visible text on every card and in its accessible name; colour and icon are never the sole carriers of severity.
- **FR-009**: `Info` notifications MUST continue to auto-dismiss 10 s after being raised and `Warning`/`Critical` MUST persist until dismissed or acted on, with one presentation exception: an `Info` notification with "Show more" or "Details" expanded MUST NOT auto-dismiss while expanded, and its 10 s period MUST restart when it is collapsed.
- **FR-010**: Each card's message (for plugin notifications, the text after the attribution icon + name) MUST be laid out at the card text width — card width `min(360, window inner width − 16)` px — and clipped to two lines with a trailing ellipsis. A "Show more" / "Show less" toggle MUST appear if and only if the full text exceeds two lines.
- **FR-011**: The device-availability notifications (`device-lost`, `device-missing-at-launch`, `device-available-again`) MUST show a short, plain-language en-US sentence naming the device in human terms and, for `device-lost` and `device-missing-at-launch`, the fallback device by name (new `$fallback` arg), per the wording in Clarifications. No raw device id may appear in their collapsed text. (`UX-38`)
- **FR-012**: The human name for `device-missing-at-launch` MUST resolve as: persisted `output_device_name` → the `<name>` of a `name:<name>` id → "Your saved output device". When the user confirms an output device, the system MUST persist its display name in a new optional settings key `output_device_name`; settings files without the key MUST load unchanged.
- **FR-013**: `Notification` MUST carry an optional non-localised `detail` string. It MUST be set to the relevant `DeviceId` display form for the three device-availability notifications and to the dropped ids for `keybindings-invalid-entries` (whose collapsed sentence no longer interpolates `$ids`). A "Details" toggle MUST be rendered if and only if `detail` is set, revealing it in a selectable, monospace, wrapping label.
- **FR-014**: The system MUST preserve every existing action button (Sign in, Open status page, Retry, Open upgrade page, Restart plugin, Disable plugin) and Dismiss, unchanged in behaviour and dispatch.
- **FR-015**: "{N} more"/"Show fewer", "Show more"/"Show less", "Details" and Dismiss MUST be keyboard-reachable (Tab) and activatable (Enter/Space) buttons with externalized, non-empty accessible names. A card's accessible text MUST include the severity word and the full, untruncated message.
- **FR-016**: All new or rewritten user-visible strings MUST be Fluent keys in `locales/en-US/`; no raw text in code.
- **FR-017**: The system MUST NOT change which events raise a notification, their severity, or their action set, nor dedupe/attribution semantics — only placement, visible cap, styling, truncation, wording/args and the `detail` payload.
- **FR-018**: The system MUST NOT add or alter any Performance Mode suppression; the stack renders whatever `NotificationCenter::visible()` yields.

### Key Entities

- **Notification**: A raised message with severity (Critical, Warning, Info), Fluent message key and args, optional actions (≤ 2), optional dedupe key and plugin attribution (all unchanged), plus a new optional non-localised `detail` technical string shown behind "Details".
- **Notification Stack (UI state)**: The bottom-right presentation of `NotificationCenter::visible()`: collapsed (three newest + "{N} more") or expanded (all, height-capped, scrollable); per-notification expanded flags for "Show more" and "Details", keyed by notification id; none persisted.
- **Settings `output_device_name`**: New optional string key persisted alongside `output_device` when a device is confirmed; used only to name a missing device in human terms.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the library open at 960 × 640 and at the default window size, after three notifications are raised, the stack's rectangle has zero-pixel intersection with the tab strip and the first list row (automated layout test + manual scenario screenshot); same for the settings category list and the plugin table headers.
- **SC-002**: With four non-dismissed notifications, exactly three cards and one "1 more" button render; one activation of that button renders four cards without dismissing any (automated UI test).
- **SC-003**: For a Critical and an Info card rendered together, the accent bar colours equal `Roles::danger` and `Roles::positive` respectively (and differ), the icon glyphs differ, and each card's accessible text contains its severity word — in light, dark and high-contrast themes (automated test).
- **SC-004**: For each device-availability notification, the collapsed rendered text is ≤ 2 lines at the 360 px card width for device names up to 40 characters and contains no substring of the raw `DeviceId`; the raw id appears only after "Details" is activated (automated test using the `UX-38` example id).
- **SC-005**: The resolved `device-lost` and `device-missing-at-launch` strings contain both the lost/missing device's display name and the fallback device's name (automated Fluent test).
- **SC-006**: Every existing notification action (Sign in, Open status page, Retry, Open upgrade page, Restart plugin, Disable plugin) still dispatches its handler from the new layout (existing action tests pass unchanged; one added test per action variant).

## Assumptions

- The 018 minimum window size (960 × 640) is the smallest layout the no-overlap requirement is checked against; this feature does not change window layout.
- Severity colours are the existing 014 `Roles` (`positive`, `warning`, `danger`), including their 017 high-contrast variants; no new colours.
- Only `locales/en-US/` exists today; other locales inherit the same keys when added.
- Existing notification identity, deduplication, plugin attribution and action dispatch are unchanged.
- Performance Mode is not yet implemented; its future suppression rules operate upstream of this presentation layer.

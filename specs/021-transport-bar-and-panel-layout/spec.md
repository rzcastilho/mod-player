# Feature Specification: Sticky Transport Bar and Panel Layout

**Feature Branch**: `feature/021-transport-bar-and-panel-layout`

**Created**: 2026-09-28

**Status**: Draft

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/010-now-playing-workbench/001-transport-bar-and-panel-layout.md (id 001, transport-bar-and-panel-layout)."

**Source breakdown**: `specs/autonomous/breakdown/010-now-playing-workbench/001-transport-bar-and-panel-layout.md`

**Source review**: `specs/autonomous/ModPlayer-UI-UX-Review.md` § 3.5 Now Playing (`UX-21`, `UX-22`, `UX-26`), § 5.5 Layout rules, § 5.4 Component rules, § 1 Summary

**Requirement IDs implemented**: FR-3.2.2 (view, reorder, remove queue items), NFR-6.1 (keyboard operable), NFR-6.2 (accessible names/roles/states), NFR-6.4 (color never the sole carrier of meaning), NFR-7.4 (40% text expansion without truncation); review findings `UX-21`, `UX-22`, `UX-26`.

**Prerequisites**: Builds on the panel card and toggle components delivered by [015-control-variants](../015-control-variants/spec.md) and [016-list-row-and-panel-components](../016-list-row-and-panel-components/spec.md) (panel card visuals, per-panel remembered open state, the shared list-row layout), and the window sizing and responsive-dock behavior delivered by [018-window-sizing-and-responsive-dock](../018-window-sizing-and-responsive-dock/spec.md).

**Scope boundary**: Covers the Now Playing screen's region layout (transport bar, waveform, panel cards) and the Queue panel's rows. The contents of the Markers, Effect Chain, and waveform surfaces themselves are restructured by later features in this wave and are out of scope here.

## Clarifications

### Session 2026-09-28 (clarify reviewer)

Sources: constitution v1.1.1; breakdown `010-now-playing-workbench/001`; review §3.5 / §5.4 / §5.5; specs 016, 018; current `now_playing.rs`, `queue_view.rs`, `actions/catalog.rs`. **Derived** = settled by a source; **Default** = conventional choice picked by the reviewer (no source settles it).

- Q: Which part of the screen scrolls? → A: **Derived** (review §5.5 "a fixed transport bar at the top … so the transport never scrolls away"; review UX-18 precedent "drop nested scroll areas; page-level scroll"): the Now Playing content area is split into exactly two vertical parts — the **transport bar** (pinned, never scrolls) and **one** vertical scroll region below it holding, top to bottom: status line / transfer banner, waveform region (overview, detail, marker lane), then the panel cards. The waveform scrolls with the panels (only the transport bar is pinned). **Default**: no panel card has its own nested vertical scroll area — the Effect Chain's current capped inner scroll (and its `effects_panel_reserved_height` reservation) is removed, since master volume and the peak meter no longer sit below it.
- Q: How does "the waveform grows with the window" relate to 018's waveform sizing? → A: **Derived** (018 Clarifications, FR on waveform heights): waveform heights stay exactly as 018 defined them — overview `max(64, round(0.08 × H))`, detail `max(120, round(0.22 × H))`, H = Now Playing available content height, independent of scroll position. This feature does not change the formula; it only places the waveform in the scroll region beneath the transport bar. Consequently, when the window is shorter than transport bar + waveform, the waveform does **not** shrink below 018's minimums — the scroll region absorbs the overflow (realigns User Story 1 Scenario 2 and User Story 3 Scenario 2).
- Q: What does "transport bar height constant" mean given 018's wrap-at-narrow-width rule and 40% text expansion? → A: **Default** (reconciles FR-002 with FR-017 and 018 contract D4): the transport bar's height MUST NOT change with window **height**, scroll position, which panels are open, playback state, or title/artist length. It MAY change with window **width** / text expansion only by wrapping whole controls onto an additional row (018 rule: controls move, labels are never elided). Title and artist each render on a single line, truncated with an ellipsis, full text in a tooltip and in the accessible name.
- Q: What exactly is in the transport bar, and in what order? → A: **Derived** (breakdown prompt; 018 "Panels" toggle): leading → trailing: artwork thumbnail (40 px, 016 list-row artwork size, same missing-artwork placeholder), title over artist, skip back, play/pause, stop, skip forward, elapsed / remaining time (monospace numerals), master volume slider with its peak meter, then the panel toggles **Queue, Effects, Transport**, then 018's **Panels** toggle (still shown only under 018's conditions). The current visible "Now Playing" heading row is replaced by this bar (its artwork/title content moves into it); the screen keeps its accessible name. **Default**: status line and transfer banner move to the top of the scroll region (not into the bar), so they cannot change the bar's height. The waveform's own elapsed/remaining labels are untouched (owned by 002-waveform-legibility).
- Q: Does the Markers panel get a toggle in the transport bar? → A: **Derived** (breakdown prompt names only "Queue, Effects or Transport" toggles; spec Assumptions: no new shortcuts): no. The Markers card is collapsible only from its own card header. **Default**: its open state is a new persisted flag `markers_open` in the existing `[now_playing_panels]` section of `settings.toml` (additive; absent → `true`, matching today's always-visible markers), stored independently of `queue_open` / `effects_open` / `transport_open`.
- Q: What order do the panel cards appear in? → A: **Derived** (breakdown prompt order; matches current render order): Markers, Effect Chain, Transport focus, Queue. Markers card renders only while a track is loaded (as today); the other three render regardless of track state (as today).
- Q: How are the toggles distinguished from transport buttons? → A: **Derived** (review UX-22 "toggles styled as toggles"; 015 control variants): each panel toggle uses the 015 toggle variant with a visible on/off state that is not conveyed by colour alone (NFR-6.4), exposes a pressed/checked state equal to its panel's open state, and is separated from the transport buttons group by spacing. The toggle's state and the card header's collapse control are one state: collapsing a card from its header un-presses its toggle, and vice versa.
- Q: What precisely does "bring into view" do? → A: **Default** (conventional "scroll into view, nearest" semantics): after opening, the scroll region scrolls by the **minimum** amount needed so the whole panel card is visible; if the card is taller than the scroll region's viewport, it scrolls so the card's header is aligned to the top of the viewport (directly beneath the transport bar). If the card is already fully visible, no scroll occurs. The scroll is applied in the same interaction (no animation requirement; no second user action). Focus stays on the toggle (or wherever it was, for the keyboard shortcut) — it does not move into the panel.
- Q: Pressing an open panel's toggle when that panel is scrolled out of view — collapse, or bring into view? → A: **Derived** (breakdown prompt: "pressing it again collapses it"): always collapse; the toggle's own pressed state visibly changes, so the press never appears to do nothing. Collapsing never changes the scroll position except as clamped by the content becoming shorter.
- Q: With another panel already open, may bringing the newly opened panel into view move the other out of view? → A: **Default**: yes — the newly opened panel takes precedence; the other panel stays open (never auto-collapsed) and remains reachable by scrolling. (Realigns the edge case that said the other panel must stay visible, which is unsatisfiable when both cannot fit.)
- Q: Which keyboard paths trigger the same behavior? → A: **Derived** (current action catalog): `HostAction::ToggleQueue`, `HostAction::ToggleEffectChain`, `HostAction::ToggleTransportPanel` via whatever bindings they currently have. No new actions, ids, or bindings. If the action fires while Now Playing is not the active screen, existing behavior (whatever it is today) is preserved; bring-into-view applies when Now Playing is shown.
- Q: What is the Queue row's right-aligned column, and how is the playing row marked? → A: **Derived** (review §5.4 list row; breakdown prompt) + **Default** for specifics: rows use 016's 3-column list row; the right-aligned column shows the 1-based queue position in monospace numerals **instead of** the duration (no "…" menu). The currently playing row is marked by (a) a leading play glyph (▶ or the app's icon equivalent) placed before the title and (b) an `accent` bar on the row's leading edge — shape plus colour, never colour alone (NFR-6.4); it does **not** use 016's selected-row accent fill, so it cannot be confused with selection. Its accessible name includes an externalized "Now playing" state (e.g. "Now playing, <title>, <artist>"); no visible "Now playing:" text prefix remains.
- Q: How are queue row actions rendered? → A: **Derived** (review §5.4 "`subtle` (text-only, for row-level actions)"; current `ButtonVariant::Quiet`): move up, move down, play next, remove render as the quiet/subtle button variant, **always visible** (not hover-revealed, so keyboard and pointer users find them identically), each a separate tab stop with its existing externalized label as accessible name. Which actions are enabled/present per row (e.g. first/last row, the current entry) is unchanged from today.
- Q: Spacing between cards? → A: **Derived** (review §5.4 panels; current `theme::space::XL` gaps): cards are separated by the `XL` spacing token (and waveform → first card by `XL`), with no separator rule anywhere in the scroll region.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The transport never disappears (Priority: P1)

A musician opens the Effect Chain, Markers, Transport focus, or Queue panel on the Now Playing screen and scrolls through its content. No matter how far they scroll, the play/pause, stop, skip, elapsed/remaining time, master volume, and panel toggle controls stay visible and usable at the top of the screen, instead of today's single column where opening any panel pushes the transport control row below the fold.

**Why this priority**: The transport is the control a musician touches every few seconds during a set; losing access to it the moment any panel is open is the review's top-rated Now Playing finding (`UX-21`, P1) and the feature's lead acceptance line.

**Independent Test**: Open any one panel (Markers, Effect Chain, Transport, or Queue) with enough content to exceed the window height, scroll its content to the bottom, and confirm every transport control (play/pause, stop, skip back/forward, elapsed/remaining time, master volume, peak meter, and all panel toggles) is still visible and responds to a click.

**Acceptance Scenarios**:

1. **Given** a panel is open and its content is taller than the window, **When** the user scrolls that content to the bottom, **Then** the transport bar (artwork, title/artist, play/pause, stop, skip, elapsed/remaining times, master volume with peak meter, panel toggles) remains fully visible and every control in it still responds to a click or keypress.
2. **Given** the Now Playing screen with no panel open, **When** the window is made shorter than the combined height of the transport bar and the waveform, **Then** the transport bar stays fully visible at its unchanged height, the waveform keeps 018's minimum heights (overview ≥ 64, detail ≥ 120), and the overflow becomes scrollable in the region below the transport bar.
3. **Given** the transport bar is visible, **When** the user presses play/pause, stop, or a skip control while a panel is open and scrolled away from the top, **Then** the action takes effect immediately, exactly as it would with no panel open.

---

### User Story 2 - A panel toggle behaves like navigation (Priority: P1)

A musician presses the Queue, Effects, or Transport toggle in the transport bar. Its panel opens (if closed) and is brought into view in the same interaction, so the toggle visibly does something even when the panel would otherwise render below the fold. Pressing the same toggle again collapses the panel, instead of today's toggle that looks identical to a transport button and appears to do nothing because its panel opens far below the visible area.

**Why this priority**: Today the panel toggles are indistinguishable from ordinary buttons and their effect is invisible at the default window size (`UX-22`, P1) — this is the feature's second acceptance line and the direct fix for that finding.

**Independent Test**: With a panel closed and the screen scrolled away from the top, press its toggle and confirm the panel opens and is visible without any further manual scrolling; press the same toggle again and confirm the panel collapses.

**Acceptance Scenarios**:

1. **Given** the Queue panel is closed, **When** the user presses the Queue toggle, **Then** the Queue panel opens and becomes visible on screen without the user performing any additional scroll action.
2. **Given** the Effects or Transport panel is closed, **When** the user presses its toggle, **Then** that panel opens and becomes visible on screen in the same interaction.
3. **Given** a panel is open and visible, **When** the user presses its toggle again, **Then** the panel collapses.
4. **Given** a panel is toggled open or closed with its keyboard shortcut instead of a pointer click, **When** the shortcut is pressed, **Then** the same open-and-bring-into-view or collapse behavior occurs as when the on-screen toggle is pressed.

---

### User Story 3 - The waveform grows with the window (Priority: P2)

A musician resizes the window taller. The waveform area — the screen's flexible middle — grows to use the extra height, while the transport bar above it keeps the same height regardless of window size, instead of today's fixed-height waveform that wastes the extra space a taller window provides.

**Why this priority**: This is the feature's third acceptance line; it makes the waveform (the primary visual reference during playback) proportionally more useful on larger windows without disturbing the always-visible transport.

**Independent Test**: Note the transport bar's height and the waveform's height at the default window size; make the window taller and confirm the waveform's height increases while the transport bar's height is unchanged.

**Acceptance Scenarios**:

1. **Given** the Now Playing screen at its default window size, **When** the window is made taller, **Then** the waveform area's height increases and the transport bar's height stays the same.
2. **Given** the Now Playing screen at its default window size, **When** the window is made shorter (down to the application's minimum size), **Then** the waveform area's height decreases per 018's formula down to its minimums, the transport bar's height never changes, and any content that no longer fits scrolls beneath the transport bar.

---

### User Story 4 - The queue reads at a glance, and the playing track is unmistakable (Priority: P2)

A musician opens the Queue panel and can identify the currently playing entry immediately, by its position on the row rather than by reading a text label, and can read every row's artwork, title, and artist the same way a Library or Search row reads — instead of today's plain text rows with "Now playing:" spelled out in front of the current one and no artwork.

**Why this priority**: This is the feature's fourth acceptance line (`UX-26`, P3 in the review, elevated here because it ships alongside the rest of the queue panel's placement in the new layout) and reuses components 016 already delivered, so it is low-risk to ship with the region layout.

**Independent Test**: Open the Queue panel with several items queued, including the currently playing one; confirm the current entry is identifiable without reading any row's text, and that every row shows artwork, a title above its artist, and a right-aligned queue position.

**Acceptance Scenarios**:

1. **Given** the Queue panel is open with a currently playing entry among its rows, **When** the panel renders, **Then** the currently playing row is visually distinguished from the others without relying on any text label reading "Now playing" or similar wording.
2. **Given** the Queue panel is open, **When** any row renders, **Then** it shows an artwork thumbnail, the track's title above its artist (not on the same line), and its position in the queue right-aligned.
3. **Given** a queue row, **When** the user looks for its move-up, move-down, play-next, or remove actions, **Then** they are present as low-emphasis ("quiet") row actions rather than prominent buttons, and each remains reachable and operable by keyboard.
4. **Given** the currently playing row, **When** it renders, **Then** its distinguishing mark is identifiable by more than color alone (e.g., also by shape, icon, or position) per the app's existing rule that color never carries meaning by itself.

---

### Edge Cases

- No track is loaded: the transport bar still renders, pinned, with its controls in their existing disabled state; no panel content exists to scroll, so the sticky behavior has nothing to demonstrate but must not regress once a track is loaded.
- More than one panel is open at once: the newly opened panel is brought into view (minimum scroll; header to top if taller than the viewport) even if that scrolls an already-open panel out of view; the already-open panel stays open and reachable by scrolling, never auto-collapsed.
- The window is at its minimum size (960 × 640) with a panel open: the transport bar is still fully visible and usable at its unchanged height; waveform and panel content scroll beneath it.
- A long track title or artist name: each truncates to one line with an ellipsis (full text in tooltip and accessible name) without increasing the transport bar's height or pushing any of its controls out of view.
- A panel toggle is pressed for a panel that is open but scrolled out of view: the panel collapses (toggle state visibly changes); it is not scrolled into view.
- A card is collapsed from its own header: its transport-bar toggle (if any) shows the unpressed state in the same frame, and vice versa.
- No track is loaded: the Markers card is absent (as today); the Effect Chain, Transport focus, and Queue cards and their toggles still work.
- Collapsing an open panel while its content is scrolled into view: the screen does not jump to an unrelated scroll position; the transport bar remains visible throughout.
- The Queue panel has only one item, and it is the currently playing one: the current-entry mark still renders correctly with a single row.
- A queued track has no artwork: the row shows the same placeholder treatment the Library and Search rows already use for missing artwork.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The Now Playing screen MUST present a transport bar — containing the artwork thumbnail, track title and artist, play/pause, stop, and skip controls, the elapsed and remaining time readouts, the master volume control with its peak meter, and the Queue/Effects/Transport panel toggles — that remains visible and operable regardless of how far any panel's content below it is scrolled. *(Prompt; UX-21; User Story 1)*
- **FR-002**: The transport bar's height MUST NOT change with window height, scroll position, which panels are open, playback state, or title/artist length. It MAY change with window width or text expansion only by wrapping whole controls onto an additional row (018 contract D4); title and artist each render on one line, ellipsis-truncated, with full text in a tooltip and the accessible name. *(Prompt "it never scrolls away"; Clarifications; User Story 1, User Story 3)*
- **FR-003**: Below the transport bar, the Now Playing screen MUST have exactly one vertical scroll region containing, in order: status line / transfer banner, the waveform region (overview, detail, marker lane), then the panel cards. The waveform heights MUST follow 018's formula unchanged (overview `max(64, round(0.08 × H))`, detail `max(120, round(0.22 × H))`, H independent of scroll), so they grow and shrink with window height down to those minimums. No panel card MAY contain its own nested vertical scroll area (the Effect Chain's capped inner scroll is removed). *(Prompt; review §5.5; 018; Clarifications; User Story 3)*
- **FR-004**: The Markers, Effect Chain, Transport focus, and Queue surfaces MUST each render as a collapsible panel card (016 panel card) positioned below the waveform, in that order, each with its own header whose collapse control is keyboard-operable, rather than as an undifferentiated column of controls. The Markers card renders only while a track is loaded; the other three render regardless of track state. Master volume and the peak meter MUST no longer appear in the scroll region (they live in the transport bar). *(Prompt; UX-21; Clarifications)*
- **FR-005**: Adjacent panel cards, and the waveform and the first panel card, MUST be separated by the `XL` spacing token, with no separator rule anywhere in the scroll region. *(Prompt "space rather than a rule between it and its neighbour"; review §5.4)*
- **FR-006**: Each panel's open/closed state MUST persist independently of the others in `[now_playing_panels]` of `settings.toml`, consistent with 016 FR-019. The Markers card gains a new additive flag `markers_open` (absent → `true`); `queue_open`, `effects_open`, `transport_open` are unchanged. A transport-bar toggle and its card's header control MUST read and write the same flag, so each always reflects the other in the same frame. *(Prompt; 016 FR-019; Clarifications)*
- **FR-007**: Pressing a closed panel's toggle (Queue, Effects, or Transport) MUST open that panel and, in the same interaction, scroll the scroll region by the minimum amount that makes the whole card visible; if the card is taller than the viewport, its header MUST be aligned to the top of the viewport directly beneath the transport bar; if already fully visible, no scroll occurs. Focus MUST NOT move into the panel. *(Prompt; UX-22; Clarifications; User Story 2)*
- **FR-008**: Pressing an open panel's toggle MUST collapse that panel, even if it is currently scrolled out of view; collapsing MUST NOT change the scroll position other than the clamp caused by shorter content. *(Prompt; Clarifications; User Story 2)*
- **FR-009**: Activating a panel's existing action (`ToggleQueue`, `ToggleEffectChain`, `ToggleTransportPanel`, via their current bindings; no new actions or bindings) MUST produce the same open-and-bring-into-view or collapse behavior as activating its on-screen toggle. *(User Story 2, Scenario 4; NFR-6.1)*
- **FR-010**: When the scroll region beneath the transport bar is scrolled to its end with any panel open, every transport bar control MUST remain visible on screen and MUST respond to activation (click or keyboard). *(Prompt "Acceptance"; User Story 1)*
- **FR-011**: When the window is made taller, the waveform's rendered heights MUST follow 018's formula (the detail view grows for every content height H above ≈ 545, the overview above ≈ 800; neither ever shrinks as H grows) while the transport bar's rendered height MUST NOT change. *(Prompt "Acceptance"; User Story 3)*
- **FR-012**: The Queue panel's rows MUST use 016's shared 3-column row layout: a 40 px artwork thumbnail (016 missing-artwork placeholder), the title rendered above the artist, and the item's 1-based queue position right-aligned in monospace numerals in place of a duration (no "…" menu). *(Prompt; UX-26; review §5.4; User Story 4)*
- **FR-013**: The Queue panel MUST mark the currently playing row directly on the row with a leading play glyph before the title plus an `accent` bar on the row's leading edge, MUST NOT use 016's selected-row fill for this mark, and MUST NOT show a visible "Now playing:"-style text prefix. The row's accessible name MUST include an externalized "Now playing" state. *(Prompt; UX-26; Clarifications; User Story 4)*
- **FR-014**: The currently playing row's mark MUST be identifiable by a visual signal beyond color alone. *(NFR-6.4; User Story 4, Scenario 4)*
- **FR-015**: Each queue row's move-up, move-down, play-next, and remove actions MUST render as the quiet/subtle button variant, always visible (not hover-revealed), each its own tab stop with its existing externalized label as accessible name; which actions are present/enabled per row is unchanged from today. *(Prompt; review §5.4; NFR-6.1, NFR-6.2)*
- **FR-016**: Every transport bar control, panel toggle, and queue row action MUST expose an accessible name, role, and state to assistive technology and MUST be operable by keyboard alone. *(NFR-6.1, NFR-6.2)*
- **FR-017**: The transport bar's controls and labels MUST continue to fit without truncating or overlapping under 40% text expansion, consistent with the responsive behavior already required by 018-window-sizing-and-responsive-dock. *(NFR-7.4)*
- **FR-018**: The transport bar MUST contain, leading → trailing: artwork thumbnail (40 px), title over artist, skip back, play/pause, stop, skip forward, elapsed / remaining time (monospace numerals), master volume with its peak meter, the Queue / Effects / Transport panel toggles, then 018's "Panels" toggle (shown only under 018's conditions). It replaces the current visible "Now Playing" heading row; status line and transfer banner render at the top of the scroll region, not in the bar. No Markers toggle is added. *(Prompt; 018; Clarifications)*
- **FR-019**: Each panel toggle MUST use the 015 toggle variant with a pressed/checked state equal to its panel's open state, conveyed by more than colour alone, exposed to assistive technology, and visually separated from the transport button group by spacing. *(UX-22; 015; NFR-6.2, NFR-6.4)*

### Key Entities

- **Transport Bar**: the pinned region at the top of the Now Playing screen; holds artwork, title/artist, playback controls, time readouts, master volume/peak meter, the panel toggles, and 018's conditional "Panels" toggle. Height invariant to window height, scroll, and panel state (FR-002); never scrolls.
- **Waveform Region**: overview, detail, and marker lane, at the top of the single scroll region beneath the transport bar (after status line / transfer banner); heights follow 018's H-based formula.
- **Panel Card**: a collapsible, individually-titled 016 card (Markers, Effect Chain, Transport focus, Queue — in that order) below the waveform region, with its own persisted open/closed flag in `[now_playing_panels]` (`markers_open` new).
- **Queue Row**: one entry in the Queue panel — artwork, title, artist, right-aligned position, a currently-playing mark when applicable, and quiet move/play-next/remove actions.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With any one panel open and its content scrolled to the end, 100% of transport bar controls remain visible and successfully respond to activation, checked across all four panels (Markers, Effect Chain, Transport, Queue) individually.
- **SC-002**: Pressing a closed panel's toggle results in that panel being fully visible on screen with zero additional scroll actions from the user, checked for each of the Queue, Effects, and Transport toggles.
- **SC-003**: At a fixed window width, across window heights from the application's minimum (640) to well above its default (820), the waveform's rendered heights match 018's formula (non-decreasing with window height) while the transport bar's rendered height varies by no more than a rounding pixel.
- **SC-004**: In a Queue panel showing multiple rows including the currently playing one, a user can correctly identify the currently playing row without reading any row's text label, in 100% of checks.
- **SC-005**: Every transport bar control, panel toggle, and queue row action is reachable and operable using the keyboard alone, with no mouse interaction required.

## Assumptions

- The panel card visuals (raised surface, header, padding, rounding), per-panel persisted open/closed state, and the shared list-row artwork/title-over-artist/right-aligned-column layout are already delivered by 015-control-variants and 016-list-row-and-panel-components; this feature composes them into the new three-region layout and applies the row layout to the Queue panel specifically, rather than redefining those components.
- "Brought into the visible viewport" is defined precisely by FR-007: minimum scroll so the whole card is visible, or header aligned to the top of the scroll viewport when the card is taller than it; applied in the same interaction, no second user action.
- The existing `ToggleQueue` / `ToggleEffectChain` / `ToggleTransportPanel` actions (with their current bindings) are the keyboard path that triggers this feature's open-and-bring-into-view behavior; no new actions or shortcuts are introduced.
- The currently-playing queue row's mark is a leading play glyph plus a leading-edge `accent` bar (FR-013), with an equivalent externalized "Now playing" accessible name for assistive technology users.
- The default and minimum window sizes are those already established by 018-window-sizing-and-responsive-dock; this feature does not change them.
- "Queue position" is the item's 1-based index in the queue's current display order (current entry, then play-next items, then upcoming context), matching the order already defined for the Queue panel.

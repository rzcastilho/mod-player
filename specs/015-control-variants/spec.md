# Feature Specification: Button, Toggle, and Meter Variants

**Feature Branch**: `016-control-variants`

**Created**: 2026-09-22

**Status**: Specified — no open [NEEDS CLARIFICATION] markers; ready for `/speckit-plan`

**Input**: User description: "Make the difference between 'continue', 'remove everything' and 'show more' visible before the user clicks, and make a toggle look like a toggle. Introduce four button variants — a filled primary used at most once per view, a default, a quiet text-only variant for row-level actions, and a destructive variant that carries the danger colour and an outline. Apply them across the app: the welcome screen's 'I understand, continue' becomes primary while 'Decline' does not; 'Clear all markers', 'Remove', 'Disable' and 'Sign out' become destructive; per-row actions such as 'Move up' or 'Play next' become quiet. A destructive action never sits immediately beside its harmless neighbour without a spacer between them. Separate toggles from buttons. The Queue, Effects and Transport controls in the now-playing view, the plugin enable checkboxes, and the bypass controls in the effect chain all render as toggles with an unmistakable on state, rather than as labels that happen to highlight. Add the interaction feedback the app has never had: a hover fill on interactive rows and controls, a focus ring distinct from the selected state, and a pressed state. Level and peak meters pick up the positive, warning and danger colours with scale marks at −6 and 0 dB and monospace numerals, and plugin health takes its colour from the same roles while continuing to spell the state out in words. Acceptance: when a view containing a destructive action is displayed, that action is visually distinct from every other button in the view. When the pointer rests on a list row or a toggle, the surface changes. When focus moves by keyboard onto a control that is also selected, the focus ring and the selection remain separately identifiable. When the output level exceeds the limiter ceiling, the meter shows the danger colour and the numeric readout stays aligned with the readouts above it."

**Source**: [ModPlayer-UI-UX-Review.md § 1 Summary](../autonomous/ModPlayer-UI-UX-Review.md#1-summary) (`UX-04`), [§ 3.5 Now Playing](../autonomous/ModPlayer-UI-UX-Review.md#35-now-playing) (`UX-22`), [§ 3.6 Plugins](../autonomous/ModPlayer-UI-UX-Review.md#36-plugins) (`UX-28`), [§ 4.5 Feedback and affordance](../autonomous/ModPlayer-UI-UX-Review.md#45-feedback-and-affordance), [§ 5.4 Component rules](../autonomous/ModPlayer-UI-UX-Review.md#54-component-rules); [ModPlayer-Software-Specification.md](../autonomous/ModPlayer-Software-Specification.md) NFR-6.1 (keyboard operable), NFR-6.2 (accessible names/roles/states), NFR-6.4 (colour never the sole carrier of meaning), NFR-6.5 (contrast minimums in both themes). **Origin**: [specs/autonomous/breakdown/008-design-foundation/002-control-variants.md](../autonomous/breakdown/008-design-foundation/002-control-variants.md). **Prerequisites**: builds on the token roles (`accent`, `positive`, `warning`, `danger`, `text.disabled`, `surface.*`, `mono`, the spacing scale) shipped by [014-design-tokens-and-type-scale](../014-design-tokens-and-type-scale/spec.md), already applied host-wide through `crates/modplayer-ui/src/theme/`.

**Scope boundary**: Styles and re-typed controls only. This feature does not move, group, or re-lay-out any screen (that is [003-list-row-and-panel-components](../autonomous/breakdown/008-design-foundation/003-list-row-and-panel-components.md)), and it does not change what any control does — no click target, keyboard shortcut, confirmation step, or persisted data changes as a result of this feature.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A destructive action is unmistakable before the click (Priority: P1)

A user looking at a screen that contains "Clear all markers", "Remove", "Disable", or "Sign out" can tell, before clicking, that the action is different in kind from the other buttons around it — instead of today's uniform default-styled button that makes every action look equally safe.

**Why this priority**: This is the feature's lead acceptance line and its only safety-relevant outcome — an irreversible or hard-to-reverse action (clearing every marker, signing out, removing an effect, disabling a plugin panel) currently carries no visual warning (`UX-04`).

**Independent Test**: Display a screen containing a destructive action (e.g. the Markers panel header, the Effect Chain row, a plugin's panel row, or the Settings › Account screen) and confirm the destructive button is visually distinct — colour and outline — from every other button in that same screen, and is separated from its nearest neighbour by a spacing gap rather than sitting flush against it.

**Acceptance Scenarios**:

1. **Given** the Markers panel header is displayed with both "New loop region" and "Clear all markers" visible, **When** the panel renders, **Then** "Clear all markers" renders in the destructive variant (danger colour, outline) and "New loop region" does not, and a spacing gap separates the two.
2. **Given** an Effect Chain row is displayed with its bypass toggle, CPU figure, and "Remove" control visible, **When** the row renders, **Then** "Remove" renders in the destructive variant and is separated from the row's other controls by a spacing gap.
3. **Given** a plugin's panel row is displayed with both its Show/Hide and Enable/Disable controls visible, **When** the row renders, **Then** "Disable" (or "Enable", when the panel is currently disabled) renders in the destructive variant only while it reads "Disable", and is separated from the Show/Hide control by a spacing gap.
4. **Given** the Settings › Account screen is displayed for a signed-in user, **When** the screen renders, **Then** "Sign out" renders in the destructive variant, and the sign-out confirmation modal's own confirming button also renders in the destructive variant.

---

### User Story 2 - Toggles look like toggles, not labels that happen to highlight (Priority: P1)

A user glancing at the Now Playing screen's Queue/Effects/Transport controls, a plugin's Enabled checkbox, or an effect's Bypass control can tell at a glance which of these persistent on/off controls is currently on — instead of today's `selectable_label`/`toggle_value` styling that looks identical to an ordinary button (`UX-22`).

**Why this priority**: This is the feature's second acceptance line. `UX-22` is rated P1 in the source audit specifically because the Queue/Effects/Transport toggles look pressed-but-inert at the default window size, making the app appear broken.

**Independent Test**: Display the Now Playing screen and toggle Queue, Effects, or Transport on and off; separately, toggle a plugin's Enabled checkbox and an effect's Bypass control; confirm each control's on-state is rendered with an unmistakable, toggle-specific visual (not a highlighted button) in every case.

**Acceptance Scenarios**:

1. **Given** the Now Playing screen is displayed, **When** the Queue, Effects, or Transport control is toggled on, **Then** it renders with the toggle's on-state visual, distinguishable from a pressed or hovered button.
2. **Given** the Plugins screen is displayed, **When** a plugin's Enabled checkbox is toggled, **Then** its on/off state is rendered with the same toggle visual language as the Now Playing controls.
3. **Given** an Effect Chain row is displayed, **When** its Bypass control is toggled, **Then** it renders with the toggle's on-state visual rather than the current button-like `toggle_value` appearance.
4. **Given** any toggle control, **When** it is compared side by side with an ordinary button in the same variant, **Then** an observer can tell which is a toggle and which is a button without reading the label.

---

### User Story 3 - Interactive rows and controls give feedback (Priority: P2)

A user moving the pointer over a list row or a toggle sees the surface respond; a user tabbing to a control that is also currently selected can still tell the focus ring apart from the selection; a user pressing a button or toggle sees it visibly depress — none of which the app does today (`§ 4.5`: "No hover states on rows, no focus ring distinct from selection, no pressed state beyond egui's default").

**Why this priority**: This is the feature's third acceptance line, covering interaction feedback that today is entirely absent app-wide. It is P2 because, unlike US1/US2, its absence does not currently mislead a user about an action's consequence — it is a comfort and clarity gap, not a safety one.

**Independent Test**: Rest the pointer on a list row and on a toggle and confirm the surface visibly changes in both cases; move keyboard focus onto a control that is also the current selection and confirm the focus ring and the selection remain two separately identifiable signals; press and hold a button or toggle and confirm it renders a distinct pressed appearance.

**Acceptance Scenarios**:

1. **Given** any interactive list row (queue row, plugin row, marker row, search/library result), **When** the pointer rests on it, **Then** its surface fill visibly changes from its resting state.
2. **Given** any toggle control, **When** the pointer rests on it, **Then** its surface fill visibly changes from its resting state, distinct from its on/off state colour.
3. **Given** a control that is both the current selection (e.g. the selected library row) and receives keyboard focus, **When** focus moves onto it, **Then** the focus ring and the selection indicator are each independently identifiable — neither one hides or is replaced by the other.
4. **Given** any button or toggle, **When** the pointer is pressed down on it, **Then** it renders a pressed appearance distinct from its resting and hovered states.

---

### User Story 4 - Meters communicate level state before it becomes a problem (Priority: P2)

A user watching the Now Playing peak meter or an Effect Chain's pre-/post-chain level pair can read, from colour alone as well as from the aligned numeric readout, whether the level is comfortably low, approaching the ceiling, or over it — instead of today's single flat accent-coloured bar that only turns red exactly at the ceiling.

**Why this priority**: This is the feature's fourth acceptance line, directly testable against the source's own acceptance wording ("When the output level exceeds the limiter ceiling, the meter shows the danger colour and the numeric readout stays aligned with the readouts above it").

**Independent Test**: Drive a peak meter's level from silence up through and past its ceiling and confirm the fill progresses positive → warning → danger with scale marks visible at −6 dB and 0 dB, and confirm its numeric readout renders in the same fixed-width numeral style as the readouts in adjacent rows.

**Acceptance Scenarios**:

1. **Given** the Now Playing peak meter, **When** the level is below −6 dBFS, **Then** the filled portion of the meter renders in the `positive` colour role.
2. **Given** the Now Playing peak meter, **When** the level is between −6 dBFS and the active limiter ceiling, **Then** the filled portion renders in the `warning` colour role.
3. **Given** the Now Playing peak meter, **When** the level reaches or exceeds the active limiter ceiling, **Then** the filled portion renders in the `danger` colour role, matching today's over-ceiling behaviour.
4. **Given** the Now Playing peak meter, **When** it renders at any level, **Then** two scale-mark ticks are visible at −6 dB and 0 dB, in addition to the existing ceiling tick.
5. **Given** the Effect Chain's pre-/post-chain peak/RMS level pair, **When** it renders at any level, **Then** its fill follows the same positive/warning/danger banding (using 0 dBFS as the danger boundary, since this meter carries no configurable ceiling) and shows the same two scale marks.
6. **Given** a marker's timestamp and a meter's dB readout shown in adjacent rows, **When** both are rendered, **Then** their digit columns remain aligned (no regression from the `mono` role applied in 014-design-tokens-and-type-scale).

---

### User Story 5 - Primary and quiet actions stay out of each other's way (Priority: P3)

A user scanning the Welcome screen can tell that "I understand, continue" is the one recommended action and "Decline" is not; a user scanning the Queue panel's per-row actions ("Move up", "Move down", "Play next", "Remove") sees them rendered quietly, so they read as row-scoped utilities rather than competing with the screen's primary actions.

**Why this priority**: This is named explicitly in the prompt but is the lowest-impact of the five stories — a filled primary and quiet row actions are a polish/hierarchy improvement, not a safety fix (US1) or a broken-affordance fix (US2), and every button remains fully functional without it.

**Independent Test**: Display the Welcome screen and confirm exactly one button ("I understand, continue") renders in the primary variant while "Decline" renders in the default variant; display the Queue panel with items present and confirm "Move up", "Move down", "Play next", and "Remove" all render in the quiet, text-only variant.

**Acceptance Scenarios**:

1. **Given** the Welcome screen, **When** it renders, **Then** "I understand, continue" renders in the primary variant (filled, accent) and "Decline" renders in the default variant.
2. **Given** any single screen, **When** it renders, **Then** at most one button on that screen uses the primary variant.
3. **Given** a Queue panel row, **When** it renders, **Then** its "Move up", "Move down", "Play next", and "Remove" controls all render in the quiet, text-only variant.

---

### Edge Cases

- A destructive button has no adjacent neighbour in its row (e.g. it is the only control shown) — the spacer rule is a no-op; nothing is added or misrendered.
- The Markers panel's two-step "Clear all markers" confirmation is active — its "Yes" control renders destructive (it performs the deletion) and "No" renders default, with a spacer between them, matching the same adjacency rule as the plain button state.
- A control eligible for a variant or toggle style is disabled (e.g. the Enabled checkbox on an `Invalid` plugin row) — it still renders its assigned variant/toggle appearance composed with the existing disabled treatment (`text.disabled`, non-interactive), and shows no hover or pressed feedback, since it cannot receive pointer interaction.
- A disabled control cannot receive keyboard focus, so it never needs to show a focus ring.
- The Effect Chain's pre-/post-chain level pair has no `ceiling_db` input (unlike the Now Playing peak meter) — its danger-band boundary is fixed at 0 dBFS rather than a per-track ceiling (User Story 4, Scenario 5).
- The active limiter ceiling is set below −6 dBFS — the warning band still runs from −6 dBFS to the ceiling; if the ceiling is above −6 dBFS this band is inverted in practice (i.e., empty) only when the ceiling is at or below −6 dBFS itself, in which case the meter shows positive below the ceiling and danger at/above it, with no warning band rendered.
- A view is resized narrower than a row's controls can fit — this feature does not change layout or wrapping behaviour (Scope boundary); any clipping present today is unaffected.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST define exactly four button variants — `primary` (filled, `accent` role), `default` (today's baseline appearance), `quiet` (text-only, no fill or outline), and `destructive` (outline plus `danger`-role text) — each visually distinguishable from the other three and from an unstyled toolkit default. *(Prompt; § 5.4)*
- **FR-002**: The `primary` variant MUST be applied to the welcome screen's "I understand, continue" (`welcome.rs`, `welcome-acknowledge`) and MUST NOT appear more than once on any single screen. *(Prompt; User Story 5)*
- **FR-003**: The `destructive` variant MUST be applied to: "Clear all markers" and its two-step confirmation's "Yes" control (`markers.rs`, `markers-clear-all`/`markers-clear-yes`); the Effect Chain row's "Remove" control (`effects_view.rs`, `effects-remove`); the plugin panel row's Enable/Disable button, only while it reads "Disable" (`plugins_view.rs`, `plugin-panel-disable`); and "Sign out" together with the sign-out confirmation modal's confirming control (`settings/account.rs`, `account-sign-out` and its modal). *(Prompt; User Story 1)*
- **FR-004**: The `quiet` variant MUST be applied to the Queue panel's per-row actions: "Move up", "Move down", "Play next", and "Remove" (`queue_view.rs`, `queue-move-up`/`queue-move-down`/`queue-play-next`/`queue-remove`). *(Prompt; User Story 5)*
- **FR-005**: Every button not named in FR-002 through FR-004 MUST render in the `default` variant, with its existing click behaviour, keyboard shortcut, and accessible name unchanged, including "Decline" (`welcome-decline`), the transport controls (play/pause, stop, skip), "New loop region", "Add" (effects), "Re-check subscription", "Sign in", and every plugin panel's Show/Hide control. *(Prompt; Scope boundary)*
- **FR-006**: Wherever a `destructive`-variant control sits in the same row or control group as a non-destructive control, a spacing-scale gap (from the token spacing scale established in 014-design-tokens-and-type-scale) MUST separate them, so the destructive control never sits flush against its harmless neighbour. Named instances: the Markers panel header ("Clear all markers" beside "New loop region"), the Effect Chain row ("Remove" beside the row's other controls), the plugin panel row ("Disable" beside Show/Hide), and the two-step marker-clear confirmation ("Yes" beside "No"). *(Prompt; User Story 1, Edge Cases)*
- **FR-007**: The system MUST define a toggle visual — a persistent on/off control whose on state is rendered with an indicator (e.g. a filled track/thumb) that is unmistakable at a glance and visually distinct from a `default`-variant button's hover or pressed state. *(Prompt; User Story 2; § 4.5)*
- **FR-008**: The toggle visual MUST be applied to: the Now Playing screen's Queue, Effects, and Transport disclosure controls (`now_playing.rs`, `queue-toggle`/`effects-toggle`/`transport-toggle`, today `selectable_label`); each plugin's Enabled checkbox (`plugins_view.rs`, `plugins-enable-toggle`); and each Effect Chain row's Bypass control (`effects_view.rs`, `effects-bypass`, today `toggle_value`). *(Prompt; User Story 2)*
- **FR-009**: Every interactive list row (queue, plugin, marker, search/library result rows) and every toggle MUST render a visible surface-fill change while the pointer rests on it, distinct from its resting appearance. *(Prompt; User Story 3; § 4.5)*
- **FR-010**: Every control that supports keyboard focus MUST render a focus ring that is visually distinct from that control's selected/active-state indicator; when a control is simultaneously focused and selected, both signals MUST remain independently identifiable — neither replaces nor visually merges with the other. *(Prompt; User Story 3; § 4.5)*
- **FR-011**: Every button and every toggle MUST render a pressed appearance, distinct from its resting, hovered, and (for toggles) on/off appearances, while the pointer is held down on it. *(Prompt; User Story 3; § 4.5)*
- **FR-012**: The Now Playing peak meter (`widgets/peak_meter.rs`) and the Effect Chain's pre-/post-chain peak/RMS level pair (`widgets/chain_meters.rs`, `level_pair`) MUST render their filled portion in three colour bands keyed to position on the dBFS scale: `positive` below −6 dBFS, `warning` from −6 dBFS up to the danger boundary, and `danger` at or above it. The danger boundary is the active limiter ceiling for the peak meter (preserving today's over-ceiling behaviour) and a fixed 0 dBFS for the level pair, which carries no ceiling input. *(Prompt; User Story 4; § 5.4 "gradient")*
- **FR-013**: The Now Playing peak meter and the Effect Chain level pair MUST each render two scale-mark ticks, at −6 dBFS and 0 dBFS, in addition to the peak meter's existing ceiling tick. *(Prompt; User Story 4; § 5.4)*
- **FR-014**: Every meter numeric readout MUST continue to render in the `mono` type role established by 014-design-tokens-and-type-scale, so that digit columns of adjacent readouts remain aligned; this feature MUST NOT regress that alignment while restyling meter fills. *(Prompt; User Story 4)*
- **FR-015**: Plugin health colouring (`plugins_view.rs`, `health_color`) MUST continue to resolve from the `positive`/`warning`/`danger` roles it already uses (established by 014-design-tokens-and-type-scale) and MUST continue to spell the health state out in words alongside the colour; this feature does not change that mapping and MUST NOT regress it while the surrounding plugin row is restyled. *(Prompt; NFR-6.4)*
- **FR-016**: Every control whose visual variant or toggle style changes under this feature MUST keep its existing accessible name, role, and state exposure, and MUST remain fully keyboard-operable exactly as it is today — restyling a `Checkbox` or `selectable_label`-based control MUST NOT change what assistive technology reports for it. *(NFR-6.1; NFR-6.2; Constitution Principle X)*
- **FR-017**: This feature MUST NOT change what any control does: no click target, keyboard shortcut, confirmation step, or persisted/controller-visible state introduced by FR-001 through FR-015 differs from the control's behaviour before this feature. *(Scope boundary)*
- **FR-018**: This feature MUST NOT move, group, or re-lay-out any screen; row grids, panel structure, and tab/chip layout are unchanged (deferred to 003-list-row-and-panel-components). *(Scope boundary)*
- **FR-019**: Every colour used by the button variants, the toggle visual, the interaction states, and the meter bands MUST be one of the semantic roles already defined in the 014-design-tokens-and-type-scale token module; this feature MUST NOT introduce a new colour literal at any call site it touches, extending 014's zero-literal guarantee (FR-018/FR-018a of 014) to the files this feature modifies. *(014-design-tokens-and-type-scale FR-018a; Constitution Principle VII)*

### Key Entities

- **Button Variant**: One of four named button appearances (`primary`, `default`, `quiet`, `destructive`), each mapped to a fixed set of call sites in this feature and carrying no state of its own — a button's variant is a static property of where it appears, not of application state.
- **Toggle**: A persistent on/off control (disclosure toggle, checkbox, or bypass control) whose current state is rendered with a dedicated on/off visual, distinct from any button's hover/pressed appearance.
- **Interaction State**: One of hover, focus, or pressed — a transient, pointer/keyboard-driven visual overlay applicable to any button, toggle, or interactive row, independent of that control's variant or selected/active state.
- **Meter Colour Band**: One of `positive`/`warning`/`danger`, assigned to a meter's fill by the displayed level's position relative to −6 dBFS and the meter's danger boundary (ceiling or 0 dBFS).
- **Scale Mark**: A fixed-position tick drawn on a meter at a named dBFS value (−6 or 0), independent of the current level or the meter's colour band.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In every screen sampled that contains a destructive-variant action, that action is visually distinguishable (colour and outline) from every other button on the same screen, with a spacing gap separating it from its nearest neighbour — 100% of the named instances in FR-003.
- **SC-002**: In every screen sampled, at most one button renders in the primary variant.
- **SC-003**: When the pointer rests on any interactive list row or toggle, the surface fill visibly changes, sampled across every row/toggle type named in FR-008/FR-009.
- **SC-004**: When keyboard focus lands on a control that is also the current selection, both the focus ring and the selection indicator are identifiable as two distinct signals, sampled across every selectable, focusable control type in the app.
- **SC-005**: When a meter's level reaches or exceeds its danger boundary (ceiling, or 0 dBFS where no ceiling applies), the meter's fill renders in the `danger` colour role.
- **SC-006**: Any two numeric meter or marker readouts shown in adjacent rows keep their digit columns aligned to the pixel — no regression from today's `mono`-role alignment.
- **SC-007**: A source-tree scan for colour literals outside the token module (the scan established by 014-design-tokens-and-type-scale) returns zero new hits across every file this feature modifies.
- **SC-008**: The existing accessibility test suite (accessible name/role/state assertions) passes unchanged for every control this feature restyles, confirming no regression from FR-016.

## Assumptions

- The three colour bands' boundaries (−6 dBFS and the danger boundary) are read from § 5.4's "positive → warning → danger gradient with a scale mark at −6 and 0 dB" as literal band edges rather than a free-form gradient, since the two named scale marks correspond exactly to the two boundaries a three-band scheme needs; no other boundary values appear anywhere in the source material.
- "Remove" in the prompt's destructive list (Prompt: "'Clear all markers', 'Remove', 'Disable' and 'Sign out' become destructive") refers to the Effect Chain's node-removal control (`effects-remove`), not the Queue row's remove action, because the same prompt separately names Queue row actions — including "Play next", immediately alongside "Move up" — as the quiet-variant example set, and the Queue's own remove action (`queue-remove`) sits among those same per-row Queue actions in `queue_view.rs`.
- "Disable" refers to the plugin panel row's Enable/Disable button (`plugin-panel-disable`/`plugin-panel-enable`), the only control in the source tree whose label literally reads "Disable"; a plugin row's own Enabled control is a `Checkbox`, not a button, and this feature gives it the toggle treatment (FR-008) rather than a button variant.
- Hover, focus, and pressed feedback (FR-009 – FR-011) apply to every interactive row and every button/toggle in the application, not only the specific toggle-conversion targets FR-008 names — because § 4.5 describes the missing feedback as an app-wide gap, distinct from the toggle-specific "looks like a label" problem FR-008 addresses.
- No new colour band or state introduced by this feature requires a value beyond the semantic roles already shipped by 014-design-tokens-and-type-scale (`accent`, `positive`, `warning`, `danger`, `text.disabled`, `surface.*`); this feature defines no new token.
- The underlying UI toolkit (established by the MVP UI and by 014-design-tokens-and-type-scale) already supports custom-drawn interactive widgets with hover/focus/pressed response state (the existing `peak_meter`/`chain_meters`/`knob`/`volume` widgets already read pointer and focus state); this feature does not evaluate or change that mechanism, only adds the visual states it renders.
- Plugin health's colour-to-role mapping and its accompanying state word are unchanged by this feature (already migrated to tokens by 014-design-tokens-and-type-scale); FR-015 is a regression guard, not new work.

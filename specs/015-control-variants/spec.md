# Feature Specification: Button, Toggle, and Meter Variants

**Feature Branch**: `016-control-variants`

**Created**: 2026-09-22

**Status**: Clarified 2026-09-22 — no open [NEEDS CLARIFICATION] markers, no open escalations; ready for `/speckit-plan`

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
- The active limiter ceiling is at or below −6 dBFS — the band test is evaluated in the fixed order of FR-012 (danger first, then warning, then positive), so the warning band is simply empty: the meter renders `positive` below the ceiling and `danger` at or above it. No band is ever inverted or drawn backwards.
- A meter's level sits exactly on a band boundary (exactly −6.0 dBFS, or exactly at the danger boundary) — the boundary belongs to the *higher* band, matching today's `peak_db >= ceiling_db` over-ceiling test (FR-012).
- A meter's fill has not yet reached a scale mark's position — that mark is drawn in `text.secondary` over the unfilled track; where the fill has passed it, it is drawn in `surface.base` as a gap cut through the fill (FR-013), so a mark is never drawn in a colour that can collide with the band behind it.
- A view is resized narrower than a row's controls can fit — this feature does not change layout or wrapping behaviour (Scope boundary); any clipping present today is unaffected.
- A quiet-variant control has neither fill nor outline at rest, so its hover and pressed feedback is the only thing that makes it read as interactive — FR-009/FR-011 therefore apply to the quiet variant exactly as to every other button.

## Clarifications

### Session 2026-09-22 (clarify pass)

Resolution ladder applied: **(D)** derived from an authoritative source (the
constitution, the source review document `ModPlayer-UI-UX-Review.md`, the
feature's breakdown file, an already-ratified prior feature spec, or the
current source tree), **(A)** assumed conventional default, recorded here and
written into the spec body as a testable statement. Every decision below is
reflected in the requirements, edge cases, and success criteria that follow —
none lives only in this section. No item met the materiality bar for human
escalation.

**Where the new values live**

- Q: FR-019 forbids a new colour literal at any call site, and 014's automated
  literal scan (014 FR-018a) fails the build on one — but hover fills, pressed
  fills, and a focus ring are colours this feature must introduce. Where do
  they come from? → A (D, 014 FR-008 + FR-018a's `divider` precedent — "8 %
  foreground" became `text.primary` at 8 % alpha computed *inside* the token
  module and exposed as a `divider` value, "neither a semantic role of its own
  nor an alpha written at a call site"): every colour this feature introduces
  is a **derived value** computed in the token module from the ten existing
  roles, exposed by name, and never written at a call site. This feature adds
  **no eleventh semantic role**. The 014 literal scan's exclusion list is
  unchanged.
- Q: `crates/modplayer-ui/src/theme/style.rs`'s `recolor_widget` currently
  gives all five `WidgetVisuals` slots (`noninteractive`, `inactive`,
  `hovered`, `active`, `open`) the *same* fill, stroke, and text colour — so
  after 014 there is no hover or pressed difference at all. Is re-differentiating
  them in scope? → A (D, 014 FR-019 defers "hover/focus/pressed states" to
  "the 002 (control variants) … feature in this same design-foundation wave",
  i.e. this one): yes — this feature owns them. They are re-differentiated
  **once per frame in the token module**, in the same single `Style`
  construction site, not per widget and not per view (014 FR-002). A view that
  calls a plain button gets the states with no call-site edit.

**Interaction-state values (FR-009 – FR-011)**

- Q: "The surface visibly changes" and "a focus ring distinct from the
  selected state" name no value, so `plan` would have to invent them. What are
  they? → A (D, § 5.4's List row bullet states both figures outright: "full-row
  hover fill at 4 % foreground, focus ring = 2 px `accent`"): the hover fill is
  `text.primary` at **4 %** alpha, and the focus ring is a **2 px** `accent`
  stroke. § 5.4 states them for the list row; they are applied as the app-wide
  values, because § 4.5 describes the missing feedback as an app-wide gap and a
  second set of figures for non-row controls would have no source.
- Q: § 4.5 and the prompt require a pressed state, but no source gives its
  value. → A (A, conventional — pressed reads as "more committed" than hover,
  so it is the same fill at greater strength, and reusing the hover derivation
  keeps one mechanism): the pressed fill is `text.primary` at **8 %** alpha,
  exactly double the hover fill. The testable property is the ordering —
  resting, hover, and pressed are three distinct fills with strictly increasing
  alpha — not the specific pair of percentages.
- Q: A focus ring in `accent` landing on a control whose *selected* state is an
  `accent` fill (014 FR-010b: "`accent` drives selection fill") would merge
  into it, which is precisely what acceptance scenario US3-3 forbids. How do
  both stay identifiable? → A (A, the conventional focus-ring *offset*): the
  ring is drawn **outside** the control's rect with a 1 px gap of the
  underlying surface between the ring and the control, so on an accent-filled
  selection it reads as a separate band rather than a thicker edge. The
  selection indicator stays interior (`accent` fill with `text.on-accent` text,
  unchanged from 014); the ring stays exterior. Distinctness is therefore
  structural — it does not depend on the two colours differing.
- Q: Does this feature animate the transitions between these states? → A (A,
  Principle X's YAGNI and 014's design note that `animation_time` is left
  untouched): no. States change on the frame the input changes. No animation,
  easing, or transition duration is introduced.

**The toggle visual (FR-007, FR-008)**

- Q: FR-007 says the on-state indicator is "e.g. a filled track/thumb" — an
  example is not a testable requirement. What is the toggle? → A (A,
  conventional, and the one form that satisfies NFR-6.4 without adding a word):
  a **switch** — a pill-shaped track (`radius.full`) with a circular thumb at
  one end. Off: `surface.raised` track, 1 px `divider` outline, thumb at the
  leading end. On: `accent` track, thumb in `text.on-accent` at the trailing
  end. The state is carried by the thumb's **position** as well as by colour,
  so colour is not the sole carrier (NFR-6.4), and a switch cannot be mistaken
  for a button at any size — which a highlighted label demonstrably can be
  (`UX-22`).
- Q: FR-008 names three groups of controls, but the source tree holds other
  persistent boolean on/off controls that it does not name — `effects_view.rs`'s
  Formant, Mute, Mono-sum, Phase-invert and Channel-swap `toggle_value`s,
  `queue_view.rs`'s Shuffle, `markers.rs`'s loop-arm `Checkbox`,
  `settings/audio.rs`'s safe-volume `Checkbox`, `settings/plugins.rs`'s boolean
  plugin-setting fields, and `plugin_panels.rs`'s plugin-contributed
  checkboxes. Do they become toggles too? → A (A, conventional — a checkbox in
  Settings that looks unlike a checkbox in Plugins is a worse outcome than
  either): **yes.** Every persistent boolean on/off control in the application
  renders as the toggle, app-wide, for the same reason § 4.5 treats the
  feedback gap as app-wide. FR-008's three named groups are the *acceptance*
  set, not the *application* set.
- Q: Then does every `selectable_label` become a toggle, since the Now Playing
  disclosure controls are `selectable_label`s? → A (D, the scope boundary and
  003's ownership of "Tabs vs. chips" in § 5.4): **no.** The line is
  *boolean on/off* versus *one-of-N selection*. Library tabs, the nav rail
  (`shell.rs`), Settings categories and search hits (`settings/mod.rs`), combo
  options (`settings/audio.rs`, `settings/plugins.rs`), and plugin list-item
  selection (`plugin_panels.rs`) are selection, not toggles: they keep their
  current appearance here and belong to 003-list-row-and-panel-components. They
  still gain the hover, focus, and pressed feedback of FR-009 – FR-011, which
  is app-wide.
- Q: Plugin-contributed panels call `ui.checkbox` through the host. Does giving
  those the toggle visual change the plugin API, triggering Principle IX's
  written change-request requirement? → A (D, `contracts/ui-panels.md` A4 and
  the identical finding in 014 FR-015b): no. The toggle is installed in the
  shared style once per frame, and plugin panels are already bound by A4 to
  read `ui.visuals()`/`ui.style()`, so plugin-contributed booleans inherit it
  with no new API surface. Principle IX is not triggered.
- Q: Restyling a `Checkbox` or a `selectable_label` as a switch — does the
  accessible role change with it? → A (D, FR-016, NFR-6.2, and the existing
  `crates/modplayer-ui/tests/accessibility.rs` role/`Toggled` assertions):
  **no.** A control keeps the accessible role and state it reports today; only
  its painted appearance changes. `Checkbox`-based controls keep their checkbox
  role, `selectable_label`-based disclosure controls keep theirs, and both
  already expose their on/off state as a toggled/selected state. Changing a
  reported role would break SC-008 and is out of scope.

**Button variants (FR-001 – FR-006)**

- Q: FR-001 names the four variants but not what each one paints, so two
  implementations could both "satisfy" it. → A (D, § 5.4's Buttons bullet —
  "`primary` (filled accent, one per view), `default`, `subtle` (text-only, for
  row-level actions), `danger` (outlined, `danger` text)" — plus 014 FR-010a
  for the text drawn on an accent fill): `primary` = `accent` fill, no outline,
  `text.on-accent` label. `default` = today's baseline (`surface.raised` fill,
  1 px `divider` outline, `text.primary` label). `quiet` = no fill, no outline,
  `text.primary` label. `destructive` = no fill, 1 px `danger` outline,
  `danger` label. (§ 5.4's `subtle`/`danger` are this spec's `quiet`/
  `destructive`; the names differ, the definitions do not.)
- Q: `destructive` is distinguished from `default` by colour alone (a
  `danger` outline and `danger` text against a `divider` outline and
  `text.primary` text). Does that violate NFR-6.4, "colour never the sole
  carrier of meaning"? → A (D, § 4.6 records NFR-6.4 as already met because
  "health, markers and severities all carry text", and the same argument
  applies): no — every destructive control's own label already states the
  action in words ("Clear all markers", "Remove", "Disable", "Sign out"), and
  the FR-006 spacer is a second non-chromatic signal. No icon or badge is
  added.
- Q: FR-006 requires "a spacing-scale gap" but names no step, and the default
  gap between two adjacent controls is already `sm` (8 px, `style.rs`'s
  `item_spacing`). Which step? → A (A, conventional — a separator that is not
  clearly larger than the default gap does not read as deliberate): **`lg`
  (16 px)**, double the default `sm` inter-control gap. Testable as "the gap
  adjacent to a destructive control is at least twice the default item
  spacing", so a later change to the spacing scale does not silently void the
  rule.
- Q: The prompt lists "Remove" as destructive *and* lists Queue row actions as
  the quiet example set, and `queue_view.rs` has its own `queue-remove`. Which
  wins for the Queue row? → A (A, and already reasoned in this spec's
  Assumptions — restated here as a decision because `plan` reads the body):
  `queue-remove` renders **quiet**, not destructive. Removing a queue entry is
  trivially reversible (re-add from Library or Search); removing an effect node
  is not. The destructive variant's value is its scarcity — spending it on a
  reversible row action weakens the signal on `effects-remove` and "Sign out".
- Q: FR-002 says `primary` appears "at most once on any single screen", but the
  window can show a screen, the plugin dock, and a modal at once. What is a
  "screen" for that rule? → A (A, conventional — the rule exists so the eye
  finds one recommended action, and the eye sees the whole window): everything
  visible in the window at one moment, including any open modal and the plugin
  dock, counts as one view for the at-most-one rule.

**Meters (FR-012 – FR-014)**

- Q: FR-012 says the fill renders "in three colour bands keyed to position on
  the dBFS scale" — is the whole fill one colour chosen by the current level,
  or is the bar segmented so each portion carries its own band? → A (A,
  conventional for a level meter, and the only reading that matches US4's own
  Independent Test, "the fill progresses positive → warning → danger"):
  **segmented.** Each horizontal portion of the filled bar is drawn in the band
  its own dB position falls in, so a bar driven past the ceiling shows all
  three. The acceptance line ("when the level exceeds the ceiling the meter
  shows the danger colour") is satisfied and made precise: the *rightmost*
  filled column is `danger`.
- Q: The Effect Chain level pair draws its RMS sub-bar as
  `selection.bg_fill.gamma_multiply(0.7)` — a dimmed copy of the peak colour.
  Does the RMS sub-bar band too, and does it keep the dim? → A (A,
  conventional): it bands identically to the peak sub-bar, and the 0.7 dim is
  **removed** — dimming a `danger` band would defeat the very signal FR-012
  exists to give, and the two sub-bars are already told apart by occupying
  separate halves of the track and by their own labelled `mono` readouts
  ("Peak: …", "RMS: …").
- Q: The peak meter's ceiling tick is painted in `warn_fg_color`, which after
  FR-012 is also a *band fill* colour — a warning tick over a warning band is
  invisible. And the 0 dBFS scale mark sits at the scale maximum, i.e. on the
  meter's own right edge. How are the marks drawn? → A (D, the contrast floors
  014 already guarantees — `positive`/`warning`/`danger` each ≥4.5:1 against
  `surface.base` (014 FR-014) and `text.secondary` ≥4.5:1 against it
  (014 FR-011)): a mark is drawn in **`surface.base`** where the fill has
  reached it (a gap cut through the band) and in **`text.secondary`** where it
  has not (a tick on the empty track). One rule, both meters, both themes, no
  new floor to verify: the mark's contrast against whatever is behind it is
  already guaranteed by 014's table. The ceiling tick adopts the same rule,
  keeping its 2 px width to stay distinguishable from the 1 px −6/0 dB marks.
  The 0 dBFS mark is drawn inset by its own width so it remains visible against
  the meter's border stroke.
- Q: The meter bands carry level state in colour. NFR-6.4 says colour is never
  the sole carrier. → A (D, § 4.6 + FR-014's `mono` readout): satisfied without
  new work — each meter already renders a `mono` dB readout and exposes the
  same figure as its accessible value, and the scale marks give the bands fixed
  positional boundaries. FR-014 is the regression guard on that readout; this
  feature must not remove or unlabel it.

**Scope seams and verification**

- Q: § 5.4's hover-fill and focus-ring figures appear in its **List row**
  bullet, and 003-list-row-and-panel-components owns list rows. Does 002 or 003
  own row hover and focus? → A (D, this feature's own prompt — "a hover fill on
  interactive rows and controls, a focus ring distinct from the selected state,
  and a pressed state" — against 003's title and this spec's scope boundary):
  **002 owns the interaction-state visuals** (hover, focus, pressed) for rows
  and controls alike; **003 owns row geometry** (the three-column grid, artwork
  size, truncation, the "…" menu). No requirement here moves or restructures a
  row.
- Q: SC-001 – SC-005 read like screenshot judgements ("visually
  distinguishable", "visibly changes"). Who verifies them? → A (D, Principle
  VIII and the Governance § Manual Scenario Sign-Off, exactly as 014 resolved
  the same question): **both, at different times.** Automated tests assert
  everything derivable from values — the four variants are four distinct
  (fill, stroke, text) triples; resting/hover/pressed are three distinct fills
  with increasing alpha; the focus ring is 2 px `accent` and offset; the band
  selector returns `positive`/`warning`/`danger` for the right dB inputs in the
  right order; scale-mark positions; and the `accessibility.rs` role/state
  assertions still pass. The quickstart's manual scenarios, driven and captured
  by the implementing agent per Governance, are the evidence that the rendered
  pixels match.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST define exactly four button variants, each painted from the semantic roles as follows, and each resolving to a (fill, outline, label) triple distinct from the other three: `primary` — `accent` fill, no outline, `text.on-accent` label; `default` — `surface.raised` fill, 1 px `divider` outline, `text.primary` label (today's baseline); `quiet` — no fill, no outline, `text.primary` label; `destructive` — no fill, 1 px `danger` outline, `danger` label. (§ 5.4's `subtle`/`danger` are this spec's `quiet`/`destructive`; the names differ, the definitions do not.) *(Prompt; § 5.4; Clarifications)*
- **FR-001a**: The `destructive` variant's distinction from `default` is carried by colour plus two non-chromatic signals — the control's own label, which states the action in words, and the FR-006 spacer — so NFR-6.4 is satisfied without adding an icon or badge. No destructive control's meaning MUST depend on colour alone. *(NFR-6.4; § 4.6; Clarifications)*
- **FR-002**: The `primary` variant MUST be applied to the welcome screen's "I understand, continue" (`welcome.rs`, `welcome-acknowledge`) and MUST NOT appear more than once in any single view. A **view**, for this rule, is everything visible in the window at one moment — the active screen together with any open modal and the plugin dock. *(Prompt; User Story 5; Clarifications)*
- **FR-003**: The `destructive` variant MUST be applied to: "Clear all markers" and its two-step confirmation's "Yes" control (`markers.rs`, `markers-clear-all`/`markers-clear-yes`); the Effect Chain row's "Remove" control (`effects_view.rs`, `effects-remove`); the plugin panel row's Enable/Disable button, only while it reads "Disable" (`plugins_view.rs`, `plugin-panel-disable`); and "Sign out" together with the sign-out confirmation modal's confirming control (`settings/account.rs`, `account-sign-out` and its modal). *(Prompt; User Story 1)*
- **FR-004**: The `quiet` variant MUST be applied to the Queue panel's per-row actions: "Move up", "Move down", "Play next", and "Remove" (`queue_view.rs`, `queue-move-up`/`queue-move-down`/`queue-play-next`/`queue-remove`). `queue-remove` MUST render `quiet`, **not** `destructive`, because removing a queue entry is trivially reversible (re-add from Library or Search) where removing an effect node is not, and because the destructive variant's value depends on its scarcity. *(Prompt; User Story 5; Clarifications)*
- **FR-005**: Every button not named in FR-002 through FR-004 MUST render in the `default` variant, with its existing click behaviour, keyboard shortcut, and accessible name unchanged, including "Decline" (`welcome-decline`), the transport controls (play/pause, stop, skip), "New loop region", "Add" (effects), "Re-check subscription", "Sign in", and every plugin panel's Show/Hide control. *(Prompt; Scope boundary)*
- **FR-006**: Wherever a `destructive`-variant control sits in the same row or control group as a non-destructive control, a gap of the `lg` step (16 px) from the 014 spacing scale MUST separate them, so the destructive control never sits flush against its harmless neighbour. The testable property is that this gap is **at least twice the application's default inter-control item spacing** (`sm`, 8 px), so a later change to the spacing scale cannot silently void the rule. Named instances: the Markers panel header ("Clear all markers" beside "New loop region"), the Effect Chain row ("Remove" beside the row's other controls), the plugin panel row ("Disable" beside Show/Hide), and the two-step marker-clear confirmation ("Yes" beside "No"). *(Prompt; User Story 1, Edge Cases)*
- **FR-007**: The system MUST define the toggle visual as a **switch**: a pill-shaped track (`radius.full`) carrying a circular thumb at one end. Off — `surface.raised` track, 1 px `divider` outline, thumb at the leading end. On — `accent` track, `text.on-accent` thumb at the trailing end. The thumb's **position** carries the state alongside the colour, so no toggle's state depends on colour alone (NFR-6.4), and the shape MUST NOT be mistakable for any button variant's resting, hover, or pressed appearance. *(Prompt; User Story 2; § 4.5; NFR-6.4; Clarifications)*
- **FR-008**: The toggle visual MUST be applied to the acceptance set: the Now Playing screen's Queue, Effects, and Transport disclosure controls (`now_playing.rs`, `queue-toggle`/`effects-toggle`/`transport-toggle`, today `selectable_label`); each plugin's Enabled checkbox (`plugins_view.rs`, `plugins-enable-toggle`); and each Effect Chain row's Bypass control (`effects_view.rs`, `effects-bypass`, today `toggle_value`). *(Prompt; User Story 2)*
- **FR-008a**: The toggle visual MUST additionally be applied to **every other persistent boolean on/off control in the application**, so that one boolean control never looks unlike another: `effects_view.rs`'s Formant, Mute, Mono-sum, Phase-invert and Channel-swap `toggle_value`s; `queue_view.rs`'s Shuffle; `markers.rs`'s loop-arm `Checkbox`; `settings/audio.rs`'s safe-volume `Checkbox`; `settings/plugins.rs`'s boolean plugin-setting fields; and `plugin_panels.rs`'s plugin-contributed checkboxes. *(§ 4.5 as an app-wide gap; Clarifications)*
- **FR-008b**: Controls that express **one-of-N selection** rather than a boolean MUST NOT be converted to toggles and MUST keep their current appearance under this feature: Library tabs (`library_view.rs`), the nav rail (`shell.rs`), Settings categories and search hits (`settings/mod.rs`), combo options (`settings/audio.rs`, `settings/plugins.rs`), and plugin list-item selection (`plugin_panels.rs`). Their appearance is 003-list-row-and-panel-components' scope. They MUST still receive the hover, focus, and pressed feedback of FR-009 – FR-011, which is app-wide. *(Scope boundary; § 5.4 "Tabs vs. chips"; Clarifications)*
- **FR-008c**: The toggle visual MUST be installed in the shared style once per frame (as 014 FR-002 installs the tokens), so plugin-contributed boolean controls inherit it through `contracts/ui-panels.md` A4's existing `ui.visuals()`/`ui.style()` binding. This feature MUST NOT add or change any plugin API surface, so Principle IX's written change-request requirement is not triggered. *(Principle IX; `contracts/ui-panels.md` A4; 014 FR-015b; Clarifications)*
- **FR-009**: Every interactive list row (queue, plugin, marker, search/library result rows), every button variant, and every toggle MUST render a **hover fill of `text.primary` at 4 % alpha** over its resting appearance while the pointer rests on it. That value MUST be computed inside the token module as a named derived value, never written at a call site. *(Prompt; User Story 3; § 4.5 and § 5.4 "hover fill at 4 % foreground"; Clarifications)*
- **FR-010**: Every control that supports keyboard focus MUST render a focus ring of **2 px `accent`, drawn outside the control's rect with a 1 px gap of the underlying surface between the ring and the control**. The offset gap is what keeps the ring identifiable when the control is simultaneously the current selection (whose indicator is an interior `accent` fill with a `text.on-accent` label, unchanged from 014 FR-010b): the two signals stay separable **structurally**, not by differing in colour. Neither signal MUST replace, hide, or visually merge with the other. *(Prompt; User Story 3; § 4.5; § 5.4 "focus ring = 2 px `accent`"; Clarifications)*
- **FR-011**: Every button variant and every toggle MUST render a **pressed fill of `text.primary` at 8 % alpha** — double the FR-009 hover fill — while the pointer is held down on it, distinct from its resting, hovered, and (for toggles) on/off appearances. The normative, testable property is the ordering: resting, hover, and pressed MUST be three distinct fills of strictly increasing alpha. *(Prompt; User Story 3; § 4.5; Clarifications)*
- **FR-011a**: The five widget-state slots that 014 currently paints identically (`noninteractive`, `inactive`, `hovered`, `active`, `open` in `theme/style.rs`'s `recolor_widget`) MUST be re-differentiated so that `hovered` and `active` carry FR-009's and FR-011's fills. This MUST happen in the token module's single `Style` construction site, once per frame, not per widget and not per view — a view that calls a plain button gets the states with no call-site edit. *(014 FR-002; 014 FR-019's deferral of hover/focus/pressed to this feature; Clarifications)*
- **FR-012**: The Now Playing peak meter (`widgets/peak_meter.rs`) and the Effect Chain's pre-/post-chain peak/RMS level pair (`widgets/chain_meters.rs`, `level_pair`) MUST render their fill **segmented by dB position**: each horizontal portion of the filled bar is drawn in the band its own position on the dBFS scale falls in, so a bar driven past the danger boundary shows `positive`, then `warning`, then `danger` across its length. The band for a given dB value is selected in this fixed order: `danger` if the value is at or above the danger boundary, else `warning` if it is at or above −6 dBFS, else `positive` — so a boundary value belongs to the higher band (matching today's `peak_db >= ceiling_db` test) and a danger boundary at or below −6 dBFS simply yields an empty warning band. The danger boundary is the active limiter ceiling for the peak meter (preserving today's over-ceiling behaviour) and a fixed 0 dBFS for the level pair, which carries no ceiling input. *(Prompt; User Story 4; § 5.4 "gradient"; Clarifications)*
- **FR-012a**: In the Effect Chain level pair, the RMS sub-bar MUST band identically to the peak sub-bar, and its existing `gamma_multiply(0.7)` dimming MUST be removed — dimming a `danger` band would defeat the signal FR-012 exists to give. The two sub-bars remain distinguishable by occupying separate halves of the track and by their own labelled `mono` readouts. *(FR-012; FR-019; Clarifications)*
- **FR-013**: The Now Playing peak meter and the Effect Chain level pair MUST each render scale-mark ticks at −6 dBFS and 0 dBFS (1 px wide), in addition to the peak meter's ceiling tick (2 px wide, so the two kinds stay distinguishable). Every one of these marks MUST be drawn in `surface.base` where the fill has reached its position — a gap cut through the band — and in `text.secondary` where it has not. This single rule guarantees the mark's contrast in both themes from floors 014 already verifies (`positive`/`warning`/`danger` ≥4.5:1 against `surface.base` per 014 FR-014; `text.secondary` ≥4.5:1 against it per 014 FR-011) and removes today's `warn_fg_color` ceiling tick, which after FR-012 would be invisible over a `warning` band. The 0 dBFS mark sits at the scale maximum and MUST be inset by its own width so it stays visible against the meter's border stroke. *(Prompt; User Story 4; § 5.4; 014 FR-011/FR-014; Clarifications)*
- **FR-014**: Every meter numeric readout MUST continue to render in the `mono` type role established by 014-design-tokens-and-type-scale, so that digit columns of adjacent readouts remain aligned; this feature MUST NOT regress that alignment while restyling meter fills. That readout, together with the meter's accessible value and FR-013's fixed-position scale marks, is what keeps the meter's level state from being carried by colour alone (NFR-6.4) — it MUST NOT be removed, unlabelled, or made hover-only where it is visible today. *(Prompt; User Story 4; NFR-6.4; Clarifications)*
- **FR-015**: Plugin health colouring (`plugins_view.rs`, `health_color`) MUST continue to resolve from the `positive`/`warning`/`danger` roles it already uses (established by 014-design-tokens-and-type-scale) and MUST continue to spell the health state out in words alongside the colour; this feature does not change that mapping and MUST NOT regress it while the surrounding plugin row is restyled. *(Prompt; NFR-6.4)*
- **FR-016**: Every control whose visual variant or toggle style changes under this feature MUST keep its existing accessible name, role, and state exposure, and MUST remain fully keyboard-operable exactly as it is today — restyling a `Checkbox` or `selectable_label`-based control MUST NOT change what assistive technology reports for it. Specifically: `Checkbox`-based controls keep their checkbox role and toggled state, and `selectable_label`-based disclosure controls keep theirs; painting either as FR-007's switch changes only pixels. The existing `crates/modplayer-ui/tests/accessibility.rs` role/state assertions MUST pass unmodified. *(NFR-6.1; NFR-6.2; Constitution Principle X; Clarifications)*
- **FR-017**: This feature MUST NOT change what any control does: no click target, keyboard shortcut, confirmation step, or persisted/controller-visible state introduced by FR-001 through FR-015 differs from the control's behaviour before this feature. *(Scope boundary)*
- **FR-018**: This feature MUST NOT move, group, or re-lay-out any screen; row grids, panel structure, and tab/chip layout are unchanged (deferred to 003-list-row-and-panel-components). The seam with 003 is: **this feature owns the interaction-state visuals** (hover, focus, pressed) for rows and controls alike, while **003 owns row geometry** — the three-column grid, artwork size, truncation, and the "…" menu. FR-006's spacer is the only spacing value this feature sets, and it sets no other geometry. *(Scope boundary; Clarifications)*
- **FR-019**: Every colour used by the button variants, the toggle visual, the interaction states, and the meter bands MUST resolve to one of the ten semantic roles already defined in the 014-design-tokens-and-type-scale token module, or to a **derived value computed inside that module** from those roles (as 014's `divider` is `text.primary` at 8 % alpha). This feature MUST NOT define an eleventh semantic role, and MUST NOT write a colour, an alpha, or a stroke width at any call site it touches — extending 014's zero-literal guarantee (014 FR-018/FR-018a) to the files this feature modifies, with 014's exclusion list (the token module itself and test code) unchanged. The derived values this feature adds are the hover fill (FR-009), the pressed fill (FR-011), and the focus-ring stroke (FR-010). *(014 FR-008/FR-018a; Constitution Principle VII; Clarifications)*
- **FR-020**: A control that is disabled MUST render its assigned variant or toggle appearance composed with the existing disabled treatment (`text.disabled`, the theme's `disabled_alpha`) and MUST show no hover, focus, or pressed feedback, since it can receive neither pointer interaction nor keyboard focus. *(Edge Cases; 014 FR-012)*
- **FR-021**: This feature MUST NOT introduce animation, easing, or transition timing for any state change: a variant, toggle, hover, focus, or pressed state changes on the frame its input changes. The toolkit's `animation_time` and `interaction` settings stay as 014 left them. *(Constitution Principle X — YAGNI; Clarifications)*
- **FR-022**: The decisions above MUST be verified by automated tests running alongside the existing `fmt`/`clippy`/`test`/`deny` gates, asserting from the token values themselves — that the four variants resolve to four distinct (fill, outline, label) triples; that resting, hover, and pressed are three distinct fills of strictly increasing alpha; that the focus-ring stroke is 2 px `accent` and is offset from the control rect; that the FR-012 band selector returns the expected role for inputs below, at, and above each boundary, including a danger boundary at or below −6 dBFS; that FR-013's scale-mark positions and colour-selection rule hold; and that `accessibility.rs`'s role/state assertions still pass. The quickstart's manual scenarios, executed and captured by the implementing agent per Governance § Manual Scenario Sign-Off, are the evidence that the rendered pixels match those values. *(Constitution Principle VIII; Governance § Manual Scenario Sign-Off; 014 FR-018b; Clarifications)*

### Key Entities

- **Button Variant**: One of four named button appearances (`primary`, `default`, `quiet`, `destructive`), each mapped to a fixed set of call sites in this feature and carrying no state of its own — a button's variant is a static property of where it appears, not of application state.
- **Toggle**: A persistent **boolean** on/off control (disclosure toggle, checkbox, bypass, or effect-parameter switch), rendered as FR-007's switch — pill track plus thumb, state carried by thumb position as well as colour. Distinct in kind from a one-of-N **selection** control (tab, nav entry, category, combo option, selected row), which this feature does not convert (FR-008b).
- **Interaction State**: One of hover, focus, or pressed — a transient, pointer/keyboard-driven visual overlay applicable to any button, toggle, or interactive row, independent of that control's variant or selected/active state. Each is a derived token value (hover 4 % and pressed 8 % of `text.primary`; focus a 2 px offset `accent` ring), installed once per frame, never written at a call site.
- **Meter Colour Band**: One of `positive`/`warning`/`danger`, assigned per horizontal position of a meter's fill by that position's dB value relative to −6 dBFS and the meter's danger boundary (ceiling, or 0 dBFS where no ceiling applies). A meter's fill may show several bands at once (FR-012).
- **Scale Mark**: A fixed-position tick drawn on a meter at a named dBFS value (−6, 0, or the ceiling), independent of the current level, drawn in `surface.base` where the fill has reached it and in `text.secondary` where it has not (FR-013).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In every screen sampled that contains a destructive-variant action, that action is visually distinguishable (danger outline and danger label) from every other button on the same screen, with a gap of at least twice the default item spacing separating it from its nearest neighbour — 100% of the named instances in FR-003.
- **SC-002**: In every view sampled (screen plus any open modal and the plugin dock, per FR-002), at most one button renders in the primary variant.
- **SC-003**: When the pointer rests on any interactive list row, button, or toggle, the surface fill visibly changes, sampled across every row/control type named in FR-008, FR-008a, FR-008b, and FR-009.
- **SC-004**: When keyboard focus lands on a control that is also the current selection, both the focus ring and the selection indicator are identifiable as two distinct signals, sampled across every selectable, focusable control type in the app — the offset gap of FR-010 is visible between them even where ring and selection share the `accent` role.
- **SC-005**: When a meter's level reaches or exceeds its danger boundary (ceiling, or 0 dBFS where no ceiling applies), the **rightmost filled column** of the meter renders in the `danger` colour role, while lower portions of the same fill still render their own bands.
- **SC-009**: The automated variant/state test (FR-022) passes: four distinct variant triples, three strictly increasing state-fill alphas, a 2 px offset `accent` focus ring, and a band selector that returns the right role at, below, and above every boundary including a danger boundary at or below −6 dBFS.
- **SC-010**: Every persistent boolean on/off control named in FR-008 and FR-008a renders the switch visual, and every one-of-N selection control named in FR-008b renders unchanged — a source-level inventory that finds no boolean control still drawn as a plain `Checkbox`/`toggle_value`/`selectable_label` and no selection control converted.
- **SC-006**: Any two numeric meter or marker readouts shown in adjacent rows keep their digit columns aligned to the pixel — no regression from today's `mono`-role alignment.
- **SC-007**: A source-tree scan for colour literals outside the token module (the scan established by 014-design-tokens-and-type-scale) returns zero new hits across every file this feature modifies.
- **SC-008**: The existing accessibility test suite (accessible name/role/state assertions) passes unchanged for every control this feature restyles, confirming no regression from FR-016.

## Assumptions

- The three colour bands' boundaries (−6 dBFS and the danger boundary) are read from § 5.4's "positive → warning → danger gradient with a scale mark at −6 and 0 dB" as literal band edges rather than a free-form gradient, since the two named scale marks correspond exactly to the two boundaries a three-band scheme needs; no other boundary values appear anywhere in the source material.
- "Remove" in the prompt's destructive list (Prompt: "'Clear all markers', 'Remove', 'Disable' and 'Sign out' become destructive") refers to the Effect Chain's node-removal control (`effects-remove`), not the Queue row's remove action, because the same prompt separately names Queue row actions — including "Play next", immediately alongside "Move up" — as the quiet-variant example set, and the Queue's own remove action (`queue-remove`) sits among those same per-row Queue actions in `queue_view.rs`.
- "Disable" refers to the plugin panel row's Enable/Disable button (`plugin-panel-disable`/`plugin-panel-enable`), the only control in the source tree whose label literally reads "Disable"; a plugin row's own Enabled control is a `Checkbox`, not a button, and this feature gives it the toggle treatment (FR-008) rather than a button variant.
- Hover, focus, and pressed feedback (FR-009 – FR-011) apply to every interactive row and every button/toggle in the application, not only the specific toggle-conversion targets FR-008 names — because § 4.5 describes the missing feedback as an app-wide gap, distinct from the toggle-specific "looks like a label" problem FR-008 addresses.
- The toggle visual reaches every persistent boolean on/off control app-wide (FR-008a), not only FR-008's three acceptance groups, while one-of-N selection controls keep their current appearance (FR-008b) — the boolean/selection line is what separates this feature from 003's tab and row work (Clarifications).
- No new **semantic role** is introduced: every colour is one of 014's ten roles or a derived value computed inside the token module from them (FR-019). The three derived values this feature adds are the 4 % hover fill, the 8 % pressed fill, and the 2 px offset `accent` focus ring, whose figures come from § 5.4's List row bullet (Clarifications).
- The underlying UI toolkit (established by the MVP UI and by 014-design-tokens-and-type-scale) already supports custom-drawn interactive widgets with hover/focus/pressed response state (the existing `peak_meter`/`chain_meters`/`knob`/`volume` widgets already read pointer and focus state); this feature does not evaluate or change that mechanism, only adds the visual states it renders.
- 014 deliberately painted all five widget-state slots identically and deferred hover/focus/pressed to this feature (014 FR-019), so re-differentiating them (FR-011a) is expected work here, not a change to 014's decisions.
- Plugin-contributed boolean controls inherit the toggle visual through the shared style and `contracts/ui-panels.md` A4, with no plugin API change and therefore no Principle IX change request — the same finding 014 recorded as its FR-015b.
- Plugin health's colour-to-role mapping and its accompanying state word are unchanged by this feature (already migrated to tokens by 014-design-tokens-and-type-scale); FR-015 is a regression guard, not new work.

# Feature Specification: Effect Chain Rows and Meters

**Feature Branch**: `feature/024-effect-chain-rows-and-meters`

**Created**: 2026-09-29

**Status**: Draft

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/010-now-playing-workbench/004-effect-chain-rows-and-meters.md (id 004, effect-chain-rows-and-meters)."

**Source breakdown**: `specs/autonomous/breakdown/010-now-playing-workbench/004-effect-chain-rows-and-meters.md`

**Source review**: `specs/autonomous/ModPlayer-UI-UX-Review.md` § 3.5 Now Playing (`UX-25`), § 5.4 Component rules, § 4.4 Typography and text

**Requirement IDs implemented**: FR-6.2.2 (each node labeled with type and owning plugin/host), FR-6.2.5 (per-node and whole-chain CPU-load indicator), FR-6.4.1 (peak/RMS levels pre- and post-chain), FR-6.4.2 (low-resolution spectrum), NFR-6.1 (keyboard operable), NFR-6.2 (accessible names/roles/states), NFR-6.4 (color never the sole carrier of meaning); review finding `UX-25`.

**Prerequisites**: Builds on the effect chain and its built-in nodes delivered by [008-effect-chain-and-built-in-nodes](../008-effect-chain-and-built-in-nodes/spec.md) (row data: identity, bypass, per-node/whole-chain CPU, drag-and-drop plus keyboard reorder, add/remove, pre-/post-chain peak+RMS and the 64-band spectrum) and the button, toggle, and meter variants delivered by [015-control-variants](../015-control-variants/spec.md) (button variants including `primary`/`destructive`, toggle styling, meter scale marks at −6/0 dB with `mono` numerals). Runs inside the panel-card and single-scroll-region layout delivered by [021-transport-bar-and-panel-layout](../021-transport-bar-and-panel-layout/spec.md).

**Scope boundary**: Covers the presentation of the effect chain panel — its rows and its meters. Node behavior, budget enforcement (which node auto-bypasses and when), and the custom buffer processor remain with 008-effect-chain-and-built-in-nodes and the later community-registry custom-buffer-processor feature; this feature does not change what any control does, only how it is grouped, labeled, and drawn.

## Clarifications

### Session 2026-09-29 (clarify pass)

Resolution ladder applied: (D) derived from an authoritative source (constitution, breakdown/review, 008/015/021 specs, existing code), (A) assumed conventional default. Every item was checked against the materiality test; none is both underivable and consequential (this feature is presentation-only — no data model, engine, or cross-feature contract changes), so no escalation section exists.

- Q: The prompt lists "enabled, bypassed" as state — is there a separate enable control? → A (D, 008 `NodeRow` has only `bypassed`/`auto_bypassed`; scope boundary forbids behavior changes): **no new control or data**. The state zone holds the existing `Bypass` toggle (on = bypassed, off = enabled/active), the per-node CPU-budget figure, and — when flagged — the existing "Auto-bypassed (over budget)" and "Quality mode auto-switched" notes. Both notes move out of the parameters zone into the state zone. FR-001 realigned.
- Q: What is "the budget" the CPU figures are measured against, and what text labels them? → A (D, 008 Clarifications 2026-09-18: `cost_pct`/`total_cost_pct` are percentages of the callback period — the real-time budget — and an overload event is >90 % for 3 consecutive callbacks or >100 % on one): the header reads **"Chain CPU: { $pct } % of real-time budget"** and **"Budget overruns: { $count }"**; each row's state zone reads **"{ $pct } % of budget"**. Numbers render with `mono` numerals. Hover text on the header figure explains "Share of each audio callback's time spent in the effect chain; an overrun is counted when it stays above 90 % or exceeds 100 %." Strings are new/updated Fluent keys in both `en-US` and `pt-BR` (Constitution X). Over-budget badge text/logic unchanged. FR-006 realigned.
- Q: Which frequency ticks does the spectrum show, and where? → A (A, conventional decade ticks on a log axis; the existing display maps 20 Hz–20 kHz log-spaced): **labeled ticks at 100 Hz, 1 kHz and 10 kHz** (labels "100", "1k", "10k"), plus unlabeled minor ticks at 50, 200, 500, 2k, 5k Hz, positioned by the same log mapping as the bars (`x = log10(f/20)/log10(1000)` of the plot width). Tick labels sit in a strip **below** the bar area, never overlapping bars; at any width ≥ the widget's 160 px minimum the three labels do not overlap each other. FR-008 realigned.
- Q: What is the spectrum's "amplitude reference"? → A (D, the existing bar scale is a 60 dB log scale, 0 dBFS top to −60 dBFS floor, −30 dBFS = half height — `chain_meters::spectrum_bar_height`): **horizontal reference lines at 0, −30 and −60 dBFS**, labeled "0 dB", "−30", "−60" in a gutter beside the plot (not over the bars), drawn in `text.secondary`/scale-mark colors so they stay visible over silence. FR-008 realigned.
- Q: How does the spectrum apply the positive/warning/danger convention (FR-009)? → A (D, review § 5.4 "Meters" and 015's meter bands, −6/0 dB boundaries): spectrum bar segments below −6 dBFS use `positive`, −6..0 dBFS `warning`, at/above 0 dBFS `danger` — replacing today's single `selection.bg_fill` color. Non-color carrier: the labeled 0 dB reference line (bar height against the labeled scale). Level pairs keep their existing banding and −6/0 dB marks unchanged.
- Q: What does "numbers in tabular figures" require beyond today's `mono` readouts? → A (A, conventional fixed-width meter readout): peak and RMS readouts keep `mono` text **and** are formatted to a constant character width (e.g., right-aligned value padded so "-inf dB", "-6.0 dB" and "-60.0 dB" occupy the same width), so the column does not shift as values change. Test: the formatted readout string has the same char count for −∞, −60.0, −6.0 and 0.0 dB.
- Q: What does the drag affordance look like? → A (A, conventional grip + cursor): the handle renders a grip glyph (e.g., "⠿"/two-column dots, replacing the lone "⋮") in `text.secondary`; pointer hover shows `CursorIcon::Grab`, an active drag shows `CursorIcon::Grabbing`; keyboard focus shows the app's standard focus ring. Pointer DnD and `↑`/`↓` behavior unchanged. FR-003 realigned.
- Q: All handles are named "Reorder" — should the name identify the node? → A (A, conventional a11y fix; NFR-6.2 needs distinguishable names): the handle's accessible name becomes **"Reorder { $kind }, position { $position }"** (new Fluent key, en-US + pt-BR); it still contains the "Reorder" word so existing role/name lookups can match by prefix. FR-013 realigned.
- Q: Accessible structure for zones in egui/AccessKit? → A (A, convention: containers need not be named if their controls are): zones are visual groups; AccessKit **tab/focus order MUST follow identity → state → parameters → actions** within a row, and each focusable control keeps (or gains) an accessible name. Zones are not required to expose their own accessibility nodes. FR-013 realigned.
- Q: What counts as "the unit inside the control"? → A (D, existing code): egui `Slider`/`DragValue` `.suffix()` renders inside the value box — already true for semitones (" st"), tempo (" %"), gain (" dB"), EQ frequency (" Hz"), EQ gain (" dB") and filter cutoff (" Hz"). Parameters with no physical unit (EQ Q, resonance 0–1, stereo width 0–2, balance −1..1) stay unitless; this feature does **not** add units or rescale them (would change displayed semantics, FR-014). The parameter's name label stays adjacent to its control within the parameters zone. SC-002 realigned to "every unit-bearing parameter".
- Q: Where does the mode combo go, and how is the destructive spacing satisfied? → A (D, review § 5.4): mode/type/filter-mode combos are parameters → parameters zone. The actions zone contains only `Remove` (`Destructive` variant, unchanged), preceded by the existing `destructive_gap` spacer and right-aligned in the row when the row fits on one line.
- Q: What does the empty state say and how does the primary action work? → A (A, conventional two-step empty state; review § 5.4 "primary, one per view"; `Variant::Primary` has no other use in Now Playing): with zero nodes the panel shows the explanation **"Effect nodes process the audio on its way to your speakers — shift pitch, change tempo, shape tone or set level. Nodes run top to bottom in the order you add them."** and one `Primary` button **"Add effect node"**. Activating it (click/Enter/Space) replaces the button with the existing kind combo + "Add" confirm (the confirm styled `Primary`, so still one primary in view) and moves keyboard focus to the kind combo. The revealed/collapsed state is egui temp memory (per-viewer convenience) and resets to collapsed whenever the chain becomes empty again. The header CPU/overrun figures and meters stay visible in the empty state. New strings in en-US + pt-BR. FR-010/FR-011 realigned.
- Q: Row layout at narrow widths? → A (D, 021 single scroll region, 5.5 min window 960×640, existing `horizontal_wrapped`): zones wrap as whole units onto additional lines (a zone never splits across the row boundary with another zone's controls interleaved); long kind/owner text truncates with ellipsis and full text on hover. Test at 960 px window width: no control overlaps another and nothing paints outside the panel card.

### Earlier notes

The three ambiguities from the specify pass were resolved with informed defaults; each default is recorded in Assumptions (now superseded where the Session 2026-09-29 entries above are more specific).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A node's row reads as four things, not one run-on line (Priority: P1)

A musician opens the Effect Chain panel with two or more nodes in it. Today each row is a single unbroken line — drag handle, position, name, source, bypass, a CPU figure, an unlabeled checkbox, a value, a unit, two mode words, a dropdown, and a remove action all run together with no separation between what the node *is* and what it is *set to*. Instead, each row groups into four visually distinct zones, left to right: **identity** (drag handle, position number, node name, host/plugin source), **state** (enabled/bypassed toggle and the node's share of the chain's CPU budget), **parameters** (this node's own controls, each value showing its unit inside the control), and **actions** (remove, and any other row-level action). The drag handle also gains a visible affordance that it is draggable, in addition to the `↑`/`↓` keyboard reordering that already exists.

**Why this priority**: This is the review's lead finding for the panel (`UX-25`) and the breakdown prompt's primary acceptance line; every other row-level improvement builds on this grouping.

**Independent Test**: Add two nodes of different kinds to the chain, open the panel, and confirm each row's identity, state, parameters, and actions are visually distinguishable as four separate groups rather than one continuous line of controls.

**Acceptance Scenarios**:

1. **Given** two nodes are in the chain, **When** the panel renders, **Then** each row shows its identity, state, parameters, and actions as four visually distinct groups (e.g., through grouping, spacing, or alignment that a user can point to without reading every label).
2. **Given** a pitch-shift node whose semitones parameter is set to two, **When** its row renders, **Then** the unit ("st") appears attached to the value inside the same control, not as a separate label elsewhere in the row.
3. **Given** a node's row, **When** the user looks at its drag handle, **Then** it is visually distinguishable as a drag control (for example, by cursor affordance and/or a grip-style glyph) independent of hover, and it remains reachable and operable by keyboard (`↑`/`↓` while focused).
4. **Given** a node is dragged onto a new position in the chain, **When** the drop completes, **Then** the chain reorders and every row's position number updates to match the new order.
5. **Given** a node's row, **When** the user looks for its remove action, **Then** it renders in the row's actions zone, separated from the parameters zone, and — being a destructive action — is not immediately adjacent to a non-destructive control without a spacer between them.

---

### User Story 2 - The chain's budget figures and meters read as measurements (Priority: P2)

A musician glances at the panel header and its meters while a set is running. Today the header shows a bare CPU percentage and an overload count with no indication that these are measured against a budget, the pre-/post-chain peak and RMS numbers are not visually distinguished from each other at a glance, and the spectrum display is an unlabeled bar chart with no way to tell what frequency or level any bar represents. Instead, the header's CPU and overload figures are labeled as the budget figures they are, the pre-/post-chain level pairs show peak and RMS with numerals that line up in a fixed-width column, and the spectrum gains visible frequency ticks (so a bar's approximate frequency can be read without hovering) and an amplitude reference (so a bar's approximate level can be read without hovering).

**Why this priority**: This is the breakdown prompt's second acceptance line — it makes the meters function as instruments a performer can read at a glance rather than decoration, building on the row-grouping work of User Story 1.

**Independent Test**: Open the panel with the chain under some load and confirm the header text identifies the CPU and overload figures as budget-relative, and that the spectrum shows frequency tick marks and an amplitude reference alongside its bars.

**Acceptance Scenarios**:

1. **Given** the panel header, **When** it renders, **Then** the whole-chain CPU figure and the overload count are labeled in a way that identifies them as measured against the chain's CPU budget (not bare, context-free numbers).
2. **Given** the pre-chain and post-chain level pairs, **When** they render, **Then** each shows peak and RMS values with numerals in a fixed-width (tabular) column, consistent with the app's existing meter numeral convention.
3. **Given** the spectrum display, **When** it renders, **Then** it shows visible frequency tick marks spanning its displayed range and a visible amplitude reference, in addition to its band bars.
4. **Given** the pre-/post-chain meters and the spectrum, **When** a value crosses into the warning or danger range, **Then** the same positive/warning/danger meter convention the rest of the app already uses is applied, never relying on color alone.

---

### User Story 3 - An empty chain explains itself (Priority: P3)

A musician opens the Effect Chain panel before adding any nodes. Today the panel shows only a bare kind dropdown and an "Add" button with no explanation of what an effect node is or does. Instead, the panel explains in plain language what an effect node does and offers adding one as a clearly primary action, so a first-time user understands what the control is for before they use it.

**Why this priority**: This is the breakdown prompt's third acceptance line — a smaller, self-contained improvement that only affects the zero-node state and does not block User Stories 1 or 2.

**Independent Test**: Open the panel with zero nodes in the chain and confirm it shows explanatory text about what an effect node does, with a single primary-styled action to add one, instead of a bare dropdown.

**Acceptance Scenarios**:

1. **Given** the chain has zero nodes, **When** the panel renders, **Then** it shows text explaining what an effect node does, and a single primary action that starts adding one, instead of showing the kind-selection dropdown as the first thing the user sees.
2. **Given** the chain has zero nodes, **When** the user activates the primary action, **Then** the add-node control (kind selection plus confirm) becomes available, consistent with how adding a node already works today.
3. **Given** the chain has at least one node, **When** the panel renders, **Then** the empty-state explanation is not shown, and the ordinary "Add node…" control appears below the rows, as it does today.

---

### Edge Cases

- A node with no adjustable parameters relevant to display (none of the six built-in kinds today, but the layout must not break if a future kind had an empty parameter zone): the parameters zone renders empty rather than collapsing the row's four-zone structure.
- A very long node name or source label: it truncates or wraps without pushing the actions zone off the visible row width, consistent with the app's existing text-expansion handling elsewhere.
- The chain is at its 16-node capacity: the existing inline "chain is full" refusal still renders below the last row; this feature does not change that behavior, only the rows' internal layout.
- A node is auto-bypassed because the chain went over budget: its existing "Auto-bypassed (over budget)" note still renders, now inside the row's state zone (grouped with the other state indicators) rather than floating among parameter controls.
- The chain is idle (not playing): CPU and overload figures show their existing zero/quiet values, still labeled as budget figures.
- The pre-/post-chain levels or a spectrum band are silent (−∞ dBFS / zero magnitude): the meters and spectrum render their existing floor/zero treatment, with tick marks and the amplitude reference still visible.
- The window is narrow (near the app's minimum size): the four row zones wrap onto additional lines rather than overlapping or clipping each other, and the spectrum's tick labels do not overlap its bars.
- The chain transitions from zero nodes to one node (or back to zero after the last node is removed): the empty-state explanation and the ordinary rows/add-control swap in the same frame the node count changes, with no stale content from the other state left visible.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Each node's row MUST present four visually distinct zones, left to right: identity (drag handle, 1-based position number, node kind name, host/plugin source), state (the existing `Bypass` toggle, the node's "{ $pct } % of budget" figure, and the "Auto-bypassed (over budget)" / "Quality mode auto-switched" notes when flagged), parameters (this node kind's own controls, including its mode/type combos), and actions (`Remove`, preceded by the destructive spacer). No new enable control or row data is added. *(Prompt; UX-25; FR-6.2.2, FR-6.2.5; User Story 1)*
- **FR-002**: Every parameter control that has a unit (semitones "st", tempo "%", gain/EQ gain "dB", EQ frequency/cutoff "Hz") MUST show that unit attached to its value inside the control itself (the value box's suffix), never as a separate label elsewhere in the row. Unitless parameters (EQ Q, resonance, stereo width, balance) stay unitless and keep their current ranges and display. *(Prompt "Acceptance"; User Story 1, Scenario 2)*
- **FR-003**: The row's drag handle MUST present a visible affordance that it is a drag control: a grip glyph (two-column dots, replacing the lone "⋮") visible without hover, `Grab` cursor on pointer hover and `Grabbing` during a drag, and the standard focus ring on keyboard focus — in addition to remaining keyboard-focusable and operable with the existing `↑`/`↓` reordering. *(Prompt "gains a visible drag affordance in addition to the existing keyboard reordering"; User Story 1, Scenario 3)*
- **FR-004**: When a node is dragged onto a new position (pointer drag-and-drop, unchanged from today) or moved with `↑`/`↓` on its focused handle, the chain MUST reorder and every row's displayed position number MUST reflect the new order. *(Prompt "Acceptance"; User Story 1, Scenario 4; regression guard — already delivered by 008, restated as an acceptance line for this feature's row rework)*
- **FR-005**: The row's actions zone MUST be visually separated from the parameters zone, and a destructive action within it (remove) MUST NOT sit immediately adjacent to a non-destructive control without a spacer between them. *(Prompt; review § 5.4 "Destructive actions never sit adjacent to their non-destructive neighbour without a spacer"; User Story 1, Scenario 5)*
- **FR-006**: The panel header's whole-chain CPU figure and overload count MUST be labeled as budget figures — "Chain CPU: { $pct } % of real-time budget" and "Budget overruns: { $count }" (en-US; pt-BR equivalents), numerals in `mono`, with hover text explaining the budget (share of each audio callback's time; overrun counted above 90 % sustained or 100 % once, per 008) — rather than as bare, context-free numbers. The budget is the callback period, as defined by 008. *(Prompt "labels them as the budget figures they are"; User Story 2, Scenario 1)*
- **FR-007**: The pre-chain and post-chain level pairs MUST show peak and RMS values with their numerals in a fixed-width (tabular) column, consistent with the app's existing meter numeral convention (review § 5.4 "Meters"): `mono` text formatted to a constant character width for every value from −∞ to 0 dB, so the readout never shifts horizontally as values change. *(Prompt "their numbers in tabular figures"; User Story 2, Scenario 2)*
- **FR-008**: The spectrum display MUST show visible frequency tick marks spanning its displayed range and a visible amplitude reference, in addition to its band bars, so it reads as a measurement rather than decoration: labeled frequency ticks at 100 Hz, 1 kHz, 10 kHz ("100", "1k", "10k") plus unlabeled minor ticks at 50, 200, 500, 2k, 5k Hz on the same 20 Hz–20 kHz log mapping as the bars, labels in a strip below the plot; and horizontal amplitude reference lines at 0, −30 and −60 dBFS labeled in a side gutter. Labels never overlap bars or each other at the widget's minimum width (160 px). *(Prompt "gains frequency ticks and an amplitude reference"; User Story 2, Scenario 3)*
- **FR-009**: The pre-/post-chain level meters and the spectrum MUST use the app's existing positive/warning/danger meter convention for values in the warning or danger range, and MUST NOT rely on color alone to convey that state. For the spectrum: bar segments below −6 dBFS `positive`, −6..0 dBFS `warning`, ≥ 0 dBFS `danger` (replacing the single selection color); the labeled 0 dB reference line is the non-color carrier. Level pairs keep their existing bands and −6/0 dB marks. *(NFR-6.4; review § 5.4 "Meters"; User Story 2, Scenario 4)*
- **FR-010**: While the chain has zero nodes, the panel MUST show text explaining what an effect node does ("Effect nodes process the audio on its way to your speakers — shift pitch, change tempo, shape tone or set level. Nodes run top to bottom in the order you add them.", en-US; pt-BR equivalent) and a single `Primary` button "Add effect node", in place of showing the kind-selection dropdown as the first visible control. The header CPU/overrun figures and meters remain visible in this state. *(Prompt "the panel says what an effect node does and offers the add control as its primary action instead of showing a bare dropdown"; User Story 3, Scenario 1)*
- **FR-011**: Activating the empty-state primary action (click, Enter or Space) MUST replace it with the existing add-node control (kind combo plus "Add" confirm, the confirm styled `Primary` so exactly one primary remains in view) and move keyboard focus to the kind combo; the revealed state is per-viewer egui temp memory and resets to collapsed whenever the chain becomes empty again — all without changing how adding a node already behaves (including the existing at-capacity refusal, which cannot occur from zero nodes). *(User Story 3, Scenario 2)*
- **FR-012**: Once the chain has at least one node, the panel MUST show the ordinary "Add node…" control below the rows and MUST NOT show the empty-state explanation. *(User Story 3, Scenario 3)*
- **FR-013**: Every row zone, its controls, the drag handle's affordance, the budget-labeled header figures, and the spectrum's tick marks and amplitude reference MUST expose an accessible name, role, and state to assistive technology and MUST remain operable by keyboard alone; none of this feature's changes MAY remove or degrade any accessible name, role, or keyboard path the panel already exposes. Zones are visual groups and need not expose their own accessibility nodes, but focus order within a row MUST run identity → state → parameters → actions. The drag handle's accessible name MUST identify its node: "Reorder { $kind }, position { $position }" (still beginning with the existing "Reorder" label). The spectrum's accessible value keeps its current peak-frequency reading. *(NFR-6.1, NFR-6.2)*
- **FR-015**: Every new or changed user-visible string (budget labels, handle name, empty-state text and button, spectrum tick/reference labels) MUST be an externalized Fluent key present in both `en-US` and `pt-BR`. *(Constitution X, NFR-7.1)*
- **FR-016**: At the minimum window size (960 × 640), row zones MUST wrap as whole units onto additional lines with no overlap and nothing painted outside the panel card; long kind/owner text truncates with an ellipsis and shows the full text on hover. *(Review § 5.5; 021 single scroll region)*
- **FR-014**: None of this feature's changes MAY alter a node's behavior, its parameter semantics or ranges, which node auto-bypasses under an over-budget condition, or the custom buffer processor — only the presentation of the chain, its rows, and its meters. *(Scope boundary)*

### Key Entities

- **Effect Node Row**: one node's line in the chain, now structured as four zones — Identity (handle, position, kind name, source), State (bypass, CPU-budget share, auto-bypassed note when applicable), Parameters (this kind's own controls, units inside each value), Actions (remove). Existing data (008): kind, owner, bypassed, auto-bypassed flag, cost percentage, per-kind parameter values, mode-note flag.
- **Chain Header**: the whole-chain CPU figure and overload count, now labeled as figures measured against the chain's CPU budget, plus the over-budget badge (unchanged).
- **Level Pair**: a pre- or post-chain peak+RMS reading, now with tabular (fixed-width) numerals; unchanged data (peak/RMS, left/right, linear amplitude).
- **Spectrum**: the post-chain band display, now with visible frequency tick marks and an amplitude reference in addition to its bars; unchanged data (per-band linear magnitude).
- **Empty Chain State**: the zero-node presentation — explanatory text plus a single primary "add a node" action, replacing the bare kind dropdown shown today in that state.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With two or more nodes in the chain, a user can identify each row's identity, state, parameters, and actions as four separate groups without reading every control's label, in 100% of rows checked.
- **SC-002**: For every unit-bearing parameter (semitones, tempo %, gain dB, EQ frequency Hz, EQ gain dB, filter cutoff Hz), the unit is visible attached to the value inside its control (e.g., pitch shift at +2 reads "2.00 st" in the value box), checked across all six kinds.
- **SC-003**: Dragging a node to a new position updates that node's and every other affected node's displayed position number to match the new order, with zero stale position numbers, in every reorder checked.
- **SC-004**: A user reading the panel header can state that the CPU and overload figures represent usage against a budget, without consulting any other part of the app, in 100% of checks.
- **SC-005**: A user can read an approximate frequency (against the 100 / 1k / 10k ticks) and an approximate level (against the 0 / −30 / −60 dB references) from the spectrum display without hovering over it, in 100% of checks.
- **SC-006**: With zero nodes in the chain, a user can state what an effect node does and identify the single primary action to add one, without opening any other panel or screen, in 100% of checks.
- **SC-007**: Every control this feature touches (row zones, drag handle, header figures, meters, spectrum, empty-state action) remains reachable and operable using the keyboard alone, with zero regressions against the panel's existing keyboard paths.

## Assumptions

- The "visible drag affordance" (FR-003) is satisfied by a combination of an on-hover/on-focus cursor or highlight change and a recognizable grip-style glyph; it does not require a new interaction model beyond the existing pointer drag-and-drop and `↑`/`↓` keyboard reordering, both of which are unchanged (008-effect-chain-and-built-in-nodes).
- "Labels them as the budget figures they are" (FR-006) is a wording/labeling change to the existing CPU and overload figures already shown in the header — it does not add a new metric, change what is measured, or change the over-budget threshold or badge logic (owned by 008).
- The spectrum's "frequency ticks" and "amplitude reference" (FR-008) reuse the scale-mark colors and `mono` label style the app already applies to the pre-/post-chain meters (015-control-variants, review § 5.4 "Meters"); the concrete tick frequencies (100 Hz / 1 kHz / 10 kHz labeled, 50/200/500/2k/5k minor) and reference levels (0 / −30 / −60 dBFS, matching the existing 60 dB bar scale) are fixed in Clarifications 2026-09-29 and FR-008.
- The empty-chain explanatory text (FR-010) describes, in general terms, that an effect node processes audio in the chain between the decoder and the output (e.g., pitch, tempo, tone, or level) — it does not enumerate all six built-in kinds or their parameters; the existing per-kind labels remain the source of that detail once a node is added.
- "Primary action" (FR-010) means the `primary` button variant already defined by 015-control-variants (filled accent, at most one per view) — the effect chain panel currently has no other primary-styled control, so this introduces the panel's one primary action without conflicting with that rule.
- Node behavior, parameter ranges/semantics, budget enforcement (which node auto-bypasses and when), and the custom buffer processor are unchanged; this feature is presentation-only, consistent with the breakdown's scope boundary.

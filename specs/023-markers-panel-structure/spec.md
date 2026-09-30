# Feature Specification: Markers Panel Structure

**Feature Branch**: `feature/023-markers-panel-structure`

**Created**: 2026-09-29

**Status**: Draft

**Input**: User description: "Give the markers panel the structure a practice session needs. It is currently one flat list where a loop boundary, a named point and a cue slot all render identically as a coloured dot, a name and a timestamp, with a destructive 'Clear all markers' button sitting immediately beside 'New loop region'. Group the panel by kind: the loop region and its A and B boundaries first, with the arm state, repeat count and crossfade shown as part of that group; then named point markers; then the numbered cue slots, with empty slots visible so a user can see which numbers are free. Each row shows its colour swatch, its name, its timestamp in tabular figures, and quiet row actions to jump to it, nudge it, or remove it. A marker can be renamed in place, and its colour picked from the marker palette, without leaving the panel. Separate the destructive action from the constructive one: 'New loop region' stays at the top of the loop group, while clearing all markers moves away from it, adopts the destructive styling, and keeps its existing two-step confirmation. Each group shows its count in the header, and the panel states plainly what to do when it is empty — naming the shortcut that places a marker rather than leaving a blank area."

**Source**: [specs/autonomous/breakdown/010-now-playing-workbench/003-markers-panel-structure.md](../autonomous/breakdown/010-now-playing-workbench/003-markers-panel-structure.md), which cites [ModPlayer-UI-UX-Review.md § 3.5 Now Playing](../autonomous/ModPlayer-UI-UX-Review.md#35-now-playing) (`UX-24`), [§ 5.4 Component rules](../autonomous/ModPlayer-UI-UX-Review.md#54-component-rules), [§ 4.4 Typography and text](../autonomous/ModPlayer-UI-UX-Review.md#44-typography-and-text), [§ 5.3 Semantic colour](../autonomous/ModPlayer-UI-UX-Review.md#53-semantic-colour-both-themes-contrast-verified). **Prerequisites**: builds on the marker/loop-region/cue primitives and keyboard table of [006-markers-loops-and-cues](../006-markers-loops-and-cues/spec.md) (`crates/modplayer-ui/src/markers.rs`, contracts/ui-markers.md §4) and the panel card and row grammar of [016-list-row-and-panel-components](../016-list-row-and-panel-components/spec.md) (`widgets::controls::collapsible_panel_card`, row hover/focus treatment). Renders inside the panel-card slot [021-transport-bar-and-panel-layout](../021-transport-bar-and-panel-layout/spec.md) already reserves for Markers, first of the four Now Playing panel cards.

**Scope boundary**: Covers only the Markers panel's internal structure — grouping, row layout, row actions, and empty states — and the in-place rename/recolor interaction the prompt names. It does not change marker/loop-region/cue semantics, keyboard shortcuts, persistence, the 64-marker limit, the crossfade/repeat-count model, or beat-grid snapping — all of that stays owned by 006-markers-loops-and-cues (and, for beat snapping, a future practice-depth feature). It does not change the panel's position among Now Playing's other cards, its collapse mechanism, or its persisted open/closed flag — all owned by 021-transport-bar-and-panel-layout. It does not change the waveform lanes' glyph shapes or the overlay rendering on the waveform itself (006/022-waveform-legibility), only what a marker's hover tooltip on the lane reflects after an in-panel rename.

**Traceability**: UX-24 (breakdown source); NFR-6.1 (keyboard-operable), NFR-6.2 (accessible names/roles/states), NFR-6.4 (colour never the sole carrier of meaning), NFR-7.1 (externalized strings) — per constitution Principle X and Governance ("Specs and tasks MUST reference the requirement IDs").

## Clarifications

### Session 2026-09-29 (clarify reviewer)

Each entry is either **Derived** (settled by a cited source) or an **Assumed default** (conventional, constitution-consistent choice, recorded so plan/tasks can test it). No material decision remained open, so nothing is escalated to the product owner.

1. **Empty-state contents** — *Derived* (current `markers::panel` always renders "New loop region"; US3 AS1 "if the panel is otherwise empty, at the top of the panel"; prompt "'New loop region' stays"). On a track with zero markers the panel shows exactly: the `markers-empty` message ("No markers — press I to set A") and the "New loop region" button. It shows no group headers, no cue-slot rows, and no "Clear all markers". This supersedes the earlier draft wording that hid "New loop region" on an empty panel (FR-017, FR-020, SC-007 and US4 AS1 are realigned).
2. **Colour picking mechanism** — *Derived* (006 contracts/ui-markers.md §4: the swatch cell is an "8-swatch popup (`markers-color { $index }`), keyboard `C` cycles"; prompt "its colour picked from the marker palette"). Activating a row's swatch (click, or `Enter`/`Space` while it has focus) opens a popover showing all 8 `theme::MARKER_PALETTE` swatches, with the current one marked by a non-colour indicator (outline/check). Choosing a swatch sets that palette index, and the popover then closes. `Esc` or clicking outside closes it without any change. The focused-marker `C` key keeps cycling, unchanged. The current code's click-to-cycle swatch is a drift from the 006 contract, and this feature corrects it.
3. **Opening rename by pointer** — *Assumed default*. A single primary click on a populated row's name cell opens the inline rename, and `F2`/`Enter` on a focused row still opens it (006). The name cell is always a click target. When a marker's name is empty, the cell shows a muted placeholder (`markers-name-placeholder`, "Add name") instead of nothing. An empty name still displays as the role only everywhere else (lane tooltip, accessible name), as in 006. Enter commits, and so does losing focus (click-away or Tab). `Esc` cancels. The 006 name content rules are unchanged.
4. **Group headers** — *Assumed default*. Each group header is a non-interactive sub-header inside the Markers card. It is **not** individually collapsible. It shows the group label followed by the count in `mono` figures, via Fluent keys `markers-group-loop { $count }` ("Loop region"), `markers-group-points { $count }` ("Points"), and `markers-group-cues { $count }` ("Cues"). The header's accessible name includes the count (e.g. "Points, 2"). A zero count renders as `0` (only the Loop Region group can show 0; see FR-001/FR-004). The Cues count is the number of occupied slots, not "n of 8".
5. **Row-action visibility, order, and form** — *Assumed default* (NFR-6.1; 016's "…" control is also always present). The row actions are **always rendered**, not revealed on hover, so keyboard and touch users can reach them. They use 015's lowest-emphasis (ghost/quiet) control variant, with icon glyphs. Each has a tooltip and an accessible name: `markers-jump` ("Jump to marker"), `markers-nudge-earlier` ("Nudge earlier"), `markers-nudge-later` ("Nudge later"), and `markers-remove` ("Remove marker"). They sit in a trailing, right-aligned column after the position/clamped glyph, in the order jump, nudge earlier, nudge later, remove. "Remove" uses the same quiet variant as the others, not the destructive variant.
6. **Row tab order** — *Assumed default*. Tab order within a populated row is swatch → name → jump → nudge earlier → nudge later → remove. Rows follow visual order: Loop Region blocks (A row, B row, then loop cells), then Points, then occupied Cues. Empty cue-slot rows are **not** tab stops. The row-level focus that drives the 006 §3 focused-marker key table (arrows, Delete, F2, C) is kept.
7. **Nudge row-action step** — *Assumed default*. Each click or activation moves the marker by exactly 1× the configured nudge step (Settings › Playback "Marker nudge step"). The keyboard's `Shift` 10× variant stays keyboard-only; the pointer actions have no modifier behaviour.
8. **Remove row action** — *Assumed default* (mirrors the 006 keyboard delete, which has no confirmation). Removal is immediate, with no confirmation and no undo. After removal, keyboard focus moves to the next populated row in visual order, or to the previous one when none follows. If the panel becomes empty, focus moves to "New loop region".
9. **Jump row action** — *Derived* (006 FR-014; contracts §2 `jump_to_cue`). It performs the same seek-preserving-play-state operation as a cue jump, for every marker kind. It does **not** change the focused marker, the current region, or the arm state. Any armed-loop consequence of the seek (e.g. jumping to an armed region's B boundary) follows the engine's existing seek semantics with no special case. While a marker drag is in progress, jump is a no-op for the dragged marker.
10. **Empty cue-slot rows** — *Assumed default*. An empty cue-slot row is non-interactive: no click action, no tab stop, and no hover fill. It renders in the muted text role and shows the slot number and its fill shortcut via `markers-cue-empty { $slot }` ("Cue { $slot } — empty · Shift+{ $slot } to set"). All 8 slot rows render whenever the panel has any content, including when 0 cues exist but a region or point does.
11. **Placement of "Clear all markers" and inline status** — *Assumed default*. "Clear all markers" renders alone in a footer row at the bottom of the Markers card, trailing-aligned, below the Cues group. It uses 015's destructive control variant. Its "Clear N markers?" confirm/cancel pair replaces it in place in that footer. The `markers-status` refusal label renders directly below the card header, above the Loop Region group (and above the empty-state message on an empty panel).
12. **Loop Region block layout** — *Derived* (006 contract §4 plus this feature's FR-003). The loop cells move from the A row to a line after the block's last boundary row. An incomplete region with only a B marker renders its B row, then its cells. Consecutive region blocks are separated by an existing spacing token, and the block itself gets no new "current region" indicator; the existing row focus highlight is unchanged. "New loop region" sits as the first line of the Loop Region group, directly below its header.
13. **Locale scope** — *Derived* (the repository's `locales/` holds only `en-US`; 006/016 add keys only to en-US). New keys go into the en-US Fluent bundle that already holds the `markers-*` keys. The constitution's pt-BR target (Principle X / NFR-7.1) is project-level work outside this feature. Externalizing every string keeps adding pt-BR purely additive.
14. **Non-colour distinction between kinds** — *Derived* (NFR-6.4). Group membership is conveyed by group headers and by each row's role label (`A`/`B`/`Cue n`/`Marker`), never by swatch colour alone. SC-001's "at a glance" is satisfied by the header and position, not by colour.
15. **Verification harness** — *Assumed default*. FR-025's tests live in `crates/modplayer-ui/tests/markers.rs`, extending the 006 contract §9 suite (egui harness plus AccessKit tree assertions). The swatch popover, row actions and group headers are asserted by accessible name and role.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See loop regions, points, and cues as three distinct groups (Priority: P1)

A musician opens the Markers panel on a track that has a loop region, a couple of named points, and a cue point, and immediately sees three labelled sections — Loop Region, Points, Cues — each stating how many items it holds, instead of today's single flat list where a loop boundary, a named point, and a cue slot render as identical rows with no way to tell them apart at a glance.

**Why this priority**: This is the feature's core structural value and its first acceptance line — everything else in this feature (row actions, in-place editing, the destructive-action move, empty states) is scoped *within* this new grouping, so nothing else can be built or verified without it existing first.

**Independent Test**: Load a track with an armed loop region, two point markers, and one cue point; confirm the panel shows exactly three labelled groups, in the order Loop Region, Points, Cues, each with an accurate count in its header, and confirm the loop region's arm toggle, repeat count, and crossfade render as part of the Loop Region group rather than as a separate block.

**Acceptance Scenarios**:

1. **Given** a track with an armed loop region, two point markers, and one cue, **When** the Markers panel renders, **Then** it shows three labelled groups — Loop Region, Points, Cues — in that order, reading counts of 1, 2, and 1 respectively.
2. **Given** the Loop Region group, **When** it renders, **Then** the region's A and B boundary rows appear together, immediately followed by that region's arm toggle, repeat-count field, crossfade field, and wraps-remaining/infinite indicator, all inside the same group.
3. **Given** a track with two loop regions (one armed, one not), **When** the Loop Region group renders, **Then** both regions appear as separate blocks inside the one group, its header count reads 2, and each block shows its own A/B rows and loop cells.
4. **Given** a track with markers of only one or two kinds (e.g. only points, no loop region and no cues), **When** the panel renders, **Then** the Loop Region group (header count 0 plus its "New loop region" control), the Points group, and the Cues group (all 8 slots empty) are shown, and the Points group is omitted whenever it has no point markers — no group renders as a bare, misleadingly-countable "0" heading with nothing useful beneath it other than Loop Region's own constructive control.
5. **Given** any two markers, **When** they are compared inside the same group, **Then** they are ordered by position (earliest first), matching the ordering already used elsewhere in the app.

---

### User Story 2 - Act on any marker row without leaving the panel (Priority: P1)

A musician looks at any row — a loop boundary, a point, or a cue — and can rename it in place, pick its colour from the marker palette, jump the playhead to it, nudge it earlier or later, or remove it, all from small, unobtrusive controls on that same row, without opening a dialog or leaving the panel.

**Why this priority**: This is the feature's second acceptance line and the reason grouping alone is not enough — a practicing musician's actual workflow (drop a marker, name it, nudge it into place, jump back to check it) has to happen inside the panel or the grouping is just a cleaner-looking list.

**Independent Test**: Create a point marker, rename it by clicking its name, recolour it from the marker palette, click its "jump" action and confirm playback seeks there without changing play/pause state, click "nudge later" and confirm it moves by the configured step, then click "remove" and confirm it disappears from its group — all without any control outside the panel.

**Acceptance Scenarios**:

1. **Given** any populated row, **When** its name cell (or its "Add name" placeholder, for an unnamed marker) is clicked once, **Then** the name becomes an editable text field in place; committing (Enter, or focus loss) saves the new name, `Esc` cancels, and the marker's hover tooltip on both waveform lanes reflects the new name on the commit frame (Clarifications 3).
2. **Given** any populated row, **When** its colour swatch is activated, **Then** an 8-swatch palette popover opens in the panel; choosing a swatch sets that colour and closes the popover, `Esc`/click-outside closes it unchanged, and the new colour is reflected on the swatch and on both waveform lanes (Clarifications 2).
3. **Given** any row (loop boundary, point, or cue), **When** its "jump" action is used, **Then** the transport seeks to that marker's position immediately, preserving the current play/pause state, exactly as an existing cue-jump does.
4. **Given** any row, **When** its "nudge earlier" or "nudge later" action is used, **Then** the marker moves by exactly 1× the configured nudge step in that direction — the same distance and clamping rules as the unmodified keyboard nudge (Clarifications 7).
5. **Given** any row, **When** its "remove" action is used, **Then** the marker is deleted immediately (no confirmation) using the same rules as the existing keyboard delete (a removed loop-region boundary leaves its region incomplete rather than deleting the whole region, unless it was the region's last remaining boundary), and keyboard focus moves to the next populated row, else the previous one, else "New loop region" (Clarifications 8).
6. **Given** a row's jump, nudge, and remove actions, **When** the panel renders, **Then** they are always rendered (not hover-revealed) as small, low-emphasis (015 ghost/quiet variant) icon controls in a trailing column, in the order jump, nudge earlier, nudge later, remove, that do not visually compete with the row's colour swatch, name, or (on a loop-region row) the arm toggle — and each is still reachable and operable by keyboard alone, in the row's tab order (Clarifications 5, 6).
7. **Given** an empty cue slot's row, **When** it renders, **Then** it carries no jump/nudge/remove actions, no colour swatch, and no editable name field, since there is nothing there yet.

---

### User Story 3 - Clear-all is visibly and spatially separate from New loop region (Priority: P2)

A musician looking at the panel can immediately tell that "New loop region" is a safe, everyday action and "Clear all markers" is a rare, destructive one — instead of today's layout where the two sit side by side, inviting a mis-click.

**Why this priority**: This is the feature's third acceptance line and a direct fix to a named hazard (a destructive control sitting immediately beside a routine one); it matters less than the grouping and row-action work because the confirmation step it already has provides a safety net, but the adjacency itself is the specific defect the prompt calls out.

**Independent Test**: Open the Markers panel on a track with several markers; confirm "New loop region" renders at the top of the Loop Region group, confirm "Clear all markers" renders elsewhere in the panel with visibly destructive styling and separated from "New loop region" by other panel content, and confirm clicking it still requires the existing two-step confirmation before anything is removed.

**Acceptance Scenarios**:

1. **Given** the Markers panel with any content, **When** it renders, **Then** "New loop region" appears as the first line of the Loop Region group as a constructive control, and "Clear all markers" appears alone in a footer row at the bottom of the card, below the Cues group, in the destructive variant; on an empty panel "New loop region" appears below the empty-state message and "Clear all markers" is absent (Clarifications 1, 11).
2. **Given** "Clear all markers" is used, **When** the user has not yet confirmed, **Then** the panel shows the existing "Clear N markers?" inline confirmation with confirm/cancel controls, unchanged from today.
3. **Given** the confirmation is accepted, **When** it completes, **Then** every marker, region, and cue for the track is removed and the panel falls back to its empty state.
4. **Given** the confirmation is shown, **When** `Esc` is pressed or "cancel" is clicked, **Then** nothing is removed and the panel returns to its normal grouped state.

---

### User Story 4 - The panel tells you what to do when it has nothing, or room, to show (Priority: P3)

A musician opening the Markers panel on a track with no markers at all sees a plain instruction naming the shortcut that places one, instead of a blank area; and a musician looking at the Cues group can see every free slot number (1–8), not just the ones already in use, so they know at a glance which numbers are available.

**Why this priority**: This closes the last named gap in the prompt's acceptance line; it ranks below the other three because a user can still discover and use the panel without it, but it is the difference between a panel that explains itself and one that looks broken or incomplete on a fresh track.

**Independent Test**: Open the panel on a brand-new track with no markers and confirm it shows a plain instruction naming the marker-placing shortcut in place of a blank area; then create a single cue in slot 3 and confirm the Cues group shows all 8 slots, with slot 3 populated and the other 7 visibly empty and labelled by number.

**Acceptance Scenarios**:

1. **Given** a track with no markers, no loop regions, and no cues, **When** the Markers panel renders, **Then** it shows a single message naming the shortcut that places a marker (the existing "No markers — press I to set A" wording) plus the "New loop region" button, and no group headers, cue-slot rows, or "Clear all markers" render (Clarifications 1).
2. **Given** a track with at least one marker of any kind, **When** the Cues group renders, **Then** it shows all 8 cue slots — occupied ones as full rows (colour, name or "Add name" placeholder, position, row actions), and empty ones as a non-interactive muted row reading "Cue n — empty · Shift+n to set" (Clarifications 10).
3. **Given** an empty cue slot row, **When** the panel renders it, **Then** it is visually distinguishable from a populated row (e.g. muted rather than full-strength styling) so a glance across the group shows which numbers are free.
4. **Given** a track whose Loop Region group has no region yet but whose Points or Cues group has content, **When** the panel renders, **Then** the Loop Region group still shows its header (count 0) and its "New loop region" control, so the loop workflow's entry point is always visible once the panel has anything to show at all.

---

### Edge Cases

- A track has more than one loop region (006 permits this) — every region renders as its own block inside the single Loop Region group, ordered by its A marker's position (an incomplete region with only a B marker orders by that marker's position); the group's header count is the number of regions, not the number of boundary markers.
- A loop region is incomplete (one boundary deleted) — its block still renders inside the Loop Region group with only the remaining boundary row and the loop cells showing the region as unarmable with its existing inline reason; the "remove" row action on its last remaining boundary deletes the region entirely, same as today's keyboard delete.
- The "remove" row action is used on a loop-region boundary that is the current region's only complete pairing while it is armed — the region disarms first (mirrors the existing keyboard-delete rule), then loses that boundary.
- The "jump" row action is used while a marker drag is in progress on the waveform — jump is refused/no-ops for that marker until the drag ends, since the two actions cannot sensibly apply at once (mirrors how the keyboard table already gates on an in-progress rename).
- The "nudge" row action is used on a marker placed within one nudge step of the track's start or end — the marker clamps to the boundary, same as the existing keyboard nudge and drag clamp rules (006 FR-019).
- A marker's name is opened for in-place rename and the user clicks a different row's jump/nudge/remove action before committing — the open rename commits (or is discarded per the existing Enter/Esc rule) before the other row's action runs, so the two never apply out of order in the same frame.
- The panel is showing the "marker limit reached" or a loop-arming refusal message (inline, below the header, unchanged from 006) at the same time as the new grouped rows — the refusal message keeps rendering exactly where it does today, unaffected by the regrouping.
- A marker name is very long — it still truncates the same way panel rows elsewhere in the app do, rather than pushing the row's position/actions out of view.
- Recolouring or renaming a marker from its row while that marker is also selected/focused on the waveform — both views (lane tooltip, panel row) reflect the change on the same frame; nothing needs to be closed or reopened.
- The Cues group's 8 slots are shown even when only 1–7 are ever going to be used on a given track — all 8 numbers always render once the panel has any content, so "which numbers are free" is always answerable without cross-referencing the waveform.

## Requirements *(mandatory)*

### Functional Requirements

**Grouping (User Story 1)**

- **FR-001**: The Markers panel MUST organize its content into up to three named groups, in this fixed order: Loop Region, Points, Cues. No group MUST render unless the panel has at least one marker of any kind (FR-020 covers the fully-empty case); once the panel has any content, the Loop Region group and the Cues group always render (FR-004, FR-016), while the Points group renders only when at least one point marker exists. Group headers are non-interactive, not individually collapsible, and show label + count per Clarifications 4. *(UX-24; NFR-6.4)*
- **FR-002**: Each group's header MUST show a count: the Loop Region group's count is the number of loop regions (complete or incomplete), the Points group's count is the number of point markers, and the Cues group's count is the number of *occupied* cue slots (not 8, even though all 8 slots render per FR-016).
- **FR-003**: Within the Loop Region group, each loop region MUST render as its own block: its region-start (A) row, its region-end (B) row when present, then that region's loop cells (arm toggle, repeat-count field, crossfade field, wraps-remaining/infinite indicator, and any armed-but-inactive or clamped-endpoint indicator) — all as today, just scoped inside the group. Multiple regions' blocks render one after another, ordered by the position of the region's earliest-present boundary marker.
- **FR-004**: The Loop Region group MUST always render once the panel has any content at all (FR-001), even when the track has zero loop regions, so its "New loop region" control (FR-017) is always reachable.
- **FR-005**: Within the Points group, point-marker rows MUST be ordered by position, earliest first.
- **FR-006**: Markers of one kind MUST NOT render inside another kind's group — a loop-region boundary never appears in Points, a point marker never appears in Cues, and vice versa, matching each marker's existing `kind`.

**Row content and row actions (User Story 2)**

- **FR-007**: Every populated row (a loop-region boundary, a point, or an occupied cue) MUST show: a colour swatch, the marker's role/kind label, its name (inline-editable, unchanged content rules from 006), its position in tabular (monospace) figures, and — for a clamped marker — its existing warning glyph.
- **FR-008**: Every populated row MUST additionally show four small, low-emphasis row actions distinct from the row's primary content — jump, nudge earlier, nudge later, remove, in that order — always rendered (not hover-revealed) as icon controls in 015's lowest-emphasis variant in a trailing right-aligned column, each with a tooltip and accessible name (`markers-jump`, `markers-nudge-earlier`, `markers-nudge-later`, `markers-remove`); none is styled as a primary or destructive action (Clarifications 5).
- **FR-009**: A row's "jump" action MUST seek the transport to that marker's position, preserving the current play/pause state exactly as an existing cue-jump does (006 FR-014), regardless of the marker's kind (loop boundary, point, or cue). It MUST NOT change the focused marker, current region, or arm state, and MUST be a no-op for a marker whose drag is in progress (Clarifications 9).
- **FR-010**: A row's "nudge earlier"/"nudge later" actions MUST move that marker by exactly 1× the configured nudge step (no modifier variant; Clarifications 7), and follow the same clamping and loop-arithmetic rules, as the existing keyboard nudge (006 FR-004, FR-027).
- **FR-011**: A row's "remove" action MUST delete that marker using the same rules as the existing keyboard delete (006 FR-006), including the loop-region-incomplete/disarm behavior when the removed marker is a region boundary, immediately and without confirmation; focus then moves per Clarifications 8.
- **FR-012**: A single primary click on a populated row's name cell MUST open it for in-place rename; an unnamed marker's name cell MUST show the muted `markers-name-placeholder` ("Add name") as its click target. Enter or focus loss commits, `Esc` cancels (Clarifications 3); on commit, the marker's name change MUST be reflected in that marker's hover tooltip on both waveform lanes on the same frame the commit lands.
- **FR-013**: Activating a row's colour swatch (click, `Enter`/`Space`) MUST open an in-panel popover of all 8 `MARKER_PALETTE` swatches (current one marked by a non-colour indicator); choosing one sets that index and closes the popover, `Esc`/click-outside closes it with no change, and the focused-marker `C` key keeps cycling (006 contracts/ui-markers.md §4; Clarifications 2). The new colour MUST be reflected on the swatch and on both waveform lanes the same frame.
- **FR-014**: Every row action introduced by FR-008 MUST be reachable and operable by keyboard alone, in the row's tab order, with an accessible name and role, consistent with the panel's existing accessibility guarantees (006 FR-022), in the tab order of Clarifications 6. *(NFR-6.1, NFR-6.2)*
- **FR-015**: An empty cue-slot row (FR-016, below) MUST NOT carry a colour swatch, editable name, or any of FR-008's row actions, MUST NOT be a tab stop or respond to clicks, and shows no hover fill, since it has no marker to act on (Clarifications 10).

**Cues group (User Story 1, User Story 4)**

- **FR-016**: Once the Cues group renders (i.e., the panel has any content at all), it MUST show all 8 cue slots, in slot-number order: an occupied slot as a full row per FR-007/FR-008, an empty slot as a muted-text-role row reading `markers-cue-empty { $slot }` ("Cue n — empty · Shift+n to set"). This holds even when zero cues exist but a region or point does.

**Destructive action separation (User Story 3)**

- **FR-017**: "New loop region" MUST render as the first line of the Loop Region group, directly below its header; on a fully empty panel it MUST render directly below FR-020's empty-state message (Clarifications 1, 12).
- **FR-018**: "Clear all markers" MUST NOT render adjacent to "New loop region"; once any content exists it MUST render alone in a trailing-aligned footer row at the bottom of the card, below the Cues group, in 015's destructive control variant; its confirm/cancel pair replaces it in place in that footer. On an empty panel it MUST NOT render (Clarifications 11).
- **FR-019**: "Clear all markers" MUST keep its existing two-step inline confirmation ("Clear N markers?" → confirm/cancel, keyboard-operable, `Esc` cancels) and its existing effect (disarms and removes every marker, region, and cue for the track, then shows the empty state) exactly as specified by 006 FR-021.

**Empty and inline states (User Story 4)**

- **FR-020**: When the track has zero markers of every kind (no loop regions, no points, no cues), the panel MUST show exactly one message naming the shortcut that places a marker (the existing "No markers — press I to set A" wording, unchanged from 006 FR-020) followed by the "New loop region" button; no group header, cue-slot row, or "Clear all markers" MUST render (Clarifications 1).
- **FR-021**: Any existing inline refusal/status message (marker-limit-reached, loop-region-incomplete, loop-region-too-short) MUST keep rendering at the panel level, directly below the card header and above the Loop Region group (or above the empty-state message), unaffected by the new grouping (Clarifications 11).

**Cross-cutting**

- **FR-022**: This feature MUST NOT change any marker/loop-region/cue keyboard shortcut, persistence behavior, the 64-marker limit, the crossfade/repeat-count model, or beat-grid snapping — all owned by 006-markers-loops-and-cues. It MUST NOT change the panel's position, collapse control, or persisted open/closed flag among Now Playing's panel cards — owned by 021-transport-bar-and-panel-layout.
- **FR-023**: Every colour, spacing, and radius value this feature introduces (group header styling, the row actions' low-emphasis treatment, the empty-cue-slot row's muted styling) MUST resolve to an existing role or token, consistent with 014-design-tokens-and-type-scale's and 015-control-variants' zero-new-literal convention already applied to this codebase.
- **FR-024**: Every new or changed string this feature introduces (group headers and their counts, the empty-cue-slot hint, the jump/nudge/remove row actions' accessible names) MUST be externalized to the existing en-US Fluent bundle that holds the `markers-*` keys (new keys at minimum: `markers-group-loop`, `markers-group-points`, `markers-group-cues`, `markers-name-placeholder`, `markers-cue-empty`, `markers-jump`, `markers-nudge-earlier`, `markers-nudge-later`, `markers-remove`), consistent with 006/016's existing convention (Clarifications 13). *(NFR-7.1)*
- **FR-025**: The decisions above MUST be verified by automated tests: the three-group partition and per-group counts across marker-kind combinations (User Story 1); each row action's effect (jump preserves play state, nudge matches the keyboard step, remove matches the keyboard delete's region-incomplete rule) and its keyboard reachability (User Story 2); the spatial/styling separation and unchanged two-step confirmation of "Clear all markers" from "New loop region" (User Story 3); the fully-empty single message and the all-8-cue-slots-always-visible rule (User Story 4); the swatch popover (Clarifications 2), click-to-rename including the unnamed placeholder (Clarifications 3), and post-remove focus (Clarifications 8). Tests extend `crates/modplayer-ui/tests/markers.rs` (Clarifications 15).

### Key Entities

- **Marker Group**: One of the panel's three fixed, ordered sections (Loop Region, Points, Cues), each with a header naming it and showing its own count, rendering only once the panel has any content and (for Points/Cues) only when relevant content exists.
- **Loop Region Block**: One loop region's rendering inside the Loop Region group — its A/B boundary rows plus its loop cells (arm, repeat, crossfade, wraps) — one block per region, however many regions the track holds.
- **Populated Row**: One marker's row (loop boundary, point, or occupied cue) — colour swatch, role label, editable name, tabular position, optional clamped-warning glyph, and the four row actions (jump, nudge-earlier, nudge-later, remove).
- **Empty Cue Slot Row**: A cue slot (1–8) with no marker in it — muted styling, slot number, and the keyboard shortcut that fills it; carries no row actions, swatch, or editable name.
- **Row Action**: One of the four small, low-emphasis controls a populated row exposes — jump-to (seek preserving play state), nudge-earlier, nudge-later (move by the configured step), and remove (delete, following 006's existing region-incomplete rule) — each keyboard-reachable in the row's tab order.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a track combining a loop region, point markers, and a cue, a user can identify which group any given marker belongs to at a glance, without reading its row content, in 100% of sampled combinations (loop-only, points-only, cues-only, and all three together).
- **SC-002**: Every group's header count matches the actual number of items it holds (regions for Loop Region, point markers for Points, occupied slots for Cues), in 100% of sampled marker combinations.
- **SC-003**: A user can rename, recolour, jump to, nudge, or remove any marker entirely from within the panel — no control outside the panel is ever needed to complete any of these five actions — in 100% of trials across all three marker kinds.
- **SC-004**: After an in-place rename commits, the marker's updated name is reflected in its waveform-lane hover tooltip within the same interaction, in 100% of trials.
- **SC-005**: "Clear all markers" is never rendered immediately adjacent to "New loop region", and is visibly styled as destructive, in 100% of panel states that show both controls.
- **SC-006**: "Clear all markers" still requires an explicit two-step confirmation before removing anything, and a cancelled confirmation removes nothing, in 100% of trials.
- **SC-007**: On a track with zero markers, the panel shows exactly one instructional message naming the marker-creation shortcut plus the "New loop region" button, with no group headers, cue-slot rows, or "Clear all markers" visible, in 100% of trials.
- **SC-008**: Once the panel has any content, all 8 cue slot numbers are visible in the Cues group — occupied and empty alike — so a user can identify every free slot number without checking the waveform, in 100% of trials.
- **SC-009**: Every row action (jump, nudge-earlier, nudge-later, remove) is operable by keyboard alone and produces the same result as its pointer equivalent, in 100% of trials.

## Assumptions

- The Loop Region group's count (FR-002) is the number of loop regions, not the number of boundary markers — an incomplete region (one boundary) still counts as 1, matching how a user thinks about "how many loops do I have," not how many marker records back them.
- A group with zero eligible items is omitted entirely, except the Loop Region group, which always renders once the panel has any content at all, so its "New loop region" entry point is never hidden behind having to first create a point or cue. This keeps the panel from showing a run of misleading "(0)" headers while still guaranteeing the loop workflow's starting control is always reachable.
- The Cues group's all-8-slots-always-visible behavior (FR-016) is gated on the panel having *any* content (a loop region, a point, or a cue already existing) rather than being unconditionally shown even on a fully empty track — this keeps the single empty-state message (FR-020, unchanged from 006) and its "New loop region" button as the only things a brand-new track's panel shows, rather than mixing it with 7 empty cue rows that would contradict "the panel states plainly what to do when it is empty ... rather than leaving a blank area" by presenting a partially-blank area instead.
- "Jump" is a new capability this feature adds to every row (loop boundary, point, and cue) — before this feature, only a cue's dedicated `1`–`8` keyboard shortcut could jump to a marker; a pointer-and-keyboard-reachable per-row jump action generalizes that same seek-preserving-play-state behavior (006 FR-014) to every marker kind, since the breakdown prompt asks for it on "each row" without carving out an exception.
- "Nudge" is exposed as two row actions (earlier/later) rather than one, mirroring the existing keyboard table's `←`/`→` pair (006 FR-004) — a single ambiguous "nudge" control with no stated direction would be a worse, not equivalent, interface for the same capability.
- Recolouring from a row (FR-013) uses the 8-swatch popover that 006 contracts/ui-markers.md §4 already specifies; the current code's click-to-cycle swatch is a drift from that contract, corrected here because the prompt asks for the colour to be "picked from the marker palette" (Clarifications 2). The `C` keyboard cycle is unchanged.
- "The waveform lane" in the prompt's acceptance line ("the new name appears on the waveform lane on commit") is read as the marker's existing hover tooltip on the lane (which already includes the marker's name in its accessible-name template, 006 contracts/ui-markers.md §1/§3) rather than a new always-visible text label painted directly on the 14px lane strip — the lane has no room for arbitrary-length names at that height, and no other part of this feature or its prerequisites asks for the lane's glyph rendering to change.
- Row actions use 015's lowest-emphasis control variant with icon glyphs and are always visible (Clarifications 5); exact glyphs and sizes resolve to existing token roles (FR-023).
- This feature assumes single loop region is the common case shown in the prompt's own acceptance line, but explicitly supports 006's existing multiple-loop-region model (Edge Cases) since the underlying data already allows it and hiding a second region from the panel would be a regression, not a simplification.

# 010-now-playing-workbench / 003 — Markers Panel Structure

**Source:** [§ 3.5 Now Playing](../../ModPlayer-UI-UX-Review.md#35-now-playing) (UX-24), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules), [§ 4.4 Typography and text](../../ModPlayer-UI-UX-Review.md#44-typography-and-text), [§ 5.3 Semantic colour](../../ModPlayer-UI-UX-Review.md#53-semantic-colour-both-themes-contrast-verified)

**Prerequisites:** Assumes markers, loop regions and cues from 001-mvp/006-markers-loops-and-cues and the panel card and row components from 008-design-foundation/003-list-row-and-panel-components.

## Prompt

> Give the markers panel the structure a practice session needs. It is currently one flat list where a loop boundary, a named point and a cue slot all render identically as a coloured dot, a name and a timestamp, with a destructive "Clear all markers" button sitting immediately beside "New loop region".

> Group the panel by kind: the loop region and its A and B boundaries first, with the arm state, repeat count and crossfade shown as part of that group; then named point markers; then the numbered cue slots, with empty slots visible so a user can see which numbers are free. Each row shows its colour swatch, its name, its timestamp in tabular figures, and quiet row actions to jump to it, nudge it, or remove it. A marker can be renamed in place, and its colour picked from the marker palette, without leaving the panel.

> Separate the destructive action from the constructive one: "New loop region" stays at the top of the loop group, while clearing all markers moves away from it, adopts the destructive styling, and keeps its existing two-step confirmation. Each group shows its count in the header, and the panel states plainly what to do when it is empty — naming the shortcut that places a marker rather than leaving a blank area.

> Acceptance: when a track has an armed loop, two point markers and one cue, the panel shows three labelled groups with those counts. When a marker's name is clicked, it becomes editable in place and the new name appears on the waveform lane on commit. When the clear-all action is used, it is visually distinct from the neighbouring actions and still asks for confirmation. When no markers exist, the panel names the shortcut that creates one.

## Scope boundary

Covers the panel's structure and in-place editing; marker semantics, persistence and beat snapping remain with 001-mvp/006-markers-loops-and-cues and 006-practice-depth/001-beat-grid-key-and-loudness-analysis.

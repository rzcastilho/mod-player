# 010-now-playing-workbench / 002 — Waveform Legibility and Scrub Feedback

**Source:** [§ 3.5 Now Playing](../../ModPlayer-UI-UX-Review.md#35-now-playing) (UX-23), [§ 5.3 Semantic colour](../../ModPlayer-UI-UX-Review.md#53-semantic-colour-both-themes-contrast-verified), [§ 5.5 Layout rules](../../ModPlayer-UI-UX-Review.md#55-layout-rules), [§ 4.1 No design tokens](../../ModPlayer-UI-UX-Review.md#41-no-design-tokens)

**Prerequisites:** Assumes the waveform from 001-mvp/005-now-playing-waveform, the loop regions from 001-mvp/006-markers-loops-and-cues, and the tokens from 008-design-foundation/001-design-tokens-and-type-scale.

## Prompt

> Make the waveform tell the user where they are, where the loop is, and where they are about to seek to — none of which it currently shows clearly.

> Draw the waveform with two tones so peak and average energy are distinguishable, and give the played portion a different treatment from the unplayed portion so progress is readable without checking the clock. The playhead becomes a clearly visible indicator that meets a minimum contrast ratio against the waveform fill in both themes; today it is a single dark line that nearly vanishes against the dark theme's fill. An armed loop region is shaded across the waveform itself rather than being indicated only by markers in the lane above it, and the shading distinguishes an armed loop from a defined but inactive one.

> Add hover feedback: moving the pointer across either the overview or the detail waveform shows a scrub position line with the timestamp under the pointer, so a user can find a passage before committing to a seek. Keep the existing click and drag seeking, the elapsed and remaining labels, and the marker lane, all using the monospace numeric style so the times do not jitter as they count. The overview and detail heights follow the proportional sizing introduced with the window work, with minimums that keep both usable.

> Acceptance: when a track is playing in the dark theme, the playhead is clearly distinguishable from the waveform fill. When a loop region is armed, the looped span is visibly shaded in both the overview and the detail view. When the pointer hovers over the detail waveform, a position line and its timestamp follow the pointer. When the loop is defined but not armed, its shading differs from the armed state.

## Scope boundary

Covers what the waveform draws and how it responds to hovering; it does not change seeking behaviour, marker semantics, or the beat-grid overlay planned in 006-practice-depth/001-beat-grid-key-and-loudness-analysis.

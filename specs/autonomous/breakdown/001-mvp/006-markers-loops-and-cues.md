# 001-mvp / 006 — Markers, Loop Regions, and Cue Points

**Source:** [FR-5.1 Markers](../../ModPlayer-Software-Specification.md#fr-51-markers), [FR-5.2 Loop regions](../../ModPlayer-Software-Specification.md#fr-52-loop-regions), [FR-5.3 Cue points](../../ModPlayer-Software-Specification.md#fr-53-cue-points), [FR-6.1 Engine](../../ModPlayer-Software-Specification.md#fr-61-engine) (clock and loop evaluation), [FR-11.3 Per-track state](../../ModPlayer-Software-Specification.md#fr-113-per-track-state), [DM-6 Marker](../../ModPlayer-Software-Specification.md#dm-6-marker), [DM-7 LoopRegion](../../ModPlayer-Software-Specification.md#dm-7-loopregion), [DM-9 TrackStateEntry](../../ModPlayer-Software-Specification.md#dm-9-trackstateentry), [Part 9 § 5 Key flows](../../ModPlayer-Software-Specification.md#5-key-flows) (5.1), [EC § 4 Markers, loops, cues](../../ModPlayer-Software-Specification.md#4-markers-loops-cues), [NFR § 1 Performance and latency](../../ModPlayer-Software-Specification.md#1-performance-and-latency) (NFR-1.2, 1.3), [NFR § 2 Reliability and availability](../../ModPlayer-Software-Specification.md#2-reliability-and-availability) (NFR-2.8), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-1, JTBD-7)

**Prerequisites:** Assumes the waveform from 001-mvp/005-now-playing-waveform.

## Prompt

> Let a practicing musician mark a passage, loop it gaplessly, and find it again tomorrow — without any plugin. Markers, loops, and cues are host primitives so that later plugins can share them and so they outlive any plugin.
>
> A marker is a sample-accurate named point in a track with a color, a kind (point, region-start/region-end pair, or cue), an owner (host or, later, a plugin), a transient flag, and visibility. The user can create, rename, recolor, move, and delete markers from the now-playing view, drag them on the waveform (the detail view zooms while dragging so a marker lands within 5 ms), and nudge them by a configurable step (default 10 ms) from the keyboard. A track holds at least 64 markers; the 65th is refused with an inline "marker limit reached".
>
> A loop region is an A/B marker pair with an armed state. When armed and the playhead reaches B, playback continues from A with no audible gap or click: the seam is rendered with a short crossfade (0–50 ms, default 5 ms) that never causes drift relative to the markers, and the loop decision is evaluated on the real-time path by the engine's audio clock, never by UI or script timing. Only one region is armed at a time. A loop can repeat N times then release; default is infinite. If the user seeks outside an armed region, the loop stays armed and re-engages only when the playhead reaches B from inside.
>
> A cue point is a point marker with a slot 1–8; jumping to a cue seeks instantly and keeps the current play state. Markers, regions, and cues persist per track identity and are restored whenever the track loads, with atomic writes so a crash mid-write never corrupts earlier markers. The empty state reads "No markers — press I to set A".
>
> Acceptance: when A is placed after B, they swap. When A equals B, the loop cannot be armed and the UI says why; when the region is shorter than the crossfade, the crossfade shrinks to fit. When a loop wraps 1,000 times on cached audio, the rendered loop length deviates by 0 samples and never accumulates drift. When the user reopens a track the next day, all markers and the loop region are back (the armed state is not).

## Scope boundary

Does not cover beat snapping, sections, per-track state export, plugin-owned markers, or the Section Loop plugin panel — only the host-level primitives and their UI.

## Open questions

- A-15: loop stays armed on seek-outside; alternative is to disarm. Confirm before planning.

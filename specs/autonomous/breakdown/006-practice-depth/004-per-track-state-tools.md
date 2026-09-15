# 006-practice-depth / 004 — Per-Track State Inspection, Clearing, and Export

**Source:** [FR-11.3 Per-track state](../../ModPlayer-Software-Specification.md#fr-113-per-track-state) (FR-11.3.3, 11.3.4), [DM-9 TrackStateEntry](../../ModPlayer-Software-Specification.md#dm-9-trackstateentry), [§ 4 Per-track user state](../../ModPlayer-Software-Specification.md#4-per-track-user-state), [EC § 10 Settings, presets, per-track state](../../ModPlayer-Software-Specification.md#10-settings-presets-per-track-state) (EC-10.2, 10.3), [EC § 4 Markers, loops, cues](../../ModPlayer-Software-Specification.md#4-markers-loops-cues) (EC-4.8), [§ 10 Retention summary](../../ModPlayer-Software-Specification.md#10-retention-summary), [NFR § 3 Scalability (local)](../../ModPlayer-Software-Specification.md#3-scalability-local) (NFR-3.4), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-7)

**Prerequisites:** Assumes per-track state from 001-mvp/006-markers-loops-and-cues and sections from 006-practice-depth/002-beat-grid-correction-and-sections.

## Prompt

> Let a user see everything ModPlayer remembers about a song — markers, loops, cues, sections, beat-grid corrections, and each plugin's per-track data — clear any of it, and carry it to another machine.
>
> From the now-playing view, "Track memory" opens a panel listing the track's stored state grouped by owner: host markers, loop regions, cue points, sections, and beat-grid corrections, then one group per plugin showing its keys and a summary of each value. The user can clear a single group, a single plugin's data, or everything for the track, with a confirmation naming what is removed. Clearing a plugin's group sends it a `track_changed`-equivalent refresh so its panel reflects the empty state.
>
> Per-track state can be exported to a file containing markers, loops, cues, sections, corrections, and plugin entries, but no audio, no credentials, and no listening history, and imported on another installation for the same track identity; on import, existing entries are merged key by key with the file's entries winning, and the result is listed. Writes remain atomic and keyed individually, so a plugin write and a user edit racing on the same track never overwrite each other's keys; the last write wins per key. A corrupt per-track entry is discarded, the rest is kept, and the user sees a one-time notice. When a track is re-released under the same identity with a different duration, markers beyond the new end are clamped and flagged with a warning glyph. The store handles one million entries across the library without visible slowdown.
>
> Acceptance: when the user opens Track memory on a drilled solo, it lists two markers, one loop region, and Key & Tempo's remembered −2 semitones. When the user clears Key & Tempo's group, the plugin's badge disappears and tempo returns to 100% on next load, while markers stay. When the user exports the track's state and imports it on a second machine, the markers and loop appear at the same sample positions. When the stored file for a track is corrupted on disk, other tracks are unaffected and the notice appears once.

## Scope boundary

Does not cover presets, which snapshot session-level configuration rather than per-track data.

## Open questions

- A-9: per-track state is assumed account-scoped and cleared on sign-out; export is the user's escape hatch if that assumption holds.

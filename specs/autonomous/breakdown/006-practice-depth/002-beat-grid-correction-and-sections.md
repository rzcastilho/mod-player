# 006-practice-depth / 002 — Beat Grid Correction and Song Sections

**Source:** [FR-6.6 Local analysis](../../ModPlayer-Software-Specification.md#fr-66-local-analysis) (FR-6.6.4), [FR-5.4 Sections](../../ModPlayer-Software-Specification.md#fr-54-sections), [Part 5 § 6.4 Analysis](../../ModPlayer-Software-Specification.md#64-analysis-analysisread) (`analysis.write`), [Part 5 § 6.2 Markers](../../ModPlayer-Software-Specification.md#62-markers-markerswrite) (define_section), [DM-8 Section](../../ModPlayer-Software-Specification.md#dm-8-section), [DM-5 Analysis](../../ModPlayer-Software-Specification.md#dm-5-analysis) (user corrections), [FR-11.3 Per-track state](../../ModPlayer-Software-Specification.md#fr-113-per-track-state) (sections, beat grid corrections), [§ 1 Primary personas](../../ModPlayer-Software-Specification.md#1-primary-personas) (Marina: jump between sections), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-12)

**Prerequisites:** Assumes the beat grid from 006-practice-depth/001-beat-grid-key-and-loudness-analysis and per-track state from 001-mvp/006-markers-loops-and-cues.

## Prompt

> Let a musician fix a beat grid the analyzer got wrong and jump straight to the chorus or the bridge without scrubbing a timeline.
>
> When the analyzed grid is off, the user can tap tempo to set a corrected tempo and nudge the downbeat earlier or later; the correction persists per track, survives re-analysis when the analyzer version changes, and immediately updates snapping and any beat-based plugin behavior. Plugins with `analysis.write` can do the same through set_tempo_correction and nudge_downbeat, and the change is attributed to them.
>
> A section is a named region of a track with a kind — intro, verse, chorus, bridge, solo, outro, or custom — and an owner (host or plugin). The user defines sections from the now-playing view by marking a start and end (snapping to beats when snapping is on) and choosing a kind and name; a plugin with `analysis.write` defines them with define_section. Sections draw on the waveform as labeled regions distinct from markers and loops, and are stored in per-track state alongside markers. The host provides "jump to next section" and "jump to previous section" actions, bindable to keyboard and MIDI like any other action, that seek to the section boundary and keep the play state.
>
> Acceptance: when the user taps tempo four times at 118 BPM on a track analyzed at 59 BPM, the grid doubles, markers snap to the new beats, and reopening the track tomorrow shows the corrected grid. When the user defines verse, chorus, and bridge and presses "next section" during the verse, playback jumps to the chorus start with no gap. When a plugin without `analysis.write` calls define_section, it receives `permission_denied`. When the analyzer version changes and the track is re-analyzed, the user's downbeat nudge is reapplied on top of the new grid.

## Scope boundary

Does not cover automatic section detection by the analyzer.

## Open questions

- Q-14: whether sections should be auto-detected in the first release or only user- and plugin-defined; this prompt assumes user- and plugin-defined only.

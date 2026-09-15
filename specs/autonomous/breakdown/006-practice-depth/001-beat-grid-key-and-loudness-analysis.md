# 006-practice-depth / 001 — Beat Grid, Key, and Loudness Analysis with Beat Snapping

**Source:** [FR-6.6 Local analysis](../../ModPlayer-Software-Specification.md#fr-66-local-analysis) (FR-6.6.1–6.6.3), [FR-2.4 Track metadata](../../ModPlayer-Software-Specification.md#fr-24-track-metadata) (FR-2.4.2 provider analysis), [§ 4 Now-playing view and waveform](../../ModPlayer-Software-Specification.md#4-now-playing-view-and-waveform) (FR-4.1.5), [FR-5.1 Markers](../../ModPlayer-Software-Specification.md#fr-51-markers) (FR-5.1.5), [FR-5.2 Loop regions](../../ModPlayer-Software-Specification.md#fr-52-loop-regions) (beat-sized nudge), [FR-13.1 Section Loop](../../ModPlayer-Software-Specification.md#fr-131-section-loop) (FR-13.1.6), [FR-13.2 Key & Tempo](../../ModPlayer-Software-Specification.md#fr-132-key--tempo) (FR-13.2.5), [Part 5 § 6.4 Analysis](../../ModPlayer-Software-Specification.md#64-analysis-analysisread) (read), [Part 5 § 5 Events (host → plugin)](../../ModPlayer-Software-Specification.md#5-events-host--plugin) (`analysis_ready`), [DM-5 Analysis](../../ModPlayer-Software-Specification.md#dm-5-analysis), [Part 9 § 3 Core services](../../ModPlayer-Software-Specification.md#core-services) (AR-11), [J-2 — Drill a solo with Section Loop (S-1)](../../ModPlayer-Software-Specification.md#j-2--drill-a-solo-with-section-loop-s-1) (ALT-2.3b), [J-5 — Write and publish a plugin (S-4)](../../ModPlayer-Software-Specification.md#j-5--write-and-publish-a-plugin-s-4) (step 6), [EC § 4 Markers, loops, cues](../../ModPlayer-Software-Specification.md#4-markers-loops-cues) (EC-4.7), [EC § 5 Audio engine and effects](../../ModPlayer-Software-Specification.md#5-audio-engine-and-effects) (EC-5.9), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-16)

**Prerequisites:** Assumes the waveform analysis service from 001-mvp/005-now-playing-waveform, markers from 001-mvp/006-markers-loops-and-cues, and both bundled plugins from 001-mvp/012 and 001-mvp/013.

## Prompt

> Let a musician snap loop markers to the beat and let plugin authors build beat-aware behaviors, by analyzing each track locally for its beat grid, tempo, key, and loudness.
>
> The background analysis service already computing the waveform overview now also produces a beat grid (beat positions with downbeats and confidence), tempo, a key estimate with mode and confidence, and integrated and peak loudness. Analysis runs off the real-time path from audio as decoded, before the effect chain, so it never changes when effects change; results are cached per track identity with the analyzer version and status (pending, partial, complete, failed). Where the streaming service supplies audio features (tempo, key, mode, loudness, time signature), they are cached as "provider analysis" and used as a fallback until local analysis supersedes them.
>
> The waveform can show the beat grid. With "snap to beat" on, host marker placement and nudging land on the nearest beat, and the nudge step becomes beat-sized. A marker placed while analysis is still pending stays unsnapped with a small pending glyph and moves only if the user re-snaps — never automatically. Plugins with `analysis.read` call get_beats, get_key, get_loudness, and get_waveform, receiving `pending` until `analysis_ready` fires; a failed analysis returns `failed` and is not retried until the analyzer version changes or the user asks. Section Loop's "snap to beat" toggle becomes live, and it gains "loop last N beats" (default 8); Key & Tempo shows the detected key and the resulting key after shift.
>
> Acceptance: when the user drops marker A near a beat with snapping on, it lands on that beat within a millisecond of the analyzed position. When a plugin calls get_beats on a track analyzed ten seconds ago, it receives beats, downbeats, tempo, and confidence immediately. When the user presses Section Loop's "loop last 8 beats" action mid-song, A and B land on the beat grid 8 beats before the playhead and the loop arms. When analysis fails on a silent track, the waveform still shows and the panel reads "analysis unavailable".

## Scope boundary

Does not cover user corrections to the beat grid, sections, or the `analysis.write` permission — those are the next slice.

## Open questions

- A-19: availability of provider audio features from the service; if absent, rely on local analysis only.

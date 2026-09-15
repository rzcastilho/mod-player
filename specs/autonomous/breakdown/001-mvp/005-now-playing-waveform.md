# 001-mvp / 005 — Now-Playing View with Waveform

**Source:** [§ 4 Now-playing view and waveform](../../ModPlayer-Software-Specification.md#4-now-playing-view-and-waveform), [FR-6.6 Local analysis](../../ModPlayer-Software-Specification.md#fr-66-local-analysis) (waveform overview only), [DM-5 Analysis](../../ModPlayer-Software-Specification.md#dm-5-analysis), [Part 9 § 3 Core services](../../ModPlayer-Software-Specification.md#core-services) (AR-11), [EC § 12 Empty, loading, and first-run states](../../ModPlayer-Software-Specification.md#12-empty-loading-and-first-run-states), [NFR § 6 Accessibility](../../ModPlayer-Software-Specification.md#6-accessibility)

**Prerequisites:** Assumes decoded playback from 001-mvp/003-streaming-playback-and-queue.

## Prompt

> Give the user a now-playing view where they can see the song and navigate it by sight — the surface that markers, loops, and plugin overlays will later draw on.
>
> The view shows artwork, title, artists, album, elapsed and remaining time, a waveform overview of the whole track, and a zoomable waveform detail around the playhead. The waveform is generated locally from decoded audio by a background analysis service that pulls decoded buffers at low priority and never contends with the real-time path. For a track still streaming, the waveform fills in progressively and the undecoded region shows as a placeholder. The overview is cached per track identity as a multi-resolution peak set, versioned by analyzer version, so reopening a track shows the waveform immediately.
>
> The user can click or drag on either waveform to seek, and can zoom the detail view. Seeking is sample-accurate on fully decoded audio and lands within 50 ms of the requested position on streamed audio. Every seek and zoom action is also reachable by keyboard alone. The waveform area is designed to host the playhead, host markers, an active loop region, and plugin overlays in later slices, so it must expose a track-time coordinate space that re-projects on zoom and scroll.
>
> When nothing is loaded, the view shows a hint to pick a track. Analysis failing on a track (silence, corrupt stream) marks it "analysis unavailable" without blocking playback.
>
> Acceptance: when a track starts playing for the first time, the waveform begins filling within a second and completes as decoding does. When the user clicks at 1:23.500 on a cached track, playback resumes from exactly that position within 10 ms. When the user reopens a track whose waveform was computed yesterday, the full waveform appears instantly from cache. When the analyzer version changes, the waveform is recomputed on next play.

## Scope boundary

Does not cover markers, loops, beat grid, snapping, plugin overlays, or detaching the view into its own window.

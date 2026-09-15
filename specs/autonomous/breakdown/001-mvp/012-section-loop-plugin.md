# 001-mvp / 012 — Section Loop Bundled Plugin

**Source:** [FR-13.1 Section Loop](../../ModPlayer-Software-Specification.md#fr-131-section-loop), [§ 13 Bundled reference plugins](../../ModPlayer-Software-Specification.md#13-bundled-reference-plugins), [J-2 — Drill a solo with Section Loop (S-1)](../../ModPlayer-Software-Specification.md#j-2--drill-a-solo-with-section-loop-s-1), [§ 5 Key scenarios](../../ModPlayer-Software-Specification.md#5-key-scenarios) (S-1), [GOV § 1 Licensing](../../ModPlayer-Software-Specification.md#1-licensing) (GOV-1.3), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-1)

**Prerequisites:** Assumes transport focus from 001-mvp/010-transport-focus and UI contributions from 001-mvp/011-plugin-ui-contributions.

## Prompt

> Ship Section Loop, the first bundled plugin: the tool a practicing musician uses to drop A and B around a four-bar solo and drill it hands-free. It must be built only on the public plugin API — no private host interface — because it doubles as living documentation and a starting point others copy, and it is released under the same license as the host.
>
> The plugin declares `transport.control`, `markers.write`, `ui.panel`, `ui.overlay`, and `ui.shortcuts`, with `analysis.read` as an optional permission reserved for beat snapping in a later wave. It registers actions: set A, set B, toggle loop, nudge A earlier/later, nudge B earlier/later, set cue 1–8, jump to cue 1–8, snap toggle, and clear markers, with default shortcuts `I`, `O`, `L`, and `[` / `]`. Its panel offers A/B/Loop controls, a marker list with rename and color, a loop count field (default infinite), and a "snap to beat" toggle that stays disabled with an explanation until analysis exists. It draws A/B and cue markers as overlays on the waveform.
>
> Markers are persisted through the host's per-track state, not the plugin's own storage, so they survive the plugin being disabled and are visible to other plugins. When the user presses `L`, the plugin requests transport focus and arms the loop; the seam is gapless because the host evaluates it on the real-time path. Cue points are single named, colored markers in slots 1–8.
>
> Acceptance: when the user plays a track, presses `I` at 1:02 and `O` at 1:10, then `L`, the waveform shows A and B and playback loops gaplessly between them until `L` is pressed again. When the user closes the app and reopens the same track tomorrow, A and B are exactly where they were. When another plugin holds transport focus and the user presses `L` in Section Loop's panel under the default policy, focus moves to Section Loop and the loop arms. When the user disables Section Loop mid-loop, the loop releases, markers stay on the waveform, and the host's own marker UI can still edit them.

## Scope boundary

Does not cover beat snapping, "loop last N beats", or MIDI mappings — those arrive with analysis and MIDI waves.

# 001-mvp / 013 — Key & Tempo Bundled Plugin and Getting Started Panel

**Source:** [FR-13.2 Key & Tempo](../../ModPlayer-Software-Specification.md#fr-132-key--tempo), [§ 13 Bundled reference plugins](../../ModPlayer-Software-Specification.md#13-bundled-reference-plugins), [FR-1.4 Getting started](../../ModPlayer-Software-Specification.md#fr-14-getting-started), [J-3 — Transpose a set (S-2)](../../ModPlayer-Software-Specification.md#j-3--transpose-a-set-s-2), [J-2 — Drill a solo with Section Loop (S-1)](../../ModPlayer-Software-Specification.md#j-2--drill-a-solo-with-section-loop-s-1) (steps 5–8), [§ 5 Key scenarios](../../ModPlayer-Software-Specification.md#5-key-scenarios) (S-2), [§ 6 Success metrics](../../ModPlayer-Software-Specification.md#6-success-metrics), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-2, JTBD-3, JTBD-7)

**Prerequisites:** Assumes the effect chain from 001-mvp/008-effect-chain-and-built-in-nodes, plugin runtime from 001-mvp/009-plugin-runtime-and-permissions, and UI contributions from 001-mvp/011-plugin-ui-contributions.

## Prompt

> Ship Key & Tempo, the second bundled plugin: transpose a song to the key the band plays it in, or slow a solo to 60% without changing pitch, and have each track remember its setting. Like Section Loop it is built only on the public plugin API and released under the host's license.
>
> The plugin declares `audio.effects`, `ui.panel`, `ui.shortcuts`, and `state.track`. It creates one pitch-shift node and one time-stretch node in the effect chain, both labeled with the plugin's name, and exposes semitones (−12..+12 with fine cents), tempo percent (25..200), formant preservation, and quality mode (performance/quality). Actions: key up/down by one semitone, tempo up/down by a configurable step (default 10%), reset key, reset tempo, toggle "remember for this track", and toggle "keep across tracks". Default shortcuts include `+` / `-` for tempo steps.
>
> Per-track memory is off by default. When "remember for this track" is on, the current key and tempo are stored under the track's identity and restored on load, and the panel shows a small "restored" badge so the user knows why the key is shifted. When a track has no stored setting, key and tempo reset to 0 and 100% on track change — unless "keep across tracks" is on, in which case the current settings persist until changed.
>
> Also in this slice: after first sign-in the app shows a dismissible "Getting started" panel introducing Section Loop and Key & Tempo, the default keyboard shortcuts, and a link to the plugin tutorial (a placeholder link until the tutorial ships). Both bundled plugins are installed and enabled by default with pre-approved permissions.
>
> Acceptance: when the user sets −2 semitones on a track, the pitch drops a whole step within 20 ms with no change in tempo and no obvious artifacts on typical mixed music. When the user turns on "remember for this track" and skips to the next song, key returns to 0; returning to the first song restores −2 with the badge. When Section Loop is looping and the user drags tempo to 60%, the loop continues with pitch unchanged and the chain shows Key & Tempo's nodes as the only effects. When the user chose "don't remember" and reopens the track, tempo is 100% while Section Loop's markers remain.

## Scope boundary

Does not cover showing the detected key from analysis, MIDI-bound tempo control, or the plugin tutorial content itself.

## Open questions

- Q-9: per-track memory default; this prompt assumes off by default with one-click enable, as the spec suggests.
- A-5: bundled plugins enabled by default with pre-approved permissions.

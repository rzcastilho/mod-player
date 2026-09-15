# 001-mvp / 007 — Named Actions and Keyboard Shortcuts

**Source:** [FR-9.1 Actions](../../ModPlayer-Software-Specification.md#fr-91-actions), [FR-9.2 Keyboard](../../ModPlayer-Software-Specification.md#fr-92-keyboard), [DM-14 Action](../../ModPlayer-Software-Specification.md#dm-14-action), [DM-15 Binding](../../ModPlayer-Software-Specification.md#dm-15-binding), [Part 9 § 3 Core services](../../ModPlayer-Software-Specification.md#core-services) (AR-8), [EC § 8 Controls](../../ModPlayer-Software-Specification.md#8-controls), [NFR § 6 Accessibility](../../ModPlayer-Software-Specification.md#6-accessibility) (NFR-6.1), [§ 1 Primary personas](../../ModPlayer-Software-Specification.md#1-primary-personas) (Marina)

**Prerequisites:** Assumes transport from 001-mvp/003-streaming-playback-and-queue and markers/loops from 001-mvp/006-markers-loops-and-cues.

## Prompt

> Let a musician with hands on their instrument drive the whole player from the keyboard, and lay the foundation for MIDI and plugin-registered controls later.
>
> Every host operation — transport (play, pause, toggle, stop, next, previous, seek step, volume up/down), markers (set A, set B, nudge active marker earlier/later, clear), loops (toggle loop), cues (set cue 1–8, jump to cue 1–8), effect-chain and navigation commands — is exposed as a named action with a stable identifier, a label, and a kind: trigger (fires once) or continuous (carries a value). Actions live in a central Action & Binding service that dispatches invocations to the owning component with minimal latency; later slices let plugins register actions in their own namespace and let MIDI messages bind to the same actions.
>
> The host ships a default shortcut set covering all transport, marker, loop, cue, and volume actions, so the player is fully operable without a mouse. Defaults include `I` set A, `O` set B, `L` toggle loop, `[` / `]` nudge the active marker, and `+` / `-` for tempo steps once effects exist. Modifier conventions follow each platform. A settings page shows the shortcut map; the user can rebind any shortcut, and a single action can carry several bindings.
>
> Conflicts are detected and displayed: when two bindings claim the same key, the map flags both and neither is active until the user resolves it. A shortcut bound to an action whose owner is currently disabled is kept but shown greyed and inactive.
>
> Acceptance: when the user presses `I` then `O` then `L` during playback, markers A and B appear on the waveform and the loop arms, with no mouse involved. When the user rebinds "toggle loop" to a key already bound to "play/pause", the map shows the conflict and neither fires until one is changed. When a binding is rebound and the app restarts, the custom binding is still in effect.

## Scope boundary

Does not cover MIDI, global (app-unfocused) shortcuts, media keys, or plugin-registered actions — only host actions and in-app keyboard bindings.

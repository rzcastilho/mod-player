# 001-mvp / 010 — Transport Focus Arbitration

**Source:** [FR-3.3 Transport focus](../../ModPlayer-Software-Specification.md#fr-33-transport-focus), [FR-7.4 Conflicts and ordering](../../ModPlayer-Software-Specification.md#fr-74-conflicts-and-ordering) (FR-7.4.3), [Part 5 § 6.1 Transport](../../ModPlayer-Software-Specification.md#61-transport-transportcontrol), [Part 9 § 5 Key flows](../../ModPlayer-Software-Specification.md#5-key-flows) (5.3), [J-8 — Reorder and resolve effect chain and transport focus](../../ModPlayer-Software-Specification.md#j-8--reorder-and-resolve-effect-chain-and-transport-focus-jtbd-13), [EC § 3 Playback and transport](../../ModPlayer-Software-Specification.md#3-playback-and-transport-1) (EC-3.4), [EC § 4 Markers, loops, cues](../../ModPlayer-Software-Specification.md#4-markers-loops-cues) (EC-4.4), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-13)

**Prerequisites:** Assumes the plugin runtime and `transport.control` capability from 001-mvp/009-plugin-runtime-and-permissions.

## Prompt

> When several plugins can move the playhead, let the user decide which one is in charge so they never fight, and guarantee the user always wins over any plugin.
>
> At any moment exactly one party holds transport focus: the host or one plugin. Only the focus holder's programmatic transport commands (play, pause, seek, arm or disarm loops) take effect; every other plugin is an observer and its transport requests return `no_focus`. The user can always issue transport commands through the host UI, keyboard, or (later) MIDI, and commands from other controllers on the account count as user commands; user commands take precedence over any plugin.
>
> A plugin acquires focus by request and the host grants it according to the user's focus policy: manual (only the user assigns focus), auto on interaction (a plugin gains focus when the user interacts with its transport controls; a host user action returns focus to the host), or first request wins (the first plugin to request focus per track keeps it). Default is auto on interaction. The previous holder receives `focus_revoked` and drops to observer; the new holder receives `focus_granted`.
>
> A Transport panel shows which plugin holds focus and which have requested it; the user can hand focus to any plugin or take it back. When the focus-holding plugin is disabled, suspended, or crashes, focus returns to the host immediately and any loop that plugin had armed is disarmed — persisted markers stay, because the loop state belongs to the plugin and the markers belong to the track.
>
> Acceptance: when plugin X holds focus and plugin Y calls seek, Y receives `no_focus` and the playhead does not move. When the user presses the host's loop-toggle shortcut while a plugin holds focus under the auto policy, the toggle applies and focus returns to the host. When a remote controller pauses while a plugin holds focus, playback pauses and the plugin receives `play_state_changed`. When the focus holder is suspended for exceeding its budget, the Transport panel shows "host" within the same second and the armed loop is released.

## Scope boundary

Does not cover effect-chain ordering, presets that snapshot focus, or the MIDI path — only focus arbitration, policy, and the panel.

## Open questions

- Q-7: whether focus should reset per track or persist across tracks; the spec implies persistent until changed.

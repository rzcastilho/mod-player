# 004-performance / 003 — Performance Mode and Crash-State Restore

**Source:** [§ 10 Performance Mode](../../ModPlayer-Software-Specification.md#10-performance-mode), [FR-7.2 Permissions](../../ModPlayer-Software-Specification.md#fr-72-permissions) (FR-7.2.6), [J-4 — Prepare and play a bar set (S-3)](../../ModPlayer-Software-Specification.md#j-4--prepare-and-play-a-bar-set-s-3) (steps 5, 11–12), [Part 9 § 3 Core services](../../ModPlayer-Software-Specification.md#core-services) (AR-13), [INT-6 Operating system services](../../ModPlayer-Software-Specification.md#int-6-operating-system-services) (INT-6.4), [Part 5 § 5 Events (host → plugin)](../../ModPlayer-Software-Specification.md#5-events-host--plugin) (`performance_mode_changed`), [EC § 9 Performance Mode](../../ModPlayer-Software-Specification.md#9-performance-mode), [NFR § 2 Reliability and availability](../../ModPlayer-Software-Specification.md#2-reliability-and-availability) (NFR-2.6), [NFR § 8 Observability (local)](../../ModPlayer-Software-Specification.md#8-observability-local) (NFR-8.2, 8.3), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-11)

**Prerequisites:** Assumes offline mode from 003-offline-and-library/002-offline-session-and-playback, MIDI from 004-performance/001-midi-mapping-and-profiles, the plugin runtime from 001-mvp/009-plugin-runtime-and-permissions, and Developer Mode from 002-developer-mode/001-dev-folder-and-hot-reload.

## Prompt

> Give a performer a mode where nothing can interrupt the audio — no update prompt, no plugin reload, no permission sheet, no screen going to sleep — and a checklist that catches problems before the first track, not during it.
>
> Performance Mode is entered and exited from the main UI and via a bindable action. Before entering, a pre-set checklist shows: output device present, buffer size preset, cache state of the current playlist as "n of m cached", remaining offline grace period, MIDI devices connected, and plugins with warnings. Failing items are highlighted, but the user can proceed. Sleep inhibition not permitted by the OS shows as a failed item.
>
> While active, the mode suppresses update checks and installs, plugin installs and updates, plugin hot reload, permission prompts (a plugin that needs one receives a denial and the prompt is deferred), non-critical notifications, and system and display sleep. Plugins receive `performance_mode_changed`. A plugin fault is handled silently — logged, suspended, audio kept — unless the plugin held transport focus, in which case focus returns to the host and a minimal indicator appears. Critical events (output device lost, session revoked) still show as a minimal, non-modal indicator. A status strip shows the audio underrun counter and real-time metrics: callback load, per-node cost, per-plugin CPU and memory, cache hit ratio, network state. Every suppressed event is recorded, and on exit a summary lists them (deferred update, plugin error, deferred prompt).
>
> Independently of the mode, application state — queue, position, markers, effect chain, focus holder — survives an unexpected termination and is restored on next launch without auto-resuming playback. If the app crashed while in Performance Mode, relaunch offers to re-enter it in one click.
>
> Acceptance: when the user enters Performance Mode with 38 of 40 tracks cached, the checklist highlights "38 of 40 cached" and lists the two missing tracks. When an update was downloaded before entering, install is deferred and appears in the exit summary. When a plugin without focus hangs during the set, audio and transport continue, nothing is shown, and the exit summary names the plugin. When the app crashes mid-set and is relaunched, the queue and armed loop region are restored and a one-click prompt offers to re-enter Performance Mode.

## Scope boundary

Does not cover the simplified full-screen layout or detachable panels — those are the next slice.

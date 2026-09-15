# 004-performance / 004 — Detachable Panels and Full-Screen Performance Layout

**Source:** [§ 4 Now-playing view and waveform](../../ModPlayer-Software-Specification.md#4-now-playing-view-and-waveform) (FR-4.1.6), [§ 10 Performance Mode](../../ModPlayer-Software-Specification.md#10-performance-mode) (FR-10.1.6), [Part 5 § 6.5 UI](../../ModPlayer-Software-Specification.md#65-ui) (detached panels), [§ 3 Edge users](../../ModPlayer-Software-Specification.md#3-edge-users) (multi-monitor performer), [NFR § 6 Accessibility](../../ModPlayer-Software-Specification.md#6-accessibility) (NFR-6.6 text scaling)

**Prerequisites:** Assumes Performance Mode from 004-performance/003-performance-mode and plugin panels from 001-mvp/011-plugin-ui-contributions.

## Prompt

> Let a performer put the waveform on one screen and the cue list on another, and drive a set from a layout built for a dark room and a hand on a controller, not a mouse on a desktop.
>
> The now-playing view can be detached into its own window and shown full-screen. Any plugin panel can be detached from the main window into a floating window that can live on a second display; detached panels keep their attribution, close/disable controls, theme, and accessibility, and their placement is remembered per plugin across restarts. Reattaching returns a panel to its dock.
>
> Performance Mode offers a simplified full-screen layout: large transport controls, cue pads 1–8, loop in/out/toggle, and the current effect controls (tempo and key from Key & Tempo, plus any plugin-exposed continuous parameters the user has bound), the status strip with the underrun counter, and the offline indicator. The layout is keyboard- and MIDI-operable end to end, scales with the platform text-size setting up to 200%, and respects the 40% text-expansion allowance for localized strings. The user chooses whether entering Performance Mode switches to this layout automatically.
>
> Acceptance: when the user drags Section Loop's panel to a second monitor and restarts the app, the panel reopens on that monitor. When the user enters Performance Mode with auto-layout on, the full-screen layout appears and the cue pads reflect the current track's cue slots. When the detached now-playing window is full-screen on display two and a marker is dragged there, the main window's waveform reflects the move immediately. When text size is set to 200%, every control in the performance layout stays visible and operable.

## Scope boundary

Does not cover Performance Mode suppression rules or MIDI mapping, which already exist.

# 004-performance / 002 — Global Shortcuts, Media Keys, and OS Now-Playing Integration

**Source:** [FR-9.2 Keyboard](../../ModPlayer-Software-Specification.md#fr-92-keyboard) (FR-9.2.3), [FR-3.1 Core transport](../../ModPlayer-Software-Specification.md#fr-31-core-transport) (FR-3.1.6), [INT-6 Operating system services](../../ModPlayer-Software-Specification.md#int-6-operating-system-services) (INT-6.3, 6.5), [EC § 3 Playback and transport](../../ModPlayer-Software-Specification.md#3-playback-and-transport-1) (EC-3.10), [EC § 8 Controls](../../ModPlayer-Software-Specification.md#8-controls) (EC-8.5), [NFR § 9 Compatibility and portability](../../ModPlayer-Software-Specification.md#9-compatibility-and-portability) (NFR-9.2), [NFR § 4 Security](../../ModPlayer-Software-Specification.md#4-security) (NFR-4.9)

**Prerequisites:** Assumes host actions and shortcuts from 001-mvp/007-keyboard-actions-and-shortcuts and the notification area from 001-mvp/001-walking-skeleton.

## Prompt

> Let a musician control ModPlayer while their sheet-music app is in front, and let the player behave like a first-class citizen of the operating system's media controls.
>
> Global shortcuts — active when the app is not focused — are supported for play/pause, next, previous, and loop toggle, subject to each platform's capabilities and modifier conventions. Registration is attempted at launch and on rebind; when the platform refuses because another app holds the combination, the shortcut settings show a warning naming the failed registration rather than failing silently. Behavior and shortcuts are identical across the three platforms apart from modifier conventions.
>
> Hardware media keys and the OS now-playing surface reflect the current track (title, artists, album, artwork, position) and accept play/pause, next, and previous, following the platform's focus rules: when another media app was the last active, ModPlayer does not steal the keys. Playback continues when the window is minimized, hidden, or on another virtual desktop. Notifications are routed through the OS notification center only when the app is not focused and shown in-app otherwise. The app requests no OS privileges beyond audio, MIDI, network, notifications, and its own data directory.
>
> Acceptance: when the user is typing in another app and presses the global loop-toggle shortcut, the loop arms and the other app keeps keyboard focus. When the user presses the hardware play/pause key after last interacting with a different media app, ModPlayer does not respond. When a track changes while the window is minimized, the OS now-playing surface shows the new title and artwork within a second. When a plugin is suspended while the app is unfocused, the warning appears in the OS notification center, not in a hidden in-app area.

## Scope boundary

Does not cover MIDI, sleep inhibition, or Performance Mode suppression rules.

# 001-mvp / 001 — Walking Skeleton: App Shell, Synthetic Source, and Audio Output

**Source:** [§ 7 Hard constraints](../../ModPlayer-Software-Specification.md#7-hard-constraints), [§ 8 Key risks](../../ModPlayer-Software-Specification.md#8-key-risks), [FR-1.3 Audio device check](../../ModPlayer-Software-Specification.md#fr-13-audio-device-check), [FR-3.4 Output](../../ModPlayer-Software-Specification.md#fr-34-output), [FR-11.1 Settings](../../ModPlayer-Software-Specification.md#fr-111-settings), [FR-14.1 Notifications](../../ModPlayer-Software-Specification.md#fr-141-notifications), [FR-14.4 Appearance and language](../../ModPlayer-Software-Specification.md#fr-144-appearance-and-language), [Part 9 § 1 Guiding decisions](../../ModPlayer-Software-Specification.md#1-guiding-decisions), [Part 9 § 3 Real-time path](../../ModPlayer-Software-Specification.md#real-time-path), [Part 9 § 4 Communication patterns](../../ModPlayer-Software-Specification.md#4-communication-patterns), [Part 9 § 7 Deployment shape](../../ModPlayer-Software-Specification.md#7-deployment-shape-logical), [NFR § 9 Compatibility and portability](../../ModPlayer-Software-Specification.md#9-compatibility-and-portability), [NFR § 10 Maintainability and quality](../../ModPlayer-Software-Specification.md#10-maintainability-and-quality), [GOV § 3 Contribution process](../../ModPlayer-Software-Specification.md#3-contribution-process), [EC § 3 Playback and transport](../../ModPlayer-Software-Specification.md#3-playback-and-transport-1)

**Prerequisites:** None — first slice of the MVP.

## Prompt

> Build the runnable skeleton of ModPlayer, a desktop music player for musicians and DJs, so that a user on any of the three major desktop operating systems can launch the app, pick an output device, hear a test tone, and play a synthetic test track through a real-time audio engine. Nothing here talks to a streaming service yet; the point is a demonstrable, testable audio path and app shell that every later feature plugs into.
>
> The app opens to a main window with navigation for Library, Now Playing, Plugins, and Settings (placeholders are fine), a non-blocking notification area classified as critical / warning / info, and light and dark themes that follow the system setting. Settings is searchable and organized into Account, Audio, Playback, Controls, Plugins, Offline, Appearance, Language, Developer, Privacy & diagnostics, and About.
>
> The audio engine runs on a dedicated real-time path that owns a monotonic audio clock and never allocates, blocks, or performs I/O in a callback. Audio comes from an Audio Source behind an interface; this slice ships only a synthetic implementation that produces a test tone and known waveforms, so the rest of the product is testable without the streaming service. After the engine sits a hard output limiter that cannot be bypassed, with a user-set ceiling between −6 dBFS and −0.1 dBFS, and a "safe volume on startup" option that caps volume at a user-defined level on launch.
>
> On first launch the app lists output devices, selects the system default, plays a short test tone at a capped safe level, and asks "Did you hear that?" The user can pick another device and re-test, and can choose a buffer size from presets labeled by approximate latency. The chosen device is remembered.
>
> Acceptance: when the app is launched fresh, the device check runs and the tone plays on the selected device within one second of confirmation. When the chosen device disappears while playing, audio falls back to the system default within one buffer duration and a critical notice names the lost device. When the device sample rate changes, only the output stage reconfigures and the gap is at most one buffer. When any component tries to write to the real-time path, it goes through a queue applied at a buffer boundary. Continuous integration builds and tests on all three platforms from the first commit.

## Scope boundary

No sign-in, no streaming catalog, no waveform, no plugins, no effect nodes — only the shell, the synthetic source, the engine, the limiter, and device handling.

## Open questions

- Reference hardware list and reference track set (A-16) are not yet defined; latency and quality tests in later slices need them.
- Q-15: maximum acceptable app footprint (download size, idle memory) is unspecified.

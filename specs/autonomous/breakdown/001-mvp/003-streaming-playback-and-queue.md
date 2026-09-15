# 001-mvp / 003 — Streaming Playback, Transport, and Queue

**Source:** [FR-3.1 Core transport](../../ModPlayer-Software-Specification.md#fr-31-core-transport), [FR-3.2 Queue](../../ModPlayer-Software-Specification.md#fr-32-queue), [FR-8.3 Streaming behavior](../../ModPlayer-Software-Specification.md#fr-83-streaming-behavior), [INT-2 Streaming service — Connect receiver protocol](../../ModPlayer-Software-Specification.md#int-2-streaming-service--connect-receiver-protocol), [INT-6 Operating system services](../../ModPlayer-Software-Specification.md#int-6-operating-system-services), [DM-2 TrackRef](../../ModPlayer-Software-Specification.md#dm-2-trackref-account-scoped), [Part 9 § 3 Core services](../../ModPlayer-Software-Specification.md#core-services), [EC § 3 Playback and transport](../../ModPlayer-Software-Specification.md#3-playback-and-transport-1), [NFR § 1 Performance and latency](../../ModPlayer-Software-Specification.md#1-performance-and-latency), [NFR § 2 Reliability and availability](../../ModPlayer-Software-Specification.md#2-reliability-and-availability), [§ 5 Scope](../../ModPlayer-Software-Specification.md#5-scope)

**Prerequisites:** Assumes the engine and synthetic source interface from 001-mvp/001-walking-skeleton and a signed-in Premium account from 001-mvp/002-first-launch-and-sign-in.

## Prompt

> Let a signed-in Premium user actually hear their catalog. ModPlayer registers itself with the streaming service as a Connect receiver device named "ModPlayer on <machine name>" (user-editable), receives track load requests and the encrypted stream, decrypts and decodes it in-client, and renders it through the audio engine built in the walking skeleton. This real Audio Source implements the same interface as the synthetic source, is the only component that knows the service's protocols, and is packaged as a separately versioned module so it can be replaced or disabled if the protocol changes.
>
> The user gets a full transport: play, pause, stop, skip forward, skip back (restart the track, or go to the previous track if within the first three seconds), seek to any position, and volume. Position is reported from the audio clock at least 60 times per second. Track transitions are gapless where the format allows. Playback continues when the window is minimized, hidden, or on another virtual desktop. A play queue derives from the current context (playlist, album, search results) plus user-added "play next" items; the user can view, reorder, and remove queue items, and use shuffle and repeat (off, one, all).
>
> The current track is pre-buffered fully as fast as the connection allows and the next queued track is pre-fetched. Other controllers on the same account can transfer playback to ModPlayer (it accepts and starts playing) or away from it (it stops locally, keeps UI state, and shows "playing on <other device>"); commands from other controllers count as user commands.
>
> Acceptance: when the user presses play on an already-buffered track, audio starts within 50 ms; on a freshly streamed track, within 1.5 s on a 10 Mbit/s connection. When the connection degrades mid-track, playback continues from buffered data; if the buffer runs dry, the transport shows "buffering" and resumes automatically rather than skipping. When the user seeks past the end of the track, the position clamps and the queue advances. When the receiver protocol breaks, the Audio Source reports "source unavailable" and the app shows a clear message with a link to the project's status page instead of crashing.

## Scope boundary

Does not cover search or library views, the waveform, the offline cache, crossfade between tracks, or any plugin access to transport.

## Open questions

- Q-2: whether the receiver protocol allows seeking into an unbuffered offset or only sequential fetch; affects the 500 ms uncached-seek target.
- Q-3: which quality tiers and formats a receiver gets, and whether any preclude gapless playback.
- A-11: independent versioning of the Audio Source module.

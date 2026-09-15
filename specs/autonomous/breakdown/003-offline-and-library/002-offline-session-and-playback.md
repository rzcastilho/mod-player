# 003-offline-and-library / 002 — Offline Session Start and Playback

**Source:** [FR-8.2 Offline operation](../../ModPlayer-Software-Specification.md#fr-82-offline-operation), [FR-2.1 Search](../../ModPlayer-Software-Specification.md#fr-21-search) (FR-2.1.4 offline search), [J-9 — Offline session start](../../ModPlayer-Software-Specification.md#j-9--offline-session-start), [J-4 — Prepare and play a bar set (S-3)](../../ModPlayer-Software-Specification.md#j-4--prepare-and-play-a-bar-set-s-3) (performance steps 7–8), [DM-22 ConnectivityState](../../ModPlayer-Software-Specification.md#dm-22-connectivitystate), [INT-2 Streaming service — Connect receiver protocol](../../ModPlayer-Software-Specification.md#int-2-streaming-service--connect-receiver-protocol) (INT-2.6), [Part 5 § 5 Events (host → plugin)](../../ModPlayer-Software-Specification.md#5-events-host--plugin) (`connectivity_changed`), [EC § 7 Offline cache and connectivity](../../ModPlayer-Software-Specification.md#7-offline-cache-and-connectivity) (EC-7.3, 7.4, 7.5, 7.8), [EC § 1 Account and session](../../ModPlayer-Software-Specification.md#1-account-and-session) (EC-1.2), [NFR § 2 Reliability and availability](../../ModPlayer-Software-Specification.md#2-reliability-and-availability) (NFR-2.5), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-6)

**Prerequisites:** Assumes the cache from 003-offline-and-library/001-offline-cache-and-pinning and sign-in from 001-mvp/002-first-launch-and-sign-in.

## Prompt

> Let a DJ launch ModPlayer at a venue with no internet and play the set they pinned earlier, and let a user whose connection drops mid-session keep listening without a dialog in their face.
>
> On launch without connectivity, the app validates the stored session credential locally and confirms it is within the offline grace period — the maximum interval the client will operate without renewing its session online, bounded by the service's credential lifetime. Settings shows the remaining grace period. Inside the grace period the app opens in offline mode: library and playlists render from cached metadata, cached tracks are playable, and uncached tracks are visibly marked and skipped by the queue with a notice. Search works over the local metadata index only, returning first results within 50 ms, with results not playable offline marked as such.
>
> Offline state shows as a persistent, unobtrusive indicator; no modal announces going offline or online. Connectivity changes are debounced so a flapping connection produces no notification storm. Plugins receive `connectivity_changed`. When connectivity returns, the app reconnects silently and refreshes. If the connection drops mid-track, playback continues from buffer or cache with no dropout.
>
> Acceptance: when the user launches offline within the grace period and plays a pinned playlist, every pinned track plays and the offline indicator is the only visual change. When the grace period has expired, the app explains it must go online once to renew and shows that nothing is playable; library stays viewable. When a partially cached track is played offline, the waveform shows the cached extent and the playhead cannot pass it. When the credential expires while playing online, the current track finishes from buffer, refresh is attempted, and a "Session expired — sign in again" notice appears only if refresh fails.

## Scope boundary

Does not cover the Performance Mode pre-set checklist that surfaces the grace period, offline library edits, or the registry's cached index.

## Open questions

- Q-1 / A-3: the actual grace period the service's credentials allow, and whether the receiver protocol permits playing cached streams without a live session — this is the single biggest risk to the offline promise and needs a spike before planning.

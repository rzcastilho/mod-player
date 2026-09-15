# 007-operations-and-polish / 005 — Playback Polish: Crossfade, Quality Tier, Library Sorting

**Source:** [FR-3.1 Core transport](../../ModPlayer-Software-Specification.md#fr-31-core-transport) (FR-3.1.5), [FR-8.3 Streaming behavior](../../ModPlayer-Software-Specification.md#fr-83-streaming-behavior) (FR-8.3.3), [FR-2.2 Library](../../ModPlayer-Software-Specification.md#fr-22-library) (FR-2.2.4), [DM-20 AudioSettings](../../ModPlayer-Software-Specification.md#dm-20-audiosettings) (crossfade between tracks), [EC § 3 Playback and transport](../../ModPlayer-Software-Specification.md#3-playback-and-transport-1) (EC-3.7), [B. Open questions](../../ModPlayer-Software-Specification.md#b-open-questions) (Q-3)

**Prerequisites:** Assumes playback from 001-mvp/003-streaming-playback-and-queue, loops from 001-mvp/006-markers-loops-and-cues, and library views from 001-mvp/004-search-and-library-browse.

## Prompt

> Round out everyday listening: smooth transitions between tracks, control over streaming quality, and a library that sorts the way the user thinks.
>
> Under Settings → Playback, the user sets a crossfade duration between tracks from 0 to 12 seconds. Crossfade is automatically disabled while a loop region is armed — a loop repeats a section, repeat-one repeats the track, and the two never interact — and the control shows "off while looping" during that time. Crossfades render on the real-time path and respect the output limiter.
>
> Under Settings → Audio, the user chooses the streaming quality tier from those the account allows; the chosen tier applies to new streams and to cache fills, and the cache view shows each entry's tier. If a tier precludes gapless transitions, the setting says so.
>
> The Library becomes sortable by name, artist, date added, and recently played, in each of saved tracks, saved albums, and playlists, with the choice remembered per view.
>
> Acceptance: when crossfade is 4 s and a track ends naturally, the next begins overlapping the last 4 seconds with a smooth level transition. When the user arms a loop with crossfade set, the transport shows "off while looping" and releasing the loop restores it. When the user switches from the highest tier to a lower one, the next track streams at the new tier and the currently playing track is unaffected. When saved tracks are sorted by date added, the newest saved track is first and the order persists after restart.

## Scope boundary

Does not cover gapless playback itself or the offline cache, which already exist.

## Open questions

- Q-3: which quality tiers and formats a receiver can obtain, and whether any preclude gapless playback.

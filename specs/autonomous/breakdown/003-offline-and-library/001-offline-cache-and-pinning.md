# 003-offline-and-library / 001 — Encrypted Offline Cache and Pinning

**Source:** [FR-8.1 Cache](../../ModPlayer-Software-Specification.md#fr-81-cache), [FR-2.3 Playlists](../../ModPlayer-Software-Specification.md#fr-23-playlists) (FR-2.3.4 pin playlist), [FR-2.1 Search](../../ModPlayer-Software-Specification.md#fr-21-search) (pin action), [DM-4 CachedStream](../../ModPlayer-Software-Specification.md#dm-4-cachedstream-account-scoped), [Part 9 § 3 Core services](../../ModPlayer-Software-Specification.md#core-services) (AR-9), [J-4 — Prepare and play a bar set (S-3)](../../ModPlayer-Software-Specification.md#j-4--prepare-and-play-a-bar-set-s-3) (preparation step 2), [EC § 7 Offline cache and connectivity](../../ModPlayer-Software-Specification.md#7-offline-cache-and-connectivity) (EC-7.1, 7.2, 7.6, 7.7), [EC § 12 Empty, loading, and first-run states](../../ModPlayer-Software-Specification.md#12-empty-loading-and-first-run-states) (cache), [NFR § 3 Scalability (local)](../../ModPlayer-Software-Specification.md#3-scalability-local) (NFR-3.2), [NFR § 4 Security](../../ModPlayer-Software-Specification.md#4-security) (NFR-4.2), [§ 7 Hard constraints](../../ModPlayer-Software-Specification.md#7-hard-constraints) (C-2, C-6), [§ 10 Retention summary](../../ModPlayer-Software-Specification.md#10-retention-summary)

**Prerequisites:** Assumes streaming playback from 001-mvp/003-streaming-playback-and-queue and library views from 001-mvp/004-search-and-library-browse.

## Prompt

> Let a DJ pin tonight's playlist in the afternoon so a venue's flaky connection cannot end the set. The cache stores the encrypted stream of every track the player plays — plus its metadata, artwork, and analysis — so recently played tracks stay playable offline; explicitly pinned items are downloaded ahead of time.
>
> The user can pin a track, an album, or a whole playlist in one action from any track row, album, or playlist. Pinning shows per-track cache progress and an estimate of storage used; pinned items are never evicted automatically. The cache has a user-set size limit, defaulting to 10 GB or 10% of free disk, whichever is smaller; unpinned tracks are evicted least-recently-played first. Partially downloaded streams are resumed, not restarted.
>
> The cache is encrypted at rest with a key held in the OS secure store and is unreadable to the user, to plugins, and to other applications — it holds the encrypted stream, never decoded audio. A cache view under Settings → Offline shows usage split into pinned versus recent, a "Pinned for offline" collection in the Library, and actions to clear recent or clear all. Cached content is invalidated on sign-out and when the service revokes the session. The empty state reads "Nothing cached yet — pin a playlist".
>
> Acceptance: when the user pins a 40-track playlist on a good connection, all 40 show progress and finish, and the cache view reflects the new size. When the cache is full while pinning, a dialog offers to raise the limit or evict unpinned tracks; pinned tracks are never touched. When the disk falls below the OS free-space threshold, caching pauses with a warning while playback continues. When the secure-store key is lost, the cache is unreadable, so it is cleared and rebuilt with a notice "offline cache was reset". When a cached entry is corrupt, it is dropped and re-fetched when online. Lookup and eviction stay sub-second at 100 GB and 20,000 tracks.

## Scope boundary

Does not cover starting or playing while offline, the offline grace period, or offline search — those are the next slice.

## Open questions

- A-6: the default cache limit; adjust if the requester prefers a different default.
- A-10: analysis data is cleared with the cache.

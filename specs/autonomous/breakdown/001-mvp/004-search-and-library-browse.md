# 001-mvp / 004 — Catalog Search and Library Browsing

**Source:** [FR-2.1 Search](../../ModPlayer-Software-Specification.md#fr-21-search), [FR-2.2 Library](../../ModPlayer-Software-Specification.md#fr-22-library), [FR-2.3 Playlists](../../ModPlayer-Software-Specification.md#fr-23-playlists), [FR-2.4 Track metadata](../../ModPlayer-Software-Specification.md#fr-24-track-metadata), [INT-3 Streaming service — catalog and library](../../ModPlayer-Software-Specification.md#int-3-streaming-service--catalog-and-library), [DM-3 PlaylistRef, AlbumRef, ArtistRef](../../ModPlayer-Software-Specification.md#dm-3-playlistref-albumref-artistref-account-scoped), [EC § 2 Catalog, library, playlists](../../ModPlayer-Software-Specification.md#2-catalog-library-playlists), [EC § 12 Empty, loading, and first-run states](../../ModPlayer-Software-Specification.md#12-empty-loading-and-first-run-states), [NFR § 3 Scalability (local)](../../ModPlayer-Software-Specification.md#3-scalability-local), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done)

**Prerequisites:** Assumes sign-in from 001-mvp/002-first-launch-and-sign-in and playback from 001-mvp/003-streaming-playback-and-queue.

## Prompt

> Let a user find and start music at least as fast as in the official client, so they never need two players. This slice is search and browse plus play; editing the library is a later slice.
>
> Search takes free text and returns results grouped into tracks, albums, artists, and playlists, updating as the user types with a short debounce, showing at least the first 20 items per group with "show more". Podcasts, audiobooks, and other non-music content never appear. Every result offers play now, play next, add to queue, and (as placeholders wired later) add to playlist, save to library, and pin for offline.
>
> The Library view mirrors the account: saved tracks, saved albums, followed artists, and playlists (own and followed; playlists the user does not own are view-only), plus a Recently Played list of at least the last 100 tracks. For every track the app holds title, artists, album, artwork, duration, explicit flag, release date, and the service's stable track identifier, which becomes the key for all per-track state in later slices. Library sync runs on launch and periodically while online.
>
> Lists are virtualized and the UI never blocks on network: a rate-limited response shows stale data with a subtle "refreshing…" indicator, not an error. Empty states are explicit — "Your library is empty — search to add music" with a search action; "No playlists yet — create one" — and loading uses skeleton rows, never a spinner over the whole view.
>
> Acceptance: when the user types a query online, first results appear within 300 ms of the last keystroke. When a track is unavailable in the user's region or has been removed, its row is greyed with the reason and the queue skips it with a notice. When artwork fails to load, a placeholder with initials appears. When a library has 50,000 saved tracks and 1,000 playlists, scrolling and search show no visible degradation.

## Scope boundary

Does not cover saving/unsaving, following, playlist editing, offline search over the cached index, pinning, or sorting options — those land in the offline-and-library wave and polish wave.

## Open questions

- A-1: library management is assumed in scope; this slice only reads, so the assumption gates later slices, not this one.

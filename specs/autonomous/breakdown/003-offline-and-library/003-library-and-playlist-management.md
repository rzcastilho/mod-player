# 003-offline-and-library / 003 — Library and Playlist Management with Offline Sync

**Source:** [FR-2.2 Library](../../ModPlayer-Software-Specification.md#fr-22-library) (FR-2.2.3), [FR-2.3 Playlists](../../ModPlayer-Software-Specification.md#fr-23-playlists) (FR-2.3.1, 2.3.3), [FR-8.2 Offline operation](../../ModPlayer-Software-Specification.md#fr-82-offline-operation) (FR-8.2.5), [Part 5 § 6.9 Library](../../ModPlayer-Software-Specification.md#69-library-libraryread--librarywrite), [DM-21 PendingSyncOp](../../ModPlayer-Software-Specification.md#dm-21-pendingsyncop-account-scoped), [Part 9 § 3 Core services](../../ModPlayer-Software-Specification.md#core-services) (AR-10), [INT-3 Streaming service — catalog and library](../../ModPlayer-Software-Specification.md#int-3-streaming-service--catalog-and-library) (INT-3.2), [EC § 2 Catalog, library, playlists](../../ModPlayer-Software-Specification.md#2-catalog-library-playlists) (EC-2.3, 2.4), [EC § 10 Settings, presets, per-track state](../../ModPlayer-Software-Specification.md#10-settings-presets-per-track-state) (EC-10.4), [§ 5 Scope](../../ModPlayer-Software-Specification.md#5-scope) (5.1 library management), [A. Assumptions](../../ModPlayer-Software-Specification.md#a-assumptions) (A-1)

**Prerequisites:** Assumes library browsing from 001-mvp/004-search-and-library-browse, the plugin runtime from 001-mvp/009-plugin-runtime-and-permissions, and offline mode from 003-offline-and-library/002-offline-session-and-playback.

## Prompt

> Let a user manage their library from ModPlayer — save and unsave tracks and albums, follow and unfollow artists, and create, rename, reorder, and delete their own playlists and add or remove tracks — so they never need the official client for the basics, and let those edits survive being made offline.
>
> Playlists the user does not own remain view-only. Every edit propagates to the account immediately when online. When offline, edits are queued as pending operations (save, unsave, follow, playlist create/rename/reorder/add/remove) with their target and payload, and applied in order on reconnect. A conflict with an edit made elsewhere is resolved by keeping adds and removes that still make sense by track identity, keeping the remote order for reorders and dropping the local reorder, and reporting the outcome. A queued op that targets a playlist deleted elsewhere is dropped and listed.
>
> Plugins gain two permissions: `library.read` (search, get playlists, get a playlist, get saved items) and `library.write` (create playlist, add to / remove from playlist, save / unsave track). Plugin writes are queued offline exactly like user writes.
>
> Acceptance: when the user adds three tracks to their own playlist while offline and reconnects an hour later, the three tracks appear in the account's playlist and the queue empties. When the user reordered a playlist offline that was also reordered elsewhere, the remote order wins and a notice "Some playlist changes could not be applied" lists the dropped reorder. When the user signs out with unsynced offline edits, a confirmation lists the pending changes and offers to sync first if online, or discard. When a plugin without `library.write` calls create playlist, it receives `permission_denied`.

## Scope boundary

Does not cover library sorting options (polish wave) or pinning, which already exists.

## Open questions

- A-1: library management assumed in scope; if the requester excludes it, this entire slice and the `library.write` permission are dropped.

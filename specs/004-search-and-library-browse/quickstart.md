# Quickstart: validating Catalog Search and Library Browsing

**Feature**: 004-search-and-library-browse | **Plan**: [plan.md](plan.md)

## Prerequisites

- Rust 1.95.0 (pinned by `rust-toolchain.toml`); `cargo deny` installed.
- For manual scenarios: a Spotify **Premium** account signed in through
  002's flow; network access. Automated gates need neither.

## Automated gates (run from the repository root)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

Named tests that must exist and pass (see the *Tests pinning this contract*
sections in [contracts/](contracts/)):

| Requirement | Test |
|---|---|
| FR-001 debounce/trim/discard | `modplayer-core tests/search_session.rs` |
| FR-002 order/omission/paging | `search_session.rs`, `modplayer-ui tests/search_view.rs` |
| FR-003 non-music dropped | `modplayer-audio-source-connect tests/catalog_mapping.rs::non_music_hits_are_dropped` |
| FR-004/005 six actions, placeholders inert | `modplayer-ui tests/accessibility.rs`, `tests/rows.rs` |
| FR-006/007 acting list & queue ops | `modplayer-ui tests/rows.rs`, `modplayer-core tests/queue_proptest.rs` |
| FR-008 track fields/badge | `catalog_mapping.rs`, `tests/rows.rs` |
| FR-009/010 sections, owner label, no edit controls | `modplayer-ui tests/library_view.rs` |
| FR-011 sync triggers, silent failure | `modplayer-core tests/library_sync.rs` |
| FR-012 / SC-008 play log | `modplayer-core tests/play_log.rs` |
| FR-013 detail views | `modplayer-ui tests/library_view.rs::detail_*` |
| FR-014 / SC-002 virtualisation at 50 000 × 1 000 | `modplayer-ui tests/library_view.rs::large_fixture_frame_budget` (run with `--release` for the timing assertion; debug asserts only row-count) |
| FR-015 / SC-005 rate-limit ⇒ stale + refreshing | `search_session.rs`, `library_sync.rs`, `search_view.rs` |
| FR-016 / SC-004 skeletons, no spinner | `search_view.rs`, `library_view.rs` |
| FR-017/018 empty/offline/no-results copy | `library_view.rs`, `search_view.rs`, `fluent_keys.rs` |
| FR-019 / SC-006 unavailable rows | `tests/rows.rs`, `catalog_mapping.rs::availability_*` |
| FR-020 / SC-007 initials | `tests/rows.rs::initials_*` |
| FR-021 first-sync-failed state | `library_sync.rs`, `library_view.rs` |
| FR-022 scaffold removed | `modplayer-ui tests/developer_removed.rs`, `fluent_keys.rs` |
| FR-024 / SC-009 keyboard + a11y | `tests/accessibility.rs` |
| FR-025 strings externalised | `fluent_keys.rs` |
| SC-001 first group ≤ 300 ms | `search_view.rs::first_group_paints_within_budget` (scripted 100 ms reply, injected clock) |
| Constitution IV guard | `modplayer tests/single_dependent.rs` (unchanged, must stay green) |
| Persistence safety | `modplayer-core tests/persist.rs` |
| Constitution VIII state-serialization proptests (`index.json`, `play_log.json`) | `modplayer-core tests/persist.rs::index_file_round_trips_any_index`, `::play_log_file_round_trips_any_log` |

## Protocol probes (manual, gate the receiver mapping — research R14)

```bash
MODPLAYER_LIVE=1 cargo test -p modplayer-audio-source-connect --test live -- --ignored search_probe library_sets_probe availability_probe catalog_wire_probe restrictions_probe
```

Expected: each probe prints which strategy answered (`searchview` /
`context`, `collection-v2` / `context-collection`) and a redacted item
count; `catalog_wire_probe` additionally prints the raw librespot
`ErrorKind`/HTTP status per endpoint so a refusal can be attributed. Record
the outcome in `research.md` R14 before wiring UI tasks that assume four
search groups or three collection sets. Last run 2026-09-17: V1 and V2
both refuted — see research.md "R14 outcomes".

## Manual scenarios (signed-in Premium account)

| # | Steps | Expected |
|---|---|---|
| M1 Search | Open Search (`Ctrl/Cmd+2`), type a common artist name, pause | Within 300 ms at least one group shows rows; groups in order Tracks, Albums, Artists, Playlists; absent kinds omitted; no podcast/episode anywhere |
| M2 Race | Type fast, keep typing before results land | Only the final query's results ever appear; no flicker back to an older query |
| M3 Show more | In a group with > 20 hits click **Show more** twice | 40 then 60 rows; button spinner only |
| M4 Play now | Focus a track row in Tracks, press Enter | Queue context = loaded Tracks rows in order, cursor on that track, playback starts (Now Playing shows it) |
| M5 Album/Playlist | Open a search album's menu → **Play now** | Full album plays from track 1; **Play next** inserts the whole album after the play-next block; **Add to queue** appends |
| M6 Placeholders | Activate Add to playlist / Save to library / Pin for offline | Info toast "Coming soon"; nothing else changes; repeatable |
| M7 Library | Open Library (`Ctrl/Cmd+1`) right after launch | Saved Tracks tab renders immediately (skeletons ⇒ rows as hydration lands); tabs in the fixed order; no Pinned tab |
| M8 Playlists | Playlists tab; open an owned and a followed playlist | Followed shows "Owner: <name>"; neither shows any edit control; tracks in playlist order with six actions |
| M9 Recently Played | Play three tracks, replay the first, restart the app | Recently Played lists them newest-first, the replayed track once at the top; a queued-but-unplayed track absent |
| M10 Offline library | Disconnect network, restart, open Library | Snapshot renders; no error; Search shows the offline state |
| M11 Reconnect | Reconnect network | "Refreshing…" not shown (unless rate-limited); sync runs; view updates in place; Search works again |
| M12 Rate limit (debug) | `MODPLAYER_CATALOG_FORCE_429=1`, search | Existing rows stay, "Refreshing…" shows, never an error; unset ⇒ recovers |
| M13 Unavailable | Search a known region-locked track | Row greyed with "Unavailable in your region"; all six actions enabled; playing it ⇒ 003's skip notice |
| M14 Artwork | `MODPLAYER_ARTWORK_FORCE_FAIL=1` | Every row shows initials (e.g. "AB"), never a broken image |
| M15 Scale | `MODPLAYER_LIBRARY_FIXTURE=large` (50 000 tracks / 1 000 playlists synthetic index) | Scrolling any tab and typing a search feel identical to a small library |
| M16 Scaffold gone | Settings › Developer | No "Play from account" control |

Debug-only environment overrides (M12/M14/M15) are read fresh on each use
like 003's `MODPLAYER_CONNECT_FORCE_UNAVAILABLE`, compiled out of release
builds. They are process environment, so a scenario that needs the
override *on* is a relaunch with it set; "unset ⇒ recovers" (M12) cannot
be driven from outside the process and is covered by
`search_session.rs`' rate-limit tests instead.

## Manual walk 2026-09-17 (T080, macOS, live Premium account)

Driven per the constitution's "Manual Scenario Sign-Off" (Quartz
`CGEventPost`/`CGEventPostToPid` + `screencapture`). Results:

| # | Result | Notes |
|---|---|---|
| M1 | pass with deviation | Tracks group only; Albums/Artists/Playlists omitted (research R14 V1: `searchview` retired, tracks-only context-resolve answers). Skeletons per group while loading. First rows ≈ 5–8 s after the debounce, not 300 ms — context-resolve + one `Track::get` per hit, sequential (SC-001 miss recorded in plan.md Post-Implementation Findings). |
| M2 | pass | Racing "beatles" ⇒ "abba": only ABBA rows ever appeared. |
| M3 | pass with deviation | Show more fetched page 2, which was empty (the search context is a single 20-track page); the button then hides — honest, no error. |
| M4 | pass | From Search and from Library: queue = loaded rows in order, cursor on the row, playback starts. |
| M5 | pass | Library Playlists › Play next inserts the whole playlist after the current item (album/playlist search rows are unreachable while M1's deviation stands; Library detail covers them). |
| M6 | pass | Info toast "Coming soon", nothing else changes, repeatable. |
| M7 | pass | Saved Tracks renders immediately (from the snapshot on relaunch; skeletons ⇒ rows on first sync); fixed tab order; no Pinned tab. Saved Albums / Followed Artists show their empty state (R14 V2). |
| M8 | pass | Followed playlists show "Owner: <name>", owned ones don't; no edit control; tracks in order with the six actions. |
| M9 | pass | Newest-first, replayed track once at the top, persisted across quit/relaunch. |
| M10 | pass | Offline simulated with `sandbox-exec -p '(version 1)(allow default)(deny network*)'` (app only, so the driving session keeps its link): snapshot renders, no error, Search shows the offline copy. |
| M11 | not executed | Reconnect cannot be driven without cutting the agent's own network; `library_sync.rs` / `search_session.rs` reconnect tests stand in. |
| M12 | pass | `MODPLAYER_CATALOG_FORCE_429=1`: snapshot rows stay, "Refreshing…" in Library and Search, never an error. |
| M13 | not executed | No known region-locked URI at hand; `catalog_mapping.rs::availability_*` pins the mapping (and R14 V4's relink finding). |
| M14 | pass | `MODPLAYER_ARTWORK_FORCE_FAIL=1`: initials everywhere, no broken image. |
| M15 | pass | `MODPLAYER_LIBRARY_FIXTURE=large`: 50 000 rows / 1 000 playlists scroll and search like a small library. |
| M16 | pass | Settings › Developer has no "Play from account". |

Defects found by the walk and fixed in the same run are listed on T080 in
tasks.md and in plan.md "Post-Implementation Findings".

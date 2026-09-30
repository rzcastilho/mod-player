# Quickstart: Search Results Structure and Feedback

**Feature**: 026-search-results-structure | **Plan**: [plan.md](./plan.md)

Validation guide only — rules live in [contracts/](./contracts/), types in [data-model.md](./data-model.md).

## Prerequisites

- Run from the worktree root. The shell may carry `RUSTUP_TOOLCHAIN`; prefix with `RUSTUP_TOOLCHAIN=1.95.0` (constitution Governance recipe).
- Manual scenarios: macOS host, signed-in account in the Keychain, Python 3 with `pyobjc-framework-Quartz`; helper scripts under `target/manual-walk/` (gitignored).

## Automated checks

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo fmt --check
RUSTUP_TOOLCHAIN=1.95.0 cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTUP_TOOLCHAIN=1.95.0 cargo test -p modplayer-core search         # SearchSession retry/in-flight (R1–R10, S1)
RUSTUP_TOOLCHAIN=1.95.0 cargo test -p modplayer-core backoff sync   # shared backoff_delay; sync unchanged (R5, R10)
RUSTUP_TOOLCHAIN=1.95.0 cargo test -p modplayer-ui search_layout    # layout props P1–P4, truncate_query
RUSTUP_TOOLCHAIN=1.95.0 cargo test -p modplayer-ui --test search_view --test accessibility --test fluent_keys --test section_memory --test design_token_literals --test high_contrast
RUSTUP_TOOLCHAIN=1.95.0 cargo test -p modplayer-audio-source-connect force_rate_limited   # D1
RUSTUP_TOOLCHAIN=1.95.0 cargo test --workspace
RUSTUP_TOOLCHAIN=1.95.0 cargo deny check
```

Expected: all green; `grep -n GROUP_VISIBLE_ROWS crates/` → no match; `grep -n search-placeholder crates locales` → no match.

## Manual scenarios (executed by the implementing agent — constitution Governance › Manual Scenario Sign-Off)

Launch: `RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer &`. Locate the window via Quartz (`owner modplayer`, name `ModPlayer`), resize it to the minimum 960 × 640, drive with `CGEventPost`, capture with `screencapture -x -o -l <id>`. Record pass/deviation + evidence path on the scenario task in `tasks.md`.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | One scroll area at 960 × 640 (SC-001, US1-AS1/AS5) | `Cmd+2` (Search) → type a broad query ("love") → wait for results → scroll-wheel down continuously over the results | Playlists header reached; no inner scrollbars on any group; field row stays put; groups stacked in one column. |
| M2 | Pinned header (SC-002, US1-AS2) | Same query; scroll to the middle of Tracks, then to Albums' boundary | "TRACKS · 20" pinned at the top while Tracks rows scroll; Albums header pushes it out; never two headers overlapping. |
| M3 | Header counts + Show more (SC-006, US1-AS3/AS4) | Click "Show more Tracks" at the end of Tracks | Header count goes 20 → 40; Show more sits after row 40; scroll position not reset; count line updates to new total. |
| M4 | New query resets scroll (RL11) | Scroll to the bottom → edit the query | Results area returns to top; skeleton rows under headers without counts. |
| M5 | Field feedback (US2-AS1–AS4) | Empty field → screenshot; type "abba" → screenshot within the first frames; wait → screenshot | Empty: no "Search" label above, hint "Tracks, albums, artists, playlists", no ×. Typing: spinner + × at trailing edge. Settled: spinner gone, "N results" line. VoiceOver (optional spot check): field reads "Search the catalog". |
| M6 | Clear via keyboard (US2-AS2) | With text in field: `Tab` → `Space` | Field empty, results idle, focus back in the field (typing continues into it). |
| M7 | Stale → retry → recovered (US3-AS1/AS2, SC-004) | Relaunch with `MODPLAYER_CATALOG_FORCE_429=paged-once`; search "love"; click "Show more Tracks"; wait ~15 s | Immediately: 20 Tracks rows stay, strip "⚠ Showing earlier results — search is busy, refreshing shortly" on raised surface; no toast. After backoff: strip gone, 40 rows, Show more available again. |
| M8 | Rate-limited, nothing stale (edge case) | Relaunch with `MODPLAYER_CATALOG_FORCE_429=1`; search "love" | Strip "⚠ Search is busy — retrying shortly"; no groups, no spinner, no count. Edit the query → strip disappears, spinner shows (retry cancelled). |
| M9 | Empty state (US3-AS3, SC-005) | Search a nonsense string > 60 chars (e.g. 80 × "q") | Message quotes the first 60 chars + "…", wraps within the body measure; "Clear search" button below; one click empties and focuses the field. |
| M10 | Offline (FR-013) | Disconnect network (Wi-Fi off) with a query typed | Only the existing offline message; no spinner, no count, no strip. Reconnect → query resolves normally. |
| M11 | High contrast (017) | Settings → Appearance → High contrast; repeat M7 first half | Strip and headers legible; colours from roles only. |

**Walk notes (T043, 2026-09-30)**: results and evidence are on T043 in [tasks.md](./tasks.md); deviations in [research.md](./research.md) R14. Live, only Tracks is shown and Show more exhausts at 20, so M1's Playlists header, M2's push-out and M3/M7's 40 rows are covered by the headless tests instead. Synthetic wheel events don't reach the app on the walk host: scroll by dragging the results scrollbar (hover the right edge first). M10 needs the host offline and was not run.

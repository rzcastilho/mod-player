# Quickstart: Library Browsing and Detail View (025)

Validation guide. Contract IDs refer to [contracts/](./contracts/); types to [data-model.md](./data-model.md).

## Prerequisites

- macOS host (manual-run platform), repo root = this worktree.
- Toolchain: `export RUSTUP_TOOLCHAIN=1.95.0` (or `env -u RUSTUP_TOOLCHAIN`) — see constitution Governance recipe.
- Live signed-in account in Keychain for manual scenarios (the app's own `ModPlayer` / `session-credential`).

## Automated checks

```bash
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test -p modplayer-ui                       # all UI tests
rtk cargo test -p modplayer-ui --test library_view   # H1–H12, K1–K6
rtk cargo test -p modplayer-ui --test rows           # RM1–RM9
rtk cargo test -p modplayer-ui --test section_memory # S1–S6
rtk cargo test -p modplayer-ui --test fluent_keys    # en-US/pt-BR parity
rtk cargo test -p modplayer-ui --test accessibility --test high_contrast
rtk cargo test --workspace && cargo deny check
```

Expected: all green; no pre-existing test modified except where a contract intentionally changes an assertion (`detail-back` value, detail owner line now shown for owned playlists — `playlist_detail_shows_owner_label…` updated to `detail-owner`).

## Manual scenarios (executed by the implementing agent)

Launch: `cargo build -p modplayer && ./target/debug/modplayer &` (relaunch with env vars where noted). Locate window via Quartz `CGWindowListCopyWindowInfo` (owner `modplayer`), drive with `CGEventPost`, capture with `screencapture -x -o -l <id>`. Helper scripts in `target/manual-walk/`. Record pass/deviation + evidence path on the scenario task in `tasks.md`.

| # | Scenario | Steps | Expected |
|---|----------|-------|----------|
| M1 | Playlist header | Library → Playlists → open a followed (non-owned) playlist | Back top-left "‹ Library"; 128 px artwork; DISPLAY title; one facts line "by Owner · N tracks · X hr Y min" (runtime appears once tracks load); Primary Play; "…" (H1–H9) |
| M2 | Album + artist headers | Open a saved album, then a followed artist | Album facts "Artists · Year · N tracks · runtime"; Artist facts "Artist · 10 top tracks"; same structure/height (H1, H4) |
| M3 | Header Play | Click Play on M1's playlist | Queue replaced with playlist from track 1, playback starts — same as row "Play now" (H7) |
| M4 | Placeholder artwork + long title | Relaunch with `MODPLAYER_ARTWORK_FORCE_FAIL=1`; open an item with a long title at 960 × 640 | Initials placeholder at 128 px; title ellipsised; Play/"…"/Back all visible; header height unchanged (H1–H3) |
| M5 | Back restores position | Relaunch with `MODPLAYER_LIBRARY_FIXTURE=large`; Saved Albums tab, scroll ~60 %; note first visible row; open it; press Backspace; repeat with Alt+Left and with the Back button | Same tab, same first visible row each time (S1, S2, H10) |
| M6 | Narrow-width row actions | Resize to 960 × 640, open plugin dock overlay if any plugin docks; check every tab and a detail list | Every "…" inside its row, Quiet (no fill at rest), hover fill + focus ring appear; popup opens next to its row (RM1–RM4) |
| M7 | Keyboard-only actions | No mouse: Tab to a row, Shift+F10 (and Menu key if supported); ↓×6 wraps to first; End/Home; Escape; reopen, Enter on "Add to queue"; then Tab to header "…", Enter, Escape; Tab order in header: Back → Play → "…" | Menu nav as RM6; focus returns to row / header "…"; action queued; placeholder shows "Coming soon" (RM5–RM8, SC-005) |
| M8 | Skeletons + empty states | Relaunch with a fresh `MODPLAYER_CONFIG_DIR=$(mktemp -d)` after sign-in to observe first sync; capture Saved Albums while loading and after; open a detail on a slow list; visit empty tabs on a sparse account | Skeleton rows 72 px on wide tabs, 56 on track tabs, no jump on load; header skeleton same height with working Back; empty states unchanged with one action (K1–K6) |

Record in `research.md` if the Menu/ContextMenu key is unavailable in egui 0.36 (R7) and M7 relies on Shift+F10 only.

## Manual scenario sign-off (T034, 2026-09-30)

Run by the implementing agent against the debug build, the live Keychain credential and the maintainer's live Premium account (source link `Active`), macOS Quartz recipe. Evidence is in `target/manual-walk/` (gitignored). Screenshots are 2× retina. The window was 960 × 640 content (the app's minimum width) for M4–M8.

| # | Result | Evidence | Notes |
|---|--------|----------|-------|
| M1 | **Pass** | `m1d-header.png`, `m1e-header-loaded.png` | Followed playlist "Progressive Metal" (owner spotify). The header shows "‹ Library" top-left, a 128 pt artwork square, the DISPLAY title, Primary "Play" and "…". Facts read "by spotify · 150 tracks" while tracks load, then "by spotify · 150 tracks · 13 hr 18 min". |
| M2 | **Deviation: not reachable live (any account)** | `m8h-albums-empty.png`, `m8i-artists-empty.png`, `m2d-crop.png` | Album and artist details open only from the Saved Albums and Followed Artists tabs (`app.rs` `LibraryOutcome::OpenAlbum/OpenArtist`). Those sets are fetched from `/collection/v2/paging`, which answers HTTP 400 to every request (004 research V2; `catalog/collection.rs`), so they always report `Unsupported` and stay empty. Re-checked 2026-09-30 after the maintainer saved 1 album and followed 2 artists: after relaunch and a ~70 s sync both tabs still show 0. The large fixture has no albums or artists, and search rows open no detail. The album and artist headers are covered by automated H1/H4 in `tests/library_view.rs` only. This is a pre-existing catalog limitation, outside 025 scope. |
| M3 | **Pass** | `m3b-nowplaying.png`, `m7k-queue.png` | Header "Play" replaced the queue with the playlist from track 1 ("Heaven on High", queue positions 1–150 in playlist order) and started playback. The log shows `Loading <Heaven on High>`. |
| M4 | **Pass (partial)** | `m4a.png`, `m4d-detail-960.png` | `MODPLAYER_ARTWORK_FORCE_FAIL=1`: the header shows a 128 pt initials placeholder ("DC"). Back, Play and "…" are visible and the header height matches the artwork case. No title in the account ellipsises at 960, and the window refuses to go narrower. Truncation is covered by automated H3. |
| M5 | **Pass (Playlists tab)** | `m5c-before-crop.png`, `m5d-open-*.png`, `m5e-after-*-crop.png` | `MODPLAYER_LIBRARY_FIXTURE=large`. The fixture has no saved albums, so the test used the Playlists tab (1,000 rows), scrolled to about 60 % with "Fixture Playlist 600" as the first visible row. Opening that row and returning with Backspace, Alt+Left and the Back button each restored the same tab, the same first visible row and the selection. |
| M6 | **Pass** | `m6a-playlists-960.png`, `m6b-hover-crop.png`, `m6c-popup-crop.png`, `m6d-playlists-back.png` | At 960 wide, every "…" sits inside its row on the Saved Tracks and Playlists tabs and in the detail list. It is Quiet at rest and gains a fill on hover. The popup opens directly under its own row. No plugin docks an overlay. |
| M7 | **Pass** | `m7tabs-grid.png`, `m7menu-grid.png`, `m7b-menu-open.png`, `m7i-placeholder.png`, `m7m-queue-end2.png`, `m7header-tabs.png`, `m7hdr-grid.png` | Keyboard only. Tab reaches the row (focus ring). Shift+F10 opens the menu with "Play now" focused. ↓×6 wraps back to "Play now"; End goes to "Pin for offline" and Home to "Play now". Escape closes the menu and returns focus to the row. Enter on "Add to queue" appended the row's 250 tracks (queue 150 → 400), and focus returned to the row. "Add to playlist" showed the "Coming soon" toast. In the header, Tab order is Back → Play → "…" → first track row; Enter on "…" opens the same 6 items and Escape returns focus to "…". There is no Menu key in egui 0.36 (see research R7), so Shift+F10 is the only key path. |
| M8 | **Pass (partial)** | `m1a.png`, `m8b-tracks-loaded.png`, `m1d-header.png`, `m4d-detail-960.png`, `m8h-albums-empty.png`, `m8i-artists-empty.png` | First sync after sign-in showed shaped skeleton rows on Saved Tracks: a square plus two bars at a 60 pt pitch, the same as loaded rows, so nothing jumps. Detail-list skeletons also match the loaded row pitch. Empty Saved Albums and Followed Artists each show their copy plus one "Search" action. Not reproducible live: (a) wide-tab skeletons, because the library now loads from the on-disk cache and a fresh `MODPLAYER_CONFIG_DIR` restarts onboarding, which would require a new OAuth sign-in; (b) the header skeleton, which only appears when the ref is not yet indexed, and section memory is not persisted, so a detail can't be reopened before sync. Both are covered by automated K1–K6. |

Observations outside 025 scope (the code is unchanged from `main`; `rows.rs` only parameterised the artwork size):

- In wide rows the text column is top-aligned while the artwork and "…" are vertically centred.
- The initials-placeholder fill is barely distinguishable from the background in the dark theme.
- A double-click on a row's title text selects the label text instead of opening the row. Double-clicking elsewhere in the row opens it.
- A wide-row action deferred while its track list is fetching only resumes while the Library view is visible.

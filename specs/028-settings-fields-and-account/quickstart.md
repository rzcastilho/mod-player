# Quickstart & Validation: Settings Fields, Placeholders, and Account Summary

**Feature**: 028-settings-fields-and-account

Runnable validation guide. Rules referenced: [contracts/settings-fields.md](./contracts/settings-fields.md) (F#), [contracts/account-and-signout.md](./contracts/account-and-signout.md) (A#/S#), [contracts/fluent-strings.md](./contracts/fluent-strings.md). Models: [data-model.md](./data-model.md).

## Prerequisites

- macOS host (manual-run platform), repo worktree on `feature/028-settings-fields-and-account`.
- Toolchain: `export RUSTUP_TOOLCHAIN=1.95.0` (shell may override `rust-toolchain.toml`).
- Signed-in Premium account in the OS Keychain (maintainer's live account) for manual scenarios.

## Automated gates (all must pass)

```bash
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test --workspace
rtk cargo test --doc -p modplayer-ui -p modplayer-core
rtk cargo deny check
```

Focused runs while iterating:

```bash
rtk cargo test -p modplayer-ui --test settings_fields            # F1–F15, F27, F28
rtk cargo test -p modplayer-ui --test settings_search_highlight  # F16–F20
rtk cargo test -p modplayer-ui --test settings_category_row      # F22–F24 (+ 020 contracts)
rtk cargo test -p modplayer-ui --test settings_account           # A1–A8, S2–S6
rtk cargo test -p modplayer-ui --test fluent_keys                # F26, pt-BR parity
rtk cargo test -p modplayer-ui --test design_token_literals      # F29
rtk cargo test -p modplayer-ui --test high_contrast              # F18 in HC
rtk cargo test -p modplayer-core settings_registry               # F25
```

Expected: 0 failures; `cargo deny` reports no new crate (chrono already in the lock via `oauth2`).

## Manual scenarios (executed by the implementing agent — Constitution › Manual Scenario Sign-Off)

Launch: `cargo build -p modplayer && ./target/debug/modplayer` (background); for M2 reset checks with a fresh file use `MODPLAYER_CONFIG_DIR=$(mktemp -d)` only if a signed-in session is not needed. Drive with Python Quartz `CGEventPost`, capture with `screencapture -x -o -l <windowid>` (helper scripts in `target/manual-walk/`). Resize the window to its 960×640 minimum for M1, M4, M5, M6.

| # | Scenario | Expected |
|---|---|---|
| M1 | Settings › Audio at 960×640 | Two cards "OUTPUT" and "LEVEL PROTECTION"; help lines indented and lighter; ceiling shows "-1.0 dBFS" inside slider value; cap "50%"; captions "-6.0 to -0.1 dBFS" and "0 to 100%". (US1, SC-001) |
| M2 | Drag limiter ceiling to -3.0; then Reset | Reset appears only on ceiling row after the change; after Reset value "-1.0 dBFS", Reset gone, focus ring on slider; `settings.toml` changed only in the ceiling key. Repeat for cap with safe-volume switch off. (US2, SC-002) |
| M3 | Playback: set nudge step 250 → Reset; set device name "Test Rig" → Reset | Units "ms" inside drag value, caption "1 to 1000 ms"; resets restore 10 ms and default device name. |
| M4 | Search "nudge", press Enter on "Playback › Marker nudge step" | Playback opens, field scrolled into view, focused, outlined; outline disappears after ~3 s; repeat and press any key → clears immediately. (US3, SC-005) |
| M5 | Open Offline, then Privacy & diagnostics; open "More" menu if collapsed | Header + "…aren't available yet…" sentence; row/menu entries show "Coming soon"; VoiceOver (Cmd+F5) reads "Offline, coming soon". (US4, SC-003) |
| M6 | Account signed in at 960×640 | Summary card first: name (strong), "Subscription tier Premium", "Last verified 30 Sep 2026, HH:MM" in local time; Re-check then Sign out below; card visible without scrolling. (US5, SC-006) |
| M7 | Click Sign out at 960×640 | Dialog 560 px wide, all consequence bullets fully visible and wrapped; focus on Cancel; Enter closes (still signed in); reopen, Escape closes; reopen, click backdrop closes. Do **not** confirm sign-out on the live account unless re-sign-in is planned. (US6, SC-004) |
| M8 | Appearance: toggle High contrast on; repeat M1 and M4 | Cards, captions, badges, highlight outline all distinguishable; highlight outline thicker (≥ 3 px HC ring role). |
| M9 | Keyboard-only walk of Audio (Tab from category row) | Focus order: each control then its Reset (if present), top to bottom; Reset activates with Space/Enter. |

Record each result (pass / deviation + evidence path) on the manual-scenario task in `tasks.md`; behavioural deviations also go into this file and `research.md`.

### Deviations recorded (T048 walk, 2026-09-30; details in research.md R13)

- **M1**: slider rails were invisible on the card fill. Fixed: the field control sinks the rail to `surface_base`.
- **M4**: Enter typed in the search box does not activate a result. Press Tab to reach the result, then Enter. This is existing search behaviour, not new in this feature.
- **M5**: the "Coming soon" badge was unreadable on the selected (accent) item. Fixed: the badge inherits the on-accent text colour. VoiceOver was not run. The a11y name "Offline, coming soon" is verified by `settings_category_row` tests.
- **M6**: the live tier/profile check failed (account crate, out of scope), so the card showed its fallbacks. Layout, order and full visibility at 960×640 passed. Populated values were not observable live.
- **M7**: the dialog opened with nothing focused. Fixed: the Cancel focus request is retried until it lands.
- **Harness note**: typing right after a click into a text field can be dropped (Cmd+A is unreliable). Clear the field with Backspace/Delete before typing.

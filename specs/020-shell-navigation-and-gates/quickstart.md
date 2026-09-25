# Quickstart: Shell Navigation and Launch Gates — Validation Guide

**Feature**: `020-shell-navigation-and-gates` | **Plan**: [plan.md](./plan.md)

Proves the feature end to end. Contracts define *what* must hold
([shell-chrome](./contracts/shell-chrome.md), [settings-category-row](./contracts/settings-category-row.md),
[section-memory](./contracts/section-memory.md), [library-tab-counts](./contracts/library-tab-counts.md),
[fluent-strings](./contracts/fluent-strings.md)); this guide says how to run it.

## Prerequisites

- Worktree root: this repository; toolchain from `rust-toolchain.toml`. If the shell exports
  `RUSTUP_TOOLCHAIN`, prefix commands with `RUSTUP_TOOLCHAIN=1.95.0` (constitution, Manual
  Scenario Sign-Off).
- macOS host for manual scenarios; Python 3 with `pyobjc-framework-Quartz` for window
  location/driving; helper scripts under `target/manual-walk/` (gitignored).
- For M1–M3: a fresh config dir so the disclosure is unacknowledged:
  `MODPLAYER_CONFIG_DIR=$(mktemp -d)` — the scope guard forbids `/tmp` writes from the agent, so
  use `target/manual-walk/fresh-config-<n>` instead.

## Automated validation

```bash
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test -p modplayer-ui --test shell_navigation        # C1–C9, L4
rtk cargo test -p modplayer-ui --test settings_category_row   # R1–R11 (incl. proptest, 960 px, +40 %)
rtk cargo test -p modplayer-ui --test section_memory          # M1–M5
rtk cargo test -p modplayer-ui --test library_view            # L1–L3
rtk cargo test -p modplayer-ui --test fluent_keys --test accessibility --test design_token_contrast --test design_token_literals --test actions
rtk cargo test --workspace
rtk cargo deny check
```

Expected: all green; no new dependency in `Cargo.lock` (`git diff --stat Cargo.lock` empty).

## Manual scenarios (executed by the implementing agent — constitution § Manual Scenario Sign-Off)

> **2026-09-25 note (US1 implementation session)**: M1/M2/M3/M5 could not be
> driven — the host session's screen was locked
> (`Quartz.CGSessionCopyCurrentDictionary()` → `CGSSessionScreenIsLocked = 1`)
> with no interactive user to unlock it, so the launched app's window never
> appeared in `CGWindowListCopyWindowInfo` for `CGEventPost`/`screencapture`
> to drive, and the Keychain-gated live-token check hung indefinitely for
> the same reason. Not a behavioural deviation — see T013 in tasks.md for
> what automated coverage (`shell_navigation.rs`) verified over the same
> code path instead, and re-run these scenarios verbatim from an unlocked
> interactive session before this feature's next release milestone.

Launch: `cargo build -p modplayer && ./target/debug/modplayer` (background). Locate the window
with `Quartz.CGWindowListCopyWindowInfo` (owner `modplayer`, name `ModPlayer`), drive with
`CGEventPost`, capture with `screencapture -x -o -l <windowid> target/manual-walk/020-Mn.png`,
and judge from the image plus the app log. Record pass/deviation + evidence on the matching
task in `tasks.md`.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | First launch, Welcome (US1-AS1) | Launch with a fresh `MODPLAYER_CONFIG_DIR`. Screenshot. Press Tab repeatedly, screenshot the focus ring each time. Open the Privacy notice sub-view and screenshot. | No rail at left; gate content starts at the window's left margin; indicator "Step 1 of 3" with Welcome current. Tab never lands on a nav item. Privacy sub-view still "Step 1 of 3". |
| M2 | Sign-in step and retry (US1-AS2, AS5) | Acknowledge. Screenshot. Click Sign in, then Cancel in the waiting state; screenshot. Press `Cmd+1`…`Cmd+5`. | "Step 2 of 3: Sign in", Welcome complete; after cancel still step 2; shortcuts do nothing; no rail. |
| M3 | Device check → Main (US1-AS3, AS4) | Complete sign-in with the maintainer's Premium account (real browser, real Keychain). At device check, choose "No, try another", then confirm. | "Step 3 of 3" with 1–2 complete; retry keeps step 3; on confirm the rail appears with five items and the indicator is gone. |
| M4 | Sign-out returns to gate (US1-AS6, US3-AS4) | In Main: scroll Library, pick a Settings category, then Settings › Account › Sign out. Sign back in. | After sign-out: no rail, "Step 2 of 3". After sign-in: Library at top on Saved tracks, Settings on its first category. |
| M5 | Settings "Test output device" preview (FR-001a) | Settings › Audio › Test output device; screenshot; close it. | Rail hidden, no step indicator during preview; rail back after close. |
| M6 | Rail selected state (US3-AS1) | Click each rail item; screenshot in Light, Dark and High contrast (Settings › Appearance). | Selected item: accent bar on its left edge, primary label, no filled background; others secondary text only. |
| M7 | Scroll retention (US3-AS2, AS3) | Library › Saved tracks with a large library (`MODPLAYER_LIBRARY_FIXTURE=large`): scroll halfway; go Search, type a query, scroll results; go Settings › Controls, scroll; return to Library, Search, Settings in turn; repeat once. | Each section returns to the same sub-view and scroll position both times. |
| M8 | Settings row at minimum size (US2) | Resize the window to 960 × 640. Open Settings. Screenshot. Select an overflowed category via More (mouse), then via keyboard only (Tab to More, Enter, ArrowDown ×n, Enter; then Enter, Escape). Widen to full screen; screenshot. | One line of categories + "More" at 960 px; picked category appears just before More and is absent from the menu; Escape returns focus to More; when wide, no More and all 11 inline. |
| M9 | Library counts (US4) | With a library where Saved albums is empty, open Library without selecting the Saved albums tab. | `0` beside Saved albums; no counts while the first sync is loading. |

## Exit criteria

All automated commands green; M1–M9 recorded as pass (or deviation with evidence and, where
behaviour differs from the spec, a note appended here and in research.md).

# Quickstart: Notification Placement, Severity, and Message Quality

**Feature**: 019-notification-presentation | **Plan**: [plan.md](./plan.md)

Validation guide only. Behaviour definitions live in the contracts:
[notification-stack-ui.md](./contracts/notification-stack-ui.md),
[notification-center-api.md](./contracts/notification-center-api.md),
[settings-output-device-name.md](./contracts/settings-output-device-name.md),
[fluent-strings.md](./contracts/fluent-strings.md); entities in [data-model.md](./data-model.md).

## Prerequisites

- macOS host (manual-run platform), repo worktree at `019-notification-presentation`.
- Toolchain per constitution recipe: `RUSTUP_TOOLCHAIN=1.95.0` (or `env -u RUSTUP_TOOLCHAIN`).
- Helper scripts/screenshots under `target/manual-walk/019/` (gitignored; scope guard forbids `/tmp`).

## Automated gates

```bash
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test --workspace
rtk cargo deny check
```

Focused suites (expected: all pass):

| Command | Proves |
|---------|--------|
| `rtk cargo test -p modplayer-core --lib notifications` | C1–C3: `detail`, hold/release, unchanged 10 s ageing |
| `rtk cargo test -p modplayer-core --test notifications` | raise-site args/detail (C4), FR-017 severities unchanged |
| `rtk cargo test -p modplayer-core --test device_policy` | `display_name_for_saved` order; raw id never returned (proptest) |
| `rtk cargo test -p modplayer-core --test settings` | K1–K4 round-trip + pre-019 fixture; keybindings detail |
| `rtk cargo test -p modplayer-ui --test notification_stack` | SC-001 zero intersection at 960×640 and 1200×820; SC-002 cap/"1 more"/expand; S3 60 % cap |
| `rtk cargo test -p modplayer-ui --test notifications` | SC-003 role colours ×3 themes; SC-004 two lines + no id substring; S5/S6 toggles; SC-006 each action variant dispatches |
| `rtk cargo test -p modplayer-ui --test accessibility` | S8 names; full-text accessible name |
| `rtk cargo test -p modplayer-ui --test fluent_keys` | new/rewritten keys resolve; SC-005 |
| `rtk cargo test -p modplayer-ui --test design_token_literals` | no colour literals added |

## Launch

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer &
```

Samples are raised from **Settings › Developer** ("Critical" / "Warning" / "Info" buttons). Window
located and driven via Python Quartz (`CGWindowListCopyWindowInfo`, `CGEventPost`); evidence via
`screencapture -x -o -l <windowid> target/manual-walk/019/<name>.png`.

## Manual scenarios (executed by the implementing agent; results recorded in tasks.md)

**M1 — Placement, library (US1 AS1, SC-001).** Resize to 960×640. Library screen with tab strip
and first row visible. Raise Critical, Warning, Info. *Expect*: stack 8 px from bottom-right
corner; tab strip and first row fully visible and clickable (click a tab while the stack shows).
Repeat at 1200×820.

**M2 — Placement, settings & plugins (US1 AS2–4).** Same three notifications over Settings
(category list) and the Plugins table (column headers). *Expect*: no overlap; nav rail clear.

**M3 — Cap and overflow (US2).** Raise 4 Warnings. *Expect*: 3 cards newest-on-top + "1 more"
below the oldest. Tab to it, press Enter → 4 cards, button reads "Show fewer". Space → collapses.
Expand again with 8 Warnings → stack scrolls within ~60 % of the window height. Dismiss down to
3 → button disappears, collapsed.

**M4 — Promotion (US2 AS5).** 4 Warnings collapsed; dismiss the top card. *Expect*: previously
hidden card appears; no button.

**M5 — Severity legibility (US3).** Raise Critical + Info. *Expect*: red (danger) vs green
(positive) left bars and icons; different glyphs; severity words visible. Toggle Settings ›
Appearance high contrast → bars use HC colours. Info vanishes ~10 s after raise; Warning/Critical
persist.

**M6 — Device missing at launch, named (US4 AS1, AS4).** Quit. In the config dir's
`settings.toml` set `[audio] output_device = "coreaudio:AppleGFXHDAEngineOutputDP:10001:0:{6D1E-7715-00097FED}"`,
`output_device_name = "Studio Monitors"`, `device_confirmed = true`. Launch. *Expect*: Warning
"Studio Monitors isn't connected. Playing through <default device name> instead." ≤ 2 lines, no
part of the id; "Details" reveals the id in selectable monospace; again hides it.

**M7 — Device missing, legacy file (US4 AS2).** Remove `output_device_name`, relaunch. *Expect*:
"Your saved output device isn't connected. …". File still loads with no settings warning.

**M8 — Truncation (US4 AS5–7).** Set `output_device_name` to a ≥ 80-character name, relaunch.
*Expect*: two lines + "…" + "Show more" → full text, "Show less". Hold check (AS7) runs in M9:
expand the device-available-again Info's Details for > 10 s → it stays; collapse → gone ~10 s later.

**M9 — Device lost / available again (US4 AS3).** With a USB/Bluetooth output confirmed in Device
Check (writes `output_device_name` — verify in `settings.toml`), unplug it while playing.
*Expect*: Critical "<device> disconnected. Now playing through <fallback>." Replug → Info
"<device> is available again. You can switch back in Settings › Audio." with Details.

**M10 — Actions preserved (US4 AS8, SC-006).** Trigger `plugin-suspended` (a test plugin that
errors, per 009 quickstart) → Restart and Disable present and work; sign-out/expired session →
"Sign in" works. *Expect*: identical behaviour to pre-019.

**M11 — Keybindings detail.** Add `[keybindings] "no.such.action" = ["Cmd+K"]` to `settings.toml`,
relaunch. *Expect*: Warning without ids in the sentence; Details shows `no.such.action`.

## M1–M11 sign-off (2026-09-25, tasks.md T040)

**NOT EXECUTED.** `modplayer` was built fresh (`RUSTUP_TOOLCHAIN=1.95.0
cargo build -p modplayer`) and launched in the background against the
real config dir (no `MODPLAYER_CONFIG_DIR` override, per the recipe's
"real signed-in account" requirement). `ps` showed the process alive and
sleeping (`SN`) for the full 29 s attempt window — it never crashed or
exited. `Quartz.CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly,
kCGNullWindowID)` was polled at ~3 s and again at ~23 s after launch and
never listed a `modplayer`-owned window among the 25 (then 14) on-screen
windows; the only owners present were `SecurityAgent` (the lock prompt),
`loginwindow`, `Dock`, `Window Server`, `Control Center`,
`SystemUIServer`, `Spotlight` and other non-GUI-session/background
agents. `python3 -c "Quartz.CGSessionCopyCurrentDictionary()"` confirmed
`CGSSessionScreenIsLocked = 1`. This is the identical macOS-host blocker
017 hit three times (R18, R19, R21): a locked GUI session never forms a
window for a newly launched app, so `screencapture -x -o -l <windowid>`
and `CGEventPost` have nothing to target — there is no windowid to
capture. The launched process was killed (not routed around the lock),
per Governance › Manual Scenario Sign-Off.

None of M1–M11 were reachable — this feature has no scenario that sits
outside the window-forms gate (unlike 017, where several scenarios needed
only chrome with no sign-in). The mechanisms M1–M11 would visually
confirm are all machine-checked this session (`tasks.md` T039, full
workspace green — 1923 passed, 0 failed):

| Scenario(s) | Mechanism | Automated stand-in |
|---|---|---|
| M1, M2 | Bottom-right anchor, zero overlap at 960×640/1200×820 | `notification_stack.rs` SC-001 |
| M3, M4 | Cap at 3, "{N} more"/"Show fewer", 60% scroll cap, promotion on dismiss | `notification_stack.rs` SC-002, S3 |
| M5 | Severity accent bar/icon colour ×3 themes, Info 10 s auto-dismiss | `notifications.rs` SC-003; core `notifications.rs` H1/H4 |
| M6, M7 | Device-missing wording (named/legacy), no raw id in the visible text, Details reveals it | `notifications.rs` SC-004, S6; `device_policy.rs` `display_name_for_saved` |
| M8 | Two-line truncation, "Show more"/"Show less", hold-while-expanded | `notifications.rs` S5; core `notifications.rs` H2/H3 |
| M9 | device-lost/device-available-again wording, Details | `notifications.rs` SC-005; core `notifications.rs` integration tests (C4) |
| M10 | Every action variant still dispatches from the new layout | `notifications.rs` SC-006 |
| M11 | Keybindings warning drops ids from the sentence, Details shows them | `fluent_keys.rs`, core `settings.rs` (K4) |

Re-attempt M1–M11 once the GUI session is unlocked.

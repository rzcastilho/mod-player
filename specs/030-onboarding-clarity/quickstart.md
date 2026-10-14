# Quickstart: Onboarding and Sign-In Clarity

**Feature**: 030-onboarding-clarity. This guide shows how to check that the feature works from start to finish. The rules it checks are in [contracts/onboarding-screens.md](./contracts/onboarding-screens.md) and [contracts/fluent-strings.md](./contracts/fluent-strings.md). The state it relies on is described in [data-model.md](./data-model.md).

## Prerequisites

- The repository root of this worktree, on the `030-onboarding-clarity` branch.
- Rust 1.95.0. The shell may set `RUSTUP_TOOLCHAIN`, which overrides `rust-toolchain.toml`. Run every command with `env -u RUSTUP_TOOLCHAIN`, or with `RUSTUP_TOOLCHAIN=1.95.0`. This follows the constitution's Manual Scenario Sign-Off recipe.
- For the manual scenarios you need a macOS host, Python with `pyobjc-framework-Quartz`, the maintainer's live Spotify Premium account, and the real Keychain. Put helper scripts in `target/manual-walk/`.

## 1. Quality gates (Constitution VII)

```bash
env -u RUSTUP_TOOLCHAIN cargo fmt --all --check
env -u RUSTUP_TOOLCHAIN cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u RUSTUP_TOOLCHAIN cargo test --workspace
env -u RUSTUP_TOOLCHAIN cargo test --workspace --doc
env -u RUSTUP_TOOLCHAIN cargo deny check
```

All five must pass. `launch_flow.rs` and `disclosure.ftl` must show no diff:

```bash
git diff --exit-code main -- crates/modplayer-account/src/launch_flow.rs locales/en-US/disclosure.ftl
```

## 2. Targeted automated scenarios

| # | Command | Proves |
|---|---|---|
| A1 | `cargo test -p modplayer-ui --test onboarding_clarity welcome_` | W1–W5 and M1 for Welcome at 960×640 and 1600×900: one Primary control, Tab order, the link roles in the compact block |
| A2 | `cargo test -p modplayer-ui --test onboarding_clarity privacy_` | N1–N4: "Back to welcome" / "Back to About", disclosure unchanged |
| A3 | `cargo test -p modplayer-ui --test onboarding_clarity pending_` | P1–P6: spinner role and name, elapsed hint at 0/1/59/60/119/120 s through injected `RawInput::time`, repaint delay ≤ 1 s, reset on a new attempt |
| A4 | `cargo test -p modplayer-ui --test onboarding_clarity status_` | S1–S5: one `Role::Status` node per reason, positioned above the actions, zero nodes otherwise |
| A5 | `cargo test -p modplayer-ui --test onboarding_clarity device_check_` | D1–D5: Back from Settings and from the launch flow, the buffer field label and help text, unchanged answers |
| A6 | `cargo test -p modplayer-ui --test onboarding_clarity summary_` | V1–V5: identity line, Continue keeps the device-check selections, a confirmed sign-out leads to SignIn, Cancel keeps the summary, the Expired variant |
| A7 | `cargo test -p modplayer-ui --test onboarding_clarity getting_started_` | G1–G3: one line per plugin and no chord prose |
| A8 | `cargo test -p modplayer-ui --test onboarding_clarity about_restore_` and `cargo test -p modplayer-core settings_registry` | A1–A6: enabled/disabled state, persistence across a rebuilt controller, the search hit |
| A9 | `cargo test -p modplayer-ui --test fluent_keys` | F1–F4 |
| A10 | `cargo test -p modplayer-ui --test accessibility --test high_contrast --test design_token_literals --test control_variants` | K1–K2; status-message colours come from theme roles and stay legible in high contrast |
| A11 | `cargo test -p modplayer-account` | The disclosure hash pin and `next_step` tests are unchanged and pass |

Expected result: every test passes. `onboarding_clarity.rs` is the new integration test file, `crates/modplayer-ui/tests/onboarding_clarity.rs`. It uses `tests/common` (`active_account`) and the `FakeBackend` / `SyntheticHost` fixtures.

## 3. Manual scenarios (Governance › Manual Scenario Sign-Off)

The implementing agent runs these itself and records pass or deviation, with screenshot evidence, on the scenario task in `tasks.md`.

Build and launch:

```bash
env -u RUSTUP_TOOLCHAIN cargo build -p modplayer
MODPLAYER_CONFIG_DIR=$(mktemp -d) ./target/debug/modplayer &
```

A fresh config directory gives a true first launch. Find the window with `Quartz.CGWindowListCopyWindowInfo` (owner `modplayer`, name `ModPlayer`). Drive it with `CGEventPost`. Capture each step with `screencapture -x -o -l <id> target/manual-walk/030-Mx-*.png`.

- **M1 Welcome at minimum size (US1, SC-001, SC-006)**
  - At the default 960 × 640 size, capture the welcome screen.
  - Expected:
    - The heading comes first, then one lead paragraph, then a framed block of three smaller lines with "Terms" and "Privacy" as links.
    - "I understand, continue" is the only filled (accent) button, and "Decline" sits beside it as an outlined button.
    - No line runs wider than about 72 characters.
  - Show the screenshot to a reader for 10 seconds. They can say what the app does and which button continues. The agent records its own reading of the capture against that criterion.
- **M2 Wide window (SC-001)**: resize to 1600 px wide. The text column stays the same width, and the block does not stretch.
- **M3 Privacy back**
  - Click "Privacy". The back control reads "Back to welcome". Click it to return to Welcome, which is unchanged.
  - Later, from Settings › About › Privacy, the back control reads "Back to About".
- **M4 Pending sign-in (US2, SC-002)**
  - Continue, then click "Sign in". The real browser opens.
  - Do not finish signing in. Over about 70 s, capture at about 5 s and about 65 s.
  - Expected: a spinner, the browser hint, "Waiting for 5 seconds", then "Waiting for 1 minute". "Open the browser again" is the filled button and "Cancel" is outlined.
  - Click "Open the browser again". The browser tab opens again.
- **M5 Status messages (US2, SC-003)**
  - From M4, click Cancel. The screen shows "Sign-in was cancelled." in a framed row with a warning-coloured bar, above the instruction line and the Retry button.
  - Relaunch while an attempt is pending, then let it expire or cancel it, to see "Your previous sign-in didn't finish." in the same style.
  - With Keychain locked (`security lock-keychain`), relaunch with an existing session. The store-unreadable message is in the same style, above Retry.
  - Unlock, click Retry, and confirm sign-in resumes.
- **M6 Device check Back during launch (US3, C16)**
  - Complete sign-in on the Premium account. On the device check, choose a non-default device and the Safe preset.
  - Click Back. Expected: "Signed in", "Signed in as <name>", "Continue" (filled) and "Use a different account".
  - Click Continue. The device check shows again with the same device and Safe still selected.
  - Click Back, then "Use a different account", then Cancel. The summary is still shown.
  - Click "Use a different account", then "Sign out". The sign-in screen shows.
  - `security find-generic-password -s ModPlayer -a session-credential` reports not found.
- **M7 Device check Back from Settings (US3, C10)**
  - Sign in again and confirm a device.
  - Open Settings › Audio › "Test output device", then click Back. Settings › Audio shows again, and the confirmed device and preset are unchanged in the Settings fields.
  - The buffer field reads "Buffer preset" with its explanation underneath.
- **M8 Getting-started card (US4, SC-005)**
  - In Library, the card shows one line per bundled plugin, the "Keyboard shortcuts" link and the tutorial link, with no key combinations in the text. "Keyboard shortcuts" opens the sheet.
  - Click Dismiss and quit. Relaunch without `MODPLAYER_CONFIG_DIR` changed: the card is hidden.
  - Open Settings, select About, click "Show getting-started card" (2 interactions). Go to Library: the card is back. Back in About, the button is disabled.
  - Relaunch: the card is still visible.
- **M9 Keyboard-only pass (constitution X)**: repeat M1 → M3 and M6 using only Tab, Shift+Tab, Space and Enter. Every new control is reachable and activates, and the focus ring is visible on each one.

Record each M-scenario's result in `tasks.md`. Where behaviour differs from the spec, also record it here and in `research.md`.

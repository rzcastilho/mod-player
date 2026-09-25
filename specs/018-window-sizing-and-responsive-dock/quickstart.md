# Quickstart: Window Sizing and Responsive Plugin Dock

**Feature**: 018-window-sizing-and-responsive-dock | **Plan**: [plan.md](./plan.md)
Contracts: [window-settings.md](./contracts/window-settings.md), [ui-responsive-dock.md](./contracts/ui-responsive-dock.md) · Model: [data-model.md](./data-model.md)

## Prerequisites

- macOS host (manual-run platform per constitution "Manual Scenario Sign-Off").
- Toolchain: `RUSTUP_TOOLCHAIN=1.95.0` (or `env -u RUSTUP_TOOLCHAIN`).
- At least one plugin that contributes a docked panel is installed and enabled
  (bundled Section Loop and Key & Tempo plugins both contribute panels —
  012/013); dock two of them for the two-panel scenarios.
- Helper scripts for Quartz driving live in `target/manual-walk/` (gitignored).

## Automated validation

```bash
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo fmt --check
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-core --test settings --test settings_window_proptest --test controller_window
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --lib layout
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --test responsive_dock --test waveform --test plugin_panels --test fluent_keys --test now_playing
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test --workspace
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo deny check
```

Expected: all pass; `responsive_dock` covers D10.5–D10.8 (incl. the +40 %
pseudo-localization run, SC-007); `settings_window_proptest` covers W4.4/W4.5.

## Manual scenarios (executed by the implementing agent)

Launch: `RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer`
(background). Locate window via `Quartz.CGWindowListCopyWindowInfo` (owner
`modplayer`, name `ModPlayer`); evidence via `screencapture -x -o -l <id>`.
Quartz bounds are in points, matching logical points on any scale factor.

Launch notes from the 2026-09-25 walk (research R15):

- An agent shell may run in launchd's `Background` session
  (`launchctl managername`). A binary started directly from there runs its
  event loop but never gets a window. Launch through LaunchServices instead,
  via a minimal `.app` wrapper whose `Contents/MacOS/modplayer` symlinks to
  `target/debug/modplayer`: `open -n -g --env MODPLAYER_CONFIG_DIR=… --stdout
  … ModPlayerWalk.app`. `open` on the bare binary works too, but opens a
  Terminal.app window for every launch.
- Quartz bounds include the 28 pt title bar: 1200 × 820 inner reads as
  1200 × 848.
- Now Playing is gated behind a signed-in Spotify session. A copy of the real
  config dir brings the account state along; an expired session needs
  "Sign in" again. Each rebuilt binary has a new code hash, so macOS shows a
  Keychain access prompt that blocks `account.launch()` before the event loop
  starts, until a human clicks Allow. Plugins that time out during that block
  (`host_busy`) don't register their panels, so relaunch once it's allowed.
  Walk several scenarios per build to keep prompts rare.
- Only send synthetic keys (Cmd+Q in particular) after checking that
  modplayer is the frontmost app.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | First launch size (US1-1, SC-001) | Launch with `MODPLAYER_CONFIG_DIR=$(mktemp -d)` | Window bounds 1200 × 820 (±1, plus title bar height in Quartz bounds — compare content area); no clipped text in shell |
| M2 | Minimum size (US1-2) | Drag bottom-right corner far up-left | Stops at 960 × 640 inner |
| M3 | Overlay at min size (US1-3, SC-002a) | At 960 × 640 with 2 docked panels: dock hidden, "Panels" toggle visible; click it | Overlay at right edge with both panels; every Float/Close/Disable label fully readable; screenshot |
| M4 | Dock at 240 (US1-4, SC-002b) | Widen to ≥ 1100; drag splitter right until it stops | Dock stops at 240; all header buttons readable (wrapped rows allowed); title truncated with "…"; hover title → full-text tooltip |
| M5 | Drag + persist (US2-1/2/3, SC-003/SC-004) | Drag dock to ~360; quit (Cmd+Q); relaunch | Width tracks pointer during drag; relaunch restores ~360; `settings.toml` `[window] dock_width` ≈ 360 |
| M6 | Keyboard resize (US2-4) | Tab to splitter; press ← ×3, → ×1, Home, End | Width +48, −16, 240, effective max; value persists after relaunch |
| M7 | Auto-hide (US3-1/2/3, SC-005) | With ≥ 1 docked panel at ≥ 1024, narrow window < 1024; toggle on; focus a panel widget; Esc | Dock disappears, toggle appears same frame; overlay shows same panels in order; Esc closes overlay, focus ring on "Panels" |
| M8 | Widen back (US3-4) | Open overlay at < 1024, widen ≥ 1024 | Overlay gone, toggle gone, docked dock at remembered width |
| M9 | Outside click doesn't dismiss (FR-008) | Overlay open; click Play/Pause | Transport acts; overlay stays open |
| M10 | Last panel leaves (edge case) | Overlay open with 1 panel; click Float | Overlay closes, toggle disappears, floated window appears |
| M11 | Waveform scaling (US4, SC-006) | With a track loaded, maximize window then restore to 640 tall | Overview/detail grow to ≈ 8 %/22 % of content height when maximized; at 640 tall no smaller than 64/120 |
| M12 | Window size persistence (FR-003) | Resize to ~1400 × 900, wait 1 s, quit, relaunch; then maximize, quit, relaunch | First relaunch opens ~1400 × 900; after maximized quit, relaunch opens at last non-maximized size (~1400 × 900), not full screen |
| M13 | Corrupt values (FR-014) | Quit; edit `settings.toml` `[window] inner_width = "wide"`, `dock_width = 9999`; relaunch | Opens at default width 1200 with saved height; dock at 480 (or effective max); other settings (theme, volume) unchanged; no "settings unreadable" warning |

Record each result (pass / deviation + screenshot path) on the manual-scenario
task in `tasks.md`; behaviour deviations also go back into this file and
`research.md`.

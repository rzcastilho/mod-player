# Quickstart: Sticky Transport Bar and Panel Layout

**Feature**: 021-transport-bar-and-panel-layout | **Plan**: [plan.md](./plan.md)
Contracts: [ui-now-playing-layout.md](./contracts/ui-now-playing-layout.md), [queue-row.md](./contracts/queue-row.md), [settings-now-playing-panels.md](./contracts/settings-now-playing-panels.md) · Model: [data-model.md](./data-model.md)

## Prerequisites

- A macOS host. It is the manual-run platform under the constitution's "Manual
  Scenario Sign-Off".
- The 1.95.0 toolchain: set `RUSTUP_TOOLCHAIN=1.95.0` or run with
  `env -u RUSTUP_TOOLCHAIN`.
- A signed-in Spotify session (Premium), because Now Playing is gated. A queue
  of at least 6 tracks, where at least one has no artwork if possible.
- For M8, at least one docked plugin panel. The bundled Section Loop and
  Key & Tempo plugins both contribute one.
- Quartz helper scripts in `target/manual-walk/`, which is gitignored. The
  scope-guard hook blocks the scratchpad and `/tmp`.
- For launch notes (LaunchServices `.app` wrapper, Keychain prompt per rebuilt
  binary, frontmost check before synthetic keys), see
  [018 quickstart](../018-window-sizing-and-responsive-dock/quickstart.md#manual-scenarios-executed-by-the-implementing-agent).

## Automated validation

```bash
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo fmt --check
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-core --lib settings::model controller
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --lib layout
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --test now_playing --test queue_view --test responsive_dock --test waveform --test actions --test markers --test effects_view --test transport_view --test section_memory --test fluent_keys --test controls
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test --workspace
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo deny check
```

**Expected**: every command passes. The contract IDs each test run covers:

| Test run | Covers |
|---|---|
| `now_playing` | T-B1, T-B3, T-B4, T-B6, T-S1, T-S3, T-C2–T-C6, T-R1, T-R2, T-R4 |
| `queue_view` | T-Q1–T-Q12 |
| `responsive_dock` | T-B5, including +40 % pseudo-localisation at 960 × 640 and at dock 240 |
| `waveform` | T-S2 |
| `actions` | T-R5 |
| `layout` (lib) | the `reveal_align` and `identity_width` proptests |
| core `settings::model` and `controller` | T-N1–T-N5, T-QC |

The 016 tests whose rules this feature supersedes are updated as part of the
feature and don't simply fail:
- C10, collapsed card draws nothing → replaced by T-C2;
- C11, Markers has no toggle → Markers has a header disclosure but no bar
  toggle;
- C13, reserved height → deleted and replaced by T-S1/T-B1.

The 014 test for the `display`-role Now Playing title is updated to the bar
title (research R12).

## Manual scenarios (executed by the implementing agent)

**Launch**: run `RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer`, then launch
through the `.app` wrapper, in the background.

**Locate the window**: `Quartz.CGWindowListCopyWindowInfo`, with owner
`modplayer` and name `ModPlayer`.

**Capture evidence**: `screencapture -x -o -l <id> target/manual-walk/021-Mx.png`.

**Drive**: `CGEventPost` for clicks, wheel (`CGEventCreateScrollWheelEvent`)
and keys (Q, E, T with the current bindings; Tab, Space).

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | Transport never disappears (US1-1, SC-001) | At 1200 × 820, open Effect Chain. Add nodes until the chain is taller than the window. Wheel-scroll to the bottom. Click Pause, then Play, then drag the master volume | The bar stays at the top the whole time, showing artwork, title and artist, the four transport buttons, times, volume, meter and three toggles. Audio pauses and resumes. Volume changes. Repeat with the Markers, Transport and Queue cards each open alone (screenshot per panel) |
| M2 | Short window (US1-2, US3-2) | No panel open. Resize to 960 × 640 | Bar height is unchanged (compare screenshots). Overview and detail are at least 64 and at least 120 pt. The content below the bar scrolls |
| M3 | Waveform grows (US3-1, SC-003) | Note bar and detail heights at 820 tall. Maximise | Detail is taller, and the bar's height is identical to within 1 pt |
| M4 | Queue toggle reveals (US2-1, SC-002) | Close all panels. Scroll to the top. Click **Queue** in the bar | The Queue card opens and is fully visible below the bar with no further scrolling. The Queue toggle shows on. Focus ring (if any) stays on the toggle |
| M5 | Effects and Transport toggles reveal (US2-2, SC-002) | Same as M4 for **Effects** (with a 16-node chain) and **Transport** | Each opens and becomes visible. For the tall chain, the card header sits directly under the bar |
| M6 | Toggle collapses (US2-3, edge case "scrolled out") | With Queue open, scroll so the Queue card is off screen. Click **Queue** | Queue collapses. The toggle turns off. There is no jump except the clamp from the shorter content |
| M7 | Keyboard parity (US2-4, SC-005) | Focus outside a text field. Press the Queue, Effects and Transport shortcuts | Same open-and-reveal and collapse behaviour as M4–M6. Then switch to Library, press the Queue shortcut, and return to Now Playing: the flag is unchanged (the toggle actions are `Scope::NowPlaying`, existing behaviour preserved per spec) and no scroll jump happened |
| M8 | Narrow window and 40 % text (FR-017) | There's no runtime pseudo-localisation override (018's is the test-only `with_pseudo_expansion`; +40 % is covered by `responsive_dock`), so run with real strings. Resize to 960 × 640 with a docked plugin panel (dock auto-hides, so the Panels toggle is present) | Bar controls wrap as whole controls. No label is elided. The Panels toggle is the last control. Title and artist are truncated with "…", and hovering the title shows the full text |
| M9 | Header collapse (FR-004, FR-006) | Collapse Effect Chain from its card header chevron | The bar's Effects toggle turns off in the same visible frame. Tab to the chevron and press Space: it reopens. Collapse **Markers** from its header, quit and relaunch: Markers is collapsed. `settings.toml` shows `markers_open = false` |
| M10 | Queue at a glance (US4, SC-004) | Open Queue with 6 or more items | Each row shows artwork (or the initials placeholder), title above artist, and the position right-aligned. The playing row has ▶ and a leading accent bar, no "Now playing:" text and no accent fill. Move up, Move down, Play next and Remove are visible without hovering. Tab reaches each one, and Enter on Remove removes that row |
| M11 | Single-item queue (edge case) | Clear the queue to only the current track | The one row shows the current mark correctly |
| M12 | No track (edge case) | Fresh `MODPLAYER_CONFIG_DIR` session, signed in, nothing played | The bar renders with the placeholder identity. Transport buttons follow `transport_enabled()` (contract B7), so they're enabled when a device is active and disabled only when transport is unavailable. There is no Markers card. The Effect Chain, Transport and Queue cards and their toggles work |
| M13 | Scroll retention (S4) | Scroll Now Playing halfway, switch to Library, then back | Offset restored. After sign-out and sign-in, the offset is reset to the top |
| M14 | Shift+wheel pan (S5) | Hover the detail waveform and Shift+wheel | The detail view pans and the page doesn't scroll. A plain wheel over the waveform scrolls the page |

**Walk deviations (2026-09-28)**: see [research R15](./research.md#r15--manual-walk-findings-2026-09-28). The M7 and M8 rows above were corrected after the walk. For synthetic wheel events, set the `CGEvent` flags explicitly, because a Shift+wheel leaves Shift latched.

Record each result (pass or deviation, with the screenshot path) on the
manual-scenario task in `tasks.md`. If behaviour deviates, also record it in
this file and in `research.md`.

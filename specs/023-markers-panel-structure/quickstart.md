# Quickstart: Markers Panel Structure

**Feature**: 023-markers-panel-structure | **Plan**: [plan.md](./plan.md) | **Contract**: [ui-markers-panel.md](./contracts/ui-markers-panel.md) | **Types**: [data-model.md](./data-model.md)

This file is a validation guide only. Implementation belongs to `tasks.md`.

## Prerequisites

- **Host and toolchain**: a macOS host (the manual-run platform) with Rust `1.95.0` (`rust-toolchain.toml`). If the shell exports `RUSTUP_TOOLCHAIN`, prefix commands with `RUSTUP_TOOLCHAIN=1.95.0` (constitution § Manual Scenario Sign-Off).
- **Account**: a signed-in ModPlayer account with real Connect playback, for M1–M9.
- **Helpers**: Python 3 with `pyobjc-framework-Quartz` for window location, event injection and `screencapture`. Keep helper scripts under `target/manual-walk/`, which is gitignored; the scope guard denies the scratchpad and `/tmp`.

## 1. Automated gates

```bash
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test --workspace
rtk cargo deny check
```

Focused runs while iterating:

```bash
rtk cargo test -p modplayer-ui --lib markers                     # PanelModel unit + proptest invariants (data-model §1)
rtk cargo test -p modplayer-ui --test markers                    # contract §9 table (P1–P9)
rtk cargo test -p modplayer-ui --test fluent_keys                # P10 keys resolve; no unexercised playback.ftl key
rtk cargo test -p modplayer-ui --test design_token_literals      # FR-023: baseline unchanged
rtk cargo test -p modplayer-ui --test accessibility --test controls --test control_variants --test now_playing   # regressions
```

**Expected result**: all green. The `markers` suite covers every row of [contract §9](./contracts/ui-markers-panel.md#9-tests-pinning-this-contract).

## 2. Launch

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer &
```

Start a track from the Library, then open Now Playing. The Markers card is the first panel card (021).

## 3. Manual scenarios

The implementing agent executes these and records pass or deviation, with the screenshot path, in `tasks.md`.

| # | Steps | Expected |
|---|---|---|
| M1 | On a track with no markers, look at the Markers card. | Only "No markers — press I to set A" and the "New loop region" button are visible. There are no group headings, no cue rows and no "Clear all markers". (FR-020, SC-007) |
| M2 | Press `I`, then `O` a few seconds later. Press `M` twice at different positions and `Shift+3` once. | Three headings appear in order: LOOP REGION 1, POINTS 2, CUES 1. The loop block shows the A row, then the B row, then arm/repeat/crossfade. The Cues group shows 8 slots: slot 3 is populated, and the other slots are muted and read "Cue n — empty · Shift+n to set". (US1 AS1/AS2, US4 AS2/AS3) |
| M3 | Arm the loop with `L` and play through it. | The wraps indicator renders inside the Loop Region block. The armed state does not move the block or add any "current region" marker. (FR-003) |
| M4 | Click a point's "Add name" or name text, type "Chorus", then click elsewhere. Hover that point's glyph on the waveform lane. | The row shows "Chorus" and the lane tooltip or glyph name includes "Chorus". Repeat and press `Esc` instead: the name is unchanged. (FR-012, SC-004) |
| M5 | Click a row's swatch and pick the 6th colour. Open it again and press `Esc`. | The first pick recolours the swatch and both lane glyphs, and the current colour is marked with ✓. `Esc` changes nothing. (FR-013) |
| M6 | While paused, click a point's jump action; play, then click the cue's jump action. | The playhead moves to each marker. Paused stays paused and playing stays playing. (FR-009) |
| M7 | Click "nudge later" on the B row five times, then "nudge earlier" on a point near 0:00 repeatedly. | B moves by exactly 5 × the Settings › Playback nudge step. The point clamps at 0:00.000. (FR-010) |
| M8 | Using only the keyboard, Tab from "New loop region" through the first rows. Press Enter on "remove" for the A row. | Tab visits swatch → name → jump → nudge− → nudge+ → remove per row and skips empty cue rows. Remove deletes A only: the region becomes incomplete (the arm toggle is disabled with its reason) and focus moves to the B row's swatch. No rename field opens. (FR-011, FR-014, Clarifications 6/8) |
| M9 | Look at the card with markers present. Click "Clear all markers", then "No"; click it again, then "Yes". | "Clear all markers" is red (destructive) at the bottom right, below the Cues group, far from "New loop region". "No" removes nothing. "Yes" returns the panel to the M1 state. (FR-018/019, SC-005/006) |
| M10 | Set 64 markers (the limit), then press `M`. | The refusal line renders under the card header, above LOOP REGION, and the grouping is unaffected. (FR-021) |
| M11 | Switch to high contrast and dark mode (Settings › Appearance) and repeat M2 by eye. | Headings, muted empty cue rows and the quiet row actions are all legible. The swatch popover's ✓ is visible on every palette colour. (FR-023, NFR-6.4) |

**Evidence**: capture with `screencapture -x -o -l <windowid> target/manual-walk/023-Mn.png` and judge each scenario from the image and the app log.

## 4. Regression spot-checks

- Lane glyph drag, the `C` colour cycle, F2 rename from a glyph, and `Delete` from a glyph all behave exactly as in 006.
- Collapsing and expanding the Markers card persists across relaunch (021).
- The Effect Chain, Transport and Queue cards below Markers are unchanged.

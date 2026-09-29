# Quickstart: Waveform Legibility and Scrub Feedback

**Feature**: 022-waveform-legibility | **Plan**: [plan.md](./plan.md) | Contracts: [ui-waveform-legibility.md](./contracts/ui-waveform-legibility.md), [analysis-rms.md](./contracts/analysis-rms.md) | Types: [data-model.md](./data-model.md)

Validation guide only — implementation belongs to `tasks.md`.

## Prerequisites

- macOS host (manual-run platform), Rust `1.95.0` (`rust-toolchain.toml`). If the shell exports `RUSTUP_TOOLCHAIN`, prefix commands with `RUSTUP_TOOLCHAIN=1.95.0` (constitution § Manual Scenario Sign-Off).
- Signed-in ModPlayer account for M1–M7 (real Connect playback, real Keychain).
- Python 3 with `pyobjc-framework-Quartz` for window location / event injection; helper scripts under `target/manual-walk/` (gitignored — the scope guard denies scratchpad and `/tmp`).

## 1. Automated gates

```bash
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test --workspace
rtk cargo deny check
```

Focused runs while iterating:

```bash
rtk cargo test -p modplayer-audio-source --test decoded             # AR1 vectors
rtk cargo test -p modplayer-core analysis                           # AR2–AR5 (refold, version, RMS8, size)
rtk cargo test -p modplayer-ui --test design_token_contrast         # W1 pairwise-distinct fills, W2 playhead ≥3:1 × 4 appearances
rtk cargo test -p modplayer-ui --test waveform                      # WL1–WL3, WL5 (split, two-tone, hover, suppression, a11y unchanged)
rtk cargo test -p modplayer-ui --test markers                       # WL4 (loop_shade table, every region, armed-last)
rtk cargo test -p modplayer-ui --test design_token_literals         # no colour literal outside theme/**
rtk cargo test -p modplayer-ui --test accessibility --test high_contrast --test responsive_dock   # SC-007 regressions
```

Expected: all green. `design_token_contrast` prints no floor failures; the worst-case playhead ratio is ≥ 4.1:1 (data-model W2).

## 2. Launch

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer &
```

Locate window: `Quartz.CGWindowListCopyWindowInfo`, owner `modplayer`, name `ModPlayer`. Capture: `screencapture -x -o -l <windowid> target/manual-walk/<scenario>.png`, then read the PNG back.

A first play after upgrading re-analyses every previously cached track once (analyzer v2) — expected, not a defect.

## 3. Manual scenarios (executed by the implementing agent; record pass/deviation + evidence in tasks.md)

| ID | Setup | Action | Expected | Covers |
|---|---|---|---|---|
| **M1** | Dark theme, normal contrast; play a track ≥ 2 min | Capture overview + detail mid-track | Playhead is a clearly visible light core with dark casing over both dense peaks and background, on both views | US1 AS1/AS3, SC-001 |
| **M2** | Light theme, then Settings → Appearance high contrast on (light, then dark) | Capture each | Playhead clearly visible in all three; two-tone fill visible | US1 AS2/AS4, FR-001 |
| **M3** | Same track, ~40 % played | Capture overview | Left of playhead: accent-hued fill; right: neutral grey fill; within each side an outer lighter peak and an inner stronger RMS band | US4 AS1/AS2, SC-006 |
| **M4** | Define two complete loop regions (A1/B1, A2/B2); arm region 1 while outside it, then seek inside it | Capture both views before and after the seek | Region 1: hatch (armed-inactive) → translucent fill (armed-active); region 2: light fill + 1 px outline throughout; both on overview and detail | US2 AS1–AS3, SC-002 |
| **M5** | From M4, arm region 2 from its marker-list row (region 1 remains current) | Capture | Region 2 now armed treatment, region 1 idle treatment — shading follows `armed`, not current | US2 AS4/AS5 |
| **M6** | Track loaded, paused | Move pointer across the detail view (CGEventPost mouse-moved), then across overview, then off the widget; then near the right edge | Thin line + `m:ss.mmm` label follows pointer on the hovered view only; disappears off-widget; label flips to the left of the line at the right edge and stays inside; audible position/transport/playhead unchanged (app log shows no seek) | US3 AS1–AS4/AS6, SC-003/SC-004 |
| **M7** | Track playing | Press on detail and drag (seek preview), then drag a marker in the lane; press `Esc` during a seek drag | No hover line/label during either drag; played/unplayed boundary follows the seek preview and reverts on `Esc`; hover returns on the next pointer move | US3 AS5, FR-009, FR-002 |
| **M8** | Shrink window to its minimum size | Capture | Overview (≥ 64 px) and detail (≥ 120 px) still show legible playhead, shading, two-tone fill; hover label fits inside | Edge case (min height), FR-010 |
| **M9** | Start a freshly uncached track while streaming | Capture during decode; hover over the undecoded placeholder | Decoded part two-tone + played/unplayed; placeholder band unchanged; hover line/timestamp still appear over the placeholder | US4 AS3, Edge cases |

## 4. Regression spot-checks (SC-007)

- Click on overview/detail seeks; drag-preview + release commits; `Esc` cancels.
- Elapsed/remaining labels still monospace and non-jittering.
- Marker lane glyphs, drag, and marker-list `m:ss.mmm` times identical to before.
- VoiceOver on the overview reads `transport-seek` with value `m:ss / m:ss` — unchanged while hovering.

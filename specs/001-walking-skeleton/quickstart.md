# Quickstart: Validating the Walking Skeleton

**Feature**: 001-walking-skeleton | Contracts: [`contracts/`](contracts/) | Data model: [`data-model.md`](data-model.md)

## Prerequisites

- Rust toolchain from `rust-toolchain.toml` (1.93.1): `rustup show` installs it on first use.
- `cargo install cargo-deny` (CI uses the action; local runs need the binary).
- Linux only: `sudo apt install libasound2-dev libxkbcommon-dev libwayland-dev pkg-config`.
- At least one audio output device for the manual scenarios; none for the automated suite.

## Automated gates (what CI runs on macOS, Windows, Linux — FR-024)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

Expected: all five exit 0. `cargo test --workspace` runs every test except those marked `#[ignore = "manual: …"]`.

Targeted runs:

| What | Command | Proves |
|---|---|---|
| Real-time safety | `cargo test -p modplayer-engine --test realtime` | FR-025 (a) no allocation, (b) buffer-boundary application, (c) monotonic clock across simulated fallback + rate change |
| Limiter | `cargo test -p modplayer-engine --test limiter` | FR-025 (d) every ceiling −6.0..−0.1; (e) clamping |
| Synthetic source | `cargo test -p modplayer-audio-source-synthetic` | segment levels, determinism, 60 s dropout check (US2-5) |
| Device policy | `cargo test -p modplayer-core --test device_policy` | US3 scenarios 1–5 and the zero-device edge case via `FakeBackend` |
| Settings | `cargo test -p modplayer-core --test settings` | FR-020 round-trip, clamping, garbage file, crash mid-write |
| Safe volume | `cargo test -p modplayer-core --test safe_volume` | US2-2, US2-3, SC-009 |
| UI keys & search | `cargo test -p modplayer-ui` | every Fluent key resolves; "ceiling" search (SC-008) |

## Run the app

```bash
cargo run -p modplayer
# fresh-install simulation on any platform:
MODPLAYER_CONFIG_DIR=$(mktemp -d) cargo run -p modplayer
```

## Manual scenarios (need hardware; each maps to a spec acceptance scenario)

### M1 — First launch and device check (US1, SC-001)

1. Run with an empty `MODPLAYER_CONFIG_DIR`. **Expect**: Device Check appears, system default selected, Balanced preset showing `(~N ms)`, a 1 s 440 Hz tone plays once, no raw frame counts visible.
2. Select another device / press "Play test tone" / press "No, try another". **Expect**: tone restarts on the selected device within 1 s each time, same loudness.
3. Open the preset combo. **Expect**: exactly Performance / Balanced / Safe with latency suffixes.
4. Press **Yes**. **Expect**: main window, transport stopped, silent. Quit and relaunch. **Expect**: no Device Check; `settings.toml` has `device_confirmed = true` and the device id.
5. Repeat step 1 and press **Skip for now**. Relaunch. **Expect**: Device Check again.
6. Settings › Audio › "Test output device". **Expect**: same screen, same behaviour.

### M2 — Synthetic playback, limiter, volume (US2, SC-004, SC-009)

1. Now Playing → Play. **Expect**: continuous audio, meter moving; 10 s sine, then a loud square (meter pinned at the ceiling tick, never above), sweep, 2 s silence, loop. Let it loop twice; no clicks or dropouts.
2. During the square segment drag the ceiling in Settings › Audio from −6.0 to −0.1. **Expect**: meter follows the tick and never exceeds it; no glitch at the change.
3. Set master volume to 90 %, enable safe volume with cap 50 %, quit, relaunch. **Expect**: volume 50 %. Raise to 90 %, relaunch. **Expect**: 50 % again. Disable safe volume, relaunch. **Expect**: 90 %.
4. Change buffer preset mid-playback. **Expect**: at most a brief gap (≤ one buffer), playback continues, Developer shows the new negotiated frames.

### M3 — Device resilience (US3, SC-002, SC-003) — `#[ignore = "manual"]` counterparts in code

1. Play on a USB/Bluetooth device, unplug it. **Expect**: audio continues on the system default within roughly one buffer; Critical notification "<device> lost"; Developer › clock readout keeps increasing without reset.
2. Re-plug it. **Expect**: audio stays on the fallback; Info "<device> is available again"; Settings › Audio still lists the original as preferred.
3. With only one device, unplug it. **Expect**: transport shows Paused, Critical notification, no crash. Plug in any device. **Expect**: Info notification, transport re-enabled.
4. Change the device's sample rate in the OS (Audio MIDI Setup / Sound control panel / `pactl`). **Expect**: gap ≤ one buffer, playback resumes at the same position, clock not reset.
5. Confirm a device, quit, disconnect it, relaunch. **Expect**: system default used, Warning notification naming the missing device, no Device Check.

### M4 — Shell, notifications, theme, accessibility (US4, SC-007)

1. Navigate Library / Now Playing / Plugins / Settings by mouse, then by keyboard only (Tab, arrows, `Ctrl/Cmd+1..4`).
2. Settings › Developer → raise Critical, Warning, Info while playing. **Expect**: stacked newest-first with icon + severity text; playback uninterrupted; Info disappears after 10 s; the other two stay until dismissed.
3. Theme = System; toggle OS dark/light. **Expect**: app follows live. Theme = Light; toggle OS. **Expect**: app unchanged.
4. Run the platform AT inspector (macOS Accessibility Inspector, Windows Accessibility Insights, Linux Accerciser). **Expect**: every control exposes name, role, state; focus order equals visual order.

### M5 — Settings (US5, SC-008)

1. Open Settings. **Expect**: eleven categories in the specified order.
2. From any category type `ceiling`. **Expect**: "Audio › Limiter ceiling" appears immediately; Enter navigates to it.
3. Change each Audio control; relaunch. **Expect**: values persisted (inspect `settings.toml`).
4. Edit `settings.toml` by hand to `limiter_ceiling_db = 5.0`, relaunch. **Expect**: ceiling shows −0.1 dBFS.
5. Replace `settings.toml` with garbage, relaunch. **Expect**: defaults + one Warning notification; changing any setting rewrites a valid file.

## Done criteria for this feature

All automated gates green on the three CI platforms (SC-005, SC-006) and M1–M5 pass on at least one machine per platform.

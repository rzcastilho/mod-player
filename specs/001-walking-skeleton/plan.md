# Implementation Plan: Walking Skeleton — App Shell, Synthetic Source, and Audio Output

**Branch**: `001-walking-skeleton` | **Date**: 2026-09-14 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/001-walking-skeleton/spec.md`

## Summary

Create the ModPlayer Cargo workspace from an empty repository and deliver a runnable desktop app on macOS, Windows and Linux that (1) shows a first-launch Device Check with an audible 440 Hz test tone, (2) plays a deterministic synthetic test track through a real-time engine with a lock-free command queue, monotonic audio clock, master volume, safe-startup cap and a non-bypassable sample-peak limiter, (3) survives output-device loss and sample-rate changes with bounded gaps, and (4) provides the app shell (navigation, notifications, theme, searchable Settings) that every later slice plugs into.

Technical approach (details in [research.md](research.md)): Rust 1.93 workspace of seven crates; **cpal 0.18** for audio I/O behind an `OutputBackend` trait with a `FakeBackend` for headless CI; **rtrb** SPSC queues + `std` atomics as the only communication with the real-time path; a **snapshot-rebuild** strategy (shared atomics + controller shadow state) to survive stream rebuilds without locks or `unsafe`; **egui/eframe 0.36** with AccessKit for the UI; **fluent-templates** for externalised strings; **TOML** settings with atomic replace; **assert_no_alloc** to make the "callback never allocates" rule a failing test.

Assumptions taken where the spec leaves a choice: dual MIT/Apache-2.0 licence header (research R13); linear-interpolation resampler (R6); limiter memory reset on device rebuild (R3); sample-rate change detected by 500 ms polling because cpal has no event (R2).

## Technical Context

**Language/Version**: Rust 1.93 (stable; `rust-toolchain.toml` pins 1.93.1; edition 2024) — resolves the constitution's `TODO(MSRV)`

**Primary Dependencies**: cpal 0.18 (audio I/O), rtrb 0.4 (SPSC queues), eframe/egui 0.36 with `accesskit` (UI), fluent-templates 0.15 (i18n), serde + toml (settings), directories 6 (config path), thiserror (library errors), anyhow (binary), assert_no_alloc 1.1 (dev, real-time test), cargo-deny (CI)

**Storage**: One per-user TOML file (`settings.toml`) in the platform config directory, written by atomic replace — see [contracts/settings-file.md](contracts/settings-file.md). No database, no cache, no network.

**Testing**: `cargo test --workspace` (unit + integration; engine tests driven by `FakeBackend`, allocation test via `assert_no_alloc`, property-style loops over all 60 ceiling values); hardware-dependent scenarios `#[ignore = "manual: …"]` and scripted in [quickstart.md](quickstart.md); `cargo clippy -D warnings`, `cargo fmt --check`, `cargo deny check`, licence-header script on a 3-OS GitHub Actions matrix

**Target Platform**: Desktop macOS (CoreAudio), Windows 10+ (WASAPI), Linux (ALSA/PipeWire via ALSA) — one binary per platform, identical behaviour (FR-001)

**Project Type**: Desktop application (Cargo workspace: 6 library crates + 1 binary crate)

**Performance Goals**: Glitch-free continuous playback at the Performance preset (128 frames ≈ 2.9 ms @ 44.1 kHz); command → audible change at the next buffer boundary; device fallback / sample-rate change / preset change gap ≤ one buffer duration (deterministic in `FakeBackend`, best-effort on hardware); settings search results < 50 ms per keystroke; test tone start < 1 s after any re-test action; 60 s continuous render with zero dropped/duplicated frames

**Constraints**: Real-time callback: no allocation, lock, block, I/O, logging (verified by test); `#![forbid(unsafe_code)]` in every crate (cpal's safe API means even the adapter crate needs no `unsafe`); no `unwrap`/`expect` outside tests and `main`; limiter ceiling ∈ [−6.0, −0.1] dBFS enforced at type, UI and command boundary; no decoded audio ever leaves the engine (no file writers, no sample-buffer API); all strings externalised; every control keyboard-operable with accessible name/role/state

**Scale/Scope**: 7 crates, ~4 screens (Device Check, Main shell with Now Playing/Library/Plugins, Settings with 11 categories / 10 working settings), 6 commands, 3 events, 1 settings file, 1 CI workflow; single user, single window, single active output stream

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | **Yes** — this slice creates the engine | ✅ PASS | `Processor::render` is the only real-time code: preallocated buffers, `rtrb` SPSC in/out, atomics for clock/position/peak; commands drained only at buffer start (FR-007); no trait objects on the path (engine generic over `AudioSource`); stream rebuilds use snapshot + shared atomics, never a lock (research R3); `assert_no_alloc` test fails the build on any allocation (FR-025a); PRs touching `modplayer-engine` carry a "real-time safety" note (added to PR template in this slice). Latency/loop-seam tests named by the constitution belong to later slices (no loops exist yet); the buffer-boundary and clock tests (FR-025 b, c) are this slice's equivalents. |
| II. Plugins Are Guests | No | N/A | No plugin runtime or gateway in this slice (FR-026). The command queue is designed as the single write path so the future gateway has exactly one seam (edge case "future plugin"). |
| III. Host Primitives, Plugin Behaviors | Yes (limiter is host DSP) | ✅ PASS | Limiter and gain are Rust modules inside `modplayer-engine`; no scripting tier exists. |
| IV. Audio Source Is Replaceable and Isolated | **Yes** | ✅ PASS | `AudioSource` trait in its own crate; only `SyntheticSource` implemented; engine, core, ui and binary depend solely on the synthetic source; no streaming crate exists or is referenced (FR-008). |
| V. No Audio Ever Leaves the Engine (non-negotiable) | Yes | ✅ PASS | No API writes samples to files; `FakeBackend::render_buffers` returns synthetic-only output inside test code and is not compiled into the binary's dependency graph (it is a test-only public type used exclusively by `#[cfg(test)]`/integration tests); no debug dump paths; peak meter exposes one scalar per buffer, not samples (FR-026). |
| VI. Security and Privacy by Default | No | N/A | No credentials, network, telemetry, updates, or plugins in this slice. Settings file holds device ids and numeric preferences only. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | One workspace, one crate per component (engine, audio-source, audio-source-synthetic, audio-io adapter, core services, ui, binary); MSRV pinned 1.93; `forbid(unsafe_code)` in all crates; `thiserror` in libraries, `anyhow` in the binary; `unwrap`/`expect` banned by clippy config outside tests; doc examples run under `cargo test --doc`; fmt/clippy/test/deny gates in CI (FR-024). |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | FR-025 (a–e) tests are named per contract; CI on three platforms; property-style exhaustive ceiling sweep. Criterion benchmarks and the 24 h soak (NFR-2.7) are **deferred**: this slice has no effect chain and the soak needs a reference-hardware runner (spec Assumptions: A-16 not required here) — recorded in Complexity Tracking. |
| IX. One Plugin API Definition | No | N/A | No plugin API in this slice. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | Only two traits: `AudioSource` (sanctioned) and `OutputBackend` (two real implementors: cpal + fake); no feature flags; each new dependency justified in research; platform differences confined to `modplayer-audio-io`; keyboard + accessible names for every control (FR-022); strings externalised, en-US only (FR-021 — pt-BR is a later slice per spec). Transport-focus override is N/A (no plugins). |
| Governance: engine changes need area-maintainer sign-off | Yes | ✅ PASS | CODEOWNERS entry for `crates/modplayer-engine/` added in this slice; PR template includes the real-time-safety note. |

**Pre-Phase-0 result**: PASS (no violations; two constitution items deferred with justification below).
**Post-Phase-1 re-check**: PASS — the design artifacts introduce no additional trait, feature flag, crate, or unsafe block beyond those listed above.

## Project Structure

### Documentation (this feature)

```text
specs/001-walking-skeleton/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: technology decisions R1–R16
├── data-model.md        # Phase 1: types, entities, state machines
├── quickstart.md        # Phase 1: automated gates + manual scenarios M1–M5
├── contracts/
│   ├── audio-source.md      # AudioSource trait + SyntheticSource guarantees
│   ├── engine-commands.md   # Command/Event queues, RtShared atomics, render pipeline
│   ├── output-backend.md    # OutputBackend trait, CpalBackend/FakeBackend behaviour
│   ├── settings-file.md     # settings.toml schema, read/write rules
│   └── ui-surface.md        # screens, widgets, Fluent keys, accessibility names
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

The repository currently contains only `.claude/`, `.specify/`, `docs/` and `specs/`; everything below is created by this feature.

```text
Cargo.toml                       # [workspace] members = crates/*, shared [workspace.package] (edition, rust-version, license) and [workspace.dependencies]
Cargo.lock
rust-toolchain.toml              # channel = "1.93.1"
deny.toml                        # cargo-deny: licenses allow-list, advisories, bans (duplicates)
clippy.toml                      # disallowed-methods: unwrap/expect outside tests
.cargo/config.toml               # -D warnings for local builds
LICENSE-MIT, LICENSE-APACHE
CODEOWNERS                       # crates/modplayer-engine/ → engine maintainer
.github/
├── PULL_REQUEST_TEMPLATE.md     # includes "Real-time safety" section
└── workflows/ci.yml             # matrix {ubuntu, macos, windows}: fmt, clippy, test, deny, license headers
scripts/
└── check-license-headers.sh     # SPDX line on every *.rs
locales/
└── en-US/
    ├── app.ftl                  # navigation, transport, notifications
    ├── device-check.ftl
    └── settings.ftl
crates/
├── modplayer-audio-source/          # AudioSource trait (Principle IV seam)
│   └── src/lib.rs
├── modplayer-audio-source-synthetic/# SyntheticSource (test track) + TestTone
│   ├── src/{lib.rs, track.rs, tone.rs, osc.rs}
│   └── tests/{segments.rs, determinism.rs, continuity.rs}
├── modplayer-engine/                # Real-time processor
│   ├── src/{lib.rs, types.rs, command.rs, event.rs, shared.rs, processor.rs, limiter.rs, output_stage.rs, resample.rs}
│   └── tests/{realtime.rs, limiter.rs, boundary.rs}
├── modplayer-audio-io/              # OutputBackend trait, CpalBackend, FakeBackend, device watcher
│   ├── src/{lib.rs, backend.rs, cpal_backend.rs, fake_backend.rs, watcher.rs, error.rs}
│   └── tests/{fake_backend.rs, cpal_smoke.rs (#[ignore = "manual"])}
├── modplayer-core/                  # Off-real-time-path services (AR-6 controller, settings, notifications, i18n, settings registry)
│   ├── src/{lib.rs, controller.rs, device_policy.rs, settings/{mod.rs, model.rs, store.rs}, notifications.rs, i18n.rs, settings_registry.rs}
│   └── tests/{device_policy.rs, settings.rs, safe_volume.rs, notifications.rs}
├── modplayer-ui/                    # egui views
│   ├── src/{lib.rs, app.rs, shell.rs, device_check.rs, now_playing.rs, settings/{mod.rs, audio.rs, appearance.rs, language.rs, developer.rs, search.rs}, notifications.rs, widgets/{peak_meter.rs, volume.rs}, theme.rs}
│   └── tests/{fluent_keys.rs, search.rs}
└── modplayer/                       # Binary: wires backend → controller → UI; anyhow errors
    └── src/main.rs
```

**Structure Decision**: Single Cargo workspace under `crates/` with one crate per architectural component as Constitution VII requires: `modplayer-audio-source` (trait) and `modplayer-audio-source-synthetic` (AR-1 synthetic), `modplayer-engine` (AR-3 + AR-5 limiter/output-stage DSP), `modplayer-audio-io` (AR-5 device adapter — the only crate that imports cpal), `modplayer-core` (AR-6 Playback Controller plus settings, notifications and i18n core services), `modplayer-ui` (AR-19 shell), and the `modplayer` binary. Locales live at the repository root in `locales/` so a later pt-BR drop touches no crate. The dependency graph is a strict DAG: `modplayer → ui → core → {engine, audio-io} → engine → {audio-source, audio-source-synthetic}`; `audio-io` depends on `engine` only for the `Processor` type it drives.

## Design notes that tasks must respect

1. **Real-time pipeline is fixed** (contracts/engine-commands.md): drain commands → source → master gain → + tone → limiter clamp → peak → output stage → clock. No command reorders or skips a stage.
2. **Controller is the single authority** (data-model §5.2): every state change updates shadow state first, then is enqueued; processor rebuilds read shadow state + `RtShared::position_frames`. `RtShared` is created once at app start and never replaced, so the clock survives every rebuild.
3. **Stream is always open** once a device is active, even when stopped, so the test tone and Play are immediate (< 1 s, FR-003) and the clock keeps advancing monotonically.
4. **Device watcher thread** (audio-io) is the only poller; it emits `BackendEvent`s on an `mpsc` channel that the controller drains on the UI tick (egui repaint at ≥ 30 Hz while a stream is open).
5. **Settings writes are debounced 250 ms** on the controller thread; the UI never touches the filesystem.
6. **Strings**: any `&str` reaching an egui labelled widget must come from `tr(...)`; the `fluent_keys` test enforces the key table in contracts/ui-surface.md.
7. **Zero-device state**: controller enters `NoDevice`; UI disables transport with the inline reason; `DeviceListChanged` re-enables.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

| Deviation / deferral | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Constitution VIII criterion benchmarks and 24 h soak test deferred to a later slice | This slice has no effect chain to benchmark and no reference-hardware runner (spec Assumptions: A-16 not required); a soak on a shared CI runner would be flaky and block every merge | Running the soak in CI anyway — rejected: 24 h wall-clock on GitHub-hosted runners is infeasible and would not measure real hardware. The 60 s continuous-render test (US2-5) is the bounded stand-in. |
| Second trait (`OutputBackend`) besides the sanctioned `AudioSource` | FR-024 requires engine/controller tests to run without hardware; FR-025(c) requires simulated device loss and rate change — a fake backend is a genuine second implementor | Mocking cpal's own traits — rejected: cpal's `Device`/`Stream` are concrete platform types; `cfg(test)`-only fake — rejected: `modplayer-core` integration tests need it across crate boundaries. |
| Sample-rate change detected by polling (500 ms) rather than an OS event | cpal 0.18 exposes no sample-rate-change callback; a per-platform listener would require three FFI adapters and `unsafe` | Ignoring external rate changes — rejected by FR-013/EC-3.9. Polling runs off the real-time path and costs one config query per 500 ms. Physical-change verification is a manual test (quickstart M3-4). |
| Limiter gain-memory not carried across a stream rebuild | Snapshot-rebuild (R3) keeps the callback lock-free; the limiter is a stateless clamp this slice, so nothing is actually lost | Carrying processor state through a return queue — rejected: cannot work when the device dies abruptly; two mechanisms for one job. |

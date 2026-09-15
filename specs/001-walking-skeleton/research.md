# Research: Walking Skeleton — App Shell, Synthetic Source, and Audio Output

**Feature**: 001-walking-skeleton | **Date**: 2026-09-14

Repository state at planning time: no source code exists (only `.specify/`, `docs/`, `specs/`). This slice creates the Cargo workspace from scratch, so every technology below is a fresh choice made within the constitution's constraints (Rust, one crate per component, `#![forbid(unsafe_code)]` outside FFI crates, YAGNI).

Versions were checked against crates.io on 2026-09-14 from the local toolchain (`cargo 1.93.1`). Exact versions are pinned in `Cargo.toml`/`Cargo.lock` at implementation time; the numbers here are the latest observed and are the floor.

---

## R1. Rust toolchain and MSRV

- **Decision**: Stable Rust, MSRV **1.93** (`rust-version = "1.93"` in every crate; `rust-toolchain.toml` pins `1.93.1`), edition 2024.
- **Rationale**: Constitution VII requires a pinned MSRV (`TODO(MSRV)`). 1.93.1 is the toolchain installed on the development machine; `eframe` 0.35+ already requires ≥ 1.92, so anything older cannot build the UI. Pinning the exact patch keeps the three CI platforms byte-identical.
- **Alternatives considered**: 1.85 (edition 2024 minimum) — rejected: below eframe's MSRV. Nightly — rejected: constitution mandates stable.
- **Follow-up**: this decision resolves the constitution's `TODO(MSRV)`; the constitution amendment (PATCH) is out of scope for this plan and is noted for the maintainers.

## R2. Audio I/O backend

- **Decision**: **cpal 0.18** (CoreAudio / WASAPI / ALSA, single crate, safe public API).
- **Rationale**: Only mature pure-Rust cross-platform audio I/O crate. Verified in 0.18.2 source: `DeviceTrait::id() -> DeviceId` ("stable across program runs, device disconnections, and system reboots where possible") with `Display`/`FromStr` for persistence — directly satisfies FR-004's "stable identifier, else name"; `BufferSize::Fixed(frames)` with `SupportedBufferSize::Range{min,max}` for FR-005's "nearest supported size"; the stream error callback receives `Error` with `ErrorKind::DeviceNotAvailable` for FR-012 device-loss detection; `build_output_stream::<f32>` callback signature `FnMut(&mut [f32], &OutputCallbackInfo)`.
- **Alternatives considered**: Direct per-platform bindings (coreaudio-rs / windows-rs WASAPI / alsa-rs) — rejected: three adapter crates and `unsafe` for a slice that needs one working path. `rodio` — rejected: it owns the mixing thread and hides the callback, which prevents the engine from owning the real-time path (FR-006). `kira`/`oddio` — rejected: game-audio engines that impose their own graph; the constitution requires the engine to be ours.
- **Known gaps + mitigation**:
  - cpal emits no event on an external sample-rate change. Mitigation: the device manager (off real-time path) polls `default_output_config()` of the active device every 500 ms; a change triggers the output-stage reconfiguration path (FR-013). A physical sample-rate change is a **manual** test (FR-024); the automated proof uses the fake backend (R5).
  - cpal reports the negotiated buffer size only implicitly (`data.len() / channels` in the callback). Mitigation: the callback stores the observed frame count in an `AtomicU32` (a plain atomic store is real-time safe); the UI reads it for the latency label.
  - The "within one buffer duration" fallback bound (FR-012) is deterministic only in the fake backend; on real hardware it is bounded by OS notification latency and is verified manually. Recorded in `quickstart.md` as a manual scenario.

## R3. Real-time processor ownership across device changes

- **Problem**: A cpal stream owns its data callback closure and therefore whatever processor state lives inside it. Device loss (FR-012) and sample-rate change (FR-013) both require a new stream, but the audio clock must not reset and playback position must be retained — with no lock and no `unsafe`.
- **Decision**: **Snapshot-rebuild with shared atomics.** All engine state that must survive a stream rebuild is either (a) shared through `Arc<RtShared>` containing only atomics — `clock_frames: AtomicU64`, `position_frames: AtomicU64`, `peak_bits: AtomicU32`, `negotiated_frames: AtomicU32` — or (b) owned by the controller as *shadow state* (transport, master gain, ceiling, buffer preset, source config). On a rebuild the controller drops the old stream, constructs a fresh `Processor` from its shadow state + the shared `Arc` + a fresh SPSC command/event queue pair, and opens the new stream. The synthetic source is a pure function of `position_frames`, so resuming from the published position reproduces the exact next sample. Limiter gain-reduction memory is deliberately reset (a gap already exists at a device change; a reset limiter is strictly safer).
- **Rationale**: Zero locks on the callback, zero `unsafe`, no mid-callback state hand-off. Commands enqueued during the switch window are not lost because the controller applies every command to its shadow state *before* enqueuing it; the new processor is constructed from that final state. FR-007's "only write path" holds: constructing a processor before it is attached to a stream is initialisation, not a write into a running engine.
- **Alternatives considered**:
  - `Arc<Mutex<Processor>>` with `try_lock` in the callback — rejected: a lock acquisition on the real-time path, forbidden by Principle I regardless of contention.
  - Dedicated engine thread rendering into a ring buffer that the cpal callback copies from — rejected: adds ≥ 1 buffer of latency, which defeats the 128-frame Performance preset (NFR-1.1) and blurs the clock's "frames rendered" definition.
  - Handing the `Box<Processor>` back through a return SPSC queue on a `Yield` command — rejected: cannot work when the device dies abruptly (callback never runs again), so the snapshot path would be needed anyway; two mechanisms for one job.

## R4. Lock-free queues and atomics

- **Decision**: **rtrb 0.4** SPSC ring buffers for commands (controller → processor) and events (processor → controller); `std::sync::atomic` for clock, position, peak, negotiated frames. Queue capacity 256 commands / 256 events, allocated once at processor construction on the controller thread.
- **Rationale**: rtrb is purpose-built for real-time audio (no allocation after construction, wait-free push/pop, MSRV 1.38). `f32` values cross atomics as `to_bits()` in `AtomicU32`.
- **Alternatives considered**: `crossbeam::ArrayQueue` (MPMC, heavier); `ringbuf` (equivalent, but rtrb's API is narrower and documented as real-time safe); hand-rolled ring — rejected: would need `unsafe` or an `AtomicUsize`+`UnsafeCell` pair.

## R5. Output backend abstraction and test sink

- **Decision**: An `OutputBackend` trait in `modplayer-audio-io` with **two** implementations: `CpalBackend` (production) and `FakeBackend` (tests: scripted device list, `simulate_device_lost()`, `simulate_sample_rate_change()`, `render_n_buffers()` that drives the processor synchronously and captures output). `FakeBackend` is a normal public type, not behind a feature flag.
- **Rationale**: FR-024 requires engine tests to run against an in-process sink on headless CI; FR-025(c) requires simulated fallback and sample-rate change. Two real implementors satisfy Principle X's single-implementor rule. Not using a feature flag avoids Principle X's "no feature flag without a second consumer" check entirely.
- **Alternatives considered**: `cfg(test)`-only fake — rejected: integration tests in other crates (`modplayer-core`) need it too. Mocking cpal's traits — rejected: cpal's trait surface is large and `Stream`/`Device` are concrete platform types.

## R6. Output-stage sample-rate conversion

- **Decision**: In-house **linear-interpolation** resampler inside the engine's output stage, with fixed-point (u64 32.32) phase accumulation, preallocated at construction for the maximum buffer size (4096 frames) and hard passthrough when source rate == device rate.
- **Rationale**: The spec explicitly does not constrain converter quality this slice; linear interpolation is ~40 lines, allocation-free, and exact about how many source frames each output buffer consumes (needed for the clock, R7). Adding `rubato` would require justifying a new dependency under Principle X for quality nobody is measuring yet.
- **Alternatives considered**: `rubato` (`FastFixedOut`/`SincFixedOut`, real-time safe with preallocation) — deferred to the slice that measures audio transparency; the output-stage interface (`OutputStage::process(src: &[f32], out: &mut [f32]) -> ConsumedFrames`) is shaped so rubato drops in later.

## R7. Audio clock definition

- **Decision**: `clock_frames` counts **source-rate frames** consumed from the source (44.1 kHz by default). It advances once per callback by exactly the number of source frames the output stage consumed for that buffer, before the callback returns. When source and device rates match, this equals output frames rendered. It never decrements; it advances even when transport is stopped/paused (silence is still rendered), which keeps it strictly monotonic across fallback.
- **Rationale**: FR-006 ("advanced only by frames rendered"), FR-013 ("source-rate processing and the clock are unaffected" by a device rate change) — a source-rate clock is the only definition that a device sample-rate change cannot perturb. `position_frames` (the transport position within the looping track) is a separate counter that advances only while playing.
- **Alternatives considered**: device-rate clock — rejected: would change meaning on a device rate change. Wall-clock-derived clock — rejected: not sample-accurate.

## R8. Limiter algorithm

- **Decision**: Sample-peak brickwall with zero lookahead: per sample, `y = clamp(x, -ceiling_lin, +ceiling_lin)` where `ceiling_lin = 10^(ceiling_db/20)`, computed once per buffer boundary from the clamped ceiling (`−6.0..=−0.1` dBFS, step 0.1). Applied after master gain and the test-tone sum, before the output stage. Ceiling clamp is applied in the command handler on the processor side as well as in the UI (defence in depth, FR-010).
- **Rationale**: FR-009 guarantees a sample-peak bound; hard clipping trivially satisfies `|y| ≤ ceiling` for every sample with no state, which makes the FR-025(d) property test exhaustive over the 60 ceiling steps. Transparency (lookahead, release) is explicitly deferred.
- **Alternatives considered**: lookahead peak limiter with release envelope — deferred; adds latency and state that R3's snapshot-rebuild would have to carry.

## R9. UI toolkit

- **Decision**: **egui / eframe 0.36** with the default `accesskit` feature, `wgpu` backend (default), native winit window.
- **Rationale**: Pure Rust (no second toolchain), one crate for all three platforms, AccessKit integration ships in the default feature set — every widget exposes name/role/state to platform assistive technology and Tab/Shift-Tab/arrow keyboard navigation works out of the box (FR-022). `egui::ThemePreference::{System, Light, Dark}` with `ctx.set_theme(...)` and live OS theme tracking via winit satisfies FR-017 without custom code. Immediate mode makes the peak meter and the per-keystroke settings search (FR-019) trivial: the search runs over ≤ ~20 settings per frame, far under 50 ms. egui is the de-facto GUI for Rust audio tooling, so plugin-panel work in later slices has precedent.
- **Alternatives considered**: Slint — good accessibility and built-in `@tr()` i18n, but its own DSL and a GPL/royalty-free dual license the maintainers have not approved; iced — accessibility integration still incomplete; Tauri/Dioxus-desktop — WebView + JS/TS toolchain contradicts "Rust workspace"; gpui — macOS-first maturity.
- **Accessibility caveat**: AccessKit on Linux requires AT-SPI (`accesskit_unix`, pulled in by egui-winit's feature); CI does not run an AT inspector — FR-022 is verified manually per platform (quickstart) plus a unit test that every widget the shell creates is given an explicit `.accessible_name()`/label.

## R10. Internationalisation

- **Decision**: **fluent-templates 0.15** (`static_loader!` embedding `locales/en-US/*.ftl` at compile time) accessed through one `tr(key)` helper in `modplayer-core`; UI code never contains literal user-facing strings. A test walks the `.ftl` files and the UI source to fail on any string literal passed to a labelled widget.
- **Rationale**: FR-021 requires externalised strings from the first commit; Fluent handles plurals/variables when pt-BR arrives; compile-time embedding keeps the binary self-contained (no runtime file lookup).
- **Alternatives considered**: `gettext` — rejected: C toolchain on Windows; `rust-i18n` — viable but YAML-based and less expressive for later plural rules.

## R11. Settings persistence

- **Decision**: **TOML** via `serde` + `toml` at `directories::ProjectDirs::from("", "ModPlayer", "ModPlayer").config_dir()/settings.toml` (macOS `~/Library/Application Support/ModPlayer/`, Windows `%APPDATA%\ModPlayer\ModPlayer\config\`, Linux `~/.config/modplayer/`). Writes go to `settings.toml.tmp` in the same directory, `sync_all()`, then `std::fs::rename` over the target (atomic replace on all three platforms; Rust's `rename` uses `MOVEFILE_REPLACE_EXISTING` on Windows). Unknown keys are ignored on read; a parse failure yields defaults + a warning notification (FR-020) and the file is rewritten on the next change.
- **Rationale**: Human-editable (the spec's edge case "settings file edit" presupposes this), stable serde ecosystem, atomic replace needs no extra crate.
- **Alternatives considered**: `tempfile::NamedTempFile::persist` — same semantics, extra dependency; JSON — less readable for users; `confy` — hides the atomic-write behaviour we must guarantee.

## R12. Allocation detection on the real-time path

- **Decision**: **assert_no_alloc 1.1** as a dev-dependency of `modplayer-engine`: the integration test installs `#[global_allocator] static A: AllocDisabler = AllocDisabler;` and wraps every `Processor::render` call in `assert_no_alloc(|| ...)`, which panics (test fails) on any allocation or deallocation inside the closure. Also applied around the fake backend's simulated fallback path *on the callback side*.
- **Rationale**: FR-025(a) asks for a test that *fails* if the callback allocates; this is the community-standard mechanism (used by nih-plug-style hosts) and is Rust-only. Debug-only cost; not compiled into the release binary.
- **Alternatives considered**: hand-rolled counting allocator — equivalent but more code to review; Miri — too slow for CI and does not model the callback.

## R13. CI and quality gates

- **Decision**: One GitHub Actions workflow, matrix `{ubuntu-latest, macos-latest, windows-latest}`, steps: `rustup toolchain install` (from `rust-toolchain.toml`), `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check` (via `EmbarkStudios/cargo-deny-action`), and `scripts/check-license-headers.sh` (fails on any `.rs` file lacking `// SPDX-License-Identifier: MIT OR Apache-2.0` on line 1). Linux job installs `libasound2-dev` and `libxkbcommon-dev`/`libwayland-dev` for cpal/winit. Tests needing physical hardware carry `#[ignore = "manual: ..."]` and are listed in `quickstart.md`.
- **Rationale**: FR-024 verbatim; a matrix job is the cheapest way to make "100 % of merges gated on all three platforms" (SC-005) a branch-protection rule.
- **Assumption**: dual MIT/Apache-2.0 licence (Rust-ecosystem convention, permissive per GOV-1.1). The header string lives in one script constant; changing the licence is a one-line change.

## R14. Test-tone mixing point

- **Decision**: The test tone is summed into the mix **after** master gain and **before** the limiter, at a fixed −20 dBFS peak (linear 0.1), with 10 ms linear ramps. It is triggered by `Command::PlayTestTone` and runs to completion (1.0 s at source rate) regardless of transport state; a retrigger restarts the tone from its ramp-in.
- **Rationale**: FR-003 "independent of master volume and the safe-volume cap … still passes through the limiter"; summing after gain is the only topology that satisfies both clauses.

## R15. Channel mapping and sample format

- **Decision**: Engine renders interleaved stereo `f32`. Output stage maps to the device's channel count: 1 → `(L+R)/2`; 2 → passthrough; N>2 → L,R on channels 0–1, zeros elsewhere. Streams are opened with `SampleFormat::F32`; if a device offers no f32 config, the adapter converts from `i16`/`u16` via cpal's `Sample` traits inside the callback (arithmetic only, no allocation).
- **Rationale**: FR-023 verbatim.

## R16. Workspace layout (one crate per architectural component)

- **Decision**: Seven crates under `crates/` (see `plan.md` → Project Structure). The limiter lives *inside* `modplayer-engine` as a module, not as an effect node, precisely because it must be non-bypassable: it is hard-wired into `Processor::render` and no command can remove it (FR-009).
- **Rationale**: Constitution VII names engine, audio-source, core services, ui as components; this slice needs no effects/plugin/gateway crate. `audio-source` (trait) and `audio-source-synthetic` are split so that the future Connect receiver crate depends only on the trait crate (Principle IV).
- **Alternatives considered**: single crate with modules — rejected by Constitution VII; separate `limiter` crate — rejected: nothing else consumes it and it would invite a future "bypass by not linking it".

---

description: "Task list for Walking Skeleton — App Shell, Synthetic Source, and Audio Output"
---

# Tasks: Walking Skeleton — App Shell, Synthetic Source, and Audio Output

**Input**: Design documents from `/specs/001-walking-skeleton/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md (all present)

**Tests**: Included. FR-024/FR-025 and the spec's acceptance scenarios explicitly require an automated test suite (allocation, buffer-boundary, clock-monotonic, limiter-sweep, clamp, settings round-trip, device-policy, notification-lifecycle, fluent-key, search tests), so test tasks are generated alongside implementation, not appended after.

**Organization**: Tasks are grouped by user story (US1–US5, priority order from spec.md) after a Setup phase and a Foundational phase. Repository is currently empty except `.claude/`, `.specify/`, `docs/`, `specs/` — this feature creates every file below.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no unmet dependency)
- **[Story]**: US1–US5 maps to spec.md's five user stories; Setup/Foundational/Polish tasks carry no story label
- Every task names its exact file path(s), per `plan.md` → Project Structure

## Path Conventions

Cargo workspace at repo root; `crates/<name>/src/...` and `crates/<name>/tests/...` per `plan.md`. Root-level files (`Cargo.toml`, CI, locales, scripts) as listed there.

---

## Phase 1: Setup (Workspace Scaffolding)

**Purpose**: Create the Cargo workspace, tooling config, CI, licensing, and empty crate skeletons so foundational code has somewhere to live.

- [X] T001 Create root `Cargo.toml` — `[workspace] members = ["crates/*"]`, `[workspace.package]` (edition 2024, `rust-version = "1.93"`, `license = "MIT OR Apache-2.0"`), `[workspace.dependencies]` (cpal 0.18, rtrb 0.4, eframe/egui 0.36 with `accesskit`, fluent-templates 0.15, serde, toml, directories 6, thiserror, anyhow, assert_no_alloc 1.1)
- [X] T002 [P] Add `rust-toolchain.toml` pinning `channel = "1.93.1"`
- [X] T003 [P] Add `deny.toml` (licenses allow-list MIT/Apache-2.0, advisories, bans on duplicate versions)
- [X] T004 [P] Add `clippy.toml` (`disallowed-methods` for `unwrap`/`expect` outside `tests`/`main`)
- [X] T005 [P] Add `.cargo/config.toml` (`-D warnings` for local builds)
- [X] T006 [P] Add `LICENSE-MIT` and `LICENSE-APACHE` at repo root
- [X] T007 [P] Add `CODEOWNERS` mapping `crates/modplayer-engine/` to the engine maintainer
- [X] T008 [P] Add `.github/PULL_REQUEST_TEMPLATE.md` with a mandatory "Real-time safety" section
- [X] T009 Add `.github/workflows/ci.yml` — matrix `{ubuntu-latest, macos-latest, windows-latest}`; steps: toolchain install, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, license-header script; Linux job installs `libasound2-dev libxkbcommon-dev libwayland-dev pkg-config`
- [X] T010 [P] Add `scripts/check-license-headers.sh` (fails if any `*.rs` lacks `// SPDX-License-Identifier: MIT OR Apache-2.0` on line 1)
- [X] T011 [P] Create empty `locales/en-US/app.ftl`, `locales/en-US/device-check.ftl`, `locales/en-US/settings.ftl` (keys filled in by later phases)
- [X] T012 Scaffold the seven crate directories with `Cargo.toml` (inheriting `workspace.package`, listing only the dependencies each crate needs per `plan.md`'s dependency graph) and a minimal stub `src/lib.rs` (or `src/main.rs` for the binary) for: `crates/modplayer-audio-source`, `crates/modplayer-audio-source-synthetic`, `crates/modplayer-engine`, `crates/modplayer-audio-io`, `crates/modplayer-core`, `crates/modplayer-ui`, `crates/modplayer`
- [X] T013 [P] Add `#![forbid(unsafe_code)]` and the SPDX header line to every stub crate root created in T012
- [X] T014 Verify `cargo build --workspace` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` succeed on the empty skeleton before any foundational code is added

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The value types, traits, real-time pipeline, settings store, notification/i18n core, output-backend trait, and controller skeleton that every user story is built on. Nothing in Phase 3+ compiles without this.

**⚠️ CRITICAL**: No user-story work starts until this phase is complete.

- [X] T015 Implement engine value types — `SampleRate`, `FrameCount`, `BufferPreset` (Performance=128/Balanced=256/Safe=1024, `requested_frames()`, default Balanced), `NegotiatedBuffer` (`latency_ms()`, `duration()`), `CeilingDb` (clamp `[-6.0,-0.1]` step 0.1, default -1.0, `to_linear()`), `VolumePercent` (clamp 0–100, `to_linear()`, `to_db()`), `SafeVolume` (`{enabled,cap}`, `apply(stored)`), `Transport` (Stopped/Playing/Paused), `DeviceId` (non-empty String newtype), `Theme` (System/Light/Dark, default System) in `crates/modplayer-engine/src/types.rs`
- [X] T016 [P] Define the `AudioSource` trait (`sample_rate`, `len_frames`, `position`, `seek`, `fill`) per `contracts/audio-source.md` in `crates/modplayer-audio-source/src/lib.rs`
- [X] T017 [P] Implement oscillator primitives (sine/square/sawtooth phase accumulation, continuous phase across calls) in `crates/modplayer-audio-source-synthetic/src/osc.rs`
- [X] T018 Implement `SyntheticSource` skeleton `{sample_rate, position, track_len, phase}` implementing `AudioSource`, `new(sample_rate)`/`Default = 44_100 Hz` in `crates/modplayer-audio-source-synthetic/src/lib.rs` (depends on T017; `fill` delegates to `track.rs`, added in US2)
- [X] T019 Implement `Command` enum (`SetMasterVolume`, `SetCeiling`, `Play`, `Pause`, `Stop`, `PlayTestTone` — `Copy`, ≤16 bytes) in `crates/modplayer-engine/src/command.rs`
- [X] T020 [P] Implement `Event` enum (`ToneFinished`, `TrackLooped{at_clock}`, `CommandDropped{kind}`) in `crates/modplayer-engine/src/event.rs`
- [X] T021 Implement `RtShared` (`clock_frames: AtomicU64`, `position_frames: AtomicU64`, `peak_bits: AtomicU32`, `negotiated_frames: AtomicU32`, ordering per `contracts/engine-commands.md`) in `crates/modplayer-engine/src/shared.rs`
- [X] T022 Implement `Limiter` — sample-peak brickwall clamp `y = clamp(x, -ceiling_lin, +ceiling_lin)`, ceiling recomputed from clamped `CeilingDb` at buffer boundary — in `crates/modplayer-engine/src/limiter.rs`
- [X] T023 Implement `OutputStage` (linear-interpolation resampler, fixed-point 32.32 phase accumulator, preallocated for 4096 frames, hard passthrough when `source_rate == device_rate`) and its resample math in `crates/modplayer-engine/src/output_stage.rs` and `crates/modplayer-engine/src/resample.rs`; channel mapping 1→`(L+R)/2`, 2→passthrough, N>2→L/R on 0–1 + silence elsewhere (FR-023)
- [X] T024 Implement `ProcessorConfig` (`{source_rate, device_rate, device_channels, max_frames, transport, position_frames, master_volume, ceiling, shared}`) in `crates/modplayer-engine/src/processor.rs`
- [X] T025 Implement `Processor<S: AudioSource>` (rtrb `Consumer<Command>`/`Producer<Event>` capacity 256, `master_gain`, `ceiling_lin`, `tone: Option<TestTone>`, `output_stage`, preallocated `scratch`) and `render()` implementing the fixed pipeline `drain commands → source.fill (or silence) → ×master_gain → +tone → limiter clamp → peak measure → output_stage → clock += consumed` in `crates/modplayer-engine/src/processor.rs` (depends on T019–T024, T018)
- [X] T026 [P] Re-export the public engine API (`types`, `Command`, `Event`, `RtShared`, `Processor`, `ProcessorConfig`) in `crates/modplayer-engine/src/lib.rs`
- [X] T027 [P] Write `render_never_allocates` test — `assert_no_alloc` around 1000 `render` calls including pending commands and tone start/finish (FR-025a) in `crates/modplayer-engine/tests/realtime.rs`
- [X] T028 [P] Write `command_applies_at_next_boundary` test — push `SetMasterVolume(0)` mid-buffer; buffer N unaffected, buffer N+1 all zero from its first sample (FR-025b) in `crates/modplayer-engine/tests/boundary.rs`
- [X] T029 [P] Write `ceiling_and_cap_clamp` test — `CeilingDb::new(0.0)==-0.1`, `CeilingDb::new(-20.0)==-6.0`, `VolumePercent::new(150)==100` (FR-025e) in `crates/modplayer-engine/tests/limiter.rs`
- [X] T030 [P] Implement `AudioSettings` model (`{output_device, device_confirmed, buffer_preset, limiter_ceiling_db, safe_volume, master_volume, theme, schema_version}`, defaults per `contracts/settings-file.md`) in `crates/modplayer-core/src/settings/model.rs`
- [X] T031 Implement the settings store — load with the read-rules table (missing→defaults/no notice; unparseable→defaults+`settings-unreadable` Warning; newer schema→defaults+`settings-newer-version` Warning, no rewrite; unknown enum→field default+`settings-invalid-value` Warning), save via `settings.toml.tmp` + `sync_all()` + `rename` (atomic replace), `MODPLAYER_CONFIG_DIR` override in `crates/modplayer-core/src/settings/store.rs` (depends on T030)
- [X] T032 [P] Wire `crates/modplayer-core/src/settings/mod.rs` exposing the model+store public API
- [X] T033 [P] Write settings tests — round-trip defaults, out-of-range file values clamp (`limiter_ceiling_db=3.0→-0.1`, `master_volume=250→100`, `cap=-5→0`), garbage file → defaults + exactly one Warning, simulated crash mid-write leaves prior file intact, unknown key ignored, newer schema not rewritten (FR-020, FR-025e) in `crates/modplayer-core/tests/settings.rs`
- [X] T034 [P] Implement `Notification{id,severity,message_key,created_at,dismissed}` and `NotificationCenter{items: VecDeque, tick(now), dismiss(id)}` (newest-first) in `crates/modplayer-core/src/notifications.rs`
- [X] T035 [P] Implement the `tr(key)` Fluent helper with `static_loader!` embedding `locales/en-US/*.ftl` at compile time in `crates/modplayer-core/src/i18n.rs`
- [X] T036 [P] Write notification lifecycle tests — Info auto-dismisses at 10 s, Critical/Warning persist until `dismiss()` in `crates/modplayer-core/tests/notifications.rs`
- [X] T037 [P] Define the `OutputBackend` trait, `OpenStream`, `BackendEvent`, `AudioIoError`, `OutputDeviceInfo`, `StreamRequest` per `contracts/output-backend.md` in `crates/modplayer-audio-io/src/backend.rs` and `crates/modplayer-audio-io/src/error.rs`
- [X] T038 Implement `FakeBackend` (`new(devices)`, `open`, `render_buffers(n)`, `remove_device`, `add_device`, `set_rate`, `set_default`, `take_processor`) in `crates/modplayer-audio-io/src/fake_backend.rs` (depends on T037, T025)
- [X] T039 [P] Re-export the audio-io public API in `crates/modplayer-audio-io/src/lib.rs`
- [X] T040 [P] Write `FakeBackend` unit tests for every method above in `crates/modplayer-audio-io/tests/fake_backend.rs`
- [X] T041 Implement the `PlaybackController` shadow-state skeleton (`{transport, master_volume, ceiling, preset, active_device, preferred_device, stream, command_tx, event_rx, shared}`, `shared: Arc<RtShared>` created once) wired to an injected `OutputBackend` in `crates/modplayer-core/src/controller.rs` (depends on T038, T031, T034)

**Checkpoint**: Foundation ready — every user story can now be implemented.

---

## Phase 3: User Story 1 - First-launch device check and test tone (Priority: P1) 🎯 MVP

**Goal**: Fresh install shows Device Check with the system default preselected and a Balanced-labelled preset; the 440 Hz/1 s/−20 dBFS test tone plays automatically and on every re-test action within 1 s; Yes persists the device+preset and skips the screen next launch; Skip leaves it unconfirmed; the same screen opens on demand from Settings › Audio.

**Independent Test**: Fresh install with no stored preferences → launch → observe device list/default selection/tone → confirm device → restart → screen does not reappear.

### Tests for User Story 1

- [X] T042 [P] [US1] Write Device Check integration tests — fresh install shows defaults + auto tone; **Yes** persists device+preset+confirmed and skips Device Check on relaunch; **Skip for now** leaves unconfirmed and the screen reappears; zero devices at launch → empty state, no tone attempted, Critical notification, transport disabled (spec US1 acceptance 1,3,4,5; edge case) in `crates/modplayer-core/tests/device_policy.rs`
- [X] T043 [P] [US1] Write `TestTone` unit tests — peak `0.1 ± 1e-6`, first/last 10 ms are monotone linear ramps, `is_finished()` true after `1.0 s × rate` frames — as `#[cfg(test)]` in `crates/modplayer-audio-source-synthetic/src/tone.rs`

### Implementation for User Story 1

- [X] T044 [US1] Implement `TestTone{rate, elapsed_frames}` — `render_add(out) -> bool` sums (never overwrites) 440 Hz/1.0 s/10 ms linear fade-in-out/−20 dBFS peak into `out` and returns `true` while playing — in `crates/modplayer-audio-source-synthetic/src/tone.rs` (make T043 pass)
- [X] T045 [US1] Wire `PlayTestTone` command handling in `Processor::render` — (re)starts the tone from its fade-in, runs to completion regardless of transport state, summed after master gain and before the limiter — in `crates/modplayer-engine/src/processor.rs`
- [X] T046 [P] [US1] Implement `CpalBackend` — `devices()`/`default_device()` enumeration, `open()` with `BufferSize::Fixed(clamp(requested,min,max))` (or `Default` when unknown), F32 preferred with arithmetic-only conversion otherwise, error callback with `ErrorKind::DeviceNotAvailable` → `BackendEvent::DeviceLost`, first-callback frame count stored in `RtShared::negotiated_frames` — in `crates/modplayer-audio-io/src/cpal_backend.rs`
- [X] T047 [US1] Add the ignored hardware smoke test — enumerate, open the default device at each preset, render 1 s of silence, print negotiated frames — `#[ignore = "manual: needs an audio device"]` in `crates/modplayer-audio-io/tests/cpal_smoke.rs`
- [X] T048 [US1] Implement device-resolution policy — match by stable id then by name; confirmed && present → `Active(preferred)`; confirmed && absent → `Active(default)` + Warning; zero devices → `NoDevice` + Critical — in `crates/modplayer-core/src/device_policy.rs`
- [X] T049 [US1] Wire the controller's launch sequence and Device Check flows (Yes → persist `{output_device, buffer_preset, device_confirmed=true}`; Skip → leave unconfirmed; device-select/"Play test tone"/"No, try another" → `PlayTestTone` on the newly selected device) in `crates/modplayer-core/src/controller.rs`
- [X] T050 [P] [US1] Implement the Device Check screen — device radio list (system default preselected), buffer preset combo labelled `"{preset} (~{ms} ms)"` with **no raw frame counts**, "Play test tone" button, "Did you hear that?" prompt with Yes/No,-try-another/Skip-for-now, empty-state (zero devices: only Skip) — in `crates/modplayer-ui/src/device_check.rs`
- [X] T051 [US1] Wire the same Device Check component to open identically from Settings › Audio › "Test output device" in `crates/modplayer-ui/src/device_check.rs` and a stub button in `crates/modplayer-ui/src/settings/audio.rs`
- [X] T052 [P] [US1] Add Fluent content for `device-check-device-list`, `buffer-preset-performance`/`-balanced`/`-safe`, `device-check-play-tone`, `device-check-question`, `device-check-yes`, `device-check-no`, `device-check-skip`, `device-check-no-devices` in `locales/en-US/device-check.ftl`

**Checkpoint**: User Story 1 is fully functional and independently testable (device check, test tone, persistence).

---

## Phase 4: User Story 2 - Continuous synthetic playback through the real-time engine and limiter (Priority: P2)

**Goal**: Play the built-in synthetic test track continuously with no dropouts; the non-bypassable limiter keeps every sample within the user-set ceiling across its full range; master volume and the safe-volume startup cap behave exactly as documented.

**Independent Test**: Play the synthetic track through two loops; verify via the peak meter and an automated sink test that output never exceeds the ceiling and no frames drop or duplicate over 60 s.

### Tests for User Story 2

- [X] T053 [P] [US2] Write test-track segment test — a 22 s fill at 44 100 Hz produces segment peaks `0.2512, 1.0, 0.2512, 0.0` (±1e-4) in order — in `crates/modplayer-audio-source-synthetic/tests/segments.rs`
- [X] T054 [P] [US2] Write determinism test — two `SyntheticSource`s seeked to the same frame produce bit-identical buffers — in `crates/modplayer-audio-source-synthetic/tests/determinism.rs`
- [X] T055 [P] [US2] Write 60 s continuity/dropout test — 256-frame chunks, no sample-to-sample jump exceeds the theoretical maximum for a 440 Hz sine at the render rate (US2-5, backs FR-025) — in `crates/modplayer-audio-source-synthetic/tests/continuity.rs`
- [X] T056 [P] [US2] Write `limiter_never_exceeds_ceiling` test — every ceiling in `-6.0..=-0.1` step 0.1 fed the 0 dBFS square segment, `max|y| ≤ ceiling_lin + 1e-6` (FR-025d, SC-004) — in `crates/modplayer-engine/tests/limiter.rs`
- [X] T057 [P] [US2] Write safe-volume tests — enabled with cap C and stored V>C clamps to C at launch; raising above C persists and re-clamps to C on next launch; disabled uses the saved value unchanged (US2 acceptance 2,3; SC-009) — in `crates/modplayer-core/tests/safe_volume.rs`
- [X] T058 [US2] Extend the boundary test — master volume, ceiling, buffer preset, and device changes during playback apply only at the next buffer boundary with no audible glitch or clock discontinuity (US2 acceptance 4) — in `crates/modplayer-engine/tests/boundary.rs`

### Implementation for User Story 2

- [X] T059 [US2] Implement `track.rs` segment sequencing — 10 s 440 Hz sine @ −12 dBFS → 5 s 1 kHz square @ 0 dBFS → 5 s sawtooth sweep 100→2000 Hz @ −12 dBFS → 2 s silence; wraps at `track_len = 22 s × rate` — in `crates/modplayer-audio-source-synthetic/src/track.rs`
- [X] T060 [US2] Wire `SyntheticSource::fill` to `track.rs` with phase continuous across segment boundaries in `crates/modplayer-audio-source-synthetic/src/lib.rs`
- [X] T061 [US2] Implement the safe-volume startup clamp — effective master volume at launch = `SafeVolume::apply(stored)` — applied before the controller's first `SetMasterVolume` enqueue, in `crates/modplayer-core/src/controller.rs`
- [X] T062 [US2] Wire `SetMasterVolume`/`SetCeiling`/`Play`/`Pause`/`Stop` producers — shadow state updated before each command is pushed; retries on a full queue without blocking or dropping the shadow update — in `crates/modplayer-core/src/controller.rs`
- [X] T063 [P] [US2] Implement the peak meter widget — dBFS scale −60..0 with the ceiling tick marked, reads `RtShared::peak_bits` every frame, accessible value = last peak in dBFS — in `crates/modplayer-ui/src/widgets/peak_meter.rs`
- [X] T064 [P] [US2] Implement the master volume widget — 0–100 % slider, ←/→ ±1, PgUp/PgDn ±10, accessible value `"{pct} %, {db} dB"` — in `crates/modplayer-ui/src/widgets/volume.rs`
- [X] T065 [US2] Implement the Now Playing screen — track title "Synthetic test track", Play/Pause toggle, Stop, the master-volume widget, the peak-meter widget, inline disabled-reason text for zero devices — in `crates/modplayer-ui/src/now_playing.rs` (depends on T063, T064)
- [X] T066 [P] [US2] Add Fluent content for `synthetic-track-title`, `transport-play`/`-pause`/`-stop`, `master-volume`, `peak-meter`, `transport-disabled-no-device` in `locales/en-US/app.ftl`

**Checkpoint**: User Stories 1 AND 2 both work independently — audible device check plus a trustworthy, safety-bounded continuous playback path.

---

## Phase 5: User Story 3 - Output device resilience (Priority: P3)

**Goal**: Device loss falls back to the system default within one buffer duration without resetting the clock; sample-rate changes reconfigure only the output stage; loss with no remaining device pauses transport safely; the app never automatically switches back to a reappeared device.

**Independent Test**: Start playback, remove the active device (or trigger a rate change) via `FakeBackend`; observe fallback within one buffer duration and a notification naming the lost device.

### Tests for User Story 3

- [X] T067 [P] [US3] Write device-resilience tests — `fallback_on_device_lost`, `pause_when_no_device_remains`, `no_switch_back_on_reappear`, `missing_preferred_at_launch_uses_default_with_warning`, `rate_change_rebuilds_output_stage_only`, `zero_devices_at_launch` (US3 acceptance 1–5; FR-014) — in `crates/modplayer-core/tests/device_policy.rs`
- [X] T068 [P] [US3] Write `clock_monotonic_across_rebuild` test — render 100 buffers, rebuild from a snapshot after a simulated device loss and again after a simulated 48 kHz rate change; clock equals Σ consumed frames, never decreases, position continues (FR-025c) — in `crates/modplayer-engine/tests/realtime.rs`

### Implementation for User Story 3

- [X] T069 [US3] Implement the device watcher thread — polls the active device's `default_output_config()` every 500 ms for rate changes, re-enumerates every 2 s for list changes, emits `BackendEvent`s off the real-time path — in `crates/modplayer-audio-io/src/watcher.rs`
- [X] T070 [US3] Wire `CpalBackend` to spawn the watcher and expose `events() -> &Receiver<BackendEvent>` in `crates/modplayer-audio-io/src/cpal_backend.rs`
- [X] T071 [US3] Implement snapshot-rebuild on `DeviceLost` — drop the old stream, build a fresh `Processor` from shadow state + the existing `Arc<RtShared>` + fresh command/event queues targeting the system default, limiter memory intentionally reset (research R3) — in `crates/modplayer-core/src/controller.rs`
- [X] T072 [US3] Implement the zero-remaining-device path — transport → Paused with position retained, Critical notification, no crash — in `crates/modplayer-core/src/device_policy.rs`
- [X] T073 [US3] Implement reappearance handling — stay on the fallback device, raise an Info notification, retain `preferred_device`, never auto switch back — in `crates/modplayer-core/src/device_policy.rs`
- [X] T074 [US3] Implement `SampleRateChanged` handling — rebuild only the output stage/device rate; source-rate processing, clock, and position are unaffected; gap ≤ one buffer duration — in `crates/modplayer-core/src/controller.rs`
- [X] T075 [P] [US3] Wire `device-lost`, `device-missing-at-launch`, `device-available-again`, `no-output-devices`, `device-appeared` notification keys (with `{ $device }` args) into `crates/modplayer-core/src/notifications.rs` and `locales/en-US/app.ftl`

**Checkpoint**: All three audio-path stories (US1–US3) are independently functional; the app survives real hardware changes.

---

## Phase 6: User Story 4 - App shell navigation and notifications (Priority: P4)

**Goal**: A navigable four-section shell (Library, Now Playing, Plugins, Settings) usable by pointer and keyboard alone; a non-blocking, severity-classified notification area; a theme that follows the OS by default and can be pinned to Light/Dark.

**Independent Test**: Navigate all four sections by mouse and keyboard, raise one notification of each severity from Settings › Developer, toggle OS theme.

### Tests for User Story 4

- [X] T076 [P] [US4] Write a `fluent_keys` test asserting every shell/nav/notification Fluent key resolves against `locales/en-US/*.ftl` in `crates/modplayer-ui/tests/fluent_keys.rs`
- [X] T077 [P] [US4] Write an accessible-name unit test asserting every shell/nav/notification widget exposes an explicit accessible name/role/state, as `#[cfg(test)]` in `crates/modplayer-ui/src/shell.rs`

### Implementation for User Story 4

- [X] T078 [US4] Implement `app.rs` — the eframe `App` entrypoint wiring the controller and UI state, calling `ctx.set_theme(...)` before the first frame — in `crates/modplayer-ui/src/app.rs`
- [X] T079 [US4] Implement `shell.rs` — left-rail navigation (Library · Now Playing · Plugins · Settings), `Ctrl/Cmd+1..4` shortcuts, Tab focus order equal to visual order — in `crates/modplayer-ui/src/shell.rs`
- [X] T080 [US4] Implement the notifications UI — newest-first stack, severity icon + severity text + message + Dismiss button, Info auto-dismissed via `NotificationCenter::tick`, never modal, never interrupts playback/navigation — in `crates/modplayer-ui/src/notifications.rs`
- [X] T081 [US4] Implement `theme.rs` — `Theme::System → ThemePreference::System` with live OS tracking; `Light`/`Dark` fixed regardless of OS changes; applied at startup and immediately on change — in `crates/modplayer-ui/src/theme.rs`
- [X] T082 [P] [US4] Implement placeholder Library/Plugins content (`placeholder-library`, `placeholder-plugins`) in `crates/modplayer-ui/src/shell.rs`
- [X] T083 [P] [US4] Implement the Settings › Developer screen — raw buffer frame readout `"requested N / negotiated M frames"`, "Raise sample notification" buttons for Critical/Warning/Info — in `crates/modplayer-ui/src/settings/developer.rs`
- [X] T084 [P] [US4] Add Fluent content for `nav-library`/`-now-playing`/`-plugins`/`-settings`, `severity-critical`/`-warning`/`-info`, `notification-dismiss` in `locales/en-US/app.ftl`, and `setting-buffer-frames`/`-desc`, `setting-raise-notification`/`-desc` in `locales/en-US/settings.ftl`

**Checkpoint**: US1–US4 all work independently; the shell every later feature plugs into exists and is accessible.

---

## Phase 7: User Story 5 - Searchable, organized Settings (Priority: P5)

**Goal**: All eleven Settings categories exist in fixed order; per-keystroke search (<50 ms) surfaces matching settings by title/description with their category path.

**Independent Test**: Open Settings, confirm all eleven categories, search "ceiling" and confirm the limiter setting surfaces with its category path.

### Tests for User Story 5

- [X] T085 [P] [US5] Write settings-search tests — `"ceiling"` and `"CEIL"` return `audio.limiter_ceiling` (search completes well under 50 ms, target <1 ms), `"zzz"` returns nothing (SC-008) — in `crates/modplayer-ui/tests/search.rs`
- [X] T086 [US5] Extend the `fluent_keys` test to cover every Settings-screen key (categories, search box, per-setting title/description) in `crates/modplayer-ui/tests/fluent_keys.rs`

### Implementation for User Story 5

- [X] T087 [US5] Implement the `SettingDescriptor`/`SettingsCategory` registry — the eleven categories in fixed order (Account, Audio, Playback, Controls, Plugins, Offline, Appearance, Language, Developer, Privacy & diagnostics, About) — in `crates/modplayer-core/src/settings_registry.rs`
- [X] T088 [US5] Implement `search(query)` — case-insensitive substring match over each descriptor's resolved title + description; categories with placeholder content contribute none — in `crates/modplayer-core/src/settings_registry.rs`
- [X] T089 [US5] Register descriptors for the working settings (`audio.output_device`, `audio.buffer_preset`, `audio.limiter_ceiling`, `audio.safe_volume_enabled`, `audio.safe_volume_cap`, `audio.test_output_device`, `appearance.theme`, `language.locale`, `developer.buffer_frames`, `developer.raise_notification`) in `crates/modplayer-core/src/settings_registry.rs`
- [X] T090 [US5] Implement the Settings shell — fixed-order category list, search box (`Ctrl/Cmd+F`), results rendered as `"{category} › {title}"`, Enter navigates to and focuses the setting — in `crates/modplayer-ui/src/settings/mod.rs`
- [X] T091 [P] [US5] Implement the Audio settings screen — output-device combo, buffer-preset combo with latency suffix, limiter-ceiling slider (`-6.0..=-0.1` step 0.1), safe-volume checkbox + cap slider, "Test output device" button — in `crates/modplayer-ui/src/settings/audio.rs`
- [X] T092 [P] [US5] Implement the Appearance settings screen — Theme combo (System/Light/Dark) — in `crates/modplayer-ui/src/settings/appearance.rs`
- [X] T093 [P] [US5] Implement the Language settings screen — locale combo, English the only option — in `crates/modplayer-ui/src/settings/language.rs`
- [X] T094 [P] [US5] Render placeholder content for the remaining categories (`placeholder-settings-category`, contributing no search descriptors) in `crates/modplayer-ui/src/settings/mod.rs`
- [X] T095 [P] [US5] Add remaining Fluent content — `settings-cat-account`…`settings-cat-about`, `settings-search`, `setting-output-device`/`-desc`, `setting-buffer-preset`/`-desc`, `setting-limiter-ceiling`/`-desc` (description contains "ceiling"), `setting-safe-volume`/`-desc`, `setting-safe-volume-cap`/`-desc`, `setting-test-output-device`/`-desc`, `setting-theme`/`-desc`, `setting-locale`/`-desc` — in `locales/en-US/settings.ftl`

**Checkpoint**: All five user stories are independently functional — the walking skeleton is complete.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Wire the binary, and close out the gates every story's tests assumed.

- [X] T096 Implement `crates/modplayer/src/main.rs` — construct `CpalBackend`, `PlaybackController`, and the `modplayer-ui` app; top-level error handling via `anyhow`; honour `MODPLAYER_CONFIG_DIR`
- [X] T097 [P] Run `scripts/check-license-headers.sh` across the repo and fix any file missing its SPDX header
- [X] T098 [P] Bring the whole workspace to a clean `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [X] T099 Run `cargo deny check` and resolve any licence/advisory/duplicate-version finding
- [X] T100 [P] Execute the quickstart.md automated gates end-to-end (`cargo fmt --all --check`, `cargo clippy ...`, `cargo test --workspace`, `cargo deny check`, license-header script) and record the result — **all five green** on 2026-09-15 (toolchain 1.95.0 per `rust-toolchain.toml`): `cargo fmt --all --check` exit 0; `cargo clippy --workspace --all-targets --all-features -- -D warnings` exit 0 (no issues); `cargo test --workspace` → 126 passed, 1 ignored (`cpal_smoke`, manual/hardware), 27 suites, 0 failed; `cargo deny check` exit 0 (advisories ok, bans ok, licenses ok, sources ok — one advisory (`RUSTSEC-2026-0192`, unmaintained `ttf-parser`, no safe upgrade) and several transitive licenses (BSL-1.0, ISC, MPL-2.0, OFL-1.1, Ubuntu-font-1.0) explicitly allow-listed in `deny.toml` with rationale; internal workspace path deps pinned with `version = "0.1.0"` to clear `bans.wildcards`); `scripts/check-license-headers.sh` exit 0
- [X] T101 Execute quickstart.md manual scenarios M1–M5 on at least one machine per platform (macOS/Windows/Linux); record pass/fail against SC-001–SC-009 — **macOS done 2026-09-15** (macOS 12.7.6, x86_64, debug build, toolchain 1.95.0; output captured on the BlackHole 16ch loopback device and analysed per 100/500 ms bin for peak/RMS/frequency; UI driven by synthetic CGEvents + window captures). **Scope decision 2026-09-15: the macOS host run is accepted as sufficient sign-off for this slice; Windows/Linux manual runs and the hot-plug steps M3.1–M3.3 are deferred to a follow-up** (automated gates still run on all three platforms in CI). Results:
  - **M1 (SC-001) PASS** — fresh config dir shows Device Check with system default pre-selected, `Balanced (~5.8 ms)`, no raw frame counts; test tone measured 440 Hz / −20.0 dBFS peak / ~1.0 s and restarted within ~0.1 s on device select, "Play test tone" and "No, try another" at identical level; combo lists exactly Performance (~2.9 ms) / Balanced (~5.8 ms) / Safe (~23.2 ms); **Yes** → main window, transport stopped, silent; relaunch skips Device Check and `settings.toml` has `device_confirmed = true` + device id; **Skip for now** writes nothing and Device Check returns on relaunch; Settings › Audio › Test output device opens the same screen with the same tone.
  - **M2 (SC-004, SC-009) PASS after fixes** — 48 s capture: 10 s 440 Hz @ −12 dBFS → 5 s 1 kHz square limited to exactly the −1.0 dBFS ceiling → 5 s 100→2000 Hz sweep → 2 s silence, loop period 22 s, no dropped bins; ceiling dragged −1.0 → −6.0 mid-square applied at the next buffer (peak −1.0 → −6.0, no glitch), later −0.1 honoured; safe-volume: stored 90 + cap 50 → launches at 50, raise to 90 → 50 again, disable → 90; preset change mid-playback Safe→Balanced gap 23.2 ms (= one Safe buffer), Balanced→Performance gap 40 ms (hardware best-effort), Performance→Safe no measurable gap, no overshoot, Developer shows `requested 1024 / negotiated 1024`. Found & fixed: (1) `Processor::scratch` sized from `config.max_frames` → `range end index 2048 out of range for slice of length 512` panic on the CoreAudio IO thread (abort) when the new stream delivered larger buffers — now sized for `OutputStage::MAX_FRAMES` (+ regression test `render_tolerates_callback_larger_than_max_frames`); (2) `open_stream_on` opened the new stream before dropping the old one → both callbacks ran concurrently for ~50 ms (OS mixed them: doubled level, duplicated position) — old stream now dropped first; (3) egui never repainted without input, so the peak meter froze and Info notifications never expired — `request_repaint_after(33 ms)` added per plan.md "≥ 30 Hz".
  - **M3 (SC-002, SC-003) PARTIAL** — M3.5 PASS after fix: confirmed-device id missing at launch → system default, no Device Check, Warning raised; the warning named the *fallback* device instead of the missing one (FR-014) — `device_policy::resolve` now names the missing device by its id (only the id is persisted; storing the name too is a settings-schema follow-up). M3.4 PASS (partial): BlackHole nominal rate switched 44.1 k → 48 k → 44.1 k mid-playback via CoreAudio; app stayed Playing, no panic, 440 Hz correct at both rates (output-stage resampling); gap length and clock continuity **not measurable** (the loopback capture restarts on a rate change, and Developer has no clock readout — quickstart M3.1/M3.4 reference a "Developer › clock readout" that does not exist in this slice). M3.1–M3.3 **NOT RUN**: no hot-pluggable (USB/Bluetooth) output device on this machine; covered only by the `#[ignore = "manual"]` counterparts.
  - **M4 (SC-007) PASS after fix** — nav by mouse, by Tab/Enter (focus ring visible), and by `Cmd+1..4`; Developer › Critical/Warning/Info stack newest-first with glyph + severity text, playback uninterrupted, Info gone after 10 s, Critical/Warning stay until Dismiss; Theme = System follows a live OS light↔dark toggle, Theme = Light stays light when the OS toggles; AX tree (System Events / AccessKit) exposes every control with name, role and value (`slider 90 %, -0.9 dB`, `progress indicator Peak meter: -12.9 dBFS`, buttons, nav items) in visual order — nav items surface as `checkbox`. Found & fixed: the floating notification stack had no background, so items rendered on top of (and unreadably through) the Settings category row — each item now sits in `Frame::popup`.
  - **M5 (SC-008) PASS after fix** — the running binary did **not** wire `settings::show` at all: `App` rendered only the Test-output-device button + Developer content (no categories, no search, no Audio/Appearance/Language screens). `App` now owns a `SettingsScreen` and calls `settings::show`. With that: eleven categories in the specified order; typing `ceiling` lists `Audio › Limiter ceiling` immediately and Enter navigates to Audio; device / preset / ceiling / safe-volume enabled / cap all persist across relaunch (`settings.toml` inspected; note `limiter_ceiling_db` serialises as `-3.200000047683716` — f32 → f64 widening, cosmetic); hand-edited `limiter_ceiling_db = 5.0` shows −0.1 dBFS on relaunch; garbage file → defaults + Warning `settings-unreadable`, first settings change rewrites a valid file.
  - Post-fix gates re-run: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace` (127 passed, 1 ignored), `cargo deny check`, `scripts/check-license-headers.sh` — all green.
- [X] T102 [P] Verify `cargo test --doc` passes across every crate with public API doc examples — `cargo test --doc --workspace` exits 0 across all six library crates (`modplayer-audio-io`, `-audio-source`, `-audio-source-synthetic`, `-core`, `-engine`, `-ui`); no doc examples exist yet in any public API, so 0 tests ran and 0 failed

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup (needs crate skeletons from T012). Blocks all user stories.
- **User Stories (Phase 3–7)**: All depend on Foundational (Phase 2) completion. Independent of each other in principle, but this feature's stories build on a shared audio path in priority order (US1 → US2 → US3), while US4/US5 (shell, settings surface) are additive UI layers that can proceed in parallel with US2/US3 once Foundational is done.
- **Polish (Phase 8)**: Depends on all five user stories (the binary in T096 wires the whole stack; T100/T101 validate every story's acceptance criteria at once).

### User Story Dependencies

- **US1 (P1)**: Needs Foundational only. No dependency on other stories.
- **US2 (P2)**: Needs Foundational; reuses `SyntheticSource`/`Processor`/`Controller` scaffolding from Foundational and US1's `CpalBackend`/`FakeBackend` plumbing, but is independently testable via `FakeBackend` without US1's UI.
- **US3 (P3)**: Needs Foundational; extends the `CpalBackend`/`FakeBackend`/`PlaybackController` built in Foundational+US1. Independently testable via `FakeBackend`'s `remove_device`/`set_rate`.
- **US4 (P4)**: Needs Foundational only (shell/notifications/theme are UI-only); does not require US1–US3 to be finished, though it is lower priority because it carries no audio risk.
- **US5 (P5)**: Needs Foundational (`settings/model.rs`, `store.rs`) and benefits from US1/US2/US4's screens existing (Audio/Appearance/Developer controls), but its own registry/search code (T087–T089) has no hard dependency on their UI being done first.

### Within Each User Story

- Tests are written first and must fail before their implementation task closes them out (e.g., T042/T043 before T044–T052).
- Value types → traits → engine pipeline → backend → controller → UI, in that order within a phase.
- A story is complete only when its checkpoint's independent test passes.

### Parallel Opportunities

- All Setup tasks marked [P] (T002–T008, T010, T011, T013) run in parallel once T001 exists.
- Within Foundational, tasks touching different files run in parallel: T016/T017 (traits/osc), T019/T020 (Command/Event), T027/T028/T029 (engine tests), T030/T034/T035/T037 (settings model/notifications/i18n/backend trait), T033/T036/T040 (test files).
- Once Foundational (Phase 2) is done, US1, US4, and the non-UI parts of US5's registry can start in parallel; US2 and US3 should follow US1 since they extend the same `controller.rs`/`cpal_backend.rs` files US1 creates.
- Within each story, tasks marked [P] touch distinct files (e.g., in US2: T063 peak meter, T064 volume widget, T066 Fluent content all parallel to each other and to the test tasks T053–T057).

---

## Parallel Example: User Story 1

```bash
# Tests first (different files):
Task: "Write Device Check integration tests in crates/modplayer-core/tests/device_policy.rs"
Task: "Write TestTone unit tests in crates/modplayer-audio-source-synthetic/src/tone.rs"

# Independent implementation slices once tests exist:
Task: "Implement CpalBackend in crates/modplayer-audio-io/src/cpal_backend.rs"
Task: "Implement the Device Check screen in crates/modplayer-ui/src/device_check.rs"
Task: "Add device-check.ftl Fluent content in locales/en-US/device-check.ftl"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational (critical — blocks every story).
3. Complete Phase 3: User Story 1.
4. **STOP and VALIDATE**: run `cargo test -p modplayer-core --test device_policy` and quickstart M1 manually.
5. This alone proves the walking skeleton's core claim: real audio reaches real hardware.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. + US1 → device check & test tone → demoable (MVP).
3. + US2 → continuous playback, limiter, volume → demoable.
4. + US3 → device resilience → demoable.
5. + US4 → full shell, notifications, theme → demoable.
6. + US5 → searchable settings → feature complete.
7. Phase 8 → binary wired, all CI gates green, quickstart M1–M5 signed off.

### Parallel Team Strategy

With multiple developers, after Foundational:

- Developer A: US1 → US3 (owns `cpal_backend.rs`, `controller.rs`, `device_policy.rs`)
- Developer B: US2 (owns `track.rs`, `now_playing.rs`, engine limiter tests)
- Developer C: US4 → US5 (owns the shell, notifications, theme, settings screens, registry)

---

## Notes

- [P] tasks touch different files with no unmet dependency.
- [Story] labels map every Phase 3–7 task to its user story for traceability back to spec.md.
- Tests are written before the implementation task that makes them pass, per FR-025's "test what the NFRs promise" mandate.
- Real-time-path files (`crates/modplayer-engine/src/processor.rs`, `limiter.rs`, `output_stage.rs`, `shared.rs`) must keep `#![forbid(unsafe_code)]` and never allocate/lock/block/log/I-O inside `render()` — every task touching them carries that constraint implicitly (Constitution Principle I).
- Avoid: vague tasks, two tasks editing the same file marked both [P], any command path into the engine other than the `Command` queue (FR-007).

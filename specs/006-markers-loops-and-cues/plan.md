# Implementation Plan: Markers, Loop Regions, and Cue Points

**Branch**: `feature/006-markers-loops-and-cues` | **Date**: 2026-09-17 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/006-markers-loops-and-cues/spec.md`

## Summary

Let a practicing musician mark a passage, loop it gaplessly and find it
again tomorrow, with no plugin: sample-accurate **markers** (point,
region-start/end, cue) with name, colour and kind; **loop regions** (an
A/B pair with crossfade `0..=50` ms, repeat count, one armed at a time)
whose wrap decision, seam crossfade and repeat evaluation run on the
engine's real-time path and never drift; **cue points** in slots 1–8
that jump instantly without changing play state; and **per-track
persistence** (atomic writes, restored on every load, armed state
session-only). All of it is editable from the Now Playing view by
pointer and keyboard (drag with a detail-view zoom-assist landing within
5 ms; nudge by a configurable step), capped at 64 markers per track with
an inline refusal, with the empty state "No markers — press I to set A".

Technical approach (details in [research.md](research.md)): the loop
lives in `modplayer_engine::Processor` (R1) — five new ≤ 16-byte
`Command`s stage A/B/seam/repeat and commit them atomically at a buffer
boundary (R4); `render` splits each buffer at B, renders a pre-roll
equal-power seam whose incoming frames `[A − x, A)` are read from the
current track's `DecodedStore` through a new provided
`AudioSource::decoded_store()` method (R2, R3), then re-seeks the source
to A exactly at B, so the period is `B − A` frames by construction (0
samples of drift after 1 000 wraps). Wraps and release surface through
`Event`s plus two `RtShared` atomics; the controller re-seeks the
streaming `Player` to A (coalesced, ≤ 4/s) so it follows the loop and
suppresses its `EndOfTrack` while the loop is active (R5). Markers,
regions and cues are a `modplayer-core::markers` module (pure model +
proptested arithmetic, R9) owned by the `PlaybackController` as shadow
state, persisted per track identity as JSON under
`<data_local_dir>/ModPlayer/track-state/<hex(track_id)>.json` (R10, R12),
debounced 250 ms and written by a dedicated writer thread with
`.tmp` + `sync_all` + `rename` (R11). The UI adds a 14 px marker lane
above each waveform (R15), implements 005's promised overlay hook (R16),
a Markers panel, the view-level shortcuts `M`/`I`/`O`/`L`/`1–8`/`Shift+1–8`
(R17), a relative-delta drag that zooms the detail view toward the live
position (R14), an 8-entry palette in `theme.rs` (R13) and the
`[markers] nudge_step_ms` setting in Settings › Playback (R18).

Assumptions taken headlessly (each recorded with its rejected
alternative in Complexity Tracking): the loop is in the engine, not the
source; the seam reads the store via an additive trait method; commands
are staged rather than widening `Command`; the `Player` is re-seeked on
wraps; file names are hex-encoded track ids; loads are synchronous; the
marker lane is a separate strip.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–005

**Primary Dependencies**: existing eframe/egui 0.36 (+accesskit), cpal 0.18, rtrb 0.4, fluent-templates 0.15, serde/serde_json 1, toml 0.9, directories 6, librespot 0.8 + tokio 1 (receiver only), assert_no_alloc 1.1 (engine dev), proptest 1 (dev). **No new runtime dependency**; `proptest` gains `modplayer-engine` as a dev-dependency (research R19/R20)

**Storage**: `<data_local_dir>/ModPlayer/track-state/<hex(track_id)>.json` (override `MODPLAYER_TRACK_STATE_DIR`), one JSON file per track identity (< 8 KiB for 64 markers; files > 64 KiB treated as unreadable), written atomically by a writer thread, unencrypted (no audio, no account data), not deleted on sign-out; `settings.toml` gains `[markers] nudge_step_ms`; no new secure-store entries; the loop's audio comes from 005's in-memory `DecodedStore` (no new cache)

**Testing**: `cargo test --workspace` with `FakeBackend` (001), `SyntheticSource::with_store`/`SyntheticHost` (fills a `Complete` store), `ScriptedHost` + `DecodeScript::Progressive` (uncached seam), injected clocks; `assert_no_alloc` on `Processor::render` with an armed loop; proptest for loop arithmetic, marker model op-sequences, state serialization and the track-id encoding; crash-mid-write test; manual scenarios M1–M16 in [quickstart.md](quickstart.md); CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers) on ubuntu / macos / windows

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical behaviour and key bindings (physical keys, no platform modifiers); platform differences confined to `directories` paths and existing adapter crates

**Project Type**: Desktop application — Cargo workspace stays at 10 crates (no new crate)

**Performance Goals**: loop seam 0-sample drift after 1 000 wraps on cached audio (SC-002); no audible click/gap on cached audio (SC-001); cue jump within NFR-1.1's control-to-audio budget (≤ 20 ms p95 / ≤ 35 ms p99 at the performance preset — the existing 005 seek path, unchanged); marker drag lands ≤ 5 ms (SC-003; the assist reaches ≤ 2.5 ms/px within ~10 frames); nudge exact to the frame (SC-004); per-render seam work ≤ 2 205 store frame reads + trig at 44.1 kHz/50 ms; overlay paint ≤ 64 lines + 1 span per view per frame; state save ≤ 1 per 250 ms, load ≈ 0.1 ms

**Constraints**: real-time path allocation/lock/I/O/log-free — `seam_in` preallocated, store reads are atomics, gains via `f32::sin/cos` (Constitution I); loop decision from `source.position()` only, never UI/timer (FR-009); `Command` stays `Copy` ≤ 16 bytes; raw samples read only inside `modplayer-engine`/`modplayer-audio-source*` (Constitution V guard unchanged); `#![forbid(unsafe_code)]` everywhere; no `unwrap`/`expect` outside tests; 64 markers per track; one armed region; armed state never persisted; every new control keyboard-operable with an accessible name; strings externalised, en-US only; no snapping, sections, export, hot-cue or plugin-owned markers (FR-015, FR-024, FR-025)

**Scale/Scope**: engine loop + commands + events + `loop_math` (~450 LOC incl. tests), trait/source deltas (~120 LOC), core `markers` module (~900 LOC incl. tests) + controller wiring (~250 LOC) + settings (~60 LOC), UI lanes/overlay/panel/drag/keys (~1 100 LOC) + settings screen (~40 LOC), ~90 automated tests, 34 new Fluent keys, single user / one current track / ≤ 64 markers / ≤ 8 cues

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | **Yes** | ✅ PASS | The loop decision, seam render and repeat evaluation run inside `Processor::render`, driven by `source.position()` (the engine's own frame cursor) at buffer boundaries (contracts/engine-loop.md §4). No allocation (`seam_in` sized in `new`), no lock (`DecodedStore::read_frames` is atomics + `OnceLock::get`), no I/O, no log, no plugin call; `assert_no_alloc` extended (`render_with_armed_loop_never_allocates`). Region edits apply only via `LoopCommit` at a boundary; an in-flight seam completes with the old bounds (FR-011a). Every engine PR carries a real-time safety note and the engine maintainer's sign-off (`CODEOWNERS`); the latency (`seek.rs`, `boundary.rs`) and new loop-seam tests are gates. |
| II. Plugins Are Guests | No | N/A | No plugin runtime exists; `owner` is always `Host` (FR-024) and no plugin-facing API is added. The `Owner` enum in the file format only reserves the field. |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | Markers, loop regions, cue points and per-track state are `modplayer-core::markers` (host) and the loop/seam is host Rust in the engine; the seam crossfade is host DSP, no scripting tier involved. |
| IV. Audio Source Is Replaceable and Isolated | **Yes** | ✅ PASS | The only trait change is one *provided* `AudioSource::decoded_store()` method (default `None`) and one additive `SourceCommand::PrefetchHint` in the dependency-free trait crate; `SyntheticSource`/`ScriptedRt` carry a store so engine, core and ui build and test the loop without the receiver (every loop test runs on the synthetic source). The receiver's own change is a two-line getter plus forwarding the hint to the existing decode-ahead `seek_hint`. `single_dependent` guard unchanged. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | **Yes** | ✅ PASS | The engine reads `[A − x, A)` from the store with `read_frames` — a caller inside the set `decoded_store_boundary.rs` already permits — and mixes it into the RT output only. Core/UI never see samples: the marker model holds frame indices, the file holds positions/names/colours. No API, flag or test helper writes decoded audio; the persisted file is not audio and needs no encryption. |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | No credential path touched; the track-state file holds a track id, positions, names and colours — nothing personal, nothing logged; no network from the marker code (the controller's `Seek`/`PrefetchHint` go through the existing source seam); no telemetry. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate; `#![forbid(unsafe_code)]` kept in every touched crate; `thiserror` for `MarkerError`/`StoreError`; no `unwrap`/`expect` outside tests; doc examples on new public items (`TrackMarkers`, `loop_math`, `encode_track_id`); `cargo deny` unchanged (no new dependency); CI matrix unchanged. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Loop-seam sample accuracy (1 000 wraps, 0 samples) and click-free seam tests on the synthetic source (NFR-1.2/1.3); proptests for marker/loop arithmetic (`loop_math`, `markers_model`) and state serialization (`markers_store`); crash-mid-write persistence test (NFR-2.8); `assert_no_alloc` on the RT delta; controller wiring tests; manual scenarios M1–M16 executed by the implementing agent (Governance › Manual Scenario Sign-Off). Criterion/soak remain 001's engine obligations; the loop adds no unbounded growth (fixed `seam_in`). |
| IX. One Plugin API Definition | No | N/A | No plugin API in this slice. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No new trait (one provided method on an existing multi-implementor trait), no feature flag, no new crate (`markers` is a core module like queue/library/analysis until plugins become a second consumer); `Command` kept ≤ 16 bytes by staging instead of a wider variant; bindings are physical keys identical on all three platforms; every action keyboard-operable with an accessible name; strings externalised (en-US as 001–005). User override N/A (no plugins). |
| Governance: engine/gateway/runtime sign-off | **Yes** | ✅ PASS (process) | `crates/modplayer-engine/` changes (processor, command, event, shared, loop_math) require the engine maintainer's review per `CODEOWNERS` with a real-time safety note in the PR; the receiver's `rt.rs`/`worker.rs` delta is the receiver maintainer's review. Requirement IDs (FR-, SC-, EC-, NFR-, DM-) referenced throughout spec, plan, contracts and tests. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no `unsafe`, no feature flag, no trait, no crate, no runtime dependency; the RT delta is allocation-free by construction and covered by `assert_no_alloc`; the raw-sample boundary is unchanged and mechanically enforced; the headless assumptions (Complexity Tracking) are all outside the non-negotiable principles.

## Project Structure

### Documentation (this feature)

```text
specs/006-markers-loops-and-cues/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R20, open verifications
├── data-model.md        # Phase 1: core model, RT copy, UI state, file format, settings, notifications, state machines
├── quickstart.md        # Phase 1: automated gates (named tests), seam harness, manual scenarios M1–M16
├── contracts/
│   ├── engine-loop.md        # AudioSource/SourceCommand delta, Command/Event/RtShared delta, RT rules, loop_math, tests
│   ├── marker-service.md     # PlaybackController marker API, engine derivation, per-track store, controller wiring, settings, notifications, tests
│   └── ui-markers.md         # Now Playing lanes/overlay/panel, shortcuts, focus/drag, waveform module delta, theme, Fluent keys, tests
├── checklists/requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

Existing layout (001–005) is kept; `+` marks new files, `~` modified files.

```text
Cargo.toml                                   (unchanged — no new workspace dependency)
locales/en-US/
├── playback.ftl                             ~ + 32 marker/loop/cue/track-state keys (contracts/ui-markers.md §8)
└── settings.ftl                             ~ + setting-nudge-step, setting-nudge-step-desc
crates/
├── modplayer-audio-source/                  ~ trait crate (still dependency-free)
│   ├── src/lib.rs                           ~ AudioSource::decoded_store() provided method (default None)
│   └── src/types.rs                         ~ SourceCommand::PrefetchHint { frame }
├── modplayer-audio-source-synthetic/
│   ├── src/lib.rs                           ~ SyntheticSource { store: Option<Arc<DecodedStore>> }, with_store(); decoded_store()
│   ├── src/host.rs                          ~ attach() installs the host's Complete store; PrefetchHint ignored
│   └── src/scripted.rs                      ~ ScriptedRt::decoded_store(); PrefetchHint recorded/ignored
├── modplayer-audio-source-connect/
│   ├── src/rt.rs                            ~ decoded_store() → self.store.as_ref()
│   ├── src/worker.rs                        ~ PrefetchHint → decode_ahead.seek_hint(frame)
│   └── tests/{rt_feed.rs ~, worker/markers tests ~}
├── modplayer-engine/                        ~ contracts/engine-loop.md (engine maintainer sign-off)
│   ├── Cargo.toml                           ~ [dev-dependencies] proptest
│   ├── src/lib.rs                           ~ pub mod loop_math; re-exports
│   ├── src/loop_math.rs                     + effective_crossfade, wrap_position, crossfade_gains (+ proptests)
│   ├── src/command.rs                       ~ LoopSetA, LoopSetB, LoopSetSeam, LoopCommit, LoopDisarm (≤ 16 bytes)
│   ├── src/event.rs                         ~ LoopWrapped { wraps, gapless }, LoopReleased { wraps }
│   ├── src/shared.rs                        ~ loop_wraps: AtomicU32, loop_state: AtomicU8 (+ accessors)
│   ├── src/processor.rs                     ~ LoopRt/SeamRt, staged/active, segmented render, seam mix, jump, published position
│   └── tests/{loop_seam.rs +, realtime.rs ~}
├── modplayer-core/
│   ├── src/lib.rs                           ~ pub mod markers; re-exports
│   ├── src/markers/mod.rs                   + MarkerError, re-exports, MAX_MARKERS, constants
│   ├── src/markers/model.rs                 + MarkerId, MarkerKind, Marker, LoopRegion, RepeatCount, CueSlot, PaletteIndex, TrackMarkers
│   ├── src/markers/store.rs                 + TrackStatePaths, TRACK_STATE_DIR_ENV, encode/decode_track_id, DTOs, load/encode, spawn_writer, PersistJob, StoreEvent
│   ├── src/controller.rs                    ~ marker API (contracts/marker-service.md §1), engine derivation, sync_marker_attachment, event_rx drain, re-seek throttle, EndOfTrack suppression, flush/shutdown, rebuild re-push
│   ├── src/settings/model.rs                ~ RawMarkers { nudge_step_ms }, AudioSettings.nudge_step_ms
│   ├── src/settings_registry.rs             ~ setting-nudge-step descriptor (Playback)
│   ├── src/notifications.rs                 ~ KEY_TRACK_STATE_UNREADABLE / _NEWER_VERSION / _SAVE_FAILED
│   └── tests/{markers_model.rs +, markers_store.rs +, controller_markers.rs +, settings.rs ~}
├── modplayer-ui/
│   ├── src/lib.rs                           ~ pub mod markers
│   ├── src/markers.rs                       + lanes (glyphs, focus, drag), overlay painter, Markers panel, shortcut handling, status line
│   ├── src/now_playing.rs                   ~ lanes + overlays around both waveforms; panel between detail and volume; shortcut guard; empty state
│   ├── src/waveform/mod.rs                  ~ overlays hook parameter on overview()/detail()
│   ├── src/waveform/paint.rs                ~ playhead split into paint::playhead (drawn after overlays)
│   ├── src/waveform/state.rs                ~ DetailWindow::zoom_assist; WaveformState { marker_drag, focused_marker, rename, clear_confirm, text_field_ids }
│   ├── src/waveform/input.rs                ~ marker-focused key table helper (arrows/Delete/F2/Enter/C/Esc)
│   ├── src/theme.rs                         ~ MARKER_PALETTE, marker_color()
│   ├── src/settings/playback.rs             ~ nudge-step DragValue
│   └── tests/{markers.rs +, now_playing.rs ~, accessibility.rs ~, fluent_keys.rs ~, waveform.rs ~}
└── modplayer/
    └── tests/{decoded_store_boundary.rs (unchanged, must stay green), single_dependent.rs (unchanged)}
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
(one crate per architectural component, Constitution VII) and add **no
crate**. The loop goes into `crates/modplayer-engine` because it is
real-time audio behaviour the constitution assigns to the engine (I,
III; FR-6.1) and must work for every `AudioSource`. The one trait change
goes into `crates/modplayer-audio-source` (the dependency-free trait
crate that already owns `DecodedStore`). The marker model and per-track
store are a module of `crates/modplayer-core` (core services), like
003's queue, 004's library and 005's analysis: a `modplayer-markers`
crate would fail Constitution X's "why is the existing crate
insufficient" test until plugins become a second consumer. UI code lives
in `crates/modplayer-ui/src/markers.rs` beside `now_playing.rs` and the
`waveform/` module it extends. The dependency graph is unchanged from
005: `modplayer → {ui, core, audio-io, account, secure-store, audio-source-connect}`;
`ui → {core, audio-io, engine, account, secure-store, audio-source}`;
`core → {engine, audio-io, audio-source, audio-source-synthetic}`;
`engine → {audio-source, audio-source-synthetic}`; the trait crate stays
dependency-free. Locale files stay under `locales/en-US/`.

## Design notes that tasks must respect

1. **The engine decides, the controller mirrors** (R1, R5): the RT copy
   of the region is derived from the core model; `LoopReleased` and the
   `loop_state`/`loop_wraps` atomics flow back; the UI never computes a
   wrap.
2. **Setters then commit, always in that order** (R4): a half-applied
   region is impossible because only `LoopCommit` swaps `loop_active`.
3. **Seam captured at its start** (R3, FR-011a): `SeamRt` freezes
   `a`/`b`/`x` for the seam in flight; the store read for `[A − x, A)`
   happens once, at the seam's first frame; a short read → hard cut +
   `gapless = false`.
4. **Jump exactly at `pos == b`**, never earlier or later, whatever the
   buffer size or device rate — the period proof depends on it.
5. **`Command::Seek` clears the carry; a loop wrap does not** (R8).
6. **The `Player` follows the loop** (R5): one coalesced
   `SourceCommand::Seek(A)` per tick with a wrap, throttled to ≥ 250 ms
   after the first; `EndOfTrack` ignored while `loop_state == 2`.
7. **Every track change disarms** (`LoopDisarm` + model), including a
   same-identity restart; every load restores markers/regions/cues
   disarmed (FR-016).
8. **Persistence: whole state, debounced 250 ms, writer thread, atomic
   rename; loads synchronous and size-capped** (R11, R12).
9. **File names are hex track ids** (R10) — reversible and case-free.
10. **Marker lane, not overlapping widgets** (R15); marker lines and the
    span go through the overlay hook painted before the playhead (R16).
11. **Relative-delta drag in detail space** (R14): never map the
    pointer's absolute x through a window that is recentring on it.
12. **Shortcut guard is "no text field of this view has focus"** (R17),
    not `wants_keyboard_input`.
13. **Theme**: marker colours only from `theme::MARKER_PALETTE`; span,
    hatch and focus highlight from existing visuals.
14. **Default marker names are externalised** (`marker-default-name`
    via `tr_args`) even though the model stores the resolved string.
15. **No disk on the RT path, ever; no disk on the UI thread except the
    bounded synchronous load at track change** (R11).

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. The headless assumptions and spec-level
decisions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Loop evaluated in `modplayer_engine::Processor`, not in each `AudioSource` (R1) | Constitution I/III and FR-6.1 assign loop evaluation to the engine clock; one implementation serves the synthetic and Connect sources alike. | Per-source loops — duplicated logic, the synthetic source (the only test source, Constitution IV) could not exercise the seam. |
| Additive provided method `AudioSource::decoded_store()` (R2) | The seam needs `[A − x, A)` while the source still delivers `[B − x, B)`; only the retained store gives sample-exact random access on the RT without a second decoder. | Passing the `Arc` through `Command` — not `Copy`, and dropping on the RT is forbidden (005 R1); a render-history ring — cannot supply frames before A that were never rendered (seek to A, A = 0). |
| Five staged `Command`s + `LoopCommit` instead of one 24-byte `SetLoop` (R4) | Keeps 001's `Command ≤ 16 bytes, Copy` invariant and gives an atomic buffer-boundary swap for FR-011a. | Widening `Command` — a contract change with a `const` assertion, and a single wide command would still need a "reset wraps or not" flag. |
| Controller re-seeks the streaming `Player` on wraps (coalesced, ≤ 4/s) and ignores `EndOfTrack` while the loop is active (R5) | Without it the `Player` keeps decoding past B, reaches its own end of track and the queue advances mid-loop; on the ring feed the ring must refill from A. | No re-seek — breaks loops longer than the remaining track; per-wrap re-seek — 1 000 librespot seeks/s for a 1 ms region. |
| Per-track file name = lowercase hex of the `TrackId` bytes (R10) | FR-016 asks for a filesystem-safe, *reversible* encoding; base62 ids are case-sensitive but default macOS/Windows filesystems are not. | Percent-encoding — case aliasing; sha256 (005's cache) — irreversible; one map file — a crash mid-write risks every track's state (NFR-2.8). |
| Synchronous load at track change; writes on a dedicated writer thread (R11) | The state must be in place before the track is treated as ready (spec assumption on FR-11.3.2); the file is ≤ 64 KiB (≈ 0.1 ms); writes still never block a frame. | Async load — needs a "markers pending" UI state for a sub-millisecond read; reusing `library::persist::PersistJob` — library-specific enum and debounce. |
| Marker lanes (14 px strips) above each waveform instead of glyphs inside the waveform rect (R15) | Two overlapping `click_and_drag` widgets make hit-testing ambiguous; lanes give each glyph an unambiguous accessible node and keep 005's widget untouched. | Glyphs inside the rect — 005's seek surface and the glyph would fight for the drag; hit-test order would depend on egui internals. |
| Relative-delta drag with animated zoom-assist (R14) | An absolute mapping feeds back on itself once the detail window recentres on the live position; relative deltas at the detail's fpp reach ≤ 2.5 ms/px from either waveform. | Absolute mapping — a still pointer keeps sliding the marker; a modifier "fine mode" — a gesture the spec does not define. |
| Repeat count `0` in `LoopSetSeam` means infinite (R4) | Keeps the payload at two `u32`s and avoids an `Option` in a `Copy` command. | `Option<u32>` — bigger niche-free layout, no benefit. |
| Fixed 8-colour literal palette in `theme.rs` (R13) | The spec fixes an index palette stable across theme changes; egui's `Visuals` offers one accent, not eight. | Deriving from `Visuals` — colours would change with the theme and cannot yield 8 distinguishable hues. |

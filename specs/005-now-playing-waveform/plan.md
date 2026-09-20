# Implementation Plan: Now-Playing View with Waveform

**Branch**: `005-now-playing-waveform` | **Date**: 2026-09-17 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/005-now-playing-waveform/spec.md`

## Summary

Give the user a now-playing view they can navigate by sight: artwork,
title, artists, album, elapsed / remaining time, a whole-track **waveform
overview** (which replaces 003's seek slider and inherits its
`transport-seek` accessibility contract) and a zoomable **waveform
detail** around the playhead that follows playback. The waveform is
computed locally by a background **Analysis Service** (AR-11, waveform
path only) from a new **retained decoded store** of the current track,
fills in progressively while a track streams (placeholders for undecoded
regions), is cached per track identity as a four-level mono 8-bit peak
ladder versioned by `ANALYZER_VERSION`, and reappears instantly on
replay. Clicking or dragging either waveform seeks — sample-accurately
when the target is in the decoded store, within 50 ms otherwise — and
every pointer action has a keyboard equivalent. Analysis failure
(silence, corrupt stream) marks the track "analysis unavailable" without
touching playback. Nothing loaded → a pick-a-track hint. The view exposes
a track-time coordinate space that later slices (markers, loops, plugin
overlays) attach to.

Technical approach (details in [research.md](research.md)): a lock-free,
safe-Rust `DecodedStore` (chunked `OnceLock<Box<[AtomicU32]>>`, f32 bit
patterns, ≤ 256 MiB) in the trait crate (R1) is filled by a second,
independent **decode-ahead** thread in the receiver that drives
`symphonia` directly over its own `AudioFile` at connection speed (R3),
and read by `ConnectRtSource`, which now plays from the store after a
covered seek and from the librespot ring otherwise — the feed is chosen
only at restart points so 003's gapless transitions and Connect state
are untouched (R2). The Analysis Service is a module of `modplayer-core`
with one below-normal-priority thread folding peaks via the store's lossy
`fold_peaks` (never raw samples — Constitution V by construction, R5),
publishing immutable `Arc<AnalysisSnapshot>`s over `mpsc` (R6) and
persisting only `Complete` results as a sectioned binary `.mpwf` file
under `<data_local_dir>/ModPlayer/analysis/` (R7). The reducer gains an
exact-frame seek (R8). The UI adds a `waveform` module with a pure
`DetailWindow`/`TimeSpace` (R11–R13). No engine-crate change.

Assumptions taken headlessly (all in Complexity Tracking or research):
the encrypted file is fetched and decoded twice (Player + decode-ahead)
because librespot's `Player` cannot decode ahead of its sink (R3); the
10 ms seek-latency budget is met at the Performance/Balanced presets and
bounded by one buffer period (23 ms) at Safe (R8); `thread-priority` is
added for below-normal priority (R9); the analysis cache is not deleted
on sign-out (R7).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–004

**Primary Dependencies**: existing eframe/egui 0.36 (+accesskit), cpal 0.18, rtrb 0.4, fluent-templates 0.15, librespot-{core,connect,playback,audio,metadata} 0.8 + tokio 1 (receiver only), serde/serde_json, directories 6, sha2 0.10, time 0.3, proptest 1 (dev); **new**: `thread-priority` (MIT) in `modplayer-core` and `modplayer-audio-source-connect`; `symphonia` 0.5 (already resolved via librespot-playback; features `ogg`, `vorbis`, `mp3`) as a direct dependency of the receiver crate; `sha2` (workspace) gains `modplayer-core` as a user; `proptest` (dev) gains `modplayer-ui` — research R16

**Storage**: `<data_local_dir>/ModPlayer/analysis/<sha256(track_id)>.mpwf` (override `MODPLAYER_ANALYSIS_DIR`), one sectioned little-endian binary file per track, ≈ 472 KB for 10 minutes, written atomically (temp + rename, user-only permissions) **only for `Complete`**, no eviction in this slice, unencrypted (peaks are lossy, non-reversible — FR-008); decoded PCM of the current track in memory only (≤ 256 MiB, released on track change); the decode-ahead's AES-encrypted temp file in 003's private temp dir; no new settings, no new secure-store entries

**Testing**: `cargo test --workspace` with `FakeBackend` (001), `ScriptedHost` (003/004, extended with `DecodeScript`), `SyntheticHost` (fills a store), injected clocks; `assert_no_alloc` on `ConnectRtSource::fill/seek` with a store attached; proptest for the coordinate space, the `.mpwf` round trip and the store's write/read identity; the binary crate's source guard for `read_frames` callers (Constitution V) and the existing `single_dependent` guard (Constitution IV); one live `#[ignore = "manual"]` probe; manual scenarios M1–M15 in [quickstart.md](quickstart.md); CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers) on ubuntu / macos / windows

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical behaviour and bindings; platform differences confined to `thread-priority`'s own adapters, `directories` paths and the existing adapter crates

**Project Type**: Desktop application — Cargo workspace stays at 10 crates (no new crate)

**Performance Goals**: first-play waveform starts filling ≤ 1 s (SC-001) and lags decode by ≤ 2 s (SC-002); `Complete` ≤ 30 s at 10 Mbit/s for a 160 kbps track (SC-011; two fetches ≈ 19 s for 10 minutes); covered click-seek sample-exact with the first new sample handed to the callback ≤ 10 ms at the default preset (SC-003); cached waveform visible ≤ 200 ms (SC-004); playhead redrawn every frame, labels ≥ 4 Hz; waveform paint ≤ 1 ms per view per frame (≤ 1 000 columns from a pre-folded level); zero measurable RT impact from analysis (SC-009)

**Constraints**: real-time path allocation/lock/I/O-free — the store's read side is atomics + `OnceLock::get` only, and the RT never drops the last `Arc` (Constitution I); raw samples readable only by `modplayer-audio-source*`/`modplayer-engine` (guard test; Constitution V, FR-021); `#![forbid(unsafe_code)]` everywhere (no raw pointers in the store); no `unwrap`/`expect` outside tests; only the current track's PCM retained (NFR-2.7); analysis/decode-ahead threads at below-normal priority; every new control keyboard-operable with an accessible name; strings externalised, en-US only; no markers/loops/overlays/re-analyze/detached window (FR-020)

**Scale/Scope**: trait-crate store (~350 LOC incl. tests), receiver decode-ahead + RT feed rules (~700 LOC), core analysis module (~900 LOC incl. cache + proptests), transport delta (~80 LOC), UI waveform module + Now Playing rewrite (~1 100 LOC), scripted/synthetic host deltas (~200 LOC), ~60 automated tests, 8 new Fluent keys, single user / one current track / one analysis thread

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | **Yes** | ✅ PASS | The only new RT code is `ConnectRtSource`'s feed logic (data-model.md §2.2): atomic loads, `OnceLock::get`, ring pops — no alloc/lock/I/O/log; `assert_no_alloc` extended (`rt_no_alloc` with a store attached). The RT never frees a store: it never drops a store `Arc` at all — every replaced store is *moved* into a bounded `rtrb` retirement ring (capacity `MARKER_CAPACITY + 1`, so `push` cannot fail) and dropped on the worker thread, whatever order the host/analysis/decode-ahead release theirs (research R1; `store_drop_never_on_rt` covers three consecutive track changes, `rt_retire_never_full` the capacity bound). The decode-ahead and analysis threads never touch the callback, its queues, or the engine (contracts A11). Engine crate unchanged — no engine PR. Latency/loop-seam tests untouched. |
| II. Plugins Are Guests | No | N/A | No plugin runtime exists yet; the `TimeSpace` overlay hook (contracts/ui-waveform.md §6) is a host-internal seam, not a plugin API. |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | The coordinate space, playhead, detail window and analysis are host primitives in `modplayer-ui`/`modplayer-core`; peak folding is host Rust. No DSP added; no scripting. |
| IV. Audio Source Is Replaceable and Isolated | **Yes** | ✅ PASS | `DecodedStore` and the additive `SourceEvent::DecodedStore` live in the dependency-free trait crate; `SyntheticHost`/`ScriptedHost` fill stores so core/ui/engine build and test without the receiver (research R14). `symphonia` and `thread-priority` are added to the receiver only for protocol-side decode (`single_dependent` guard extended); core's `thread-priority` use is protocol-free. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | **Yes** | ✅ PASS | Decoded PCM lives in `DecodedStore`, whose raw accessor `read_frames` is callable only from the source/engine crates (binary guard test `decoded_store_boundary.rs`); analysis uses the lossy `fold_peaks` (mono 8-bit min/max — not reconstructable); the cache stores peaks only (FR-008); `Debug` impls redact; the second temp file holds AES-encrypted bytes only (as 003 R6). No API, flag or test helper writes decoded audio anywhere. |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | The decode-ahead reuses the receiver's existing `Session` (no new credential path, token never logged — 003's `credential_leak` test extended to the new module); the cache file contains a track id and peaks, nothing personal; no telemetry; no network from core/ui. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate; `#![forbid(unsafe_code)]` kept in every touched crate (the store uses `OnceLock`/atomics, no raw pointers); `thiserror` for `CacheError`/`DecodeAheadError`; no `unwrap`/`expect` outside tests; doc examples on public items; `cargo deny` for `thread-priority` (MIT) and `symphonia` (MPL-2.0, already allowed via librespot-playback); CI matrix unchanged. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Named tests per FR/SC in quickstart.md; proptests for the coordinate space (marker/loop arithmetic precursor), the `.mpwf` state serialization and the store identity; `assert_no_alloc` on the RT delta; scripted-host coverage for progressive fill, failure, silence, cache; manual scenarios M1–M15 executed by the implementing agent (Governance › Manual Scenario Sign-Off). Criterion/soak remain 001's engine obligations (engine unchanged); memory bound covered by `store_cap_never_allocates_past_bound` and M13. |
| IX. One Plugin API Definition | No | N/A | No plugin API in this slice. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No new trait (the store is a struct; `SourceHost` gains no method — one additive event variant); no feature flags; no new crate (analysis is a `modplayer-core` module until a second consumer exists); each dependency justified in research R16; bindings and behaviour identical on all three platforms; every control keyboard-operable with an accessible name; strings externalised (en-US as 001–004). User override N/A (no plugins). |
| Governance: engine/gateway/runtime sign-off | No | N/A | No change under `crates/modplayer-engine/`. The receiver's RT half changes (`rt.rs`) are the receiver maintainer's review per `CODEOWNERS`, with a real-time safety note in the PR description. Requirement IDs (FR-, SC-, EC-, NFR-, AR-, DM-) referenced throughout. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no `unsafe`, no feature flag, no trait, no crate; the raw-sample boundary is mechanically enforced; the one spec-level deviation forced by librespot (double fetch/decode) and the Safe-preset latency bound are outside the non-negotiable principles and recorded in Complexity Tracking with rejected alternatives.

## Project Structure

### Documentation (this feature)

```text
specs/005-now-playing-waveform/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R16, dependency justification, open verifications
├── data-model.md        # Phase 1: DecodedStore, RT feed state, Analysis types, transport delta, UI state, .mpwf format
├── quickstart.md        # Phase 1: automated gates (named tests), live probe, manual scenarios M1–M15
├── contracts/
│   ├── decoded-store.md          # DecodedStore API, writer/reader rules, Constitution V boundary, host behaviours
│   ├── connect-source-delta.md   # decode-ahead thread, marker/event delta, ConnectRtSource feed guarantees, live probe
│   ├── analysis-service.md       # AnalysisService/Snapshot/cache API, rules A1–A13, tests
│   ├── transport-delta.md        # seek_frames, reducer delta, controller ↔ analysis wiring
│   └── ui-waveform.md            # Now Playing layout, pointer/keyboard tables, rendering, follow, TimeSpace hook, tests
├── checklists/requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

Existing layout (001–004) is kept; `+` marks new files, `~` modified files.

```text
Cargo.toml                                   ~ workspace deps: thread-priority; symphonia 0.5 (ogg, vorbis, mp3) for the receiver
deny.toml                                    ~ licence/advisory notes if surfaced by thread-priority
locales/en-US/
└── playback.ftl                             ~ + now-playing-pick-a-track, now-playing-album, now-playing-artwork, waveform-unavailable, waveform-detail, waveform-detail-window, waveform-overview-desc, time-elapsed, time-remaining; − transport-position
crates/
├── modplayer-audio-source/                  ~ trait crate (still dependency-free)
│   ├── src/lib.rs                           ~ pub mod decoded; re-exports DecodedStore, StoreState, PeakBucket
│   ├── src/decoded.rs                       + DecodedStore (chunks, atomics, write/read/fold_peaks), CHUNK_FRAMES, MAX_STORE_FRAMES
│   └── src/types.rs                         ~ SourceEvent::DecodedStore { track, store } (PartialEq = ptr_eq, Debug redacted)
├── modplayer-audio-source-synthetic/
│   ├── src/host.rs                          ~ SyntheticHost builds + fills a store on Initialize; emits DecodedStore
│   └── src/scripted.rs                      ~ DecodeScript { Instant, Progressive, FailAt, Silent, None }; handle.script_decode; progressive fill in poll()
├── modplayer-audio-source-connect/          ~ contracts/connect-source-delta.md
│   ├── Cargo.toml                           ~ + symphonia, thread-priority
│   ├── src/decode_ahead.rs                  + DecodeAhead: format pick, AudioFile/AudioDecrypt/Subfile, symphonia loop, seek hints, stop, force-fail toggle
│   ├── src/subfile.rs                       + Subfile<R>: Read + Seek + symphonia MediaSource (0xa7 offset)
│   ├── src/program.rs                       ~ MarkerKind::TrackStart { store }; Marker no longer Copy
│   ├── src/rt.rs                            ~ cursor/feed/ring_pos/store/retired/parked; feed rules (data-model.md §2.2); replaced store moved into the retirement ring, never dropped
│   ├── src/events.rs                        ~ TrackChanged → store creation + marker with store
│   ├── src/worker.rs                        ~ spawn/stop decode-ahead per track; seek hints; DecodedStore event; Shutdown join; drain_retired before every marker push
│   ├── src/lib.rs                           ~ RETIRED_CAPACITY + retirement ring creation; current_store only; BufferStatus.current_prefetched from store state
│   └── tests/{rt_feed.rs +, rt_no_alloc.rs ~, markers.rs ~, live.rs ~}
├── modplayer-core/
│   ├── Cargo.toml                           ~ + thread-priority, sha2 (workspace)
│   ├── src/lib.rs                           ~ pub mod analysis; re-exports
│   ├── src/analysis/mod.rs                  + ANALYZER_VERSION, AnalysisStatus, WaveformPeaks, PeakLevel, AnalysisSnapshot, AnalysisService (channels, attach/detach/drain)
│   ├── src/analysis/peaks.rs                + LADDER, level folding, present bitmaps, silence test
│   ├── src/analysis/cache.rs                + AnalysisPaths, ANALYSIS_DIR_ENV, encode/decode/load/store, CacheError
│   ├── src/analysis/worker.rs               + the `analysis` thread loop (rules A1–A13)
│   ├── src/transport.rs                     ~ Input::Seek / Effect::SeekTo gain position_frames
│   ├── src/controller.rs                    ~ seek_frames; analysis wiring (attach/attach_store/detach/drain/shutdown); analysis() accessor
│   └── tests/{analysis.rs +, transport_reducer.rs ~, controller_streaming.rs ~}
├── modplayer-ui/
│   ├── Cargo.toml                           ~ [dev-dependencies] proptest
│   ├── src/lib.rs                           ~ pub mod waveform
│   ├── src/waveform/mod.rs                  + overview(), detail(), WaveformResponse, WaveformState, DragPreview
│   ├── src/waveform/coords.rs               + TimeSpace (x_of, frame_at, frames_per_pixel, visible_window)
│   ├── src/waveform/state.rs                + DetailWindow (initial, zoom_about, zoom_step, reset, pan, follow_playhead, recenter, suspend_follow_if_outside)
│   ├── src/waveform/paint.rs                + column folding from a PeakLevel, placeholder band, playhead, highlight, "analysis unavailable" label
│   ├── src/waveform/input.rs                + pointer (click/drag/Esc/wheel/pinch) and keyboard table handling
│   ├── src/now_playing.rs                   ~ artwork/title/artists/album, elapsed/remaining labels, both waveforms; seek slider + position label removed; empty-state hint
│   ├── src/app.rs                           ~ owns WaveformState (session)
│   └── tests/{now_playing.rs ~, accessibility.rs ~, fluent_keys.rs ~, waveform.rs +}
└── modplayer/
    └── tests/{decoded_store_boundary.rs +, single_dependent.rs ~}
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
(one crate per architectural component, Constitution VII) and add **no
crate**. The retained decoded store goes into the existing trait crate
`crates/modplayer-audio-source` because every source implementor must
be able to fill it and the receiver's RT half must read it without a
dependency on any host crate. The decode-ahead thread goes into
`crates/modplayer-audio-source-connect` because it speaks the protocol
(`AudioFile`, audio keys) — Constitution IV's sole protocol crate. The
Analysis Service is a module of `crates/modplayer-core` (core services,
Part 9 §3), like 003's queue/transport and 004's library: a separate
crate would fail Constitution X's "why is the existing crate
insufficient" test until a second consumer (beat grid, plugins) exists.
The waveform widgets live in `crates/modplayer-ui/src/waveform/` next to
`now_playing.rs`. The dependency graph stays the strict DAG of 003/004:
`modplayer → {ui, core, audio-io, account, secure-store, audio-source-connect}`;
`ui → {core, audio-io, engine, account, secure-store, audio-source}`;
`core → {engine, audio-io, audio-source, audio-source-synthetic}`;
`audio-source-connect → {audio-source, librespot-*, symphonia, tokio, rtrb, thread-priority}`;
`engine → {audio-source, audio-source-synthetic}`; the trait crate
remains dependency-free. Locale files stay under `locales/en-US/`.

## Design notes that tasks must respect

1. **Feed switches only at restart points** (research R2): `seek()` and
   `TrackStart` choose the feed; the two rescue transitions (ring
   underrun → store, store exhaustion → ring) are the only mid-play
   switches, and both happen where audio is already discontinuous.
2. **The RT never frees a store**: `ConnectRtSource` holds one store
   `Arc` and never drops it — a `TrackStart` moves the replaced `Arc`
   into the retirement ring (`RETIRED_CAPACITY = MARKER_CAPACITY + 1`);
   the worker drains that ring before every marker push, so the push
   cannot fail and every last-reference drop lands on the worker thread.
   No host-side previous-store retention, no `track_seq` release rule
   (R1, contracts/connect-source-delta.md §2).
3. **Chunk-aligned writes**: the decode-ahead discards frames before a
   chunk boundary after every `FormatReader::seek`, so every chunk is
   valid from its first frame (contracts/decoded-store.md rule 1).
4. **Raw samples stay inside the source/engine crates**: only
   `fold_peaks`/`coverage`/`covers` are used by core/ui; the binary's
   guard test fails the build otherwise (Constitution V).
5. **Order of events per track**: marker with store → `TrackStarted` (or
   `BecameActive`) → `SourceEvent::DecodedStore`. The controller attaches
   analysis on `TrackStarted` (cache lookup first) and adds the store
   when it arrives; a store for a non-current track is ignored.
6. **Only `Complete` reaches disk**; `Failed` is per-session memory;
   silence is decided only after `Complete` coverage; `Detach` writes
   nothing (contracts/analysis-service.md A7–A9).
7. **Exact frames end-to-end**: UI `frame_at(x)` → `seek_frames(frame)`
   → `Effect::SeekTo { position_frames: Some }` → `Command::Seek(frame)`;
   ms only for `SourceCommand::Seek` and cluster reporting (R8).
8. **No disk or network on the UI thread**: cache load/store run on the
   analysis thread; the decode-ahead runs on its own thread; `tick()`
   only drains channels.
9. **Thread priority failures are non-fatal**: log and continue (R9).
10. **egui widget info**: both waveforms report `WidgetInfo::slider`
    so accesskit exposes role slider + value text; the detail view's
    window bounds go into its description (contracts/ui-waveform.md §1).
11. **Keys consumed only with waveform focus**: 003's `Space`,
    `Cmd/Ctrl+Q`, queue-row shortcuts must keep working unchanged.
12. **Theme tokens only** for the waveform, placeholder, highlight and
    playhead colours (`theme.rs`); no new literals.
13. **Debug-only toggle** `MODPLAYER_DECODE_FORCE_FAIL` mirrors 004's
    `MODPLAYER_ARTWORK_FORCE_FAIL` pattern: read once, receiver only,
    never affects the `Player`.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. The following spec-level deviations and
headless assumptions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| The encrypted file is fetched **twice** and decoded **twice** per track (librespot `Player` + decode-ahead; research R3) | librespot's `Player` decodes only as fast as its sink drains (pacing is what keeps Spirc's cluster state, gapless preload and end-of-track advance correct), and its `AudioFileStreaming` is private to it. FR-021 needs decoding at connection speed; the only public path is a second `AudioFile::open`. Bandwidth stays inside SC-011 (≈ 19 s for a 10-minute 160 kbps track at 10 Mbit/s); decode CPU is negligible at below-normal priority. | Unbounded sink → `Player` finishes the track in seconds and Spirc moves on while the user listens; pausing the `Player` when ahead → `Paused` events flip cluster state; reading the `Player`'s temp file → private download map; librespot `Cache` → only complete files, fails SC-001's 1 s; forking librespot → `unknown-git = "deny"`, permanent merge burden (003 R6). |
| Seek response latency is bounded by one buffer period: ≤ 10 ms at Performance/Balanced, ≤ 23 ms at the Safe preset (research R8) | Engine commands apply at buffer boundaries (001 FR-025b, Constitution I: no mid-buffer parameter changes). SC-003 is verified at the default (Balanced) preset. | Mid-buffer seek application — contradicts Constitution I's boundary rule and 001's contract; forcing ≤ 441-frame buffers — removes the Safe preset. |
| New dependency `thread-priority` (research R9) | `std` has no thread-priority API; FR-003's below-normal priority is a MUST. Cross-platform, MIT, confines platform calls. | Hand-written FFI — `unsafe` in non-listed crates (Constitution VII); ignoring the MUST. |
| `symphonia` used directly in the receiver instead of librespot's `SymphoniaDecoder` (research R3) | Exact `SeekedTo::actual_ts` / packet `ts` are needed for a sample-exact store; librespot's wrapper truncates to ms. Already in the tree with identical features — no new transitive surface. | librespot's wrapper + ms positions — ≤ 22-frame misplacement at chunk joins after seeks, breaking sample accuracy. |
| Decode-ahead follows host seeks (re-points to the target chunk) rather than decoding strictly sequentially (research R3) | Makes the neighbourhood of a seek-ahead decodable (and its waveform visible) within a few seconds instead of after the whole prefix; FR-005 explicitly anticipates multi-gap coverage. | Sequential only — simpler, but a seek-ahead on a slow connection leaves the playhead region undecoded (no sample-accurate seeks, hatched waveform under the playhead) until the whole prefix downloads. |
| Analysis Service is a `modplayer-core` module, not a crate; `ANALYZER_VERSION` lives there (the spec says "analysis crate") | Constitution X: no crate without a stated reason an existing one is insufficient; there is no second consumer yet. The constant's semantics (single integer, written to every entry, compared on read) are unchanged. | A `modplayer-analysis` crate — premature; revisit when beat grid / key / loudness or plugins need it. |
| Cache entries are not deleted on sign-out; no eviction (research R7) | Peaks are derived from catalogue audio, not account data; the spec defers eviction to the offline-and-library wave. | Deleting on sign-out — loses SC-004's instant replay for no privacy gain. |
| `.mpwf` is a hand-written sectioned binary, not serde JSON (research R7) | ≈ 472 KB vs > 2 MB JSON for 10 minutes; byte arrays; unknown sections skipped so DM-5's later fields extend without a format bump. | JSON like 004's index — 4–5× larger, slower to parse on the analysis thread. |
| `SourceEvent` gains a variant carrying `Arc<DecodedStore>` (ptr-equality, redacted `Debug`) rather than a new `SourceHost` method | Keeps the seam additive and the event order explicit (store announced right after `TrackStarted`); avoids a per-tick poll. | A `SourceHost::decoded_store()` accessor — polling and ambiguity about which track it belongs to. |
| Level-0 bucket = 128 frames (2.9 ms), ladder ×8 (research R4) | Meets "≤ 3 ms" and "≤ 4 096 buckets for the overview" (up to ~1 h 41 min) with ≈ 472 KB for 10 minutes. | ×4 ladder — more levels, no rendering benefit; 64-frame level 0 — doubles the file for no visible gain at a 200 ms window. |

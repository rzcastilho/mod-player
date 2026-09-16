# Implementation Plan: Streaming Playback, Transport, and Queue

**Branch**: `003-streaming-playback-and-queue` | **Date**: 2026-09-15 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/003-streaming-playback-and-queue/spec.md`

## Summary

Let a signed-in Premium user hear their real catalogue through the 001
engine: a new, independently versioned **Connect Audio Source** crate
registers ModPlayer as a Connect receiver ("ModPlayer on <hostname>",
editable), receives load requests and the encrypted stream, decrypts and
decodes in-client and feeds decoded PCM into the real-time engine through a
lock-free ring; the host gains a **full transport** (play/pause/stop/skip
back with the 3 s rule/skip forward/seek/volume, position from the audio
clock at ≥ 60 Hz, `buffering` sub-state), a **cursor-based queue** with
play-next block, shuffle, repeat off/one/all and a bounded history, and
**Connect transfer** in both directions with remote commands treated as
user commands. Network trouble degrades to `Reconnecting…`/`buffering`
without skipping; a broken protocol yields "source unavailable" with a
status-page link instead of a crash.

Technical approach (details in [research.md](research.md)): the receiver
crate `modplayer-audio-source-connect` wraps librespot 0.8
(`Session` + `Spirc` + `Player`) with a custom `Sink` writing into an `rtrb`
ring read by `ConnectRtSource: AudioSource` (R1, R4). Transport and queue
rules are **host-owned** pure code in `modplayer-core`; the host hands the
source a *program* (full effective order + cursor) which Spirc executes and
preloads for gapless transitions (R2). The `AudioSource` seam is extended
**additively** with a `SourceHost` trait (`attach/command/poll/health`)
implemented by the synthetic source, a scripted test double and the Connect
source (R10); `PlaybackController` becomes generic over it. The engine gains
`Command::Seek`, a leftover-carry instead of rewind-by-seek, and a
seqlock position anchor + `PositionClock` for ≥ 60 Hz UI sampling (R11).
The account crate gains three read-only Web API calls for "Play from
account", the other device's name and the launch-time Connect state (R3,
R9). No async runtime leaks outside the receiver crate; every host
integration is `std::sync::mpsc` drained in `tick()` as in 001/002.

Assumptions taken where the spec deferred or where librespot constrains
(all recorded in Complexity Tracking): the encrypted stream is buffered in
a private temp file rather than RAM (R6); next-track prefetch starts at
librespot's 30 s-before-end point (R6); outbound shuffle state is reported
as off (R2); transfer-in seeds a single-item, source-driven queue that grows
as the source reveals tracks (R3); the reusable AP credential blob is not
stored in this slice (R8).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001/002

**Primary Dependencies**: existing eframe/egui 0.36 (+accesskit), cpal 0.18, rtrb 0.4, fluent-templates 0.15, serde + toml, keyring 4.2, ureq 3.4, time 0.3, getrandom 0.3; **new (receiver crate only)**: librespot-core / -connect / -playback / -audio 0.8.0 (MIT, MSRV 1.85; `default-features = false`, feature `rustls-tls-webpki-roots`), tokio 1 (`rt-multi-thread`, `sync`, `time`); **new dev-only**: proptest 1 in `modplayer-core` — see research.md § Dependency additions

**Storage**: `settings.toml` gains `[playback] device_name`, `connect_device_id` (device-scoped, atomic replace, 001 store); queue/cursor/modes/position are **in-memory only**; encrypted stream chunks in a private OS-temp subdirectory (`modplayer-stream-<pid>/`, purged on start/exit — research R6, Complexity Tracking); no decoded audio ever on disk; no new secure-store entries

**Testing**: `cargo test --workspace` with `FakeBackend` (001), `ScriptedHost` (new source double with throttling/unavailable/transfer/health scripts), `FakeAuthorizationService` (002, extended), proptest for the queue; engine `assert_no_alloc` extended to the new RT half; live receiver test `#[ignore = "manual"]`; manual scenarios M1–M9 in [quickstart.md](quickstart.md); same CI gates as 001/002 on ubuntu/macos/windows

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical behaviour; platform differences confined to existing adapter crates (`modplayer-audio-io`, `modplayer-secure-store`) and librespot's own TLS/socket layer inside the receiver crate

**Project Type**: Desktop application — Cargo workspace grows from 9 to 10 crates (1 new library crate, the receiver)

**Performance Goals**: play on a buffered track → audible ≤ 50 ms (SC-001); un-streamed track → audible ≤ 1.5 s at 10 Mbit/s (SC-002); position published ≥ 60 Hz, ≤ 5 ms jitter, audio-clock derived (SC-003); gapless on ≥ 95 % of eligible transitions (SC-004); transfer in/away visible ≤ 2 s (SC-007/008); state reported to other controllers ≤ 1 s (SC-012); seek on buffered audio lands ≤ 50 ms; `tick()` O(events) per frame; UI thread never blocks on network or audio

**Constraints**: real-time path allocation/lock/I/O-free (Constitution I; RT half verified with `assert_no_alloc`); decoded audio never leaves the engine (V); only the receiver crate knows the protocol and only the binary depends on it (IV); `#![forbid(unsafe_code)]` in every new/changed crate; no `unwrap`/`expect` outside tests; token never logged/serialised (VI); all strings externalised, en-US only; every control keyboard-operable with an accessible name; no tray/background mode — closing the window quits; at most current + next track buffered

**Scale/Scope**: 1 new crate (~2 500 LOC incl. tests), engine delta (~300 LOC), core additions (queue, transport reducer, controller integration, settings; ~2 000 LOC incl. proptests), account read delta (~400 LOC), UI (Now Playing rewrite, Queue panel, Settings › Playback, Developer action; ~1 200 LOC), 1 new `.ftl` (~60 keys), ~70 automated tests, single user / single account / one active queue

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | **Yes** | ✅ PASS | The only new real-time code is `ConnectRtSource::fill/seek` (ring pop, marker apply, atomics — no alloc/lock/I/O/log, `assert_no_alloc` test) and the engine delta (`Command::Seek`, preallocated leftover carry, seqlock anchor write; `Instant::now()` is a vDSO read). Everything librespot runs on the receiver crate's worker/player threads; the host talks to it over `mpsc`/atomics. Engine PR carries the real-time note; latency/loop-seam tests unchanged (contracts/engine-delta.md). |
| II. Plugins Are Guests | No | N/A | No plugin runtime exists yet; no plugin-facing API added. Transport focus (FR-3.3) is explicitly out of scope. |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | Transport state and queue are host primitives implemented in `modplayer-core` (research R2), not in the source and not scriptable. No DSP added. |
| IV. Audio Source Is Replaceable and Isolated | **Yes** | ✅ PASS | New crate `modplayer-audio-source-connect` is the only importer of `librespot-*`; it sits behind the unchanged `AudioSource` trait plus the additive `SourceHost` trait with three implementors (synthetic, scripted, connect). Only the `modplayer` binary depends on it (guard test). Every other crate builds/tests with the synthetic/scripted hosts. Independently versioned (contracts/connect-source.md). |
| V. No Audio Ever Leaves the Engine (non-negotiable) | **Yes** | ✅ PASS | Decoded PCM flows `Player → RingSink → rtrb → ConnectRtSource → Processor` and nowhere else; no API/flag/test helper exposes sample buffers or writes decoded audio; the librespot temp file holds only the AES-encrypted stream and is decrypted only for playback (research R6); no offline cache in this slice. |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | The access token is read from the OS store on demand via `ReceiverCredentials` and passed straight to librespot; never logged, never in `Debug` (leak test extended to the new crate); rustls TLS everywhere; no telemetry; device id is random, non-identifying. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | One new crate for one component (receiver); `#![forbid(unsafe_code)]` in all new/changed crates (librespot's own `unsafe` stays inside its dependencies, as with `keyring` in 002); `thiserror` errors; no `unwrap`/`expect` outside tests; doc examples; `cargo deny` licences (MIT/Apache/MPL-2.0 already allowed; additions with comments if surfaced); CI matrix unchanged; `CODEOWNERS` gains the receiver crate. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Named tests per FR/SC in quickstart.md; proptest for queue state (SC-011); `assert_no_alloc` on the new RT half; position-clock jitter test (SC-003); scripted-double coverage for buffering, gapless, transfer, timeouts, health; 001's loop-seam/latency tests untouched. Criterion benchmarks and 24 h soak remain 001's engine obligations (no new DSP). |
| IX. One Plugin API Definition | No | N/A | No plugin API in this slice. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | New traits each have ≥ 2 real implementors used across crates: `SourceHost` (3), `ReceiverCredentials` (binary impl + test fake). No feature flags. Randomness for shuffle is a 20-line xorshift in-crate rather than a `rand` dependency. Platform differences confined to existing adapters. Every new control keyboard-operable with an accessible name; strings externalised (en-US, as 001/002). User override N/A (no plugins). Each dependency justified in research.md. |
| Governance: engine/gateway/runtime sign-off | **Yes (engine)** | ✅ PASS | Engine changes are confined to contracts/engine-delta.md and require the engine maintainer's sign-off (`CODEOWNERS` already routes `crates/modplayer-engine/`). Requirement IDs (FR-, NFR-, INT-, EC-) referenced throughout. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no unsafe code, no feature flags, no single-implementor trait and exactly one crate; the four spec-level deviations forced by librespot (temp file, prefetch timing, shuffle report, transfer-in context) are outside the non-negotiable principles and are recorded below with rejected alternatives.
**Post-implementation re-check** (T099, Phase 7): PASS. Verified against the merged tree, not just the design:
- I: `assert_no_alloc` still covers `ConnectRtSource::fill`/`seek` and the engine's carry/anchor writes (`rt_no_alloc.rs`, engine's extended no-alloc test); both green.
- IV: `cargo metadata` guard (`modplayer::single_dependent`) passes; `librespot-*` appears only in `crates/modplayer-audio-source-connect/Cargo.toml`, and only `crates/modplayer/Cargo.toml` depends on that crate.
- V/VI: no `Debug`/`Clone` impl anywhere in the new crate exposes the access token or decoded PCM; `credential_leak` (extended) and `settings`/`queue` tests are green.
- VII: `#![forbid(unsafe_code)]` present in every new/changed crate (including `modplayer-audio-source-connect`); `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace` (457 passed, 3 ignored/manual), `cargo deny check`, and `scripts/check-license-headers.sh` all pass clean (T097). `cargo deny check` required documenting 7 additional advisory ignores (`RUSTSEC-2023-0071`, `-2025-0134`, `-2026-0049`, `-0098`, `-0099`, `-0104`, `-0194`, `-0195`) — all transitively forced by `librespot-core` 0.8.0's own pinned `quick-xml`/`rsa`/`rustls` versions, each with "no safe upgrade available" upstream or blocked by librespot's own version pin; recorded in `deny.toml` with per-crate justification, following the pre-existing `ttf-parser` precedent. This is a dependency-provenance disclosure, not a Constitution violation, so it is not added to Complexity Tracking.
- X: `SourceHost` has 3 implementors (`SyntheticHost`, `ScriptedHost`, `ConnectSource`); no `rand` dependency was added (in-crate xorshift, as planned); no new feature flags.
- No undocumented deviation beyond the entries already in Complexity Tracking was found.

## Project Structure

### Documentation (this feature)

```text
specs/003-streaming-playback-and-queue/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R14, dependency justification, open verifications
├── data-model.md        # Phase 1: TrackRef/Queue/Program/Transport/Connect device/source internals/settings/notifications
├── quickstart.md        # Phase 1: automated gates (named tests) + manual scenarios M1–M9
├── contracts/
│   ├── audio-source-host.md     # SourceHost trait, SourceCommand/SourceEvent, RT-half rules
│   ├── connect-source.md        # receiver crate API, librespot mapping, health classification, threads
│   ├── transport-and-queue.md   # controller surface, reducer rules T1–T24, program sync, queue API, settings delta, links
│   ├── engine-delta.md          # Command::Seek, leftover carry, position anchor + PositionClock
│   ├── account-read-delta.md    # AuthorizationService reads, scopes, AccountService requests
│   └── ui-surface.md            # Now Playing, Queue panel, Settings › Playback/Developer, notifications, App wiring
├── checklists/requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

Existing layout (001/002) is kept; `+` marks new files, `~` modified files.

```text
Cargo.toml                                   ~ workspace deps: librespot-{core,connect,playback,audio} 0.8, tokio 1; proptest stays dev-only
deny.toml                                    ~ any licence surfaced by librespot's tree, with crate comments
CODEOWNERS                                   ~ crates/modplayer-audio-source-connect/ (receiver maintainer)
locales/en-US/
├── playback.ftl                             + transport, queue, banner, status, notification, device-name keys
└── settings.ftl                             ~ Playback / Developer descriptor titles
crates/
├── modplayer-audio-source/                  ~ trait crate (still no deps)
│   └── src/{lib.rs ~, host.rs +, types.rs +}      SourceHost, SourceCommand, SourceEvent, SourceHealth, BufferStatus, SourceRtShared, TrackId, TrackRef, Repeat
├── modplayer-audio-source-synthetic/
│   └── src/{lib.rs ~, host.rs +, scripted.rs +}   SyntheticHost; ScriptedHost + ScriptedRt (pub test double)
├── modplayer-audio-source-connect/          + the receiver crate (independently versioned)
│   ├── Cargo.toml                           version 0.1.0; librespot-*, tokio, rtrb, thiserror, getrandom
│   ├── src/
│   │   ├── lib.rs                           ConnectSource (impl SourceHost), ConnectConfig, ReceiverCredentials, VERSION
│   │   ├── worker.rs                        tokio runtime thread: Session/Player/Spirc lifecycle, command loop, backoff
│   │   ├── events.rs                        PlayerEvent → SourceEvent + marker emission
│   │   ├── program.rs                       ProgramMap (generation, order, index_of)
│   │   ├── health.rs                        ErrorKind → SourceHealth classification, backoff
│   │   ├── sink.rs                          RingSink (librespot Sink) with back-pressure
│   │   ├── mixer.rs                         HostMixer (librespot Mixer), volume scale
│   │   ├── rt.rs                            ConnectRtSource (AudioSource), Marker, anchor math
│   │   ├── tmp.rs                           private temp dir create/purge
│   │   └── credentials.rs                   ReceiverCredentials trait + CredentialError
│   └── tests/{rt_no_alloc.rs, markers.rs, sink_backpressure.rs, health.rs, live.rs (#[ignore])}
├── modplayer-engine/                        ~ contracts/engine-delta.md (maintainer sign-off)
│   ├── src/command.rs                       ~ Command::Seek
│   ├── src/processor.rs                     ~ leftover carry; anchor write; Seek drain
│   ├── src/output_stage.rs                  ~ carry buffer
│   ├── src/shared.rs                        ~ anchor atomics (seqlock)
│   ├── src/position_clock.rs                + PositionClock
│   └── tests/{seek.rs +, carry.rs +, position_clock.rs +}
├── modplayer-core/
│   ├── Cargo.toml                           ~ [dev-dependencies] proptest
│   ├── src/queue.rs                         + Queue, QueueItem, Program, QueueChange, xorshift RNG
│   ├── src/transport.rs                     + TransportState, ActiveState, reducer + Effect
│   ├── src/controller.rs                    ~ generic over SourceHost; source polling in tick(); new commands; sync_program; timers; shutdown
│   ├── src/links.rs                         + STATUS_PAGE_URL
│   ├── src/notifications.rs                 ~ actions: OpenStatusPage, RetrySource, OpenUpgradePage; multi-action
│   ├── src/settings/model.rs                ~ [playback] device_name, connect_device_id; DeviceName type
│   ├── src/settings_registry.rs             ~ playback.device_name, developer.play_from_account descriptors
│   └── tests/{queue_proptest.rs +, transport_reducer.rs +, controller_streaming.rs +, settings.rs ~}
├── modplayer-account/
│   ├── src/auth_service.rs                  ~ fetch_recently_played / fetch_saved_tracks / fetch_playback_state; AuthError::Forbidden
│   ├── src/spotify.rs                       ~ endpoints + JSON → TrackRef mapping; new scopes
│   ├── src/fake_auth.rs                     ~ scripted read results
│   ├── src/service.rs                       ~ request_recent_tracks / request_playback_state / access_token; ReadResult event
│   ├── Cargo.toml                           ~ depends on modplayer-audio-source (TrackRef)
│   └── tests/reads.rs                       +
├── modplayer-ui/
│   ├── src/app.rs                           ~ generic over SourceHost; account→controller mapping; Ticker; on_exit shutdown; launch state query
│   ├── src/now_playing.rs                   ~ full transport, status line, banner, seek slider, queue toggle
│   ├── src/queue_view.rs                    + queue panel with keyboard actions + drag
│   ├── src/ticker.rs                        + repaint ticker thread
│   ├── src/settings/{mod.rs ~, playback.rs +, developer.rs ~}
│   ├── src/notifications.rs                 ~ multiple action buttons; OpenStatusPage/Retry/Upgrade handling
│   └── tests/{fluent_keys.rs ~, accessibility.rs +, now_playing.rs +, queue_view.rs +, credential_leak.rs ~}
└── modplayer/
    ├── Cargo.toml                           ~ depends on modplayer-audio-source-connect (only dependent)
    ├── src/main.rs                          ~ build ConnectSource with SecureStore-backed ReceiverCredentials; pass to controller
    └── tests/single_dependent.rs            + cargo metadata guard for Constitution IV
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
(one crate per component, Constitution VII) and add exactly one crate,
`crates/modplayer-audio-source-connect`, the `audio-source-*` crate
Constitution IV names as the sole protocol speaker, independently versioned
per A-11. The queue and transport reducer are modules of the existing
`crates/modplayer-core` (core services, Part 9 §3) rather than new crates —
they are pure host primitives with no new dependencies, so a new crate would
fail Constitution X's "why is an existing crate insufficient" test. The
dependency graph stays a strict DAG:
`modplayer → {ui, core, audio-io, account, secure-store, audio-source-connect}`;
`ui → {core, audio-io, engine, account, secure-store}`;
`core → {engine, audio-io, audio-source, audio-source-synthetic}`;
`account → {secure-store, audio-source}`;
`audio-source-connect → {audio-source, librespot-*, tokio, rtrb}` (no dependency on secure-store or account — the binary implements `ReceiverCredentials`);
`engine → {audio-source, audio-source-synthetic}`. The trait crate
`crates/modplayer-audio-source` remains dependency-free. Locale files stay
under the repository-root `locales/en-US/` (auto-embedded by 001's
`static_loader!`).

## Design notes that tasks must respect

1. **Host is authoritative, source executes** (research R2): every queue or
   mode change goes through `Queue` → `sync_program()` → `LoadProgram`;
   the host never calls `SkipNext/SkipPrev` in host-driven mode. The
   `TrackStarted.program` generation check is what makes late events safe.
2. **`attach()` per stream build**: `open_stream_on` must call
   `source_host.attach(shared.position_frames())` and hand the returned RT
   half to `Processor::new`; never cache an `Rt` across streams.
3. **Buffering is derived, never commanded**: `buffering = intent Playing ∧
   (Loading seen since last Playing ∨ underrun observed ∧ ring < 50 %)`;
   the engine never learns about it.
4. **No network on the UI thread**: the receiver worker, librespot's player
   thread and 002's account workers are the only places that touch the
   network; `poll()`/`tick()` are non-blocking.
5. **Token discipline**: `ReceiverCredentials::access_token` is the only
   read path; the receiver crate never stores the token beyond the
   `Credentials` value handed to librespot; `Debug` impls redact.
6. **Timers are injectable**: the controller takes a `now: fn() -> Instant`
   (or a `Clock` like 002) so the 5 s transfer and 30 s transient timers are
   tested without sleeping.
7. **Sign-out ordering**: `App` must call `controller.clear_for_sign_out()`
   *before* it processes 002's `SignedOut/SessionRevoked` report so the
   device is deregistered before the sign-in step renders (FR-027, SC-013).
8. **Temp dir hygiene**: the receiver purges `modplayer-stream-*` on
   `Initialize` and `Shutdown`; `App::on_exit` always calls
   `controller.shutdown()` (FR-008).
9. **Engine PR is separate and first**: `Command::Seek`, carry, and
   `PositionClock` land in their own PR with the real-time safety note and
   maintainer sign-off before any consumer lands.
10. **`open_url` only from the UI** (002 note 10) — status page and upgrade
    page links are opened by the notification button handler in `modplayer-ui`.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. The following spec-level deviations and scope
decisions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Encrypted stream buffered in a private OS-temp file, not RAM (FR-009 says "in memory only"; research R6) | `librespot-audio` streams every file into a `NamedTempFile` in `SessionConfig.tmp_dir`; the Player's fetch path is not pluggable. The file holds only the AES-encrypted stream, is `0o700`/private, deleted on drop and purged on start/exit; Constitution V (decoded audio, decryption only for playback) is fully honoured. | Forking `librespot-audio` for an in-memory backing store — `unknown-git = "deny"`, permanent merge burden contrary to A-11's goal of tracking upstream protocol fixes. RAM-disk `tmp_dir` — not portable to macOS/Windows. Recorded as a follow-up spike. |
| Next-track prefetch starts 30 s before the end of the current track, not "once the current fetch completes" (FR-009; research R6) | librespot exposes no fetch-complete signal; `TimeToPreloadNextTrack` fires at `PRELOAD_NEXT_TRACK_BEFORE_END_DURATION_MS = 30 s`. The observable outcomes (gapless when both are buffered, ≤ 2 tracks held, stale prefetch dropped on program change) hold. | Calling `player.preload(next)` immediately on `TrackStarted` — two concurrent full-file fetches share bandwidth and slow the current track's pre-buffer, hurting SC-002. |
| Outbound shuffle flag reported to other controllers as **off**; repeat-all/one reported correctly (FR-025; research R2) | Spirc's shuffle would re-order its own copy of the program; the host's order must be honoured for FR-012/013. Remote shuffle commands are still applied as host shuffle toggles. | Letting Spirc shuffle — the queue view and FR-013's rules could not be satisfied because Spirc exposes no read API for its order. |
| Transfer-in seeds `[current]` in source-driven mode and grows as the source reveals tracks; local skip delegates to the source until a local edit (FR-015; research R3) | Spirc does not expose the transferred context; the spec explicitly allows a single-item queue when only the current track is supplied. | Forking `librespot-connect` to expose `ConnectState` — same fork burden as above; reading the context via the Web API — Connect contexts (radio, autoplay, mixed) are not reliably representable there. |
| Other device's name and launch-time Connect state via Web API `GET /v1/me/player` (research R3) | Spirc does not expose cluster device names. Adds scope `user-read-playback-state`; graceful fallback to "another device". | Guessing from the last transfer — not available at launch (FR-019). |
| A program reload on the currently playing track may cause a ≤ 50 ms discontinuity (queue edits while playing; research R2) | `Player::load` of the same track seeks the running decoder to the host's position estimate; only happens on user queue edits, never on natural transitions. | Deferring program reloads to track boundaries — the source would preload/advance to a stale next item, breaking FR-009's "recomputed within 1 s" and FR-014. |
| Reusable AP credential blob not stored (research R8) | Login with the fresh OAuth token measured < 1 s; avoids a second secret and registry entry. `EntryName::ReceiverCredential` stays reserved. | Storing it now — more security surface for no user-visible gain in this slice. |
| `PlaybackController` and `App` gain a second type parameter `H: SourceHost` (research R10) | Keeps `Processor<S>` monomorphic (001's "no trait object on the real-time path") while letting the binary inject the receiver. | `Box<dyn AudioSource>` in the processor — reverses a documented 001 decision; `dyn SourceHost` with an erased `Rt` — impossible without boxing the RT half. |
| Engine `render` reads `Instant::now()` once per buffer for the position anchor (research R11) | FR-006 demands ≥ 60 Hz position with ≤ 5 ms jitter; at the Safe preset the render rate is ≈ 43 Hz, so per-render publication cannot meet it. `Instant::now()` is a vDSO/`mach_absolute_time` read, non-blocking. | A 60 Hz publisher thread reading only atomics — adds a thread and still cannot resolve between renders; forcing ≤ 735-frame buffers — removes the Safe preset. |
| New Web API scopes require re-authorisation for sessions created under 002 (research R9) | "Play from account" and the device-name lookup need `user-read-recently-played`, `user-library-read`, `user-read-playback-state`. Handled with an inline "sign in again" hint and silent fallbacks. | Forcing re-sign-in on upgrade — disruptive; the feature is a developer surface. |
| Ticker thread for repaints while minimized (research R12) | `tick()` runs on egui frames; occluded windows may stop repainting on some platforms. Audio advancement itself never depends on ticks. | Moving the controller to its own thread — the controller owns the cpal stream and the settings store; a large refactor for no other benefit in this slice. |

## Post-Implementation Findings (T098)

The T098 manual walkthrough (M1, real Premium account, macOS, 2026-09-16)
surfaced one design-level blocker and two implementation bugs. The bugs were
fixed in place; the blocker is a scope escalation recorded here.

### BLOCKER — single OAuth token cannot serve both the session and the Web API

Real streaming (US1, SC-001/002) **cannot pass** under the current
single-credential design. As of 2026 Spotify returns a blanket **429** for
Web API calls bearing a Keymaster-desktop-client token, and rejects a
Developer-Dashboard-app token at the librespot receiver session login with
**`INVALID_CREDENTIALS`**. No single token satisfies both surfaces. Full
evidence, the verification table, and what remains unverified are in
**research R8 → Amendment (2026-09-16)**.

- **Impact**: the tier check (`/v1/me`), "Play from account" (FR-022) and the
  other-device-name lookup (FR-016/019) all need a Dashboard-app token, which
  in turn cannot authenticate the Connect receiver. With the Keymaster
  default, tier stays `Unknown` → `playback_permitted()` false → the receiver
  is never even asked to connect.
- **Fix direction (out of 003 scope)**: a **two-credential architecture** —
  Keymaster/session credential for the Connect receiver, separate
  Dashboard-app token for Web API reads — spanning 002 (OAuth flow,
  SecureStore: a second entry) and 003 (`ReceiverCredentials` source,
  `SpotifyAuthorizationService` transport). Needs a spec/plan revision, not a
  patch. The `EntryName::ReceiverCredential` slot reserved in 002 and the
  `MODPLAYER_OAUTH_CLIENT_ID` / new `MODPLAYER_OAUTH_PORT` overrides are the
  natural seams for it.
- **Verification (2026-09-16, T044)**: the Keymaster→session path is now
  **confirmed** — with a Keymaster-minted `streaming` token the receiver
  session authenticated, the device activated, and ~4.9 s of real audio
  (216 404 non-silent frames) played through the RT ring. So the streaming
  half works; the blocker is solely that the *same* token cannot also serve
  the Web API. A two-credential design is therefore sufficient, not just
  necessary.

### Bugs found and fixed in this run

| Bug | Cause | Fix |
|---|---|---|
| `connect-worker` panics "there is no reactor running" the moment it tries to connect (latent since T038; no non-`#[ignore]` test drives the real `ConnectSource`) | `Session::new` calls `tokio::runtime::Handle::current()` but ran outside the worker's runtime | scope a `runtime.enter()` guard around `Session::new` (`worker.rs`) |
| Tier check / account reads could hang forever with no feedback if a worker thread failed to spawn | all 5 `AccountService` spawners discarded `thread::Builder::spawn`'s `Result` | send a synthetic `Err(AuthError::Transient)` on spawn failure so the existing result path runs (`service.rs`) |
| `TierCheckFailed` produced no user-visible signal (panel silently stuck on "Never checked online") | the event only reset internal state | raise a Warning notification `account-recheck-failed` (`app.rs`, `account.ftl`) |
| A host-initiated `LoadProgram` never started playback: the device registered but never became active, and librespot ignores `Load` while Not Active ("will be ignored while Not Active") | the `LoadProgram` command arm called `spirc.load()` but only `RequestTransferHere` ever called `spirc.activate()` | activate on `LoadProgram` when a new `device_active` flag shows the device is inactive; the flag also stops arming `transfer_requested` for an already-active device (which would corrupt a later US3 remote-transfer classification) (`worker.rs`) |

Support seam added for testing a BYO Dashboard app: `MODPLAYER_OAUTH_PORT`
pins the loopback redirect port so a dashboard app's exact registered
redirect URI can be honoured (`listener.rs`).

**Resolution (2026-09-16, superseding the BLOCKER above)**: the two-credential
architecture sketched as the "fix direction" was not built. Instead, per
spec.md's **Amendment (2026-09-16): Session-sourced reads, not the public Web
API**, the design stays single-credential (Keymaster only, no second OAuth
flow, no second `SecureStore` entry) and sources tier, the other device's
name, and "Play from account" from the receiver **session** itself (its
`SpClient` and Connect cluster state) instead of the 429'd public Web API.
This required a session-sourced-tier change (device now registers as soon as
the AccountSession is Active, with the session's own
`PremiumAccountRequired` rejection as the tier authority — commit
`24a27b1`), a `SpClient`-based "Play from account" (commit `f6c4e61`), a
rootlist raw-proto parse fix (commit `39e78d0`), and dropping the now-
misleading launch-time public-tier-check warning (commit `508d6b1`). With
these in place, **T098 M1 PASSED** (2026-09-16, live, real Premium account):
session registers, "Play from account" sources real tracks via `SpClient`,
and the track plays end-to-end with position advancing and the peak meter
live. M2–M9 (other platforms/scenarios) were dismissed by decision, not
attempted — see tasks.md T098 for the exact scope-closure note. The `research.md`
R8 amendment and `AuthorizationService` Web-API reads (R9) remain accurate as
a description of what was tried and found infeasible, not as the shipped
design; the shipped design is R8's follow-up note plus spec.md's Amendment.

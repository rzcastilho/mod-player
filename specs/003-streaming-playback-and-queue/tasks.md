---
description: "Task list for 003-streaming-playback-and-queue"
---

# Tasks: Streaming Playback, Transport, and Queue

**Input**: Design documents from `/specs/003-streaming-playback-and-queue/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/*, quickstart.md (all present)

**Tests**: Included. Constitution Principle VIII ("Test What the NFRs Promise") and the plan's Testing section mandate tests for this feature; quickstart.md names the exact automated gates each task below must satisfy.

**Organization**: Tasks are grouped by user story (spec.md priorities P1–P4) so each story is independently implementable and testable. Requirement IDs (FR-, SC-, T-rules, R-decisions) are cited so traceability survives into code review, per the constitution's Governance section.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no unmet dependency — safe to run in parallel
- **[Story]**: US1 (P1) / US2 (P2) / US3 (P3) / US4 (P4) — omitted for Setup/Foundational/Polish
- File paths are exact, per plan.md's Project Structure

## Path Conventions

Cargo workspace at repo root; one crate per component under `crates/`. Locale files under `locales/en-US/`. See plan.md § Project Structure for the full file tree this feature touches.

---

## Phase 1: Setup

**Purpose**: Workspace and new-crate scaffolding so all later phases compile.

- [X] T001 Add `librespot-{core,connect,playback,audio}` 0.8 (`default-features = false`, feature `rustls-tls-webpki-roots`) and `tokio` 1 (`rt-multi-thread`, `sync`, `time`) to workspace `Cargo.toml`; add `proptest` to `crates/modplayer-core/Cargo.toml` `[dev-dependencies]` (research R1, R14)
- [X] T002 [P] Update `deny.toml` with any licence newly surfaced by librespot's dependency tree (MPL-2.0/`symphonia` already allowed), each addition with a crate comment (research: Dependency additions summary)
- [X] T003 [P] Add `crates/modplayer-audio-source-connect/` entry to `CODEOWNERS` (receiver maintainer)
- [X] T004 Scaffold new crate `crates/modplayer-audio-source-connect` (`Cargo.toml` version `0.1.0`, independently versioned; `src/lib.rs` with `#![forbid(unsafe_code)]`; deps `librespot-*`, `tokio`, `rtrb`, `thiserror`, `getrandom`, `modplayer-audio-source`) per contracts/connect-source.md §1
- [X] T005 [P] Add `modplayer-audio-source-connect` to workspace `members` in root `Cargo.toml` and as a dependency of `crates/modplayer/Cargo.toml` (sole dependent, Constitution IV)
- [X] T006 [P] Create `locales/en-US/playback.ftl` scaffold file (empty section headers for transport/queue/banner/status/notification/device-name keys)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The additive `SourceHost` seam, engine deltas, core `Queue`/`TransportState`, and a generic `PlaybackController` that every user story builds on.

**⚠️ CRITICAL**: No user-story work starts before this phase is green.

### Audio-source trait crate (no dependencies, Constitution IV)

- [X] T007 Define `TrackId`, `TrackRef`, `Availability`, `Repeat`, `SourceHealth`, `BufferStatus`, `SourceCommand`, `SourceEvent` in `crates/modplayer-audio-source/src/types.rs` (data-model.md §1, contracts/audio-source-host.md §2–3)
- [X] T008 Define `SourceHost` trait and `SourceRtShared` atomics in `crates/modplayer-audio-source/src/host.rs`; re-export from `src/lib.rs` (~) (contracts/audio-source-host.md §1) — depends on T007

### Synthetic / scripted implementors (proves the seam is additive)

- [X] T009 [P] Implement `SyntheticHost` (`impl SourceHost`) in `crates/modplayer-audio-source-synthetic/src/host.rs`; update `src/lib.rs` (~) — depends on T008
- [X] T010 [P] Implement `ScriptedHost` + `ScriptedRt` test double (`throttle(ms)`, `unavailable(id)`, `remote(cmd)`, `transfer_in(ctx)`, `transfer_out()`, `health(h)`, deterministic per-track fixture audio) in `crates/modplayer-audio-source-synthetic/src/scripted.rs` — depends on T008
- [X] T011 [P] Test `synthetic_host_attach_restores_position` and confirm every 001 `AudioSource`/synthetic test still passes unchanged — `crates/modplayer-audio-source-synthetic` tests — depends on T009

### Engine deltas (own PR per design note 9; maintainer sign-off + real-time safety note required)

- [X] T012 Add `Command::Seek(u64)` to `crates/modplayer-engine/src/command.rs` (`Copy`, `size_of::<Command>() ≤ 16` static assert unchanged) (engine-delta.md §1)
- [X] T013 Replace rewind-by-`seek` with a preallocated leftover carry in `crates/modplayer-engine/src/output_stage.rs` and `src/processor.rs`; published position = `source.position() − carried` (engine-delta.md §2) — depends on T012
- [X] T014 Add position-anchor atomics (`anchor_position_frames`, `anchor_instant_nanos`, `anchor_generation` seqlock, `anchor_playing`) to `crates/modplayer-engine/src/shared.rs`, written once per render (engine-delta.md §3)
- [X] T015 Implement `PositionClock::now(shared, source_rate) -> Duration` in `crates/modplayer-engine/src/position_clock.rs` (+) — depends on T014
- [X] T016 [P] Test `seek_command_applies_at_boundary` — `crates/modplayer-engine/tests/seek.rs` (+) — depends on T012
- [X] T017 [P] Test `leftover_carry_is_bit_exact_with_001` (golden comparison, 44.1k↔48k at 128/256/1024 frames) — `crates/modplayer-engine/tests/carry.rs` (+) — depends on T013
- [X] T018 [P] Test `position_clock_60hz_jitter` (≤ 5 ms jitter, SC-003) and `position_clock_frozen_when_paused` — `crates/modplayer-engine/tests/position_clock.rs` (+) — depends on T015
- [X] T019 [P] Verify `render_never_allocates` (`assert_no_alloc`) still passes with the carry and anchor writes — extend existing engine no-alloc test — depends on T013, T014

### Core `Queue` (pure, in-memory, proptest-able)

- [X] T020 Implement `QueueItem`, `Queue` (`context`/`play_next`/`cursor`/`current`/`shuffle`/`repeat`/`history`/`mode`), `Program`, `QueueChange`, in-crate xorshift RNG in `crates/modplayer-core/src/queue.rs` (+), including `replace_context`, `add_context`/`reveal`, `play_next`/`play_next_track`, `move_up`/`move_down`/`reorder`, `remove`, `set_shuffle`/`set_repeat`, `advance(reason)`, `skip_back(position_ms)`, `mark_unavailable`, `program()`, `next_prefetch_target()` (data-model.md §2, contract §4, FR-004/011–014/026)
- [X] T021 [P] Unit tests for every `Queue` operation above, including play-next FIFO contiguity, shuffle-keeps-current+block, repeat-all re-shuffle-at-wrap, unavailable skip, history bound 100 — `crates/modplayer-core/src/queue.rs` inline `#[cfg(test)]` or `tests/queue.rs` — depends on T020
- [X] T022 [P] Proptest `advance_is_consistent_with_fr013_fr014` (SC-011: random ≤64-op sequences over ≤12-track fixtures with duplicates/unavailable) — `crates/modplayer-core/tests/queue_proptest.rs` (+) — depends on T020

### Transport reducer + settings + controller skeleton

- [X] T023 Implement `TransportState`, `ActiveState`, `PendingTransferCommand`, and pure `transport::reduce(state, input) -> (state, Vec<Effect>)` for rules T1–T12 (play/pause/stop/seek/skip/track-mirroring; transfer/health/session rules land with their stories) in `crates/modplayer-core/src/transport.rs` (+) (contract §2, data-model.md §3)
- [X] T024 [P] Table-driven tests for reducer rules T1–T12 — `crates/modplayer-core/tests/transport_reducer.rs` (+) — depends on T023
- [X] T025 Add `crates/modplayer-core/src/links.rs` (+) with `STATUS_PAGE_URL` (`option_env!("MODPLAYER_STATUS_PAGE_URL")` fallback) (contract §6)
- [X] T026 Extend `NotificationAction` with `OpenStatusPage`, `RetrySource`, `OpenUpgradePage` and multi-action (`actions: Vec<NotificationAction>`, ≤ 2) support in `crates/modplayer-core/src/notifications.rs` (~)
- [X] T027 Add `[playback] device_name` / `connect_device_id` to the settings model plus `DeviceName::parse` (trim, 1–64 chars, empty→default, >64→`Err(TooLong)`) in `crates/modplayer-core/src/settings/model.rs` (~); register descriptors in `crates/modplayer-core/src/settings_registry.rs` (~) (data-model.md §8, FR-001)
- [X] T028 [P] Test `[playback]` round-trip and `device_name_validation` — `crates/modplayer-core/tests/settings.rs` (~) — depends on T027
- [X] T029 Make `PlaybackController<B, H: SourceHost>` generic over the source host: `open_stream_on` calls `source_host.attach(position)`; hold `Queue`/`TransportState`; `tick()` drains `poll()` and runs `reduce`; injectable `now: fn() -> Instant` clock (design note 6) in `crates/modplayer-core/src/controller.rs` (~) — depends on T008, T020, T023

**Checkpoint**: Foundation ready — `cargo test --workspace` green on Setup+Foundational scope; user-story work can begin.

---

## Phase 3: User Story 1 - Hear real music through a full transport (Priority: P1) 🎯 MVP

**Goal**: A signed-in Premium user plays a real catalogue track and controls it with a complete transport (play/pause/stop/skip/seek/volume), position from the audio clock at ≥ 60 Hz, playback surviving minimize/hide/desktop-switch.

**Independent Test**: With a signed-in Premium account, enqueue real tracks via Settings › Developer › "Play from account", press play, verify audible output, full transport response, and position advancing at the audio-clock rate while the window is hidden/restored (SC-001–SC-003, SC-010).

### Connect Audio Source crate — real-time + streaming halves

- [X] T030 [US1] Implement `ReceiverCredentials` trait + `CredentialError` in `crates/modplayer-audio-source-connect/src/credentials.rs` (contracts/connect-source.md §1, research R8)
- [X] T031 [US1] Implement private temp-dir create/purge (`0o700`, purge on `Initialize`/`Shutdown`, stale `modplayer-stream-*` siblings >24h removed) in `crates/modplayer-audio-source-connect/src/tmp.rs` (research R6, contract §6)
- [X] T032 [US1] Implement `RingSink` (librespot `Sink`) writing into `rtrb::Producer<f32>` with 500 µs-park back-pressure and `written_frames: AtomicU64` in `crates/modplayer-audio-source-connect/src/sink.rs` (research R4)
- [X] T033 [US1] Implement `HostMixer` (librespot `Mixer`, `NoOpVolume`, `VolumePercent↔u16` scale with echo suppression) in `crates/modplayer-audio-source-connect/src/mixer.rs` (data-model.md §5)
- [X] T034 [US1] Implement `ConnectRtSource` (`impl AudioSource`: `fill`/`seek`/`position`/`len_frames`, marker application, `underrun` atomic, never allocates/locks/blocks) in `crates/modplayer-audio-source-connect/src/rt.rs` (research R4, contracts/audio-source-host.md §4) — depends on T032
- [X] T035 [US1] Implement `Marker` and `ProgramMap { generation, order, index_of }` in `crates/modplayer-audio-source-connect/src/program.rs` (data-model.md §5)
- [X] T036 [US1] Implement health classification (`ErrorKind → SourceHealth`, backoff 1 s→60 s cap) in `crates/modplayer-audio-source-connect/src/health.rs` (research R7, contracts/connect-source.md §4)
- [X] T037 [US1] Implement `PlayerEvent → SourceEvent` mapping and marker emission in `crates/modplayer-audio-source-connect/src/events.rs` (contracts/connect-source.md §3) — depends on T035
- [X] T038 [US1] Implement `ConnectWorker` (dedicated thread + tokio multi-thread runtime: `Session`/`Player`/`Spirc` lifecycle, `SourceCommand` → librespot mapping table, `LoadProgram`→`spirc.load`, backoff/reconnect) in `crates/modplayer-audio-source-connect/src/worker.rs` (contracts/connect-source.md §2) — depends on T030, T031, T033, T036, T037
- [X] T039 [US1] Implement `ConnectSource` (`impl SourceHost`, `Rt = ConnectRtSource`), `ConnectConfig`, `VERSION` const in `crates/modplayer-audio-source-connect/src/lib.rs` (contracts/connect-source.md §1) — depends on T034, T038
- [X] T040 [P] [US1] Test `ConnectRtSource` `fill`/`seek` under `assert_no_alloc` — `crates/modplayer-audio-source-connect/tests/rt_no_alloc.rs` — depends on T034
- [X] T041 [P] [US1] Test marker sequencing (`TrackStart` after `TrackEnd` at the same `at_written_frame` advances `track_seq` exactly once) — `crates/modplayer-audio-source-connect/tests/markers.rs` — depends on T035
- [X] T042 [P] [US1] Test `RingSink` back-pressure: N samples through a ring of capacity < N lost none (fake consumer thread) — `crates/modplayer-audio-source-connect/tests/sink_backpressure.rs` — depends on T032
- [X] T043 [P] [US1] Test `health::classify` table and `volume::{to_u16,to_pct}` round-trip within ±1% — `crates/modplayer-audio-source-connect/tests/health.rs` — depends on T036
- [X] T044 [US1] Live manual test `plays_five_seconds_of_a_real_track` (`#[ignore = "manual"]`, needs `MODPLAYER_TEST_ACCESS_TOKEN`) — `crates/modplayer-audio-source-connect/tests/live.rs` — depends on T039

### Binary wiring

- [X] T045 [US1] Build `ConnectSource` with `SecureStore`-backed `ReceiverCredentials` and pass it to `PlaybackController::new` in `crates/modplayer/src/main.rs` (~); update `crates/modplayer/Cargo.toml` (~) — depends on T039, T029
- [X] T046 [P] [US1] Test `receiver_crate_has_single_dependent` (`cargo metadata` guard, Constitution IV) — `crates/modplayer/tests/single_dependent.rs` (+) — depends on T045

### Controller streaming behavior

- [X] T047 [US1] Wire `play()`/`pause()`/`stop()`/`seek()`/`skip_forward()`/`skip_back()` to `SourceCommand::LoadProgram/Play/Pause/Stop/Seek` and `Queue::advance`/`skip_back` per reducer rules T1–T10 in `crates/modplayer-core/src/controller.rs` (~) — depends on T029, T039
- [X] T048 [P] [US1] Test `buffered_play_is_audible_within_50ms` (SC-001) — `crates/modplayer-core/tests/controller_streaming.rs` (+) — depends on T047
- [X] T049 [P] [US1] Test `first_play_buffers_until_two_seconds_then_plays` (SC-002, FR-010) — `controller_streaming.rs` — depends on T047
- [X] T050 [P] [US1] Test `adjacent_buffered_tracks_transition_without_gap` (SC-004, FR-007 gapless) — `controller_streaming.rs` — depends on T047
- [X] T051 [P] [US1] Test pause/resume at same position with no re-buffer, stop resets position and retains queue, seek lands within 50 ms — `controller_streaming.rs` — depends on T047

### Account read integration ("Play from account" sources real tracks for this story's Independent Test)

- [X] T052 [US1] Add `fetch_recently_played`/`fetch_saved_tracks`/`fetch_playback_state` to `AuthorizationService` trait and `SpotifyAuthorizationService` in `crates/modplayer-account/src/auth_service.rs` (~), `spotify.rs` (~) (contracts/account-read-delta.md §1)
- [X] T053 [US1] Add `user-read-recently-played`, `user-library-read`, `user-read-playback-state` to `ClientConfig.scopes`; add `AuthError::Forbidden` — `crates/modplayer-account/src/auth_service.rs` (~) — depends on T052
- [X] T054 [US1] Script `FakeAuthorizationService` reads (`RecentTracks(Ok/Err(Forbidden))`, `SavedTracks`, `PlaybackState`) in `crates/modplayer-account/src/fake_auth.rs` (~) — depends on T052
- [X] T055 [US1] Add `request_recent_tracks`/`request_playback_state`/`access_token` + `AccountEvent::ReadResult` to `AccountService` in `crates/modplayer-account/src/service.rs` (~); depend on `modplayer-audio-source` in `Cargo.toml` (~) — depends on T052
- [X] T056 [P] [US1] Test recently-played parse fixture (local-file item + duplicate), 204/403 handling, fallback to saved tracks on empty, stale attempt dropped — `crates/modplayer-account/tests/reads.rs` (+) — depends on T052–T055
- [X] T057 [P] [US1] Extend credential-leak test: `Debug` of new request/response types never contains the token — existing `modplayer-account` leak test — depends on T052

### UI — Now Playing full transport

- [X] T058 [US1] Rewrite Now Playing with full transport (play/pause toggle, stop, skip back/forward, seek slider with `aria-valuetext`, volume unchanged from 001), status line, inline disabled reasons in `crates/modplayer-ui/src/now_playing.rs` (~) (contracts/ui-surface.md §1)
- [X] T059 [US1] Add position readout driven by `PositionClock` at ≥ 60 Hz (`request_repaint_after(16ms)` while playing) — `now_playing.rs` (~) — depends on T058, T015
- [X] T060 [US1] Add `Ticker` thread (repaint every 33 ms while intent `Playing`, keeps `tick()` running while minimized/hidden/backgrounded) in `crates/modplayer-ui/src/ticker.rs` (+); wire into `crates/modplayer-ui/src/app.rs` (~) (research R12, FR-008/SC-010)
- [X] T061 [US1] Make `App` generic over `H: SourceHost`; wire `on_exit → controller.shutdown()`; map 002 `AccountEvent`s to `set_playback_permitted`/`on_tier_free` in `crates/modplayer-ui/src/app.rs` (~)
- [X] T062 [US1] Add Settings › Playback screen with device-name field (commit on Enter/blur, inline too-long error, empty→default placeholder) in `crates/modplayer-ui/src/settings/playback.rs` (+); register in `crates/modplayer-ui/src/settings/mod.rs` (~) — depends on T027
- [X] T063 [US1] Add Settings › Developer "Play from account" action (fetch 20 recent, fallback saved, `queue_replace` + `play()`, hidden when not permitted, inline loading/empty/forbidden states) in `crates/modplayer-ui/src/settings/developer.rs` (~) — depends on T055
- [X] T064 [US1] Add new locale keys (`transport-*`, `status-*`, `now-playing-*`, `setting-device-name*`, `setting-play-from-account*`) to `locales/en-US/playback.ftl` (+) and `settings.ftl` (~)
- [X] T065 [P] [US1] Test Now Playing disabled reasons per `ActiveState`/health, seek slider commits once per release, position readout updates — `crates/modplayer-ui/tests/now_playing.rs` (+) — depends on T058

**Checkpoint**: User Story 1 is fully functional and independently testable — real music plays through a complete transport.

---

## Phase 4: User Story 2 - Manage the play queue (Priority: P2)

**Goal**: View, reorder, and remove queue items; use shuffle and repeat (off/one/all); play-next block behaves per FIFO/precedence rules.

**Independent Test**: Enqueue several tracks, open the queue view, reorder and remove items, verify skip-forward respects the new order; toggle shuffle and each repeat mode and verify next-track selection (FR-011–014, SC-011).

- [X] T066 [US2] Implement `QueueView` projection (`items: Vec<QueueRow>`, effective order, current first, history excluded) exposed via `controller.queue_view()` in `crates/modplayer-core/src/controller.rs` (~) (contract §1) — depends on T029, T020
- [X] T067 [US2] Wire `queue_replace`/`queue_play_next`/`queue_play_next_track`/`queue_move_up`/`queue_move_down`/`queue_reorder`/`queue_remove`/`set_shuffle`/`set_repeat` to `Queue` ops + `sync_program()` (debounced ≤ 1 send/250 ms, `SourceDriven→HostDriven` on mutation) in `crates/modplayer-core/src/controller.rs` (~) (contract §3) — depends on T066
- [X] T068 [P] [US2] Test `sync_program` debounce, mode switch on mutation, wrap re-shuffle sends `[last, ...reshuffled]` — `crates/modplayer-core/tests/controller_streaming.rs` (extend) — depends on T067
- [X] T069 [US2] Build Queue panel: header (shuffle toggle, repeat cycle button), rows (title/artist, origin badges, current marker, drag reorder), Move up/down/Play next/Remove keyboard actions in `crates/modplayer-ui/src/queue_view.rs` (+) (contracts/ui-surface.md §2, FR-012, NFR-6.1) — depends on T066
- [X] T070 [US2] Wire Queue toggle button (`Ctrl/Cmd+Q`) in Now Playing to show the queue panel — `crates/modplayer-ui/src/now_playing.rs` (~) — depends on T069
- [X] T071 [US2] Add `queue-*` locale keys (`queue-shuffle`, `queue-repeat-off/-one/-all`, `queue-row`, `queue-badge-*`, `queue-move-up/down`, `queue-play-next`, `queue-remove`, `queue-empty`) to `locales/en-US/playback.ftl` (+)
- [X] T072 [P] [US2] Test queue rows in effective order, badges, keyboard actions mutate the controller's queue — `crates/modplayer-ui/tests/queue_view.rs` (+) — depends on T069
- [X] T073 [P] [US2] Test `unavailable_item_marked_skipped_and_notified` (FR-026) — `crates/modplayer-core/tests/controller_streaming.rs` (extend) — depends on T067

**Checkpoint**: User Stories 1 AND 2 both work independently.

---

## Phase 5: User Story 3 - Transfer playback to and from another Connect controller (Priority: P3)

**Goal**: Accept a transfer-to from another controller and start playing; transfer-away stops locally, retains UI/queue state, shows "Playing on <device>"; remote commands are mirrored; ModPlayer reports its own state within 1 s.

**Independent Test**: From a second Connect controller, transfer to ModPlayer (starts playing); transfer away (stops locally, shows other device's name); issue play/pause/seek from the other controller while ModPlayer holds playback and confirm it reflects; pause locally and confirm the other controller reflects within 1 s (SC-007/008/012).

- [X] T074 [US3] Implement transfer-in mapping: `BecameActive{context}` → `SourceDriven` queue seeded `[current]`, growth via `reveal()` as further tracks appear — `crates/modplayer-audio-source-connect/src/worker.rs` (~), `crates/modplayer-core/src/queue.rs` (~) (research R3, data-model.md §6) — depends on T038, T020
- [X] T075 [US3] Implement transfer-away/back reducer rules T16–T19 (`Inactive` freeze with Paused semantics, `TransferRequested` + 5 s timer, `BecameActive` resume with pending command) in `crates/modplayer-core/src/transport.rs` (~) (contract §2) — depends on T023
- [X] T076 [US3] Wire `play_here()`/`RequestTransferHere` and pending-command application (rule T17/T18) in `crates/modplayer-core/src/controller.rs` (~) — depends on T075
- [X] T077 [US3] Fetch the other device's name via `fetch_playback_state` on `BecameInactive` and at launch/reconnect (FR-016/019, research R3) in `crates/modplayer-core/src/controller.rs` (~), `crates/modplayer-ui/src/app.rs` (~) — depends on T052, T075
- [X] T078 [US3] Implement outbound state reporting (FR-025: play state, track, position, volume, shuffle, repeat on every change within 1 s, coalesced ≤ 1/200 ms) in `crates/modplayer-audio-source-connect/src/worker.rs` (~) (rule T24)
- [X] T079 [US3] Add "Playing on <device>" banner + **Play here** button to Now Playing in `crates/modplayer-ui/src/now_playing.rs` (~) (contracts/ui-surface.md §1) — depends on T075
- [X] T080 [US3] Add `banner-*`, `transfer-request-failed` locale keys to `locales/en-US/playback.ftl` (~)
- [X] T081 [P] [US3] Test `transfer_in_replaces_queue_and_plays`, `transfer_away_freezes_and_shows_banner`, `play_here_requests_transfer` — `crates/modplayer-core/tests/controller_streaming.rs` (extend) — depends on T074–T076
- [X] T082 [P] [US3] Test `transfer_request_times_out_with_warning` (5 s, `FakeClock`-style injected `Instant`) — `controller_streaming.rs` (extend) — depends on T075
- [X] T083 [P] [US3] Test remote command mirroring: `RemoteCommand(Play/Pause/Seek/SkipNext/SkipPrev/Volume/Shuffle/Repeat)` applied and reflected in ModPlayer's own transport (FR-017) — `controller_streaming.rs` (extend) — depends on T075

**Checkpoint**: All three user stories (US1–US3) work independently and together.

---

## Phase 6: User Story 4 - Stay playing through network trouble, and fail clearly when the protocol breaks (Priority: P4)

**Goal**: Pre-buffer current + pre-fetch next; degraded connection continues from buffer, dry buffer shows `buffering` without skipping; unrecoverable protocol failure reports "source unavailable" with Status page/Retry instead of crashing; session events (sign-out, revocation, expiry, downgrade) interact correctly with playback.

**Independent Test**: Start playback, simulate degraded then cut connectivity mid-track — confirm continuation from buffer then `buffering` without skipping; simulate a receiver-protocol failure — confirm "source unavailable" with a status-page link instead of a crash (SC-005, SC-009, SC-013).

- [X] T084 [US4] Implement pre-buffer/pre-fetch policy (fetch current fully as fast as the connection allows, preload next at the 30 s-before-end mark, discard previous track's buffers on advance, recompute/cancel stale prefetch on program change within 1 s) in `crates/modplayer-audio-source-connect/src/worker.rs` (~) (research R5/R6, FR-009)
- [X] T085 [US4] Set `AudioFetchParams` once per process (`read_ahead_before_playback = 2s`, `read_ahead_during_playback = 1h`) in `crates/modplayer-audio-source-connect/src/lib.rs` (~) (research R5) — depends on T039
- [X] T086 [US4] Implement `buffering` derivation (`Loading`/underrun/`ring_fill < 50%`) and `Reconnecting…` inline status + 30 s warning timer + session-loss reconnect-with-backoff (rules T13, T20) in `crates/modplayer-core/src/transport.rs` (~) (research R5/R7, FR-010/021) — depends on T023
- [X] T087 [US4] Implement `Unavailable`/`client_update_required` transport-disable + Status page/Retry notification (rule T21) in `crates/modplayer-core/src/transport.rs` (~) (FR-020) — depends on T023
- [X] T088 [US4] Implement FR-027 session-event handling: `NotRegistered` reasons, `clear_for_sign_out()` ordering before 002's report, credential-expiry-as-transient, `TierRejected`/downgrade finishes-then-disables (rules T22, T23) in `crates/modplayer-core/src/controller.rs` (~), `crates/modplayer-core/src/transport.rs` (~) — depends on T086, T087
- [X] T089 [US4] Add `MODPLAYER_CONNECT_FORCE_UNAVAILABLE` debug-only env hook to the health classifier in `crates/modplayer-audio-source-connect/src/health.rs` (~) (quickstart M6) — depends on T036
- [X] T090 [US4] Wire `Reconnecting…` status line and `stream-reconnect-warning`/`stream-source-unavailable`/`stream-source-update-required`/`subscription-downgraded` notifications with their actions in `crates/modplayer-ui/src/notifications.rs` (~), `crates/modplayer-ui/src/now_playing.rs` (~) — depends on T086–T088
- [X] T091 [P] [US4] Test `continues_from_buffer_when_throttled`, `dry_buffer_shows_buffering_and_resumes_without_skip` (SC-005) — `crates/modplayer-core/tests/controller_streaming.rs` (extend) — depends on T086
- [X] T092 [P] [US4] Test `seek_past_end_advances_per_fr014` (SC-006) — `controller_streaming.rs` (extend) — depends on T023
- [X] T093 [P] [US4] Test `transient_30s_raises_warning_and_clears`, `five_minute_transient_never_unavailable`, `unavailable_disables_transport_and_keeps_navigation` (SC-009) — `controller_streaming.rs` (extend) — depends on T086, T087
- [X] T094 [P] [US4] Test `sign_out_stops_clears_and_deregisters`, `downgrade_finishes_track_then_disables` (SC-013) — `controller_streaming.rs` (extend) — depends on T088

**Checkpoint**: All four user stories independently functional; app degrades gracefully and fails clearly.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Whole-suite gates and manual validation across all stories.

- [X] T095 [P] Test `fluent_keys` coverage for `playback.ftl`/`settings.ftl` (every key used above exists, no unused keys) — `crates/modplayer-ui/tests/fluent_keys.rs` (~)
- [X] T096 [P] Test accessibility: every new control (transport, queue rows/actions, device-name field, banner/Play here, notification actions) exposes a non-empty accessible name and correct role/state via AccessKit with `ScriptedHost` + `FakeBackend` (FR-023) — `crates/modplayer-ui/tests/accessibility.rs` (+)
- [X] T097 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, `scripts/check-license-headers.sh`; fix any findings
- [X] T098 Execute quickstart.md manual scenarios — **macOS M1 PASSED (2026-09-16, live, real Premium account)**: real Spotify audio plays end-to-end through ModPlayer (Keymaster session registers → "Play from account" sources tracks via the session's SpClient → track plays, position advancing, peak meter live). Reaching this took the single-credential + SpClient redesign (spec Amendment 2026-09-16) plus six fixes committed this run: connect-worker tokio-runtime panic, silent worker/tier failures, missing Connect-device activation on LoadProgram, session-sourced tier (device registered despite the Web API 429), rootlist raw-proto parse, and dropping the noisy launch tier-check warning. **Scope closed by decision**: Windows and Linux dismissed (not available in this environment); macOS M2–M9 not executed (only M1 validated). See plan.md "Post-Implementation Findings (T098)" and research R8 Amendment.
- [X] T099 Re-run plan.md's Constitution Check post-implementation and confirm no undocumented deviation beyond Complexity Tracking

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational (Phase 2)**: depends on Setup — BLOCKS all user stories
- **User Story 1 (Phase 3)**: depends on Foundational only
- **User Story 2 (Phase 4)**: depends on Foundational; extends US1's controller/UI but is independently testable once US1 exists to play against
- **User Story 3 (Phase 5)**: depends on Foundational + the Connect worker built in US1 (T038/T039); independently testable
- **User Story 4 (Phase 6)**: depends on Foundational + the Connect worker (US1) and the transport reducer's health/session rules; independently testable
- **Polish (Phase 7)**: depends on all four stories

### User Story Dependencies

- US1 (P1): no dependency on other stories — this is the MVP
- US2 (P2): builds on US1's controller/UI plumbing but its acceptance scenarios are self-contained once a queue exists
- US3 (P3): builds on US1's `ConnectSource`/worker; independent of US2
- US4 (P4): builds on US1's `ConnectSource`/worker and the shared transport reducer; independent of US2/US3

### Within Each Phase

- Types → traits → implementors → tests, in that order per crate
- Engine PR (T012–T019) ships before any consumer per design note 9
- `Queue`/`transport::reduce` (T020–T024) before `PlaybackController` generic wiring (T029)
- `ConnectWorker`/`ConnectSource` (T038–T039) before any US1/US3/US4 task that sends it commands

---

## Parallel Example: User Story 1

```bash
# RT-half and streaming-half building blocks (different files, no cross-deps):
Task: "Implement ReceiverCredentials trait in crates/modplayer-audio-source-connect/src/credentials.rs"
Task: "Implement temp-dir create/purge in crates/modplayer-audio-source-connect/src/tmp.rs"
Task: "Implement RingSink in crates/modplayer-audio-source-connect/src/sink.rs"
Task: "Implement HostMixer in crates/modplayer-audio-source-connect/src/mixer.rs"

# Once the crate compiles, its unit tests run together (different test files):
Task: "assert_no_alloc test in tests/rt_no_alloc.rs"
Task: "Marker sequencing test in tests/markers.rs"
Task: "RingSink back-pressure test in tests/sink_backpressure.rs"
Task: "Health classification test in tests/health.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL — blocks all stories)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: run quickstart M1, SC-001/002/003/010
5. Demo: real music through a full transport

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. + US1 → test independently → MVP demo
3. + US2 → test independently → queue demo
4. + US3 → test independently → Connect-transfer demo
5. + US4 → test independently → resilience demo
6. Polish → full CI matrix + manual M1–M9 across platforms

---

## Notes

- [P] tasks touch different files with no unmet dependency
- Every file path above is exact, from plan.md's Project Structure
- Engine changes (T012–T019) carry the "real-time safety" note and need the engine maintainer's sign-off (Constitution I, Governance) before any consumer lands
- `crates/modplayer-audio-source-connect` is the only crate depending on `librespot-*`; `crates/modplayer` is its only dependent (T046 guards this)
- Commit after each task or logical group; stop at any checkpoint to validate a story independently

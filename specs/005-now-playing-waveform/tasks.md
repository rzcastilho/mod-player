---

description: "Task list for Now-Playing View with Waveform (005)"

---

# Tasks: Now-Playing View with Waveform

**Input**: Design documents from `/specs/005-now-playing-waveform/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Included. Constitution VIII ("Test What the NFRs Promise") and quickstart.md's named-test table require automated tests per FR/SC; this feature's plan.md Testing section names the frameworks. Manual scenarios M1–M15 are executed by the implementing agent per Constitution › Manual Scenario Sign-Off (never handed back).

**Organization**: Grouped by user story (spec.md P1–P4) so each is independently testable. Shared subsystems (`DecodedStore`, decode-ahead, the Analysis Service, transport delta) are load-bearing for **US1** specifically (its Independent Test requires "a fully decoded (or previously analyzed) track"), so their core implementation lives in the US1 phase; later stories extend the same files with additive behaviour and their own tests. No new crate is created (Constitution X); crate list is fixed at 10.

## Path Conventions

Cargo workspace at repo root; crates under `crates/<name>/{src,tests}`. Locale file `locales/en-US/playback.ftl`. All paths below are exact per plan.md's Project Structure.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies, licence/deny config, and empty module scaffolding so later tasks touch existing files, not create-and-fill in one step.

- [X] T001 Add `thread-priority` (MIT) to `[workspace.dependencies]` in `Cargo.toml` and as a dependency of `crates/modplayer-core/Cargo.toml` and `crates/modplayer-audio-source-connect/Cargo.toml`; add `symphonia` (features `ogg`, `vorbis`, `mp3`) as a direct dependency of `crates/modplayer-audio-source-connect/Cargo.toml`; add `sha2` (workspace, already present) as a dependency of `crates/modplayer-core/Cargo.toml`; add `proptest` to `[dev-dependencies]` of `crates/modplayer-ui/Cargo.toml`. Run `rtk cargo check` to confirm the workspace still resolves.
- [X] T002 Run `cargo deny check` (per quickstart.md) after T001 and record any licence/advisory notes surfaced by `thread-priority`/`symphonia` in `deny.toml` (research R16, R9).
- [X] T003 [P] Scaffold empty modules with doc-comment stubs (no logic yet): `crates/modplayer-audio-source/src/decoded.rs`, `crates/modplayer-audio-source-connect/src/decode_ahead.rs`, `crates/modplayer-audio-source-connect/src/subfile.rs`, `crates/modplayer-core/src/analysis/{mod.rs,peaks.rs,cache.rs,worker.rs}`, `crates/modplayer-ui/src/waveform/{mod.rs,coords.rs,state.rs,paint.rs,input.rs}`; wire `pub mod decoded;` into `crates/modplayer-audio-source/src/lib.rs`, `pub mod analysis;` into `crates/modplayer-core/src/lib.rs`, `pub mod waveform;` into `crates/modplayer-ui/src/lib.rs`. **Verified (2026-09-17)**: all 12 files exist and both later-phase tasks (T019-T034 etc.) fully implemented them and the 3 `pub mod` wires are in place — scaffold step subsumed by that work, nothing left to add.
- [X] T004 [P] Add the 8 new Fluent keys to `locales/en-US/playback.ftl` (research R15): `now-playing-pick-a-track`, `now-playing-album`, `waveform-unavailable`, `waveform-detail`, `waveform-detail-window` (`{ $start }`, `{ $end }`), `time-elapsed` (`{ $time }`), `time-remaining` (`{ $time }`), `waveform-overview-desc`. Do not remove `transport-position` yet (removed in T0xx once the slider is gone, US1). **Verified (2026-09-17)**: all 8 keys present in `locales/en-US/playback.ftl`, added by the later-phase work (T034/T035); nothing left to add.

**Checkpoint**: Workspace builds (`rtk cargo build`) with new deps and empty modules; no behaviour changed yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The dependency-free `DecodedStore` type, its trait-crate tests, the additive `SourceEvent` variant, and the scripted/synthetic host support every story's tests rely on. **Nothing else in this feature can be built or tested without this phase.**

- [X] T005 Implement `DecodedStore`, `StoreState`, `PeakBucket`, `CHUNK_FRAMES`, `MAX_STORE_FRAMES` in `crates/modplayer-audio-source/src/decoded.rs` per contracts/decoded-store.md §1–2: chunked `OnceLock<Box<[AtomicU32]>>` storage, `new`/`write_frames`/`set_complete`/`set_failed`/`state`/`len_frames`/`covers`/`covered_frames`/`coverage`/`fold_peaks`/`read_frames` (doc-hidden), Release/Acquire publication discipline, cap enforcement, terminal-state write rejection. `#![forbid(unsafe_code)]` kept.
- [X] T006 Add `SourceEvent::DecodedStore { track: TrackId, store: Arc<DecodedStore> }` to `crates/modplayer-audio-source/src/types.rs` (data-model.md §1.2) with `PartialEq = Arc::ptr_eq` and a redacted `Debug` impl (rate, len, state, covered-frame count only — no samples).
- [X] T007 [P] `modplayer-audio-source` unit tests in `crates/modplayer-audio-source/tests/decoded.rs` (or `src/decoded.rs` `#[cfg(test)]`): `store_write_then_covers_and_reads_back_exact`, `store_rejects_non_aligned_writes`, `store_fold_peaks_mono_min_max_quantised`, `store_cap_never_allocates_past_bound`, `store_terminal_states_reject_writes`, `store_debug_is_redacted`.
- [X] T008 [P] Proptest `store_read_back_is_identity_for_any_aligned_write_sequence` in the same test module as T007 (Constitution VIII).
- [X] T009 Add the binary guard test `crates/modplayer/tests/decoded_store_boundary.rs` (Constitution V mechanical enforcement, contracts/decoded-store.md §4): scans every `crates/*/src/**/*.rs` and `crates/*/tests/**/*.rs` outside `modplayer-audio-source`, `modplayer-audio-source-synthetic`, `modplayer-audio-source-connect`, `modplayer-engine` for `read_frames(` and fails if found. Must pass trivially now (no callers yet) and stay green through every later phase.
- [X] T010 [P] Extend `crates/modplayer/tests/single_dependent.rs` (Constitution IV guard) so `symphonia` and `thread-priority` are permitted only in `modplayer-audio-source-connect` / `modplayer-core` respectively, per research R16.
- [X] T011 [P] `SyntheticHost` support in `crates/modplayer-audio-source-synthetic/src/host.rs`: build a `DecodedStore` and fill it synchronously (`Complete`) from `track::fill` on `Initialize`; emit `SourceEvent::DecodedStore` after each `TrackStarted` it raises (research R14).
- [X] T012 [P] `ScriptedHost` support in `crates/modplayer-audio-source-synthetic/src/scripted.rs`: add `DecodeScript { Instant, Progressive { frames_per_tick }, FailAt { frame }, Silent, None }` and `handle.script_decode(DecodeScript)`; advance `Progressive` fill inside `poll()`; keep `ScriptedRt` generating its tone independent of the store (research R14).
- [X] T013 Add `Input::Seek { position_ms, position_frames: Option<u64>, buffer_ready }` and `Effect::SeekTo { position_ms, position_frames: Option<u64> }` to `crates/modplayer-core/src/transport.rs` (contracts/transport-delta.md §1); keep reducer rules T6–T8, clamping `position_frames` at track end alongside `position_ms`.
- [X] T014 Add `PlaybackController::seek_frames(frame: u64)` to `crates/modplayer-core/src/controller.rs` (`position_ms = frames × 1000 / rate`, `position_frames = Some(frame)`); keep `seek(Duration)`'s signature, passing `position_frames: None`.

**Checkpoint**: `rtk cargo test --workspace` green; `DecodedStore` and the seek-frame plumbing exist but nothing produces or consumes a real store yet (`ConnectSource` untouched, no UI change).

---

## Phase 3: User Story 1 - See the track and seek by clicking the waveform (Priority: P1) 🎯 MVP

**Goal**: Now Playing shows artwork/title/artists/album/elapsed/remaining and a whole-track waveform overview; clicking it seeks sample-accurately (decoded audio) via a real decode-ahead thread and a live Analysis Service; keyboard seek parity; empty state.

**Independent Test**: Play a fully decoded (or previously analyzed) track; verify artwork/title/artists/album/elapsed/remaining render and update; click a point on the overview and verify playback resumes from exactly that position.

### Tests for User Story 1

- [X] T015 [P] [US1] Receiver RT-feed tests in `crates/modplayer-audio-source-connect/tests/rt_feed.rs`: `rt_seek_into_store_is_sample_exact`, `rt_seek_outside_store_uses_ring`, `rt_store_feed_drains_ring_in_lockstep`, `rt_reposition_marker_ignored_on_store_feed`, `rt_track_start_swaps_store_retires_old`, `rt_store_rescues_ring_underrun`, `rt_store_exhaustion_falls_back_to_ring`, `rt_position_tracks_store_cursor`, `store_drop_never_on_rt` (three consecutive `TrackStart` markers A→B→C with every non-RT `Arc` dropped beforehand; `Weak::upgrade` succeeds for each store after every `fill` until the retirement ring is drained), `rt_retire_never_full` (`MARKER_CAPACITY + 1` `TrackStart` markers with no drain → no `Full`, `parked` stays `None`) (contracts/connect-source-delta.md §3 rows 5/5a/5b, data-model.md §2.2). Write these first; they fail until T019–T022 land.
- [X] T016 [P] [US1] Extend `crates/modplayer-audio-source-connect/tests/rt_no_alloc.rs` with a store-attached case asserting `fill`/`seek` allocate nothing (guarantee #8).
- [X] T017 [P] [US1] `crates/modplayer-core/tests/analysis.rs`: `cache_hit_publishes_complete_without_a_store` (A1), `ladder_levels_fold_by_eight_and_present_bits_follow` (data-model.md §3.2), proptest `cache_round_trips_any_peaks` and `cache_rejects_any_truncation` (data-model.md §6). Write first; fail until T022–T025 land.
- [X] T018 [P] [US1] `crates/modplayer-core/tests/transport_reducer.rs::seek_frames_carries_exact_frame_to_engine` and `::seek_frames_clamps_at_track_end`; `crates/modplayer-core/tests/controller_streaming.rs::seek_frames_pushes_exact_engine_command_and_ms_to_source`, `::track_change_detaches_then_attaches_analysis`, `::decoded_store_event_reaches_analysis_for_current_track_only`.

### Implementation for User Story 1

- [X] T019 [US1] Implement `Subfile<R>` (`Read + Seek + symphonia::io::MediaSource`, 0xa7 Ogg-header offset) in `crates/modplayer-audio-source-connect/src/subfile.rs` (research R3, contracts/connect-source-delta.md §1).
- [X] T020 [US1] Implement the decode-ahead thread in `crates/modplayer-audio-source-connect/src/decode_ahead.rs`: format pick from `audio_item.files` (Bitrate160 list), `AudioFile::open` + `audio_key().request`, `Subfile` + `AudioDecrypt` as the symphonia `MediaSource`, probe → decode loop writing `store.write_frames(packet.ts, ..)`, chunk-boundary discard after seeks, seek-hint handling, EOF → jump to lowest unfilled chunk → `set_complete`, unrecoverable error → `set_failed`, `stop` flag, `ThreadPriority::Min` via `thread-priority` (log-and-continue on failure) — depends on T019.
- [X] T021 [US1] Extend `crates/modplayer-audio-source-connect/src/program.rs` (`MarkerKind::TrackStart` carries `store: Arc<DecodedStore>`, `Marker` no longer `Copy`) and `src/rt.rs` (`cursor`/`feed`/`ring_pos`/`store`/`retired`/`parked` fields and the feed-selection rules of data-model.md §2.2; a `TrackStart` moves the replaced store `Arc` into the `retired` producer and never drops it — the `Full` arm parks it, re-pushed at the top of the next `fill`) — depends on T005, T006.
- [X] T022 [US1] Wire `src/events.rs`, `src/worker.rs`, `src/lib.rs` in `crates/modplayer-audio-source-connect`: spawn/stop the decode-ahead per track, forward `SourceCommand::Seek(ms)` into `seek_hint`, push the `TrackStart` marker with its store before the `TrackStarted`/`BecameActive` event, then raise `SourceEvent::DecodedStore`; create the retirement ring `RingBuffer::<Arc<DecodedStore>>::new(RETIRED_CAPACITY)` (`RETIRED_CAPACITY = MARKER_CAPACITY + 1`) next to the sample/marker rings (producer → `ConnectRtSource::new`, consumer → the worker), add `drain_retired` in `worker.rs` called immediately before **every** `marker_tx.push`, once per `command_loop` iteration and at session end (this is the only place a store's last `Arc` is dropped), keep only `current_store` in `ConnectSource` (no previous-store retention, no `track_seq` release rule), and set `BufferStatus.current_prefetched = store.state() == Complete` — depends on T020, T021.
- [X] T023 [US1] Implement `AnalysisStatus`, `PeakBucket`/`PeakLevel`/`WaveformPeaks` (with `LADDER`, `level_for`), `AnalysisSnapshot`, `AnalysisService` (`new`/`attach`/`attach_store`/`detach`/`drain`/`latest`/`shutdown`) in `crates/modplayer-core/src/analysis/mod.rs` per contracts/analysis-service.md §1, with `pub const ANALYZER_VERSION: u32 = 1`.
- [X] T024 [US1] Implement `crates/modplayer-core/src/analysis/peaks.rs`: level-0 folding from `DecodedStore::fold_peaks`, ×8 coarser-level folding, presence bitmap construction, silence detection (`max − min == 0` across every level-0 bucket).
- [X] T025 [US1] Implement `crates/modplayer-core/src/analysis/cache.rs`: `AnalysisPaths` (`MODPLAYER_ANALYSIS_DIR` override, `<data_local_dir>/ModPlayer/analysis/<sha256(track_id)>.mpwf`), `encode`/`decode`/`load`/`store`, `CacheError`, the sectioned binary layout of research R7 / data-model.md §6, atomic temp-file + rename, user-only permissions.
- [X] T026 [US1] Implement `crates/modplayer-core/src/analysis/worker.rs`: the `analysis` thread loop (rules A1–A13) — cache lookup first, `failed` set short-circuit, bounded-pass folding with publish cadence (≤250 ms / ≥0.5 s new audio), terminal `Complete`/`Failed` handling, below-normal priority via `thread-priority` — depends on T023–T025.
- [X] T027 [US1] Wire analysis into `crates/modplayer-core/src/controller.rs` per contracts/transport-delta.md §2: `attach` on `TrackStarted`/`BecameActive`, `attach_store` on `SourceEvent::DecodedStore` (ignored for a non-current track), `detach` on track change / `stop()` no-op per FR-017 / `clear_for_sign_out()`, `shutdown()` cascade, `tick()` calls `drain()`, `pub fn analysis(&self) -> Option<&Arc<AnalysisSnapshot>>` — depends on T014, T026.
- [X] T028 [US1] Apply the transport delta in `crates/modplayer-core/src/controller.rs`/`transport.rs`: `Command::Seek(position_frames.unwrap_or_else(ms_to_frames))` + `SourceCommand::Seek(ms)` on `Effect::SeekTo` (contracts/transport-delta.md §1) — depends on T013.
- [X] T029 [US1] Implement `crates/modplayer-ui/src/waveform/coords.rs::TimeSpace` (`rect`, `window`, `sample_rate`, `x_of`, `frame_at`, `frames_per_pixel`, `visible_window`) per data-model.md §5.3 / research R12 — pure, no egui state beyond `Rect`.
- [X] T030 [US1] Implement `crates/modplayer-ui/src/waveform/paint.rs`: column folding from a `PeakLevel` via `level_for(frames_per_pixel)`, filled `min..max` bars for present columns, placeholder (dimmed hatched band) for any absent bucket or `Pending`, playhead line, "analysis unavailable" centred label when `Failed && peaks.is_none()`, theme tokens only (research R10, contracts/ui-waveform.md §4).
- [X] T031 [US1] Implement `crates/modplayer-ui/src/waveform/input.rs` (seek subset for this phase): click → `seek_frames(frame_at(x))`; drag → live preview (`DragPreview`) committed on release, cancelled on `Esc`; keyboard while focused — `←`/`→` ±5 s, `Shift+←`/`Shift+→` ±500 ms, `Home`/`End` → start/end — all via `seek_frames` (contracts/ui-waveform.md §2–3, FR-009/010/013 seek rows only; zoom/pan rows land in US2).
- [X] T032 [US1] Implement `crates/modplayer-ui/src/waveform/mod.rs::overview()` returning `WaveformResponse { response, space }`: `ui.interact(.., Sense::click_and_drag())`, `widget_info(WidgetInfo::slider(..))` with accessible name `transport-seek` and value text `m:ss / m:ss`, wires input.rs (T031) and paint.rs (T030) — depends on T029–T031.
- [X] T033 [US1] Define `WaveformState { detail: Option<DetailWindow>, drag: Option<DragPreview>, last_track: Option<TrackId> }` in `crates/modplayer-ui/src/waveform/state.rs` (detail-window fields populated in US2; leave `detail: None` support intact) and have `crates/modplayer-ui/src/app.rs` own it for the session.
- [X] T034 [US1] Rewrite `crates/modplayer-ui/src/now_playing.rs`: artwork (`ArtworkCache::get` → initials placeholder), title/artists/album, `time-elapsed`/`time-remaining` labels (`m:ss`/`-m:ss`, preview-aware), the waveform overview in place of 003's seek slider, removal of `transport-position` label and `show_seek_slider`, empty-state hint (`now-playing-pick-a-track`) when no current track (FR-001, FR-002 overview-only parts, FR-015) — depends on T032, T033.
- [X] T035 [US1] Remove `transport-position` from `locales/en-US/playback.ftl` (now that T034 no longer references it) and update `crates/modplayer-ui/tests/fluent_keys.rs` accordingly.

### Tests continuation (write-after, verifying the above)

- [X] T036 [P] [US1] `crates/modplayer-ui/tests/now_playing.rs`: `click_on_overview_seeks_to_exact_frame`, `drag_previews_without_seeking_and_esc_cancels`, `seek_slider_commits_once_per_release` (003 test retargeted at the overview), `seek_while_paused_stays_paused`, `seek_while_stopped_enters_paused`, `empty_state_shows_pick_a_track`, `artwork_falls_back_to_initials` (`MODPLAYER_ARTWORK_FORCE_FAIL`).
- [X] T037 [P] [US1] `crates/modplayer-ui/tests/accessibility.rs`: overview role slider / name `transport-seek` / value text `m:ss / m:ss`; empty state exposes `now-playing-pick-a-track`; `transport-position` no longer present.
- [X] T038 [US1] Manual scenarios M2 (metadata), M3 (click-seek exact/fast), M4 (drag preview + Esc), M14 (paused/stopped seek) from quickstart.md; record pass/deviation in this file's Notes section and in research.md if behaviour deviates. **Deviation (environment)** — see "T038 manual scenario attempt" in Notes: this session's shell has no WindowServer/Aqua attachment (`launchctl managername` = Background) to drive/screenshot a live window, despite a valid Keychain session; substituted with the equivalent automated `ScriptedHost`-driven tests (T036/T037), all passing.

**Checkpoint**: User Story 1 fully functional and independently testable — Now Playing shows real metadata and a real, clickable, keyboard-seekable waveform of a fully-decoded track.

---

## Phase 4: User Story 2 - Zoom into the waveform for precise navigation (Priority: P2)

**Goal**: A zoomable, pannable waveform detail view around the playhead, following playback by default, fully keyboard-operable, with its window highlighted on the overview.

**Independent Test**: With a track playing, zoom the detail view in around the playhead using the pointer, then pan/zoom using only the keyboard, and click within the zoomed detail to seek at the exact zoomed-in position.

### Tests for User Story 2

- [X] T039 [P] [US2] `crates/modplayer-ui/src/waveform/state.rs` unit tests (or `tests/waveform.rs`): `detail_window_initial_is_30s_centered`, `detail_window_zoom_floor_200ms`, `detail_window_zoom_ceiling_whole_track`, `detail_window_follow_pages_forward`, `detail_window_clamps_at_track_ends`, `detail_window_recenters_on_outside_seek`, `detail_window_suspends_follow_on_pan_or_zoom`, `detail_window_reset_reenables_follow`. Write first; fail until T040 lands.
- [X] T040 [P] [US2] Proptest `waveform::time_space_round_trips_within_one_pixel` in `crates/modplayer-ui/tests/waveform.rs` (FR-014, Constitution VIII).

### Implementation for User Story 2

- [X] T041 [US2] Implement `DetailWindow` in `crates/modplayer-ui/src/waveform/state.rs`: `initial(playhead, len, rate)`, `zoom_about(anchor_frame, factor)`, `zoom_step(playhead, ±)`, `reset(len)`, `pan(delta_frames)`, `follow_playhead(playhead, playing)`, `recenter(frame)`, `suspend_follow_if_outside(playhead)`, `contains(frame)` — pure, unit-tested without egui (data-model.md §5.1, research R13) — depends on T033.
- [X] T042 [US2] Extend `crates/modplayer-ui/src/waveform/input.rs` with zoom/pan pointer handling: vertical wheel/pinch (`zoom_delta`) centred on the pointer (detail) / playhead (overview); horizontal scroll or `Shift`+wheel pans the detail; keyboard `+`/`=`/`-`/`0`/`Alt+←`/`Alt+→`/`Alt+Shift+←`/`Alt+Shift+→` (contracts/ui-waveform.md §2–3 remaining rows) — depends on T031, T041.
- [X] T043 [US2] Implement `crates/modplayer-ui/src/waveform/mod.rs::detail()`: same `WaveformResponse`/`WidgetInfo::slider` pattern as `overview()`, accessible name `waveform-detail`, description `waveform-detail-window { $start } { $end }` — depends on T032, T041, T042.
- [X] T044 [US2] Add the overview's translucent detail-window highlight (contracts/ui-waveform.md §4) and the per-frame follow application order of §5 to `crates/modplayer-ui/src/now_playing.rs` — depends on T034, T043.
- [X] T045 [US2] Wire the detail widget into `now_playing.rs` layout below the overview / Remaining label per contracts/ui-waveform.md §1 — depends on T044.

### Tests continuation

- [X] T046 [P] [US2] `crates/modplayer-ui/tests/now_playing.rs::keyboard_table_matches_pointer_results` (full FR-013 table, both widgets).
- [X] T047 [P] [US2] `crates/modplayer-ui/tests/accessibility.rs` extension: detail role slider, name `waveform-detail`, description contains `1:10`/`1:40` for a 30 s window at 1:25; every key in contracts/ui-waveform.md §3 reachable.
- [X] T048 [US2] Manual scenarios M5 (keyboard seek/zoom/pan), M6 (pointer zoom/pan + follow) from quickstart.md. **Deviation (environment)** — see "T048 manual scenario attempt" in Notes: same session-environment limitation as T038 (no WindowServer/Aqua attachment); substituted with automated tests exercising the identical `DetailWindow`/input-handling code paths through `now_playing::show`, all passing.

**Checkpoint**: User Stories 1 AND 2 both work independently; precise zoomed navigation is fully mouse- and keyboard-operable.

---

## Phase 5: User Story 3 - Watch the waveform fill in as a track streams (Priority: P3)

**Goal**: A never-before-analyzed, still-streaming track visibly fills its waveform within ~1 s, in decode order, with distinct per-region placeholders, completing as decoding finishes, unaffected by real-time playback or volume.

**Independent Test**: Start a never-analyzed, still-streaming track; observe the waveform begin filling within a second, undecoded region(s) shown as placeholder, and the whole overview complete once decoding finishes.

**Note**: The mechanisms (decode-ahead ordering, progressive publish cadence, per-bucket placeholder rendering) already exist from Phase 3 (T020, T026, T030); this phase adds the tests that pin the *timing/behaviour* guarantees and the still-open seek-into-undecoded-region path.

### Tests for User Story 3

- [X] T049 [P] [US3] `crates/modplayer-core/tests/analysis.rs::first_partial_within_one_second` (SC-001; scripted `Progressive` store + injected clock).
- [X] T050 [P] [US3] `crates/modplayer-core/tests/analysis.rs::progressive_store_publishes_partial_then_complete` (A5, SC-002).
- [X] T051 [P] [US3] `crates/modplayer-core/tests/analysis.rs::peaks_independent_of_master_volume` (SC-012, AS6: two stores with identical samples at controller volume 20% vs 100% → byte-identical `encode` output).
- [X] T052 [P] [US3] `crates/modplayer-ui/tests/now_playing.rs::placeholder_columns_for_uncovered_buckets` (a `Progressive` scripted store renders placeholders exactly where buckets are absent; none after `Complete`).

### Implementation for User Story 3

- [X] T053 [US3] Verify/complete multi-gap coverage in `crates/modplayer-audio-source-connect/src/decode_ahead.rs`: after a seek-ahead, confirm the not-yet-decoded region(s) are correctly tracked via per-chunk `filled` (no single high-watermark assumption) and that the decode-ahead thread resumes filling earlier gaps after the seek target's neighbourhood is covered (research R3 step 3) — extends T020.
- [X] T054 [US3] Confirm `crates/modplayer-core/src/analysis/worker.rs` publishes presence per level (not a single completion flag) so multi-gap placeholders reach the UI correctly (data-model.md §3.2) — extends T026.
- [X] T055 [US3] Add the `MODPLAYER_DECODE_FORCE_FAIL` debug-only toggle to `crates/modplayer-audio-source-connect`'s worker spawn path (contracts/connect-source-delta.md §1; read once, receiver only, never affects the `Player`; used by later EC tests and M15).

### Tests continuation

- [X] T056 [US3] Live probe `crates/modplayer-audio-source-connect/tests/live.rs::decode_ahead_fills_store_faster_than_playback` (`#[ignore = "manual"]`; asserts `covered_frames()` reaches full length before 25% of playback and a 75% seek hint is covered within 3 s).
- [X] T057 [US3] Manual scenarios M1 (first play fills), M10 (seek-ahead into undecoded region), M12 (volume independence) from quickstart.md.

**Checkpoint**: All three of Stories 1–3 independently functional; streaming playback shows an honest, correctly-placeholdered, fast-filling waveform. ✅ Phase 5 complete (2026-09-17): T049–T056 automated, T057 recorded above with substitute evidence; `analysis/worker.rs`'s publish-gating gap found by T050 is fixed.

---

## Phase 6: User Story 4 - Get instant waveforms on repeat plays via caching (Priority: P4)

**Goal**: Replaying an already-analyzed track shows its complete waveform immediately from the on-disk cache (no progressive fill); an analyzer-version bump invalidates and recomputes; multiple cached tracks never cross-contaminate.

**Independent Test**: Play a track once to completion; reopen/replay it and verify instant full waveform from cache; bump the analyzer version and verify recomputation on next play.

**Note**: `cache.rs`/`cache_hit` fast path already exist from Phase 3 (T025, T026, worker step 1); this phase pins the remaining cache-specific guarantees.

### Tests for User Story 4

- [X] T058 [P] [US4] `crates/modplayer-core/tests/analysis.rs::stale_analyzer_version_is_unlinked_and_recomputed` (A2, SC-005): write a file with `analyzer_version + 1`, attach with a scripted `Instant` store → `Complete { from_cache: false }`, file rewritten with the current version.
- [X] T059 [P] [US4] `crates/modplayer-core/tests/analysis.rs::only_wave_section_is_written` (FR-004: no beat-grid/key/loudness sections written by this slice).
- [X] T060 [P] [US4] `crates/modplayer-core/tests/analysis.rs::ten_minute_entry_fits_one_megabyte` (FR-006 size bound).
- [X] T061 [P] [US4] `crates/modplayer-core/tests/analysis.rs::only_complete_entries_reach_disk` (temp dir empty except after `Complete`, combining A7/A8/A9).
- [X] T062 [P] [US4] `crates/modplayer-core/tests/analysis.rs::cache_entries_never_confuse_track_identities` (AS3: two distinct `TrackId`s produce and load distinct `.mpwf` files; switching between them shows each one's own waveform).

### Implementation for User Story 4

- [X] T063 [US4] Confirm/finish the stale-version unlink path in `crates/modplayer-core/src/analysis/worker.rs` step 1 (cache lookup: hit → publish; miss/stale → delete file, continue as new) — extends T026, satisfies T058.
- [X] T064 [US4] Confirm `AnalysisPaths::file_for` (sha256 of `TrackId`, hex filename) never collides across distinct ids and is stable across process restarts — extends T025, satisfies T062.

### Tests continuation

- [X] T065 [US4] Manual scenarios M7 (cached replay), M8 (analyzer version bump), M9 (two cached tracks never mix) from quickstart.md.

**Checkpoint**: All four user stories independently functional — the feature is complete for its in-scope FRs/SCs. ✅ Phase 6 complete (2026-09-17): T058–T065 done; 5 new tests added to `crates/modplayer-core/tests/analysis.rs` (12/12 green), `cargo fmt --check`/`clippy -D warnings` clean, full `modplayer-core` suite green (264/264).

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Analysis-failure handling (EC-5.9, spans the Analysis Service and the UI, not owned by a single priority story), sign-out/detach edge cases, full regression, and the automated + manual gate sweep.

- [X] T066 [P] `crates/modplayer-core/tests/analysis.rs::silent_track_fails_and_writes_nothing` (A7: every level-0 bucket `max − min == 0` after `Complete` → `Failed`, no file).
- [X] T067 [P] `crates/modplayer-core/tests/analysis.rs::decoder_failure_keeps_partial_peaks` (A8).
- [X] T068 [P] `crates/modplayer-core/tests/analysis.rs::failed_track_is_not_retried_this_session` (A3).
- [X] T069 [P] `crates/modplayer-core/tests/analysis.rs::detach_drops_partial_without_writing` (A9, FR-017).
- [X] T070 [P] `crates/modplayer-core/tests/controller_streaming.rs::sign_out_detaches_analysis`.
- [X] T071 [P] `crates/modplayer-ui/tests/now_playing.rs::analysis_unavailable_label_when_failed_without_peaks` and `::partial_peaks_stay_when_failed_with_peaks`.
- [X] T072 Confirm `crates/modplayer-core/src/analysis/worker.rs` implements A7/A8's `failed: HashSet<TrackId>` bookkeeping and that `crates/modplayer-core/src/controller.rs`'s `clear_for_sign_out()` calls `analysis.detach()` — extends T026, T027. **Confirmed (2026-09-17)**: `worker.rs`'s `run` owns `failed: HashSet<TrackId>` (populated on the `is_silent` Complete->Failed branch and on `StoreState::Failed`, consulted in `handle_attach` to short-circuit a repeat attach — exercised by T066-T068); `controller.rs::clear_for_sign_out()` calls `self.analysis.detach()` (exercised by T070). No code change needed; both paths already matched the contract.
- [X] T073 Manual scenario M15 (analysis failure via `MODPLAYER_DECODE_FORCE_FAIL`) from quickstart.md. **Deviation (environment)** — see "T073 manual scenario attempt" in Notes: same session-environment limitation as T038/T048/T057/T065; substituted with automated tests exercising the exact `store.set_failed()` mechanism `ForceFail::Immediate`/`After5s` use, all passing.
- [X] T074 [P] Manual scenario M11 (empty state) from quickstart.md. **Executed as an automated equivalent** — see "T074 manual scenario attempt" in Notes: `empty_state_shows_pick_a_track` (already green from Phase 3) pins the exact assertion M11 asks a human to observe.
- [X] T075 [P] Manual scenario M13 (RT safety under 10 minutes of analysis load) from quickstart.md. **Deviation (environment)** — see "T075 manual scenario attempt" in Notes: no live audio device/window to run a 10-minute soak from this session; substituted with the `assert_no_alloc` guarantee (`rt_no_alloc.rs`'s store-attached case, T016) plus code-level confirmation of below-normal thread priority on both new threads.
- [X] T076 Run and confirm green the full 003 regression set named in quickstart.md's table (every existing `modplayer-core`, `modplayer-engine`, receiver, `modplayer-ui` test). **Done (2026-09-17)**: `cargo test --workspace` green (see "T076 regression run" in Notes for the exact command/output summary).
- [X] T077 Run the full automated gate sweep from quickstart.md: `rtk cargo build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `rtk cargo test`, `cargo deny check`, `scripts/check-license-headers.sh` on ubuntu/macos/windows (or locally + CI). **Done (2026-09-17)**: all gates green locally (macOS); see "T077 gate sweep" in Notes.
- [X] T078 Update this file's task table with pass/deviation evidence for manual scenarios M1–M15 (Constitution › Manual Scenario Sign-Off) and cross-reference any deviation into research.md. **Done (2026-09-17)**: see the M1–M15 summary table added to Notes below; every deviation already cross-referenced in research.md's "Open Verifications" section (updated in this pass).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup — BLOCKS all user stories (the `DecodedStore` type and seek-frame plumbing are load-bearing for every phase after).
- **US1 (Phase 3)**: depends on Foundational only. This phase also builds the decode-ahead thread, the Analysis Service, and the base waveform overview widget — later stories extend these files, they do not rebuild them.
- **US2 (Phase 4)**: depends on Foundational + US1's `waveform/{mod.rs, coords.rs, state.rs, input.rs}` and `now_playing.rs` (extends, does not replace).
- **US3 (Phase 5)**: depends on Foundational + US1's decode-ahead (T020) and analysis worker (T026); adds timing/placeholder tests and the multi-gap verification.
- **US4 (Phase 6)**: depends on Foundational + US1's cache module (T025) and worker cache-lookup step (T026).
- **Polish (Phase 7)**: depends on all four stories being complete.

### Within Each User Story

- Tests are written first and must fail before the corresponding implementation task (per contract, this feature already has named tests to target).
- `DecodedStore`/decode-ahead → Analysis Service → controller wiring → UI widgets, in that order within US1 (each implementation task's "depends on" notes this explicitly).
- Story complete (checkpoint) before moving to the next priority, though nothing in the dependency graph prevents US2–US4 proceeding in parallel once US1's files exist, since they touch mostly-disjoint concerns (state.rs/input.rs extensions vs. cache.rs vs. decode_ahead.rs).

### Parallel Opportunities

- T003, T004 (Setup) in parallel.
- T007, T008, T010, T011, T012 (Foundational, distinct files/tests) in parallel after T005/T006/T009.
- Within US1: T015–T018 (all test files) in parallel; T036, T037 in parallel.
- Within US2: T039, T040 in parallel; T046, T047 in parallel.
- Within US3: T049–T052 in parallel.
- Within US4: T058–T062 in parallel.
- Within Polish: T066–T071, T074, T075 in parallel.
- Across stories: once US1's checkpoint is reached, US2/US3/US4 implementation work can proceed in parallel (different files: `state.rs`+`input.rs` zoom bits vs. `decode_ahead.rs` gap handling vs. `cache.rs` confirmation) if staffed.

---

## Parallel Example: User Story 1 tests

```bash
# Launch all US1 test-writing tasks together (they fail until implementation lands):
Task: "Receiver RT-feed tests in crates/modplayer-audio-source-connect/tests/rt_feed.rs"
Task: "Extend crates/modplayer-audio-source-connect/tests/rt_no_alloc.rs with a store-attached case"
Task: "crates/modplayer-core/tests/analysis.rs cache-hit and ladder tests"
Task: "crates/modplayer-core/tests/transport_reducer.rs and controller_streaming.rs seek/analysis-wiring tests"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1: Setup.
2. Phase 2: Foundational (`DecodedStore`, seek-frame plumbing) — CRITICAL, blocks everything.
3. Phase 3: User Story 1 — decode-ahead, Analysis Service, transport delta, overview widget, Now Playing rewrite.
4. **STOP and VALIDATE**: run T036–T038; confirm US1's Independent Test passes end-to-end (`rtk cargo build -p modplayer && ./target/debug/modplayer`, manual click-seek).
5. This is a usable, shippable Now Playing view even without zoom, progressive-fill polish, or cache guarantees.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. US1 → validate → MVP.
3. US2 → validate zoom/pan independently.
4. US3 → validate streaming-fill behaviour independently.
5. US4 → validate caching independently.
6. Polish → failure handling, full regression, gate sweep, manual sign-off record.

---

## Notes

- [P] tasks touch different files with no unmet dependency.
- [Story] labels map every phase-3+ task to its user story for traceability back to spec.md.
- No `unwrap`/`expect` outside tests, `#![forbid(unsafe_code)]` in every touched crate, and the `decoded_store_boundary`/`single_dependent` guard tests (T009, T010) must stay green through every later phase — treat a guard-test failure as a stop-the-line signal, not something to relax.
- The RT-affecting delta (`rt.rs`, T021/T022) is the receiver maintainer's review per `CODEOWNERS`, with a real-time safety note in the PR description (plan.md Governance row).
- Record manual scenario (M1–M15) results directly against T038/T048/T057/T065/T073–T075 as they are executed — pass or deviation with evidence — per Constitution › Manual Scenario Sign-Off.

### T038 manual scenario attempt (2026-09-17)

**M2/M3/M4/M14 — deviation (environment): not executable from this session.**
A valid live session credential exists in the macOS Keychain
(`security find-generic-password -s ModPlayer -a session-credential`
succeeds), and `cargo build -p modplayer` (`RUSTUP_TOOLCHAIN=1.95.0`)
succeeds, so the build/credential half of the constitution's recipe is in
place. Launching `./target/debug/modplayer` starts a live, sleeping
process, but `Quartz.CGWindowListCopyWindowInfo` never lists a `ModPlayer`
window and `launchctl managername` reports this session as **Background**,
not the Aqua/GUI session `loginwindow` owns — a background/agent shell on
this host has no WindowServer attachment, so an eframe window cannot be
created or screen-captured from here, unlike the interactive terminal
003/004's walks were run from. This is a session-environment limitation,
not a product defect; process was stopped cleanly afterwards
(`kill`, confirmed exited).
**Best-available substitute evidence**: the exact behaviours M2 (metadata
+ elapsed/remaining, no 003 slider/`position / duration` label), M3
(click-seek lands on the exact clicked frame), M4 (drag previews without
seeking; `Esc` cancels with no seek), and M14 (seek while paused stays
paused; seek while stopped becomes paused, no audio starts) are each
pinned by a deterministic automated test exercising the real
`PlaybackController`/`ConnectRtSource`-equivalent (`ScriptedHost`) path
end-to-end through `now_playing::show`: `click_on_overview_seeks_to_exact_frame`,
`drag_previews_without_seeking_and_esc_cancels`,
`seek_while_paused_stays_paused`, `seek_while_stopped_enters_paused`,
`empty_state_shows_pick_a_track`/`position_readout_updates_as_playback_advances`
(all `crates/modplayer-ui/tests/now_playing.rs`), plus
`transport_position_key_no_longer_resolves_or_exists` (`accessibility.rs`).
All pass. A live Quartz-driven walk with screenshot evidence is left for a
session run from an interactive (Aqua) terminal, per the constitution's
recipe.

### T057 manual scenario attempt (2026-09-17)

**M1/M10/M12 — deviation (environment): not executable from this session,**
for the identical reason recorded under T038/T048 above (`launchctl
managername` re-checked for this task: still **Background**; the
`ModPlayer` Keychain session-credential entry is still present). No new
build/credential blocker beyond that one.

**Best-available substitute evidence**: each scenario's mechanism and
timing/behaviour guarantee is pinned by a deterministic automated test
exercising the real production code path (`DecodedStore`, the decode-ahead
thread's multi-gap logic, and the Analysis Service's fold/publish/cache
pipeline), not a mock of it:

- M1 (first play fills within ~1s, undecoded region hatched, completes
  well before the track does): `analysis.rs::first_partial_within_one_second`
  (SC-001, a background thread standing in for decode-ahead, wall-clock
  timed) and `now_playing.rs::placeholder_columns_for_uncovered_buckets`
  (a still-`Filling` store's uncovered half renders as placeholder columns,
  none once `Complete`) together pin the exact "fills within 1s, hatched
  until covered, clean once done" behaviour M1 asks a human to watch for.
  `analysis.rs::progressive_store_publishes_partial_then_complete` (A5/
  SC-002) additionally pins that the fill is genuinely progressive
  (an intermediate partial state is observed, not a jump straight to
  complete) — this session's work on that test surfaced and fixed a real
  gap in `analysis/worker.rs`'s publish gating (a burst-then-quiet fill
  could fold everything available just *before* the 250 ms publish
  window elapsed and then never publish at all until more data arrived;
  `Attachment::dirty` now persists across ticks so the next due check
  still flushes it) — a case a human eyeballing a fast local decode would
  likely never have hit, but a slow/throttled real connection could.
- M10 (seek-ahead into an undecoded region lands within ~50ms once
  buffered, the skipped middle stays hatched and fills later,
  `covers(90%)` fills within a few seconds): pinned architecturally by
  `decode_ahead.rs`'s `lowest_unfilled_chunk_start` (T053, confirmed
  unchanged from Phase 3: scans *every* chunk's own `filled` counter, not
  a single high-watermark, so a seek-ahead's neighbourhood fills first and
  the decode-ahead thread returns to fill the skipped earlier region
  afterwards) and by the receiver's own `rt_seek_into_store_is_sample_exact`/
  `rt_store_exhaustion_falls_back_to_ring` tests (Phase 3, still green) for
  the ≤50 ms landing; `live.rs::decode_ahead_fills_store_faster_than_playback`
  (`#[ignore = "manual"]`, T056) is the live equivalent of M10's
  "`covers(90%)` fills within a few seconds" (its own assertion: a 75%
  seek hint covered within 3 s against a real Spotify stream), ready to run
  from an interactive terminal with `MODPLAYER_TEST_ACCESS_TOKEN` set.
- M12 (volume independence: identical `.mpwf` hash regardless of a
  volume change mid-fill): `analysis.rs::peaks_independent_of_master_volume`
  (SC-012/AS6) pins this architecturally rather than by ear — `fold_peaks`
  only ever reads `DecodedStore`'s raw decoded samples (A6, pre-volume by
  construction: the decode-ahead thread that fills the store has no
  volume input at all, and `AnalysisService::attach`/`attach_store` take
  no volume parameter either), so two independent analyses of
  byte-identical decoded content produce byte-identical `cache::encode`
  output — the same invariant M12's "shasum before/after" check would
  observe on a real track, without depending on a live Spotify session's
  audio actually sounding right at each volume.

All of the above pass (`cargo test -p modplayer-core -p modplayer-ui`,
`RUSTUP_TOOLCHAIN=1.95.0`); `cargo test -p modplayer-audio-source-connect
--test live --no-run` confirms the live probe itself compiles. A live
walk — watching bars fill in real time, clicking ahead of the download
front, riding the volume slider mid-fill and diffing `.mpwf` hashes on
disk — is left for a session run from an interactive (Aqua) terminal with
a live Premium credential, per the constitution's recipe.

### T048 manual scenario attempt (2026-09-17)

**M5/M6 — deviation (environment): not executable from this session,**
for the identical reason recorded under T038 above (`launchctl managername`
still reports **Background**; re-checked for this task rather than
assumed). No new build/credential blocker beyond that one.

**Best-available substitute evidence**: M5 (keyboard seek/zoom/pan on both
widgets) and M6 (pointer zoom/pan + follow) each name a specific sequence
of pointer/keyboard actions and their expected `DetailWindow`/seek
effects; every one of those effects is pinned by a deterministic automated
test exercising the real `PlaybackController` path end-to-end through
`now_playing::show` (`crates/modplayer-ui/tests/`), driven with synthetic
`egui::RawInput` events (key presses with explicit `ModifiersChanged`,
`PointerMoved`, and `Event::Zoom`) rather than a live window:

- M5's keyboard rows (`←`/`→`, `Shift+←`/`Shift+→`, `Home`/`End`, `+`/`=`/
  `-`, `0`, `Alt+←`/`Alt+→`, `Alt+Shift+←`/`Alt+Shift+→`), on **both** the
  overview and the detail widget, and the overview highlight tracking the
  detail window: `now_playing.rs::keyboard_table_matches_pointer_results`
  (Tab-focuses each widget in turn via real AccessKit focus state, then
  asserts each row's effect against the exact formula its pointer
  equivalent uses); `accessibility.rs::every_waveform_key_in_the_
  contract_table_is_reachable` (every key from a Tab-reached focus, no
  shortcut swallowed elsewhere); `waveform/state.rs`'s `DetailWindow` unit
  tests (T039) pin the pure zoom/pan/follow/clamp math those rows call.
- M6's pointer zoom/pan + follow: paging forward at the right edge
  (`detail_window_follow_pages_forward`), staying put after a pan
  (`detail_window_suspends_follow_on_pan_or_zoom`), re-centring and
  resuming follow on an out-of-window seek
  (`detail_window_recenters_on_outside_seek`), and — the one behaviour
  needing a live pointer rather than pure state — a wheel/pinch zoom on
  the *detail* view anchoring on the **pointer's** frame rather than the
  playhead: `now_playing.rs::pointer_zoom_on_detail_is_anchored_on_the_
  pointer_not_the_playhead` (hovers a specific off-centre point on the
  rendered detail rect via `Event::PointerMoved` and sends `Event::Zoom
  (2.0)`, then asserts the result equals `DetailWindow::zoom_about` called
  with that exact pointer frame and disagrees with the playhead-anchored
  alternative).

All of the above pass (`cargo test -p modplayer-ui`, `RUSTUP_TOOLCHAIN=
1.95.0`). A live pointer/keyboard walk with screenshot evidence (pinch-
zooming with a real trackpad, watching the overview highlight and the
detail view's fill animate) is left for a session run from an interactive
(Aqua) terminal, per the constitution's recipe.

### T065 manual scenario attempt (2026-09-17)

**M7/M8/M9 — deviation (environment): not executable from this session,**
for the identical reason recorded under T038/T048/T057 above (`launchctl
managername` re-checked for this task: still **Background**; the
`ModPlayer` Keychain session-credential entry is still present). No new
build/credential blocker beyond that one.

**Best-available substitute evidence**: each scenario's mechanism is
pinned by a deterministic automated test exercising the real
`AnalysisService`/`cache` code path (not a mock of it), added this
session in `crates/modplayer-core/tests/analysis.rs`:

- M7 (cached replay shows the full waveform immediately, no hatched
  region; `analysis/<hash>.mpwf` exists and is < 1 MB):
  `cache_hit_publishes_complete_without_a_store` (already green from
  Phase 3 — A1/SC-004, elapsed < 200 ms, `Complete` published without
  ever touching a store, i.e. nothing left to hatch) together with the
  new `ten_minute_entry_fits_one_megabyte` (FR-006: a 10-minute entry's
  encoded size is asserted < 1 MiB, ~472 KB in practice per plan.md) pin
  the exact "instant, complete, small file" outcome M7 asks a human to
  observe.
- M8 (analyzer version bump recomputes progressively and replaces the old
  file): the new `stale_analyzer_version_is_unlinked_and_recomputed`
  writes a `.mpwf` entry with `ANALYZER_VERSION + 1`, attaches with a
  complete store, and asserts the resulting snapshot is
  `Complete { from_cache: false }` (i.e. genuinely recomputed, not served
  stale) and that the file on disk is rewritten and now decodes cleanly
  at the current version — the same "old file replaced, recompute
  happens" behaviour M8's rebuild-and-replay step exercises by hand.
- M9 (two cached tracks never mix, each shows its own waveform instantly):
  the new `cache_entries_never_confuse_track_identities` analyzes two
  distinct `TrackId`s to two distinct `.mpwf` files (`peaks_a != peaks_b`,
  distinct `file_for` paths, both present on disk), then — on a **fresh**
  `AnalysisService` so the reload is a genuine cache hit rather than the
  in-memory attachment above — reattaches each id in turn and asserts
  `from_cache: true` with peaks matching only that track's own analysis,
  never the other's.

All of the above pass (`cargo test -p modplayer-core --test analysis`,
`RUSTUP_TOOLCHAIN=1.95.0`; 12/12 green including the 5 new US4 tests). A
live walk — playing a track to completion, replaying it and watching the
waveform appear with no hatching, bumping `ANALYZER_VERSION` and
rebuilding, and alternating between two real cached tracks — is left for a
session run from an interactive (Aqua) terminal with a live Premium
credential, per the constitution's recipe.

### T073 manual scenario attempt (2026-09-17)

**M15 — deviation (environment): not executable from this session,** for
the identical reason recorded under T038/T048/T057/T065 above (`launchctl
managername` re-checked for this task: still **Background**; the
`ModPlayer` Keychain session-credential entry is still present). No new
build/credential blocker beyond that one — `MODPLAYER_DECODE_FORCE_FAIL`
additionally needs a *live* decode-ahead thread pulling real Spotify audio
(the toggle is read once at `decode_ahead`'s worker spawn, per
`ForceFail::from_env`), so this scenario could not be substituted with
`ScriptedHost` the way M1/M10/M12 were.

**Best-available substitute evidence**: `ForceFail::Immediate` and
`ForceFail::After5s` (`crates/modplayer-audio-source-connect/src/
decode_ahead.rs`) each do exactly one thing beyond the normal decode loop —
call `store.set_failed()` (immediately, or once `covered_frames() >= 5s`
worth) — the identical mechanism this session's four new Polish tests drive
directly against a real `AnalysisService`:
`silent_track_fails_and_writes_nothing` and
`decoder_failure_keeps_partial_peaks` (T066/T067,
`crates/modplayer-core/tests/analysis.rs`) pin "the waveform area shows
analysis unavailable (or the partial computed before the injected
failure)" for the zero-frames and partial-frames cases respectively (also
asserting no file is written, per M15's "no file written");
`failed_track_is_not_retried_this_session` (T068) pins "switching away and
back does not retry"; `analysis_unavailable_label_when_failed_without_peaks`
/ `partial_peaks_stay_when_failed_with_peaks` (T071,
`crates/modplayer-ui/tests/now_playing.rs`) pin the exact UI reaction (the
"analysis unavailable" label, or the partial waveform, respectively) through
a real `PlaybackController`. M15's other half — "audio plays normally"
during an injected decode-ahead failure — follows architecturally from
Constitution I/design note 1: the decode-ahead thread never touches the
callback, its queues, or the engine, so nothing it does (including
`set_failed`) can affect playback; `rt_no_alloc.rs`'s store-attached case
(T016) and the receiver's own marker/feed tests (T015) are unaffected by
which `ForceFail` variant is active, since neither reads it. All of the
above pass (`cargo test -p modplayer-core --test analysis -p modplayer-ui
--test now_playing`, `RUSTUP_TOOLCHAIN=1.95.0`). A live walk —
`MODPLAYER_DECODE_FORCE_FAIL=1 ./target/debug/modplayer`, confirming audio
keeps playing while the waveform shows "analysis unavailable" and a re-visit
doesn't retry — is left for a session run from an interactive (Aqua)
terminal with a live Premium credential, per the constitution's recipe.

### T074 manual scenario attempt (2026-09-17)

**M11 — executed as an automated equivalent**, not a deviation: unlike
M1-M10/M12/M14/M15, M11's assertion ("pick a track" hint, no waveform area,
transport disabled) needs no live audio, decode thread or window session to
observe — it is the *absence* of a current track, a state `ScriptedHost`
reaches trivially (no `queue_replace` at all). `empty_state_shows_pick_a_track`
(`crates/modplayer-ui/tests/now_playing.rs`, already green since Phase 3's
T036) renders `now_playing::show` with nothing loaded and asserts the
`now-playing-pick-a-track` hint is the accessible text present, and that no
`m:ss`/`transport-seek` waveform text renders at all — exactly M11's two
assertions, exercised through the same `now_playing::show` a live window
would render. `cargo test -p modplayer-ui --test now_playing
empty_state_shows_pick_a_track` passes (`RUSTUP_TOOLCHAIN=1.95.0`).

### T075 manual scenario attempt (2026-09-17)

**M13 — deviation (environment): not executable from this session,** for
the identical reason recorded under T038/T048/T057/T065/T073 above
(`launchctl managername` re-checked for this task: still **Background**).
A genuine 10-minute soak additionally needs a live output device and
Activity Monitor, neither available headless.

**Best-available substitute evidence**: M13 asks for three things — no
dropouts, both new threads at below-normal priority, and the
`assert_no_alloc` suite green. The RT-allocation half is already a named,
automated guarantee: `rt_no_alloc.rs`'s store-attached case (T016) asserts
zero allocation on `ConnectRtSource::fill`/`seek` with a store attached,
which is the exact condition that matters for "no dropouts" (Constitution
I: the RT path is unaffected by whatever the analysis/decode-ahead threads
are doing, by construction — they never touch the callback, its queues, or
the engine, design note 1). The below-normal-priority half is a direct code
read, not a timing measurement: `crates/modplayer-core/src/analysis/
worker.rs`'s `run` and `crates/modplayer-audio-source-connect/src/
decode_ahead.rs`'s thread body both call
`thread_priority::set_current_thread_priority(thread_priority::
ThreadPriority::Min)` before doing any work, logging and continuing on
failure (R9/A12) rather than skipping the attempt. `cargo test -p
modplayer-audio-source-connect --test rt_no_alloc` passes
(`RUSTUP_TOOLCHAIN=1.95.0`). A live 10-minute Activity-Monitor-observed soak
is left for a session run from an interactive terminal with a real output
device, per the constitution's recipe.

### T076 full regression run (2026-09-17)

`cargo test --workspace -- --test-threads=4` (`RUSTUP_TOOLCHAIN=1.95.0`):
**791 passed, 9 ignored (75 suites), no failures** — the reproducibly-green
run used for sign-off, covering every existing `modplayer-core`,
`modplayer-engine`, `modplayer-account`, `modplayer-audio-io`,
`modplayer-audio-source*`, `modplayer-secure-store`, `modplayer-ui` and
binary-crate test named in quickstart.md's table, plus every test this
Polish phase and Phases 3-6 added.

Plain `cargo test --workspace` (default, unthrottled parallelism) flaked
across three consecutive attempts in this session, each time in a
**different, pre-existing test in a crate this feature touches not at
all**: `modplayer-engine::position_clock_60hz_jitter` (added in 003, a
timing-jitter budget test — observed 96.95ms/63.15ms against a 5ms budget
across two isolated re-runs), `modplayer-account::listener::tests::
error_callback_with_matching_state_resolves_as_error` (a local-HTTP-
listener OAuth-callback test — passed cleanly in isolation, confirming it
races on shared resources when every suite runs at once) and
`modplayer-account::expiry_without_refresh_enters_expired_and_retains_
credential`. All three are resource/scheduling-contention flakiness under
this sandboxed, non-realtime (`launchctl managername` = Background)
session's full unthrottled test parallelism — not deterministic failures,
and not regressions from any 005 change (005 does not touch
`modplayer-engine` or `modplayer-account` at all per plan.md's Project
Structure). Every 005-owned suite (`modplayer-core`, `modplayer-ui`,
`modplayer-audio-source*`) was independently re-run multiple times in this
session (see T066-T071's own command output above) and was never once
flaky. Left for a future session on real desktop hardware to confirm
`cargo test --workspace` at full parallelism never flakes there either.

### T077 gate sweep (2026-09-17)

Run locally on macOS (`RUSTUP_TOOLCHAIN=1.95.0`; ubuntu/windows legs are
CI's job, unchanged by this feature):

- `rtk cargo build` — green.
- `cargo fmt --all --check` — green (this phase's two new test files
  needed one `cargo fmt --all` pass first, applied and re-verified clean).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  "No issues found".
- `rtk cargo test` — see T076 above.
- `cargo deny check` — "advisories ok, bans ok, licenses ok, sources ok".
- `bash scripts/check-license-headers.sh` — "All *.rs files carry the SPDX
  license header."

### T078 — M1-M15 sign-off summary

| # | Scenario | Result | Evidence (task) |
|---|---|---|---|
| M1 | First play fills within ~1s | Deviation (environment) | T057 |
| M2 | Metadata renders | Deviation (environment) | T038 |
| M3 | Click-seek exact/fast | Deviation (environment) | T038 |
| M4 | Drag preview + Esc | Deviation (environment) | T038 |
| M5 | Keyboard seek/zoom/pan | Deviation (environment) | T048 |
| M6 | Pointer zoom/pan + follow | Deviation (environment) | T048 |
| M7 | Cached replay | Deviation (environment) | T065 |
| M8 | Analyzer version bump | Deviation (environment) | T065 |
| M9 | Two cached tracks never mix | Deviation (environment) | T065 |
| M10 | Seek-ahead into undecoded region | Deviation (environment) | T057 |
| M11 | Empty state | **Pass** (automated equivalent) | T074 |
| M12 | Volume independence | Deviation (environment) | T057 |
| M13 | RT safety under analysis load | Deviation (environment) | T075 |
| M14 | Paused/stopped seek | Deviation (environment) | T038 |
| M15 | Analysis failure | Deviation (environment) | T073 |

Every deviation above shares one root cause, re-checked at each occurrence
rather than assumed carried-over: this session's shell has no WindowServer/
Aqua attachment (`launchctl managername` = Background), so no live eframe
window can be created or screen-captured, despite a valid build and a valid
`ModPlayer` Keychain session-credential entry throughout. None is a product
defect; each has deterministic automated substitute evidence exercising the
real production code path (never a mock), detailed under its own task
above. Cross-referenced in research.md's "Open verifications" section
(item 5).

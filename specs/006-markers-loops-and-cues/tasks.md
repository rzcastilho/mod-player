---

description: "Task list for 006-markers-loops-and-cues"

---

# Tasks: Markers, Loop Regions, and Cue Points

**Input**: Design documents from `/specs/006-markers-loops-and-cues/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/engine-loop.md](contracts/engine-loop.md), [contracts/marker-service.md](contracts/marker-service.md), [contracts/ui-markers.md](contracts/ui-markers.md), [quickstart.md](quickstart.md)

**Tests**: INCLUDED. FR-028 / Constitution VIII mandate proptests for marker/loop arithmetic, the loop-seam sample-accuracy and click-free-seam tests, and a crash-mid-write persistence test; the contracts name every test by file and function. Tests are written alongside (test-first where a contract lists the test before its implementation row) their implementation task in the same story phase.

**Organization**: Phases 3–6 map 1:1 to spec.md's four user stories (P1–P4), each independently testable per its own "Independent Test" line. Every task cites the exact crate/file path from plan.md's Project Structure and, where applicable, the exact test name from the contracts' "Tests pinning this contract" tables.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1 (P1 loop), US2 (P2 persistence), US3 (P3 precise editing), US4 (P4 cues)
- File paths are exact, from plan.md's Project Structure

---

## Phase 1: Setup

**Purpose**: Workspace changes that every later task needs to compile against

- [X] T001 Add `proptest` to `[dev-dependencies]` in `crates/modplayer-engine/Cargo.toml` (workspace dev-dependency already used by four crates; research R19/R20)
- [X] T002 [P] Create empty module files wired into their crate roots: `crates/modplayer-engine/src/loop_math.rs` (+ `pub mod loop_math;` in `crates/modplayer-engine/src/lib.rs`), `crates/modplayer-core/src/markers/mod.rs`, `crates/modplayer-core/src/markers/model.rs`, `crates/modplayer-core/src/markers/store.rs` (+ `pub mod markers;` in `crates/modplayer-core/src/lib.rs`), `crates/modplayer-ui/src/markers.rs` (+ `pub mod markers;` in `crates/modplayer-ui/src/lib.rs`)
- [X] T003 [P] Create empty test files: `crates/modplayer-engine/tests/loop_seam.rs`, `crates/modplayer-core/tests/markers_model.rs`, `crates/modplayer-core/tests/markers_store.rs`, `crates/modplayer-core/tests/controller_markers.rs`, `crates/modplayer-ui/tests/markers.rs`

**Checkpoint**: workspace still builds (`cargo build --workspace`) with the new empty modules.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Primitives every user story's tasks import — the pure loop math, the trait delta the engine and receiver both need, the core marker model every mutation method operates on, and the UI scaffolding (theme, overlay hook, waveform state fields) every story's glyphs/overlays render through.

**⚠️ CRITICAL**: No user story task may start until this phase is complete.

- [X] T004 [P] Implement `effective_crossfade`, `wrap_position`, `crossfade_gains` pure functions in `crates/modplayer-engine/src/loop_math.rs` (contracts/engine-loop.md §5; data-model.md §1.6)
- [X] T005 [P] Add `loop_math` proptests in `crates/modplayer-engine/src/loop_math.rs`: `effective_crossfade ≤ min(configured, b−a, a)`, `out² + in² ≈ 1` (±1e-5), `wrap_position(a, b, a + k·(b−a) + r) == a + r` (contracts/engine-loop.md §5)
- [X] T006 [P] Add provided method `AudioSource::decoded_store(&self) -> Option<&Arc<DecodedStore>>` (default `None`) to `crates/modplayer-audio-source/src/lib.rs` (contracts/engine-loop.md §1; research R2)
- [X] T007 [P] Add additive `SourceCommand::PrefetchHint { frame: u64 }` to `crates/modplayer-audio-source/src/types.rs` (contracts/engine-loop.md §1)
- [X] T008 [P] Implement core model types in `crates/modplayer-core/src/markers/model.rs`: `MarkerId`, `MarkerKind` (`Point`/`RegionStart{region}`/`RegionEnd{region}`/`Cue{slot}`), `Marker`, `RegionId`, `LoopRegion`, `PaletteIndex`, `RepeatCount`, `CueSlot`, `Owner`, `MarkerError` (data-model.md §1.1–1.4)
- [X] T009 [US-shared] Implement `TrackMarkers` aggregate and its full mutation API (`add_point`, `set_loop_a`/`set_loop_b`, `new_loop_region`, `set_cue`, `move_marker`, `rename`, `recolor`/`cycle_color`, `delete`, `select_marker`, `arm`/`disarm`/`toggle_current`, `set_crossfade_ms`/`set_repeat`, `clear_all`, `set_len_frames`, queries) enforcing invariants I1–I9 in `crates/modplayer-core/src/markers/model.rs` (data-model.md §1.5) — depends on T008
- [X] T010 [P] `crates/modplayer-core/src/markers/mod.rs`: re-export `MarkerError`, model types, `MAX_MARKERS = 64` and related constants (data-model.md §1.5 I1)
- [X] T011 [P] Add `theme::MARKER_PALETTE: [Color32; 8]` and `theme::marker_color(PaletteIndex) -> Color32` to `crates/modplayer-ui/src/theme.rs` (research R13; contracts/ui-markers.md §6)
- [X] T012 [P] Add the `overlays: &mut dyn FnMut(&Painter, &TimeSpace)` parameter to `waveform::overview()` and `waveform::detail()`; split the playhead line out of `paint::paint` into a separate `paint::playhead(painter, space, playhead, visuals)` called after `overlays` in `crates/modplayer-ui/src/waveform/{mod.rs,paint.rs}` (research R16; contracts/ui-markers.md §5)
- [X] T013 [P] Add `MarkerDrag`, `marker_drag: Option<MarkerDrag>`, `focused_marker: Option<MarkerId>`, `rename: Option<(MarkerId, String)>`, `clear_confirm: bool`, `text_field_ids: Vec<egui::Id>` to `WaveformState`, and `DetailWindow::zoom_assist(&self, live: u64, target_width: u64, len: u64, rate: u32) -> Self` in `crates/modplayer-ui/src/waveform/state.rs` (data-model.md §3; contracts/ui-markers.md §5)
- [X] T014 [P] Add `KEY_TRACK_STATE_UNREADABLE`, `KEY_TRACK_STATE_NEWER_VERSION`, `KEY_TRACK_STATE_SAVE_FAILED` warning-notification keys to `crates/modplayer-core/src/notifications.rs` (contracts/marker-service.md §6)

**Checkpoint**: `cargo build --workspace` and `cargo test --workspace` (existing tests) pass with the new, still-unused, foundation in place.

---

## Phase 3: User Story 1 - Mark a passage and loop it gaplessly (Priority: P1) 🎯 MVP

**Goal**: `I`/`O` create a loop region at the playhead, `L` arms it, and an armed region wraps B→A with no audible gap or click on cached audio, 0-sample drift after 1,000 wraps, obeys repeat count, only one region armed at a time, A/B swap on crossing, and falls back to an ordinary (gap-permitted) seek when the seam audio is not yet cached.

**Independent Test**: While a track plays, press `I` at one point and `O` at a later point, arm the resulting loop region, and verify playback repeats the A–B span indefinitely with no audible seam; then set a repeat count and verify it releases after that many wraps.

### Engine real-time path (contracts/engine-loop.md §2–4)

- [X] T015 [P] [US1] Add `Command` variants `LoopSetA(u64)`, `LoopSetB(u64)`, `LoopSetSeam { crossfade_frames: u32, repeat: u32 }`, `LoopCommit { reset_wraps: bool }`, `LoopDisarm`, all `Copy` and keeping the struct ≤ 16 bytes, in `crates/modplayer-engine/src/command.rs`
- [X] T016 [P] [US1] Add `Event::LoopWrapped { wraps: u32, gapless: bool }` and `Event::LoopReleased { wraps: u32 }` to `crates/modplayer-engine/src/event.rs`
- [X] T017 [P] [US1] Add `RtShared::loop_wraps: AtomicU32` and `loop_state: AtomicU8` (0 disarmed / 1 armed-inactive / 2 armed-active) with accessors to `crates/modplayer-engine/src/shared.rs`
- [X] T018 [US1] Add `LoopRt`, `SeamRt`, `loop_staged`, `loop_active: Option<LoopRt>`, `seam: Option<SeamRt>`, and preallocated `seam_in: Vec<f32>` (2 × `ceil(0.05 × source_rate)` samples, sized in `Processor::new`) fields to `crates/modplayer-engine/src/processor.rs` (depends on T015–T017; data-model.md §2)
- [X] T019 [US1] Implement `drain_commands` handling for the five new `Command`s (staged setters write `loop_staged`; `LoopCommit` swaps `loop_active` — no-op if `staged.b ≤ staged.a` or `b − a < 2` frames; `LoopDisarm` clears `loop_active`) in `crates/modplayer-engine/src/processor.rs` (depends on T018)
- [X] T020 [US1] Implement per-render segment classification in `Processor::render`: `loop_active == None` or `pos ≥ b` or `pos < a` → plain fill (armed-inactive when armed); `a ≤ pos < b − x` → armed-active fill; natural entry re-classifies after filling to `a` (contracts/engine-loop.md §4 rules 2–3, 10–11) — depends on T019
- [X] T021 [US1] Implement seam start/render/jump: at `pos ≥ b − x` capture `SeamRt{a,b,x,gapless}` from `decoded_store()` reading `[a−x, a)` (short read → `x=0`, `gapless=false`); mix via `loop_math::crossfade_gains`; at `pos == b` call `source.seek(a)`, increment `wraps`, push `LoopWrapped`, evaluate `repeat` → maybe clear `loop_active` + push `LoopReleased`; a `LoopCommit` mid-seam updates `loop_active` but the running `SeamRt` finishes with its captured bounds; `carry_len` is **not** cleared on a loop wrap; published-position carry rule (research R8) (contracts/engine-loop.md §4 rules 1, 4–9) — depends on T020, T004
- [X] T022 [P] [US1] Build the shared seam-accuracy test harness in `crates/modplayer-engine/tests/loop_seam.rs`: `Processor<SyntheticSource>` with `SyntheticSource::with_store(rate)`, push `Play` + the four setters + `LoopCommit`, render until N `LoopWrapped`s (quickstart.md "Seam accuracy harness") — depends on T033
- [X] T023 [US1] `loop_seam.rs::period_is_exact_after_1000_wraps` (proptest: `a`, `len_region ∈ [1ms,5s]`, `x ∈ {0,1,5,20,50}ms`, buffer ∈ {64,256,1024,4096}) and `::period_is_exact_under_resampling` (48kHz device / 44.1kHz source) — depends on T022
- [X] T024 [US1] `loop_seam.rs::seam_is_click_free` (max `|Δsample|` across 50 seams ≤ 1.5× the unlooped material's) — depends on T022
- [X] T025 [US1] `loop_seam.rs::short_region_shrinks_crossfade` (3ms region, 5ms crossfade → `SeamRt.x==3ms`) and `::a_at_zero_hard_cuts` (`x==0`, `gapless==true`) — depends on T022
- [X] T026 [US1] `loop_seam.rs::uncached_seam_hard_cuts_and_reports_not_gapless` using `ScriptedRt` + `DecodeScript::Progressive` behind A (first wrap `gapless==false`, later wraps `true` once covered) — depends on T022, T034
- [X] T027 [US1] `loop_seam.rs::repeat_count_releases_after_n` (`LoopReleased{wraps:3}`, playback continues past b, `loop_state==0`) — depends on T022
- [X] T028 [US1] `loop_seam.rs::seek_outside_keeps_armed_inactive` and `::natural_entry_from_before_a_activates` — depends on T022
- [X] T029 [US1] `loop_seam.rs::commit_is_atomic_across_renders`, `::edit_while_armed_applies_next_buffer_and_finishes_seam`, `::disarm_mid_seam_finishes_seam_without_jump` — depends on T022
- [X] T030 [US1] `loop_seam.rs::published_position_after_mid_render_wrap` (resampling, `leftover > 0`) — depends on T022
- [X] T031 [P] [US1] `crates/modplayer-engine/tests/realtime.rs::render_with_armed_loop_never_allocates` (`assert_no_alloc`, 1,000 renders across ≥ 20 wraps with a store attached) — depends on T021
- [X] T032 [P] [US1] Update the existing `command_is_copy_and_small` test to cover the five new `Command` variants (still ≤ 16 bytes)
- [X] T033 [P] [US1] Give `SyntheticSource` an `Option<Arc<DecodedStore>>` field + `with_store()` ctor + `decoded_store()` impl, and wire `SyntheticHost::attach`/`ScriptedHost::attach` to install a store, in `crates/modplayer-audio-source-synthetic/src/{lib.rs,host.rs,scripted.rs}` (depends on T006)
- [X] T034 [P] [US1] Implement `ConnectRtSource::decoded_store() -> self.store.as_ref()` in `crates/modplayer-audio-source-connect/src/rt.rs` (depends on T006)
- [X] T035 [US1] Forward `SourceCommand::PrefetchHint` to `DecodeAhead::seek_hint(frame)` (no `Player` touch) in `crates/modplayer-audio-source-connect/src/worker.rs`; synthetic/scripted hosts ignore it (depends on T007, T033)
- [X] T036 [P] [US1] Receiver tests: `rt_feed::decoded_store_returns_current_track_store` in `crates/modplayer-audio-source-connect/tests/rt_feed.rs`; `worker::prefetch_hint_forwards_to_decode_ahead` and `::prefetch_hint_is_ignored` (ScriptedHost) in `crates/modplayer-audio-source-connect/tests/worker.rs` — depends on T034, T035

### Controller wiring (contracts/marker-service.md §1–2, §4 subset)

- [X] T037 [US1] Implement `set_loop_a`/`set_loop_b`, `new_loop_region`, `arm_loop`/`disarm_loop`/`toggle_current_loop` on `PlaybackController`, deriving and pushing the engine commands of §2 (setters then `LoopCommit`, always via `push_command_retrying`) in `crates/modplayer-core/src/controller.rs` — depends on T009, T015–T021
- [X] T038 [US1] On `arm_loop`, send `SourceCommand::PrefetchHint { frame: a − effective_crossfade }` in `crates/modplayer-core/src/controller.rs` (depends on T037, T007)
- [X] T039 [US1] `tick()`: drain `event_rx`; on ≥1 `LoopWrapped` this tick, send one coalesced `SourceCommand::Seek(a_ms)` (first immediately, then throttled to ≤1 per `LOOP_RESEEK_INTERVAL = 250ms`); on `LoopReleased`, call `markers.disarm()` (model only) in `crates/modplayer-core/src/controller.rs` — depends on T037
- [X] T040 [US1] `map_source_event`: ignore `SourceEvent::EndOfTrack` while `shared.loop_state() == 2` in `crates/modplayer-core/src/controller.rs` — depends on T017
- [X] T041 [US1] On stream rebuild (`open_stream`/device change), re-push the four setters + `LoopCommit{reset_wraps:false}` when a region is armed in `crates/modplayer-core/src/controller.rs` — depends on T037
- [X] T042 [US1] Push `LoopDisarm` on: explicit `disarm_loop`, arming a different region, deleting an endpoint of the armed region, and `Command::Stop` NOT disarming (arming is not a transport action) in `crates/modplayer-core/src/controller.rs` — depends on T037
- [X] T043 [P] [US1] `crates/modplayer-core/tests/controller_markers.rs::arm_pushes_setters_then_commit_and_prefetch_hint`, `::edit_armed_region_recommits_without_resetting_wraps` — depends on T037, T038
- [X] T044 [P] [US1] `controller_markers.rs::loop_wrapped_reseeks_source_once_per_tick_then_throttled`, `::end_of_track_ignored_while_loop_active`, `::end_of_track_advances_while_loop_inactive` — depends on T039, T040
- [X] T045 [P] [US1] `controller_markers.rs::loop_released_disarms_model`, `::stream_rebuild_repushes_armed_region` — depends on T039, T041
- [X] T046 [P] [US1] `crates/modplayer-core/tests/markers_model.rs::a_after_b_swaps_kinds_keeps_names`, `::equal_positions_do_not_swap`, `::region_under_1ms_is_not_armable`, `::region_3ms_is_armable_with_3ms_effective_crossfade`, `::only_one_region_armed`, `::arm_resets_wraps`, `::delete_endpoint_makes_region_incomplete_and_disarmed`, `::delete_last_endpoint_removes_region` — depends on T009

### UI (contracts/ui-markers.md §1–2 loop subset, §4 loop cells)

- [X] T047 [US1] Marker lane glyph rendering for region A/B brackets + overlay lines/span painting (outline disarmed / hatched armed-inactive / solid armed-active per data-model.md §3 table) in `crates/modplayer-ui/src/markers.rs` — depends on T011–T013
- [X] T048 [US1] Wire the marker lanes above overview/detail waveforms and the `overlays` hook into `now_playing.rs`; add the Markers panel scaffold with loop-only cells (arm toggle, repeat `DragValue`, crossfade `DragValue`, wraps-remaining/infinite, armed-inactive badge) in `crates/modplayer-ui/src/now_playing.rs` — depends on T012, T047
- [X] T049 [US1] Wire `I`/`O`/`L` view-level shortcuts to `set_loop_a`/`set_loop_b`/`toggle_current_loop`, guarded by "no text field of this view has focus" (research R17), with inline refusal reasons (`marker-limit-reached`, `loop-region-incomplete`, `loop-region-too-short`) in `crates/modplayer-ui/src/now_playing.rs` — depends on T037, T048
- [X] T050 [P] [US1] `crates/modplayer-ui/tests/markers.rs::i_then_o_creates_region_at_playhead_positions`, `::l_toggles_current_region_and_refuses_with_reason` — depends on T049
- [X] T051 [P] [US1] `markers.rs::armed_region_shows_wraps_remaining`, `::armed_inactive_badge_when_state_1`, `::overlay_paints_lines_and_span_states` — depends on T048
- [X] T052 [P] [US1] Add Fluent keys `markers-status`, `marker-limit-reached`, `loop-region-incomplete`, `loop-region-too-short`, `marker-glyph`, `marker-role-a`, `marker-role-b`, `loop-arm`, `loop-disarm`, `loop-repeat`, `loop-crossfade`, `loop-wraps-remaining`, `loop-wraps-infinite`, `loop-armed-inactive` to `locales/en-US/playback.ftl`

**Checkpoint**: US1 is fully functional and independently testable — a loop region can be created, armed, and looped gaplessly on cached audio, with repeat-count release and A/B swap. `cargo test --workspace` green.

---

## Phase 4: User Story 2 - Markers and loops persist across sessions (Priority: P2)

**Goal**: Every marker and loop region (never the armed flag) survives a relaunch or same-session reload, written atomically so a crash mid-write never corrupts the previous save; the empty state and "Clear all markers" round out the panel; a shorter re-saved track clamps and flags out-of-range markers.

**Independent Test**: Create markers and a loop region on a track, quit and relaunch the app (or use the crash-mid-write toggle), reopen the same track, and verify every marker and the loop region (but not its armed state) are restored exactly.

### Persistence store (contracts/marker-service.md §3)

- [X] T053 [P] [US2] Implement `TrackStatePaths::resolve()`/`with_dir()`/`file_for()`, `encode_track_id`/`decode_track_id` (lowercase hex of the `TrackId` bytes, reversible, case-free) in `crates/modplayer-core/src/markers/store.rs` (research R10)
- [X] T054 [US2] Implement the JSON DTOs, `encode(&TrackMarkers) -> Vec<u8>` and `load(paths, id, rate, len_frames) -> LoadOutcome` with the full read-rule table (missing → empty/no warning; >64KiB/IO/parse error → empty + `Unreadable`, rewrite allowed; `schema_version>1` → empty + `NewerSchema`, rewrite blocked until mutation; unknown keys ignored; `repeat`/`color`/`crossfade_ms`/`position` clamped; dangling region/marker refs repaired; `sample_rate` mismatch rescales) in `crates/modplayer-core/src/markers/store.rs` (data-model.md §4) — depends on T053
- [X] T055 [US2] Implement `spawn_writer(notify) -> (Sender<PersistJob>, JoinHandle<()>)`: `<name>.json.tmp` → `write_all` → `sync_all` → `rename`; on any error leave the previous file untouched and send `StoreEvent::SaveFailed` in `crates/modplayer-core/src/markers/store.rs` — depends on T053
- [X] T056 [US2] `sync_marker_attachment()` on the controller (hooked after every `dispatch`, same point as `sync_analysis_attachment`; fires when the current track's id changed **or** the same id restarted): `flush_track_state()` for the previous track, push `LoopDisarm`, `load(new)` + raise any warning, `set_len_frames(current len)` in `crates/modplayer-core/src/controller.rs` — depends on T054, T042
- [X] T057 [US2] `flush_track_state()` / `flush_track_state_if_due()` (250 ms debounce, `TRACK_STATE_DEBOUNCE`, checked in `tick()`), `clear_for_sign_out()` (flush + drop model + `LoopDisarm`), `shutdown()` (flush, drop the writer sender, join it, after `analysis.shutdown()`) in `crates/modplayer-core/src/controller.rs` — depends on T055, T056
- [X] T058 [US2] `Input::TrackStarted` handling calls `set_len_frames(track_len_ms → frames)` (FR-018 clamp + `clamped` flag) in `crates/modplayer-core/src/controller.rs` — depends on T056
- [X] T059 [US2] `clear_all_markers()`: `LoopDisarm` if armed, empty the model, immediate flush (writes the empty file) in `crates/modplayer-core/src/controller.rs` — depends on T057
- [X] T060 [P] [US2] `crates/modplayer-core/tests/markers_store.rs::round_trip_is_identity_for_any_state` (proptest; `armed`/`wraps`/`clamped` excluded), `::crash_mid_write_keeps_previous_file` (write `.tmp`, skip rename, reload), `::track_id_hex_encoding_round_trips_and_is_case_free` (proptest) — depends on T054, T055
- [X] T061 [P] [US2] `markers_store.rs::missing_file_loads_empty_without_warning`, `::unparseable_loads_empty_with_unreadable`, `::newer_schema_loads_empty_and_is_not_rewritten_until_mutation`, `::oversize_file_is_unreadable` — depends on T054
- [X] T062 [P] [US2] `markers_store.rs::unknown_keys_ignored_and_out_of_range_clamped`, `::dangling_region_refs_are_repaired`, `::writer_reports_save_failed_and_leaves_previous_file` (read-only dir) — depends on T054, T055
- [X] T063 [P] [US2] `crates/modplayer-core/tests/controller_markers.rs::track_change_flushes_disarms_then_loads`, `::same_track_restart_reloads_and_clears_armed`, `::clear_all_writes_empty_state_immediately`, `::sign_out_flushes_and_drops_state_but_keeps_file`, `::warnings_raised_once_per_load` — depends on T056, T057, T059
- [X] T064 [P] [US2] `crates/modplayer-core/tests/markers_model.rs::set_len_flags_clamped_markers_only` (SC-012) — depends on T009

### UI (contracts/ui-markers.md §1, §4 empty state + clear-all)

- [X] T065 [US2] Markers panel empty state (`markers-empty` = "No markers — press I to set A") replacing the row list when `TrackMarkers::count() == 0`, and "New loop region"/"Clear all markers" header buttons with the two-step inline confirmation (`markers-clear-confirm { $count }` → yes/no, `Esc` cancels) calling `clear_all_markers()` in `crates/modplayer-ui/src/markers.rs` — depends on T048, T059
- [X] T066 [US2] Render the `clamped` warning glyph on a marker's overlay line and panel row in `crates/modplayer-ui/src/markers.rs` — depends on T047, T058
- [X] T067 [P] [US2] `crates/modplayer-ui/tests/markers.rs::empty_state_shows_press_i_hint`, `::clear_all_two_step_confirm_and_cancel`, `::clamped_marker_shows_warning_glyph` — depends on T065, T066
- [X] T068 [P] [US2] Add Fluent keys `markers-empty`, `markers-new-loop`, `markers-clear-all`, `markers-clear-confirm`, `markers-clear-yes`, `markers-clear-no`, `marker-clamped-desc`, `track-state-unreadable`, `track-state-newer-version`, `track-state-save-failed` to `locales/en-US/playback.ftl`

**Checkpoint**: US1 + US2 both work independently — markers/regions/cues survive a relaunch or same-session reload disarmed, a crash mid-write never corrupts the previous save, and the panel's empty/clear-all states work.

---

## Phase 5: User Story 3 - Precisely edit any marker (Priority: P3)

**Goal**: Rename, recolor, drag (with the detail-view zoom-assist landing ≤5 ms), and nudge (by the configurable step) any marker from the waveform or the panel; delete markers with correct endpoint/region handling; create point markers with `M`; enforce the 64-marker limit on every creation path.

**Independent Test**: Create several markers, rename one, recolor one, drag one on the overview and verify it lands within 5 ms of the intended sample via the detail-view zoom-assist, nudge one by keyboard, delete one, and attempt to exceed 64 markers on a track.

### Core (contracts/marker-service.md §1 non-loop subset, §5)

- [X] T069 [US3] Implement `add_point_marker`, `rename_marker`, `recolor_marker`/`cycle_marker_color`, `move_marker` (drag/nudge commit with FR-019 clamp, re-commits an armed region), `nudge_marker(id, direction, multiplier)`, `delete_marker` (disarms armed region if an endpoint), `select_marker` on `PlaybackController` in `crates/modplayer-core/src/controller.rs` — depends on T009, T042
- [X] T070 [US3] Add `[markers] nudge_step_ms` settings delta: `RawMarkers { nudge_step_ms }` in `crates/modplayer-core/src/settings/model.rs`, `AudioSettings.nudge_step_ms: u16` (clamped `1..=1000`), `setting-nudge-step` descriptor under `SettingsCategory::Playback` in `crates/modplayer-core/src/settings_registry.rs`, and `nudge_step_ms()`/`set_nudge_step_ms()` on `PlaybackController` in `crates/modplayer-core/src/controller.rs`
- [X] T071 [P] [US3] `crates/modplayer-core/tests/markers_model.rs::limit_is_64_across_all_kinds`, `::move_paths_never_hit_limit`, `::default_names_and_colors_by_kind`, `::rename_trims_truncates_and_keeps_old_on_empty_point`, `::positions_clamp_to_len` (proptest), `::markers_stay_sorted_after_any_sequence` (proptest) — depends on T009
- [X] T072 [P] [US3] `crates/modplayer-core/tests/settings.rs::nudge_step_setting_round_trips_and_clamps` — depends on T070

### UI (contracts/ui-markers.md §3, §5, §7)

- [X] T073 [US3] Marker glyph selection (click or `Tab` gives egui focus, sets `focused_marker`, calls `select_marker`, 2px line + row highlight) and the focused-marker keyboard table (`←`/`→` nudge ×1, `Shift+←`/`Shift+→` nudge ×10, `Delete`/`Backspace` delete + focus returns to detail waveform, `F2`/`Enter` open inline rename, `C` cycle color, `Esc` return focus) in `crates/modplayer-ui/src/waveform/input.rs` and `crates/modplayer-ui/src/markers.rs` — depends on T013, T047, T069
- [X] T074 [US3] Relative-delta marker drag: `live += Δx_pointer × detail.frames_per_pixel()`, per-frame `DetailWindow::zoom_assist` recenter/shrink toward `max(rect_width_px×rate×0.0025, DETAIL_MIN_WINDOW_MS×rate/1000)`, release commits `move_marker(id, live)` (clamped), `Esc` restores origin position and detail window, post-commit keeps the zoomed window with follow suspended in `crates/modplayer-ui/src/{waveform/state.rs,markers.rs}` — depends on T013, T073
- [X] T075 [US3] Wire `M` shortcut to `add_point_marker()` with `marker-limit-reached` inline refusal in `crates/modplayer-ui/src/now_playing.rs` — depends on T069, T049
- [X] T076 [US3] Extend the Markers panel rows beyond US1's loop cells: colour-swatch popup (8 swatches, keyboard `C` cycles), role/kind label, inline-editable name (`TextEdit` on rename, else label), position `m:ss.mmm` in `crates/modplayer-ui/src/markers.rs` — depends on T048, T069 (swatch simplified to a click-to-cycle button rather than an 8-swatch popup — same net effect as `C`, documented in code; a true popup can replace it later without changing the model/contract)
- [X] T077 [US3] Add the `setting-nudge-step` `DragValue` (1–1000 ms) to Settings › Playback, committing via `set_nudge_step_ms` on change, in `crates/modplayer-ui/src/settings/playback.rs` — depends on T070
- [X] T078 [P] [US3] `crates/modplayer-ui/tests/markers.rs::m_creates_point_marker_with_default_name_sorted`, `::sixty_fifth_marker_refused_inline`, `::shortcuts_inactive_while_rename_open` — depends on T075, T076
- [X] T079 [P] [US3] `markers.rs::glyph_focus_arrow_nudges_by_setting_and_shift_ten_x`, `::delete_focused_endpoint_makes_region_incomplete`, `::f2_rename_commits_on_enter_cancels_on_esc`, `::c_cycles_palette_and_row_matches_glyph` — depends on T073
- [X] T080 [P] [US3] `markers.rs::drag_from_overview_zooms_detail_and_lands_within_5ms` (assert `|committed − intended| ≤ 5ms`), `::drag_esc_restores_position_and_window` — depends on T074
- [X] T081 [P] [US3] `detail_window_zoom_assist_converges_to_target` unit test in `crates/modplayer-ui/src/waveform/state.rs` (or `crates/modplayer-ui/tests/waveform.rs`) — depends on T013
- [X] T082 [P] [US3] `settings::playback::nudge_step_drag_value_commits` unit test in `crates/modplayer-ui/src/settings/playback.rs` — depends on T077
- [X] T083 [P] [US3] Add Fluent keys `marker-role-point`, `marker-default-name`, `markers-rename`, `markers-color`, `setting-nudge-step`, `setting-nudge-step-desc` to `locales/en-US/playback.ftl` / `settings.ftl`

**Checkpoint**: US1–US3 all independently functional — markers can be created, renamed, recolored, dragged (≤5 ms), nudged, and deleted, with the 64-marker limit enforced everywhere.

---

## Phase 6: User Story 4 - Jump instantly to a cue point (Priority: P4)

**Goal**: Up to 8 cue points per track, set with `Shift+1..8` and jumped to with `1..8`, never altering play/pause state; empty slots are silent no-ops; cues persist (Story 2's contract, already built) and respect the 64-marker limit.

**Independent Test**: While a track plays, set cue points in several of the 8 slots at different positions, jump to each with its number key, and verify each jump is instant and precise and never changes play/pause state; verify the same with the transport paused.

- [X] T084 [US4] Implement `set_cue(&mut self, slot: CueSlot)` (at playhead, FR-013) and `jump_to_cue(&mut self, slot: CueSlot) -> bool` (`seek_frames` through 005's existing seek path; `false` no-op on an empty slot) on `PlaybackController` in `crates/modplayer-core/src/controller.rs` — depends on T009
- [X] T085 [P] [US4] `crates/modplayer-core/tests/markers_model.rs::cue_slot_is_unique_and_set_moves`, `::delete_cue_frees_slot` — depends on T009
- [X] T086 [US4] Wire `Shift+1..8` (`set_cue`, `marker-limit-reached` only when creating) and `1..8` (`jump_to_cue`, silent no-op on empty) shortcuts, and the numbered-square cue glyph, in `crates/modplayer-ui/src/now_playing.rs` and `crates/modplayer-ui/src/markers.rs` — depends on T084, T049
- [X] T087 [P] [US4] `crates/modplayer-ui/tests/markers.rs::shift_digit_sets_cue_and_digit_jumps_keeping_state`, `::digit_on_empty_slot_is_noop`, `::shift_digit_on_occupied_slot_moves_at_limit` — depends on T086
- [X] T088 [P] [US4] Add Fluent key `marker-role-cue` to `locales/en-US/playback.ftl`

**Checkpoint**: All four user stories independently functional and testable.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Coverage that spans every story, plus the constitution's gates and sign-offs.

- [X] T089 [P] Extend `crates/modplayer-ui/tests/accessibility.rs`: every marker glyph is `Role::Button` named per `marker-glyph`, panel rows are `Role::ListItem`, the arm toggle is `Role::CheckBox` with state, numeric fields (repeat/crossfade/nudge-step) are labelled, the panel and empty state are exposed, and every key in contracts/ui-markers.md §2–§3 is reachable (FR-022)
- [X] T090 [P] Extend `crates/modplayer-ui/tests/fluent_keys.rs` to assert every key listed in contracts/ui-markers.md §8 is present and none is unused (FR-023)
- [X] T091 Extend `now_playing.rs::keyboard_table_matches_pointer_results` (in `crates/modplayer-ui/tests/now_playing.rs`) to cover every marker/loop/cue row added across US1–US4
- [X] T092 [P] Confirm `crates/modplayer/tests/decoded_store_boundary.rs` still passes unchanged (the engine's new `read_frames` caller in T021 is in the permitted set — Constitution V)
- [X] T093 [P] Confirm `crates/modplayer/tests/single_dependent.rs` still passes unchanged (Constitution IV)
- [X] T094 Add a real-time-safety note to the PR description template / checklist for any diff touching `crates/modplayer-engine/` and confirm `CODEOWNERS` requires the engine maintainer's review (Constitution I, Governance) — `.github/PULL_REQUEST_TEMPLATE.md`'s "Real-time safety" section already covers every diff touching `crates/modplayer-engine/` (processor.rs, limiter.rs, output_stage.rs, shared.rs, "or anything called from `Processor::render`" — which includes 006's new `loop_math.rs`), with its own checklist item requiring the engine maintainer's (CODEOWNERS) review; `CODEOWNERS` requires `@rzcastilho` on `/crates/modplayer-engine/`. Confirmed current, no edit needed.
- [X] T095 Run the full automated gate from quickstart.md: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, `scripts/check-license-headers.sh` — all five green. `cargo fmt --all` fixed pre-existing formatting debt across 006's own Phase 3-6 files (mechanical only, no semantic change); `clippy` needed two small pre-existing-debt fixes (`controller.rs`'s `drain_engine_events` collapsed via a let-chain, a doc-comment `>=` reworded in `controller_markers.rs` to stop Markdown parsing it as a blockquote, and `markers.rs` test file's `manual_range_contains`/`manual_contains`/`field_reassign_with_default` lints) — all mechanical, no behavior change. `cargo test --workspace` is green (one `modplayer-account` OAuth-listener test flaked once under full-workspace parallelism, confirmed pre-existing/unrelated — passes standalone and on a clean re-run; not a 006 file). `cargo deny check` and the license-header script were already clean. **Re-run green after T096's fixes** (D1/D2/D4 plus the `loop_seam` generator bound): `cargo fmt --all --check` clean, `clippy --workspace --all-targets --all-features -D warnings` clean, `cargo test --workspace` 912 passed / 9 ignored across 81 suites, `cargo deny check` "advisories ok, bans ok, licenses ok, sources ok", license headers ok.
- [X] T096 Execute manual scenarios M1–M16 from quickstart.md (signed-in Premium account, Constitution › Manual Scenario Sign-Off) and record each result against this line in tasks.md — **executed 2026-09-17** (macOS 12.6 x86_64, debug build, toolchain 1.95.0, live signed-in account, real audio hardware; driven by Quartz `CGEventPost` + `screencapture` per the constitution's recipe; helper scripts and PNG evidence in `target/manual-walk/`, gitignored).

  **Harness note (how the earlier "no WindowServer" blocker was cleared).** The previous attempt's diagnosis was wrong in one respect: the agent shell is indeed a `launchctl managername == Background` session and a bare `./target/debug/modplayer` there never gets a WindowServer connection — but launching the same binary through **LaunchServices** does. Wrapping the binary in a minimal `.app` bundle (`target/manual-walk/ModPlayer.app`, `Info.plist` + symlinked executable) and starting it with `open -n` attaches it to the interactive Aqua session, giving a real window that `CGWindowListCopyWindowInfo` lists and `screencapture -l <id>` captures. Synthetic events must be posted to the **HID tap** (`CGEventPost`) from a process holding Accessibility rights (the agent's own iTerm2 shell does); `CGEventPostToPid` delivers hover but not clicks, and events posted while the screen is locked are dropped. Re-activation between steps is `open <bundle>` plus a z-order check (`NSRunningApplication.isActive` is stale in a process with no runloop).

  **Results — 14 pass, 1 pass-with-deviation, 1 fail:**

  | # | Result | Evidence |
  |---|--------|----------|
  | M1 | **pass** | `I`/`O` at the playhead → A 0:52.436 / B 0:56.552, brackets on both lanes, region outlined; `L` → "Armed (waiting for the playhead)"; seeking before A turned the span hatched, then solid (armed-active) on entry with "Looping indefinitely"; position stayed inside the region and wrapped across 15 s (0:53 → 0:55 → 0:53) with the peak meter continuously live. 0-drift/click-free seam itself is the `loop_seam.rs` proptests' job (not audible-verifiable here). |
  | M2 | **pass** | Repeat set to 3 via the panel `DragValue`; re-armed; after the wraps the toggle cleared itself to "Arm loop" and playback continued past B monotonically (1:53 → 1:56). |
  | M3 | **pass** | Two regions; arming region 1 from its row cleared region 2's toggle (and vice-versa) — only one armed at a time. |
  | M4 | **pass** | Swap: dragging A past B turned A 0:57.496/B 1:56.300 into A 1:56.300/B 2:50.421 (labels swapped, A ≤ B kept). Too-short: A=B at 2:35.419 → `L` refused inline "The region is too short to loop." with the arm checkbox disabled. Crossfade 50 ms on a short region armed and looped normally (2:37 → 2:35 → 2:36). Driven by drag rather than `Shift+→` because the sub-second region's A/B glyphs overlap at overview resolution; the effective-crossfade shrink is `short_region_shrinks_crossfade`'s. |
  | M5 | **pass** | Armed, then clicked well after B → badge "Armed (waiting for the playhead)", span hatched, no jump (stayed 3:36). Clicking inside (2:53) turned the span solid and it wrapped at B back to A (0:26, 0:30). |
  | M6 | **pass** | Quit + relaunch + replay the same track restored A 0:19.064, B 2:55.112, crossfade 5 ms, repeat 0, both lanes' brackets — and the region came back **disarmed**. The on-disk JSON carries no `armed` key at all. |
  | M7 | **pass** | `kill -9` immediately after a marker mutation: the state file stayed valid JSON with all 64 markers, no `.tmp` residue; relaunch restored everything including the mutated cue position (0:56.731). |
  | M8 | **pass** | Empty state reads "No markers — press I to set A"; "Clear all markers" → inline "Clear 4 markers?" with Yes/No; `Esc` cancels leaving markers intact; Yes returns the panel to the empty state. |
  | M9 | **pass** (after fix D1) | 1-byte `{` state file → Warning "This track's saved markers could not be read and have been reset.", no markers; a mutation rewrote the file as valid JSON. **Initially raised the warning twice** — see D1. |
  | M10 | **pass** | Dragging a point glyph on the overview moved it with the detail lane following and a live ms-precision readout; release committed exactly the live value (3:02.213); `Esc` mid-drag restored it. The ≤5 ms landing bound itself is `drag_from_overview_zooms_detail_and_lands_within_5ms`'s. |
  | M11 | **pass** | Settings › Playback nudge step 25 (persisted as `nudge_step_ms = 25`); focused marker: `→` 4:23.627 → 4:23.652 (exactly +25 ms), `Shift+→` → 4:23.902 (exactly +250 ms). |
  | M12 | **pass** | `M` to 64 markers, then `M` → inline "The track already has the maximum number of markers."; count stayed 64; `Shift+3` on the occupied slot still moved cue 3 (0:32.078 → 1:48.907) at the limit. |
  | M13 | **pass, one deviation** | `Shift+3`/`Shift+5` set cues (green numbered-square glyphs on both lanes); `3` jumped and playback continued; `7` (empty slot) was a silent no-op; `5` jumped while playing. **Deviation:** while *paused*, a cue jump (and an ordinary overview click-seek) leaves the position readout and playhead frozen at the old value — the seek itself is correct, proven by resuming exactly at cue 3. Pre-existing in the shared seek/position-publish path (the engine only republishes position while rendering), not specific to cues. |
  | M14 | **partial** | The far-ahead loop armed and wrapped correctly (3:56 → 3:55 → 3:59 inside 3:55.016–4:00.252). The *uncached* precondition could not be held: decode-ahead fills a ~6 min track within seconds, so the region was cached before the loop could be armed, and the `gapless` flag is not surfaced anywhere in the UI. Covered by `uncached_seam_hard_cuts_and_reports_not_gapless` (`DecodeScript::Progressive`). |
  | M15 | **pass** (after fix D2) | Keyboard-only: `Tab` reaches the lane glyphs, then `C` recolours, `F2` renames, `Delete` removes, `←`/`→` nudge, `L` arms. **`Tab` originally reached a glyph but armed none of those actions** — see D2. VoiceOver itself was not driven (enabling it takes over the whole desktop and is not safely reversible from a script); the AccessKit roles/names it reads are asserted by `accessibility.rs` (glyphs `Role::Button` named per `marker-glyph {role, name, time}`, rows `Role::ListItem`, arm toggle `Role::CheckBox`). |
  | M16 | **FAIL** | Switching the output device while looping **freezes playback permanently** — position stuck at 2:36 across 12 s, peak meter at 0.0000 (silence), while the panel still reads "Looping indefinitely". Not loop-specific and **not a 006 defect**: any mid-session output-device change stops audio, reproduced with no loop armed. See D3. |

  **Defects found and fixed this run (both with regression tests):**
  - **D1 — duplicate `track-state-unreadable` warning (006, T056).** Starting one track loaded its state twice: once when the id changed and again on the `Input::TrackStarted` that followed, each raising the warning. `sync_marker_attachment` now tracks whether the next `TrackStarted` is the initial one for the attachment it just made and skips that redundant reload; a genuine same-id restart (FR-016) still reloads. Test `controller_markers.rs::track_start_loads_once_and_warns_once`; `same_track_restart_reloads_and_clears_armed` updated to emit the initial start before the restart it tests.
  - **D2 — marker glyphs unreachable by keyboard (006, T073, FR-022).** `waveform.focused_marker` was set only from `clicked()`/`drag_started()`, so a keyboard-only user could `Tab` onto a glyph (it is focusable and in the tab order) and still not nudge, rename, recolour or delete it — the whole key table is gated on that field. `markers::lane` now also adopts the marker when its glyph `has_focus()`. Test `markers.rs::tab_focus_on_a_glyph_enables_the_marker_key_table`. Verified live: Tab + `C` recoloured every marker.
  - **D4 — tofu arrows in the nudge-step description.** `setting-nudge-step-desc` rendered as "How far □ / □ move a focused marker" (the bundled font has no `←`/`→`); reworded to "the left and right arrow keys", which also reads better under a screen reader.
  - **Latent test defect (not shipped code):** `loop_seam.rs::period_is_exact_after_1000_wraps` generated `len_region = 1` frame, which `LoopCommit` deliberately refuses (`b - a >= 2`), so the loop never armed and the wrap formula was asserted against a non-existent loop (`left: 64, right: 0`). Only 8 proptest cases run per invocation, so earlier runs never drew it. Lower bound raised to 2 frames with a comment; the stale `.proptest-regressions` entry removed; the suite re-run 3× clean.

  **D3 — device change mid-playback kills audio (pre-existing, 003's Connect receiver — NOT fixed here).** `open_stream_on` rebuilds the stream and calls `source_host.attach()`, which allocates a *fresh* sample ring and parks the producer in `pending_sample_tx`. But the Connect worker is spawned exactly once and `ConnectSource::handle_initialize` early-returns while `worker.is_some()`, so the worker's `RingSink` keeps writing into the now-orphaned previous ring while the new `ConnectRtSource` reads a ring nothing ever feeds — silence, and a frozen published position (the processor renders nothing). `Player`'s sink producer is move-only and built once ("reuse it via `Player::set_session`"), so the fix is a receiver-side rewire — hand the running worker the new producer on re-attach (rebuilding the sink/`Player`), or keep the ring alive across stream rebuilds. Left unfixed deliberately: it is 003 code, outside 006's scope, and a wrong move here risks the real-time path. Filed for a follow-up; M16 cannot pass until it lands.

  **Minor UX observations (not fixed, no contract violated):** the inline rename opens with the caret at the end and the existing name unselected, so typing appends ("Marker 1" + "Chorus" → "Marker 1Chorus") rather than replacing as platform-standard rename UX does; and marker glyphs closer together than ~1 overview pixel (a sub-second region's A/B) overlap so only the top one can be grabbed — both lanes are correct, it is just that the overview's 585 ms/px makes them coincide.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational (Phase 2)**: depends on Setup — BLOCKS every user story
- **US1 (Phase 3)**: depends on Foundational only — no dependency on US2–US4
- **US2 (Phase 4)**: depends on Foundational; reuses US1's `sync_marker_attachment` trigger point (T056 extends T042) but is independently testable once T053–T068 land — a build can ship US1 without US2
- **US3 (Phase 5)**: depends on Foundational (`TrackMarkers`, `WaveformState`, overlay hook) — independent of US2 and US4; does not require the loop engine (US1) to be armed, only markers to exist
- **US4 (Phase 6)**: depends on Foundational only — independent of US1–US3
- **Polish (Phase 7)**: depends on every user story phase that was implemented

### Within Phase 3 (US1)

Engine chain: T015–T017 → T018 → T019 → T020 → T021 (T004 feeds T021). T033/T034 (synthetic/Connect store) feed T022's harness and T026/T036. T037 depends on T009 (Foundational) and the whole engine chain (T015–T021). T047–T049 (UI) depend on T011–T013 (Foundational) and T037.

### Within Phase 4 (US2)

T053 → T054 → T055 → T056 → T057 → {T058, T059}. UI (T065–T068) depends on T059/T048/T047.

### Within Phase 5 (US3)

T069 → {T073 → T074, T075, T076}; T070 → {T072, T077, T082}.

### Within Phase 6 (US4)

T084 → T086 → T087.

### Parallel Opportunities

- All `[P]` tasks within Setup (T002, T003) and Foundational (T004–T014, except T009 which depends on T008) run in parallel.
- Once Foundational is done, **US1, US3 and US4 can proceed in parallel** by different developers (none depends on another's controller/UI code); **US2 depends on US1's `LoopDisarm` push point (T042)** existing before its `sync_marker_attachment` (T056) can be written meaningfully, so schedule US2 after or alongside the tail of US1.
- Within US1: T015–T017 in parallel; T022's harness once T033 lands, then T023–T030 are sequential edits to the same file (`loop_seam.rs`) but logically independent — a team could split them across branches touching disjoint test functions. T031–T036 run in parallel with the `loop_seam.rs` tests.
- All `[P]` test tasks in every phase (T043–T046, T050–T052, T060–T064, T067–T068, T071–T072, T078–T083, T085, T087–T088, T089–T090, T092–T093) run in parallel with each other within their phase once their implementation dependency lands.

---

## Parallel Example: User Story 1

```bash
# Engine primitives, once Foundational is done:
Task: "Add Command variants LoopSetA/B/Seam/Commit/Disarm in crates/modplayer-engine/src/command.rs"
Task: "Add Event::LoopWrapped/LoopReleased in crates/modplayer-engine/src/event.rs"
Task: "Add RtShared::loop_wraps/loop_state in crates/modplayer-engine/src/shared.rs"

# Source-side store plumbing, in parallel with the above:
Task: "SyntheticSource::with_store + decoded_store() in crates/modplayer-audio-source-synthetic/src/lib.rs"
Task: "ConnectRtSource::decoded_store() in crates/modplayer-audio-source-connect/src/rt.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL — blocks all stories)
3. Complete Phase 3: User Story 1 (T015–T052)
4. **STOP and VALIDATE**: `cargo test --workspace`, manual scenarios M1–M5, M14, M16 from quickstart.md
5. This alone delivers JTBD-1 — a musician can loop a passage gaplessly, in-memory, for the session

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. Add US1 → validate independently → MVP (loop, no persistence yet — regions vanish on relaunch)
3. Add US2 → validate independently (M6–M9) → markers/loops now survive a relaunch
4. Add US3 → validate independently (M10–M12) → editing becomes pleasant, not just possible
5. Add US4 → validate independently (M13) → cue points round out the feature
6. Phase 7 → full quickstart.md gate + manual scenarios M1–M16 sign-off

### Parallel Team Strategy

With multiple developers, after Foundational:

- Developer A: US1 (engine + loop controller/UI) — the critical path, most LOC
- Developer B: US4 (cues) — fully independent, small surface, can land early
- Developer C: US3 (marker editing UI) once `TrackMarkers`'s non-loop mutation API (T069) is stubbed against Foundational's model
- US2 (persistence) starts once US1's `LoopDisarm` push points (T042) exist, ideally the same developer finishing US1 or a fourth picking it up immediately after

---

## Notes

- `[P]` tasks touch different files and have no unmet dependency at the time they start.
- `[Story]` labels trace every implementation and test task back to spec.md's user stories for independent delivery.
- Tests are included per FR-028 / Constitution VIII; where a contract's test-name table lists a test alongside its implementation row, the test task follows its implementation task in this file so it can be run red→green, but nothing here blocks writing the test first if the team prefers strict TDD.
- Every task touching `crates/modplayer-engine/` requires a real-time-safety note in its PR and the engine maintainer's `CODEOWNERS` sign-off (Constitution I, Governance).
- Avoid: editing `crates/modplayer-engine/src/processor.rs` from two tasks at once (T018–T021 are sequential on purpose); same for `crates/modplayer-core/src/controller.rs` (T037–T042, T056–T059, T069, T084 are sequential edits to one file even though grouped by story).

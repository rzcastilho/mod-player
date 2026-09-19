# Tasks: Effect Chain and Built-In Effect Nodes

**Input**: Design documents from `/specs/008-effect-chain-and-built-in-nodes/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md (all present)

**Tests**: FR-018 explicitly mandates the named automated tests (latency, click-free, clamping proptests, real-time-safety, overload/auto-bypass, criterion benches, soak). They are therefore **not optional** in this feature and are folded into the implementation task that introduces the code they pin, named exactly as the contracts/quickstart specify, rather than split into separate "write failing test" tasks — the module and its named tests are a single unit of work throughout this list.

**Organization**: Phase 2 (Foundational) builds the generic, node-kind-agnostic chain substrate — proven end to end with the trivial `Gain` kernel — because every user story's own automated tests run a real `Processor` with a real chain and cannot exist without it. Each user story phase after that is independently verifiable by its own named tests without depending on a later story's code.

## Path Conventions

Cargo workspace, one crate per architectural component (`crates/<name>/{src,tests,benches}`). New crate: `crates/modplayer-effects/`. Locale strings: `locales/en-US/*.ftl`.

---

## Phase 1: Setup

**Purpose**: stand up the new `modplayer-effects` crate and its workspace wiring.

- [X] T001 Create `crates/modplayer-effects` crate: `Cargo.toml` (package metadata matching the other crates' convention, e.g. `crates/modplayer-engine/Cargo.toml`; no runtime dependencies; dev-dependencies `proptest.workspace = true` and `criterion = { version = "0.7", default-features = false, features = ["cargo_bench_support"] }`) and `src/lib.rs` (SPDX header, `#![forbid(unsafe_code)]`, `#![deny(clippy::unwrap_used, clippy::expect_used)]`, empty stub `pub mod catalog; pub mod consts; pub mod smooth; pub mod crossfade; pub mod biquad; pub mod nodes; pub mod spectrum; pub mod rt;`)
- [X] T002 Add `modplayer-effects` as a path dependency (`{ path = "../modplayer-effects", version = "0.1.0" }`, the same pattern used for existing internal crates) to `crates/modplayer-engine/Cargo.toml`, `crates/modplayer-core/Cargo.toml`, and `crates/modplayer-ui/Cargo.toml`
- [X] T003 Run `cargo deny check` after T001/T002 and confirm `criterion` (MIT OR Apache-2.0, `default-features = false`) adds no new licence surface, per research R16
- [X] T004 [P] Create `locales/en-US/effects.ftl` (header comment only — keys added incrementally by later tasks) and add the `action-nav-toggle-effect-chain` key placeholder to `locales/en-US/controls.ftl`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the parameter catalog, the allocation-free chain runtime, the core shadow model and the controller façade — proven with the `Gain` kernel as the first working node. No user story can be implemented, let alone independently tested, before this phase is complete.

**⚠️ CRITICAL**: every later phase's tests exercise a real `Processor`/`ChainRt`/`ChainModel`; none of that exists until this phase is done.

- [X] T005 `crates/modplayer-effects/src/catalog.rs`: `NodeKind` (6 variants + `ALL` + `label_key()`), `NodeOwner`/`PluginId`, `ParamId`/`ParamShape`/`Unit`/`ParamDef`, `pub fn params(kind: NodeKind) -> &'static [ParamDef]` covering all six kinds' parameters exactly per data-model.md §1.3 (including the EQ bands' `16 + 4·b + n` id scheme), `pub fn clamp(kind, id, value, source_rate) -> f32` (NaN → default; Nyquist clamp `min(max, 0.45 × source_rate)`), `pub fn is_continuous(kind, id) -> bool`
- [X] T006 `crates/modplayer-effects/tests/catalog.rs`: proptests `clamp_is_idempotent_in_range_and_monotone`, `nyquist_clamp_tracks_rate`, `defaults_are_in_range` over every `ParamId` of every `NodeKind` — pins FR-006, FR-010, SC-005
- [X] T007 `crates/modplayer-effects/src/catalog.rs`: `QualityMode`, `ModeState`, `pub fn stage_use(kind, value) -> bool` (`|semitones| <= 3`, ratio in `[0.5, 1.5]`), `pub fn mode_after_value_change(kind, value, state) -> ModeState` (FR-008 rules 1–2), `pub fn mode_after_user_set(mode) -> ModeState` (rule 3)
- [X] T008 `crates/modplayer-effects/tests/mode_rule.rs::auto_switch_table` — pins FR-008
- [X] T009 [P] `crates/modplayer-effects/src/consts.rs`: full constant table from data-model.md §1.5 (`MAX_NODES=16`, `PARAM_RAMP_MS=20`, `SWITCH_CROSSFADE_MS=5`, `SPECTRUM_BANDS=64`, `SPECTRUM_FFT=1024`/hop 256, `COST_WINDOW_SECS=1.0`, `OVERLOAD_PCT=90`/`OVERLOAD_STREAK=3`, `TEMPO_STEP=0.10`, `Q_BUTTER`/`Q_MAX=8.0`, `STRETCH_PERF`/`STRETCH_QUALITY` timing, `LPC_ORDER=16`, `CHAIN_BUS_FRAMES`)
- [X] T010 [P] `crates/modplayer-effects/src/smooth.rs`: `Smoothed { current, target, step, remaining }`, `set_target` (restart-from-current), per-frame `advance()`; tests `crates/modplayer-effects/tests/smooth.rs::ramp_reaches_target_in_exactly_ramp_frames`, `::retarget_restarts_from_current` — pins FR-010
- [X] T011 [P] `crates/modplayer-effects/src/crossfade.rs`: `Crossfade` equal-power state (reusing `loop_math::crossfade_gains`'s curve shape); test `crates/modplayer-effects/tests/crossfade.rs::equal_power_and_exact_endpoints` — pins FR-005
- [X] T012 [P] `crates/modplayer-effects/src/nodes/gain.rs` (+ `src/nodes/mod.rs`): `GainDsp` kernel (`y = x × 10^(dB/20)`, mute = 5 ms fade to 0); test `crates/modplayer-effects/tests/nodes.rs::gain_identity_at_defaults` (sample-exact at 0 dB) — the first working kernel, used to prove the pipeline below
- [X] T013 [P] `crates/modplayer-effects/src/rt/cost.rs`: `CostRing` (`[f32; 1024]` ring + running sum + len `N`), percentage-of-callback-period accounting (FR-012a); the overload *state machine* itself lands in Phase 6
- [X] T014 `crates/modplayer-effects/src/rt/slot.rs`: `NodeSlot` (`active`, `kind`/`owner`, `dsp: NodeDsp` enum starting with the `Gain` variant, `params: [Smoothed; MAX_PARAMS]`, `mix: Crossfade`, `bypassed`/`auto_bypassed`, `pending_move: Option<u8>`, `removing`, `cost: CostRing`)
- [X] T015 `crates/modplayer-effects/src/rt/chain.rs`: `ChainRt` (`slots`, `order`, `bus_a`/`bus_b` sized `CHAIN_BUS_FRAMES`, `plan`, `combined`/`spectrum`/`render_pct_window` stubs) with `plan()`/`process()` for in-place kernels only (`Gain` today — the stretch pull-plan branch lands in Phase 3), `latency_frames()`/`advance_rate()` returning `0`/`1.0` for now, `reset_history()` no-op stub, and `ChainInsert`/`ChainRemove`/`ChainMove`/`ChainSetBypass`/`ChainSetParam` application per contracts/engine-effect-chain.md §2 and §4 (dry/wet mix, two-phase reorder); test `crates/modplayer-effects/tests/chain_rt.rs::plan_conserves_frames` (proptest, `Gain`-only nodes, 16 in any order)
- [X] T016 `crates/modplayer-engine/src/command.rs`: `Command::ChainInsert{slot,position,kind,owner}`, `ChainRemove{slot}`, `ChainMove{slot,position}`, `ChainSetBypass{slot,bypassed}`, `ChainSetParam{slot,param,value}` — all `Copy`, const-assert `size_of::<Command>() <= 16`
- [X] T017 `crates/modplayer-engine/src/event.rs`: `Event::Overload{costliest_slot,render_pct}`, `Event::AutoBypassed{slot}` (data types only; raised starting Phase 6)
- [X] T018 `crates/modplayer-engine/src/processor.rs`: extract `fill_from_source` — 006's loop/seam/seek `render` body, unchanged logic, destination and frame count now parameters — preparing for the chain to sit between it and master gain
- [X] T019 `crates/modplayer-engine/src/processor.rs`: wire `ChainRt` into `Processor` per contracts/engine-effect-chain.md §1 render order (`fill_from_source → [pre-chain meter stub] → ChainRt::process → master gain/tone → [post-chain meter stub] → limiter`); drain the five new `Command` variants at the buffer boundary; test `crates/modplayer-engine/tests/effects_pipeline.rs::empty_chain_is_bit_exact_with_001_pipeline` — pins FR-001
- [X] T020 `crates/modplayer-engine/src/shared.rs`: `RtShared` additions `node_cost_bits[MAX_NODES]`, `chain_cost_bits`, `render_pct_bits` (meter/overload/spectrum atomics land in Phase 6)
- [X] T021 `crates/modplayer-engine/tests/realtime.rs::render_with_full_chain_never_allocates`: extend to drive add/param/move/bypass/remove/seek through a `Gain`-only chain under `assert_no_alloc` (grows per node kind in later phases) — pins Constitution I
- [X] T022 `crates/modplayer-engine/tests/boundary.rs::chain_commands_apply_at_next_boundary` (generic add/bypass/remove timing, `Gain` node) — pins FR-005
- [X] T023 `crates/modplayer-core/src/effects/mod.rs` + `model.rs`: `NodeId`, `NodeModel`, `ChainModel` (`new`/`add`/`remove`/`move_to`/`move_by`/`set_bypass`/`set_param`/`set_mode`/`set_source_rate`/`mark_auto_bypassed`/`first_time_stretch`/`node_by_slot`/`replay`/`nodes`/`capacity`), `ChainError{Full,UnknownNode,WrongKind}` (`thiserror`) per contracts/effects-service.md §1 rules G1–G9
- [X] T024 `crates/modplayer-core/tests/controller_effects.rs`: `add_17th_is_refused_without_corruption` (G1), `ids_never_reused_slots_are` (G2), proptest `set_param_returns_and_stores_clamped` (G3), `move_by_at_ends_is_noop` (G6), `rate_change_reclamps_only_frequencies` (G7), `replay_is_complete_and_ordered` (G8), `first_time_stretch_ignores_bypass` (G9)
- [X] T025 `crates/modplayer-core/src/effects/view.rs`: `ChainView`/`NodeRow` projection (data-model.md §2.4) and `MeterSnapshot`/`LevelPair` (§2.5; real `RtShared` reads land in Phase 6)
- [X] T026 `crates/modplayer-core/src/controller.rs`: `chain: ChainModel` field + façade (`chain`, `chain_view`, `chain_add_node`, `chain_remove_node`, `chain_move_node`, `chain_move_node_by`, `chain_set_bypass`, `chain_set_param`, `chain_set_mode`) per contracts/effects-service.md §2 rules C1, C2, C9, C10; `open_stream_on` pushes `chain.set_source_rate(new_rate)` then `chain.replay()`'s commands after the existing loop re-push (C4, generic form)
- [X] T027 `crates/modplayer-core/tests/controller_effects.rs`: `commands_are_pushed_in_model_order` (C1), `set_param_returns_clamped` (C2), `stream_rebuild_replays_chain` (C4, `Gain`-only), `view_reports_zero_cost_when_not_playing` (C9), `chain_survives_sign_out_and_is_empty_at_construction` (C10)
- [X] T028 `crates/modplayer-core/src/lib.rs`: `pub mod effects;` re-export wiring

**Checkpoint**: an empty or `Gain`-only chain runs end to end — add/remove/move/bypass/param all land click-free at the buffer boundary, nothing allocates, and the chain is session-scoped shadow state on the controller. Every user story below builds on this substrate.

---

## Phase 3: User Story 1 - Slow down or transpose a song in real time (Priority: P1) 🎯 MVP

**Goal**: pitch-shift and time-stretch nodes work correctly alone, combined, and non-adjacent, click-free, with the playhead and the streaming Player tracking any tempo, and the `+`/`-` tempo-step keys driving the first time-stretch node.

**Independent Test**: fully automated — a chain with a time-stretch node and a pitch-shift node, exercised directly through `Processor`/`ChainModel`/`invoke`, proves each dimension changes independently with no dropout (the visual panel is Phase 4's deliverable; these tests need none of it).

- [X] T029 [P] `crates/modplayer-effects/src/nodes/lpc.rs`: `LpcState` — order-16 autocorrelation + Levinson–Durbin (fixed arrays), whiten (inverse filter) / resynthesize (all-pole synthesis) over 1024-frame Hann windows at a 512-frame hop
- [X] T030 `crates/modplayer-effects/src/nodes/stretch.rs`: `StretchStage { ring, voices: [Voice; 2], active, fading_from, stretch, step }` — WSOLA (Hann windows, 50 % COLA overlap, normalised cross-correlation search ±search_radius, shared stereo offset) + fractional-delay resampler (linear = performance, cubic Hermite = quality), pass-through short-circuit at exact unity, formant correction via `nodes/lpc.rs` when `formant = on`, and the pitch/tempo/combined product-parameter table from research R4
- [X] T031 `crates/modplayer-effects/tests/stretch.rs`: `unity_is_pass_through`, `tempo_half_doubles_click_interval`, `pitch_up_7_keeps_duration`, `formant_on_keeps_envelope_peaks`, `quality_mode_uses_cubic` — pins US1 AS1/AS2/AS3/AS6, SC-013
- [X] T032 `crates/modplayer-effects/src/rt/slot.rs`: add `NodeDsp::Stretch(StretchStage)` variant and its activation in `ChainInsert` (buffers preallocated per slot at `Processor::new`, sized from the source rate)
- [X] T033 `crates/modplayer-effects/src/rt/chain.rs`: implement the two-pass pull plan for an engaged stretch stage (`input_for(out)` per contracts/engine-effect-chain.md §3, ping-pong bus swap, clamp to `CHAIN_BUS_FRAMES`, starved-stage repeats-last-grain fallback); extend `plan_conserves_frames` to mixed `Stretch`/`Gain` chains; add `starved_stage_repeats_not_silence`
- [X] T034 `crates/modplayer-effects/src/rt/chain.rs`: FR-007 combined-stage detection — recomputed on every `order` mutation and bypass change, an adjacent `(PitchShift, TimeStretch)` pair runs one `StretchStage` with product parameters (`stretch = p/r`, `step = p`, `mode = Quality` if either is quality, formant from the pitch node; a bypassed member contributes identity), cost split half/half; tests `crates/modplayer-effects/tests/stretch.rs::combined_stage_products`, `crates/modplayer-effects/tests/rt_chain.rs::combined_cost_split_half` — pins FR-007
- [X] T035 `crates/modplayer-effects/src/nodes/stretch.rs`: two-voice crossfade for mode / formant / engage-disengage discrete switches (research R6 last row) — both voices read the same ring and mix across the 5 ms `Crossfade`
- [X] T036 `crates/modplayer-engine/tests/effects_transitions.rs::every_edit_and_switch_is_click_free`: extend the table with `Stretch`/`PitchShift` rows (mode switch, formant switch, add/remove/reorder with a stretch stage present), 006's first-difference metric — pins FR-005, SC-002
- [X] T037 `crates/modplayer-core/src/effects/model.rs`: `mode_state: Option<ModeState>` wired for `PitchShift`/`TimeStretch`; `set_param` runs `mode_after_value_change` on semitones/ratio and emits the second `ChainSetParam` for `mode` (G4); `set_mode` runs `mode_after_user_set`
- [X] T038 `crates/modplayer-core/tests/controller_effects.rs`: `auto_switch_flags_and_reverts`, `user_forced_performance_persists_until_next_excursion` — pins FR-008, SC-006
- [X] T039 `crates/modplayer-effects/src/rt/chain.rs`: `latency_frames()` (Σ engaged stretch stages' buffered lead) and `advance_rate()` (Π engaged time-stretch ratios) — research R7
- [X] T040 [P] `crates/modplayer-engine/src/loop_math.rs`: `rewind_in_loop(raw, lead, a, b)` (modular generalisation of 006's rewind) + proptest `rewind_in_loop_lands_in_region`
- [X] T041 `crates/modplayer-engine/src/processor.rs`, `src/shared.rs`, `src/position_clock.rs`: publish `lead = carry_len + chain.latency_frames()`; `AnchorSnapshot` gains `advance_rate`; `PositionClock::now` extrapolates at `source_rate × advance_rate` — pins FR-001a
- [X] T042 `crates/modplayer-engine/tests/effects_position.rs`: `position_advances_at_ratio_with_loop`, `seek_resets_history_wrap_does_not`, `lead_is_subtracted` — pins FR-001a, SC-011
- [X] T043 `crates/modplayer-effects/src/rt/chain.rs`: `reset_history()` (clears every stretch ring/voice, biquad and LPC state), wired to `Command::Seek`/`Stop` only, never to a loop wrap
- [X] T044 `crates/modplayer-engine/tests/effects_latency.rs::ratio_change_audible_within_20ms_p95` (128 frames / 44.1 kHz click train, 200 randomised trials) — pins SC-001, NFR-1.1
- [X] T045 `crates/modplayer-core/src/controller.rs`: research R8 Player-follow — `last_tempo_reseek_at`, `tick()` re-seeks the Player (006's coalesced `SourceCommand::Seek`) once predicted drift reaches 500 ms while `advance_rate != 1.0` (C7); `end_of_track_pending`, `engine_ended_track_seq`, `drain_source_events` engine-gates `EndOfTrack` below unity and mirrors + swallows it above unity (C8)
- [X] T046 `crates/modplayer-core/tests/controller_effects.rs`: `player_reseeks_when_drift_reaches_half_second` (renamed `player_is_never_reseeked_for_tempo_drift` on 2026-09-19 when rule C7 was withdrawn), `no_reseek_at_unity`, `end_of_track_deferred_at_half_speed`, `engine_ends_track_first_at_double_speed_and_player_event_is_swallowed`, `unity_behaviour_unchanged` — pins research R8
- [X] T047 `crates/modplayer-core/src/controller.rs`: `tempo_step(direction)` façade (C3) — targets `first_time_stretch()`, `chain_set_param(ratio, current ± 0.10)` clamped `[0.25, 2.0]`; raises keyed Info `effects-no-time-stretch` (coalesced) when no node exists; `crates/modplayer-core/src/notifications.rs` gains `KEY_EFFECTS_NO_TIME_STRETCH`
- [X] T048 `crates/modplayer-core/tests/controller_effects.rs`: `tempo_step_moves_first_time_stretch_by_ten_points`, `tempo_step_clamps_at_bounds`, `tempo_step_without_node_notifies_once` — pins FR-017, SC-012
- [X] T049 `crates/modplayer-core/src/actions/catalog.rs`: flip `TempoStepUp`/`TempoStepDown` to `enabled_by_default: true` (bindings unchanged)
- [X] T050 `crates/modplayer-ui/src/actions.rs`: `invoke` maps `TempoStepUp`/`TempoStepDown` to `controller.tempo_step(±1)`; tests `crates/modplayer-ui/tests/actions.rs::tempo_actions_enabled_and_repeat`, `::plus_minus_step_tempo_unless_waveform_focused`, `::plus_without_time_stretch_notifies_once_while_held` (dispatch-level, no panel needed — the waveform's existing `WAVEFORM_CLAIMS` already own `Plus`/`Minus`/`Equals`) — pins FR-017, SC-012, US1 AS7/AS8

**Checkpoint**: pitch shift and time stretch are fully correct and independently verified by automated tests — no UI panel required yet.

---

## Phase 4: User Story 2 - Build and rearrange the effect chain without a glitch (Priority: P2)

**Goal**: the Effect Chain panel — list, add, remove, bypass, reorder (pointer and keyboard), capacity refusal, per-node and whole-chain CPU.

**Independent Test**: open the panel (`E`/header), add and rearrange nodes of the kinds available so far (time stretch, pitch shift, gain), verifying every action lands cleanly with no click and the panel reflects the new state immediately (the full "EQ before time-stretch" scenario and all six kinds land once Phase 5 completes).

- [X] T051 `crates/modplayer-ui/src/effects_view.rs` (new file): `panel_open_id()` (egui temp memory `now-playing-effect-chain-open`, same pattern as `queue_panel_open_id`), `toggle_effect_chain_panel(ctx)`, `show(ui, controller, state)` entry point
- [X] T052 `crates/modplayer-ui/src/now_playing.rs`: "Effects" `selectable_label` beside "Queue"; draw `effects_view::show` after the waveform/markers block and before the Queue panel when open, regardless of whether a track is loaded
- [X] T053 [P] `crates/modplayer-core/src/actions/catalog.rs`: `HostAction::ToggleEffectChain` (`host.nav.toggle_effect_chain`, category Navigation, scope `NowPlaying`, no repeat, enabled, default `["E"]`); `CATALOG: [ActionDef; 45]`
- [X] T054 `crates/modplayer-core/tests/actions.rs::catalog_has_45_entries_and_effects_enabled`, `::toggle_effect_chain_defaults_to_e_in_now_playing` — completes FR-017's action-catalog delta
- [X] T055 `crates/modplayer-ui/src/actions.rs`: `invoke(HostAction::ToggleEffectChain)` → `effects_view::toggle_effect_chain_panel(ctx)`; test `crates/modplayer-ui/tests/actions.rs::toggle_effect_chain_dispatches_in_now_playing_only`
- [X] T056 `crates/modplayer-ui/src/effects_view.rs`: row rendering per node — kind+owner label, `toggle_value` bypass, CPU `%` label, `Button` remove, each row inside a `dnd_drop_zone`; "Add node…" `ComboBox` of the currently-implemented kinds + `Add` button; inline refusal label under the add row on `ChainError::Full`
- [X] T057 `crates/modplayer-ui/src/effects_view.rs`, `src/app.rs`: drag handle — focusable `Button` "⋮" as `dnd_drag_source(handle_id, payload: NodeId)`; register `EFFECT_HANDLE_CLAIMS = {ArrowUp, ArrowDown}` with 007's `FocusClaims`; `handle_focused_handle_keys` consumes `↑`/`↓` → `chain_move_node_by(id, ∓1)`, keeping focus on the same node's handle across the move
- [X] T058 `crates/modplayer-ui/src/effects_view.rs`: panel header — title, whole-chain CPU % label, overload-counter/badge placeholders (wired live in Phase 6)
- [X] T059 `crates/modplayer-ui/tests/effects_view.rs` (new file): `e_and_header_toggle_panel_and_it_survives_track_change`, `rows_list_nodes_in_processing_order_with_type_owner_bypass_handle_cost`, `seventeenth_add_is_refused_inline`, `drag_drop_reorders_and_calls_move_to`, `arrow_up_on_focused_handle_moves_node_and_keeps_focus`, `bypass_toggle_updates_state_immediately`, `remove_row_disappears_order_preserved` — pins FR-003, FR-004, US2 AS1/AS1a/AS1b/AS2/AS3/AS4/AS5, SC-007, SC-012
- [X] T060 `crates/modplayer-ui/src/effects_view.rs`: parameter controls for the kinds available so far — Pitch shift (`Slider` semitones −12..+12 step 0.01, `toggle_value` formant, `ComboBox` mode, "quality mode auto-switched" note label), Time stretch (`Slider` shown as 25..200 %, `ComboBox` mode, same note label), Gain (`Slider` dB −60..+12, `toggle_value` mute); every change goes through `controller.chain_set_param`/`chain_set_mode` and redraws with the **returned** (clamped) value
- [X] T061 `crates/modplayer-ui/tests/effects_view.rs`: `quality_note_appears_at_25_percent_and_clears_at_100`, `out_of_range_entry_displays_clamped_value` (gain +20 → +12 case; EQ/filter cases land in Phase 5) — pins FR-008/SC-006, FR-010/SC-005
- [X] T062 `crates/modplayer-ui/tests/accessibility.rs` extended: accessible name/role/state for every control added in this phase — pins FR-015
- [X] T063 `locales/en-US/effects.ftl`, `controls.ftl`: add this phase's keys (`effects-toggle`, `effects-panel-title`, `effects-chain-cpu`, `effects-add-node`, `effects-add`, `effects-remove`, `effects-bypass`, `effects-reorder-handle`, `effects-cpu`, `effects-mode-note`, `effects-chain-full`, `effects-owner-{host,plugin}`, `effects-kind-{pitch-shift,time-stretch,gain}`, `effects-param-{semitones,formant,mode,ratio,level,mute}`, `effects-mode-{performance,quality}`, `action-nav-toggle-effect-chain`); extend `crates/modplayer-ui/tests/fluent_keys.rs`

**Checkpoint**: the Effect Chain panel opens by `E`/header, lists nodes in processing order with type/owner/bypass/handle/cost, supports add/remove/reorder/bypass glitch-free, and refuses a 17th node inline.

---

## Phase 5: User Story 3 - Shape tone with the full built-in node catalog (Priority: P3)

**Goal**: gain (already built), equalizer, filter, and stereo tools all process correctly, clamp correctly (including the Nyquist clamp and the resonance cap), and are controllable from the panel.

**Independent Test**: add one of each remaining node type to a playing chain, adjust each parameter across and past its documented range, and verify every value clamps correctly and every change is audible without a click.

- [X] T064 [P] `crates/modplayer-effects/src/biquad.rs`: RBJ cookbook coefficients (peak/low-shelf/high-shelf/HP/LP), `Biquad` state, shadow-biquad switch (research R6)
- [X] T065 [P] `crates/modplayer-effects/src/nodes/eq.rs`: `Eq8Dsp` — 8 always-present bands, coefficients recomputed at 32-frame sub-block boundaries from each band's ramped value, shadow biquad per band during a type switch
- [X] T066 [P] `crates/modplayer-effects/src/nodes/filter.rs`: `FilterDsp` — RBJ HP/LP, `Q = Q_BUTTER + res × (Q_MAX − Q_BUTTER)`, explicit pass-through short-circuit at the documented default (`mode == HP && cutoff == 20 && res == 0`), shadow biquad on mode switch
- [X] T067 [P] `crates/modplayer-effects/src/nodes/stereo.rs`: `StereoDsp` — research R13 order: width (mid/side) → balance (far-channel attenuation) → mono sum (phase invert applied first when on) → channel swap
- [X] T068 `crates/modplayer-effects/tests/nodes.rs`: `gain_eq_filter_stereo_identity_at_defaults` (sample-exact), `eq_peak_plus_6db_at_1khz`, `filter_hp_minus_3db_at_cutoff`, `resonance_caps_at_q_max`, `stereo_truth_table`, `mono_sum_invert_cancels_centre` — pins FR-006, FR-009, SC-013, US3
- [X] T069 `crates/modplayer-effects/src/rt/slot.rs`: add `NodeDsp::{Eq(Eq8Dsp), Filter(FilterDsp), Stereo(StereoDsp)}` variants and their activation in `ChainInsert`
- [X] T070 `crates/modplayer-engine/tests/realtime.rs::render_with_full_chain_never_allocates`: extend to a full 16-node chain covering every kind under `assert_no_alloc` — pins Constitution I, FR-018
- [X] T071 `crates/modplayer-engine/tests/effects_transitions.rs::every_edit_and_switch_is_click_free`: extend the table with EQ band-type switch and filter mode switch — pins FR-005, SC-002
- [X] T072 Verify `ChainModel::set_source_rate` (G7, already generic from T023) correctly re-clamps EQ band frequencies and filter cutoff on a rate change; tests `crates/modplayer-engine/tests/effects_pipeline.rs::rate_change_rebuild_recomputes_coefficients`, `crates/modplayer-core/tests/controller_effects.rs::rate_change_rebuild_reclamps_eq` — pins FR-014
- [X] T073 `crates/modplayer-ui/src/effects_view.rs`: parameter controls for Equalizer (8 × `DragValue` Hz/dB/Q + `ComboBox` type), Filter (`ComboBox` mode, `DragValue` cutoff, `Slider` resonance), Stereo tools (`Slider` width/balance, `toggle_value` mono sum / phase invert with `add_enabled(mono_sum)` / swap)
- [X] T074 `crates/modplayer-ui/tests/effects_view.rs`: `add_each_kind_appends_at_defaults_and_shows_zero_cost` (all six kinds now exist), `out_of_range_entry_displays_clamped_value` (EQ 30 000 Hz → 19 845 Hz, resonance 1.5 → 1.0), `phase_invert_disabled_unless_mono_sum` — pins US2 AS1, US3 AS1/AS3/AS4/AS4b/AS6, SC-005
- [X] T075 `crates/modplayer-ui/tests/accessibility.rs`, `tests/fluent_keys.rs` extended for the EQ/Filter/Stereo controls
- [X] T076 `locales/en-US/effects.ftl`: add the remaining kind/parameter keys (`effects-kind-{equalizer,filter,stereo-tools}`, `effects-param-{band-freq,band-gain,band-q,band-type,filter-mode,cutoff,resonance,width,balance,mono-sum,phase-invert,channel-swap}`, `effects-band-type-{peak,low-shelf,high-shelf}`, `effects-filter-{high-pass,low-pass}`)

**Checkpoint**: all six built-in node kinds process correctly and are fully controllable from the panel.

---

## Phase 6: User Story 4 - See what the chain is doing and get warned before it breaks audio (Priority: P4)

**Goal**: pre-/post-chain peak/RMS meters, the 64-band spectrum, per-node/whole-chain CPU figures live in the panel, and the overload/auto-bypass mechanism (inert for host nodes, exercised by engine tests with a non-host owner).

**Independent Test**: build a chain heavy enough to approach the real-time budget, verify CPU figures and meters update live, push it over budget and verify the overload counter increments, a warning names the costliest node, audio keeps playing, and — with a non-host-owned test node — exactly that node is auto-bypassed.

- [X] T077 [P] `crates/modplayer-effects/src/spectrum.rs`: 1024-point radix-2 real FFT (precomputed twiddles), Hann window, mono `(L+R)/2` ring of the last 1024 post-chain frames, folded into 64 log-spaced bands 20 Hz–20 kHz
- [X] T078 [P] `crates/modplayer-engine/src/shared.rs`: `RtShared` meter/spectrum atomics — `pre_peak_l/r`, `pre_rms_l/r`, `post_peak_l/r`, `post_rms_l/r`, `spectrum_bits[64]`, `spectrum_generation`
- [X] T079 `crates/modplayer-engine/src/processor.rs`: pre-chain peak/RMS (L/R) over `bus_a` right after `fill_from_source`; post-chain peak/RMS (L/R) + spectrum recompute (every ≥ 256 new frames) over `fresh` before the limiter
- [X] T080 `crates/modplayer-engine/tests/effects_meters.rs`: `pre_ignores_nodes_and_master_post_reflects_both`, `spectrum_peaks_in_expected_band` — pins FR-011, SC-008
- [X] T081 `crates/modplayer-effects/src/rt/chain.rs`, `src/rt/cost.rs`: overload state machine (research R10) — `render_pct_window`/`over_90_streak`/`under_90_streak`/`last_auto_bypass_at`; overload event when `render_pct >= 100` or `>= 90` on 3 consecutive callbacks; on event, rank all active slots by cost and, if the costliest is non-host, not already bypassed, and no auto-bypass happened in the current window, bypass it (5 ms fade) and flag `auto_bypassed`
- [X] T082 `crates/modplayer-engine/src/processor.rs`: raise `Event::Overload{costliest_slot, render_pct}` / `Event::AutoBypassed{slot}`; `RtShared::overload_count`/`over_budget` atomics
- [X] T083 `crates/modplayer-engine/tests/effects_overload.rs`: `overload_counts_and_names_costliest`, `non_host_costliest_is_bypassed_exactly_once_per_window`, `host_costliest_is_never_bypassed`, `over_budget_clears_after_clean_window` (using a `cfg(test)`-only `burn_ns` cost-inflation hook on a `NodeOwner::Plugin(1)` test node) — pins FR-012, SC-004
- [X] T084 `crates/modplayer-core/src/controller.rs`: `over_budget_notified` field; `drain_engine_events` mirrors `Event::Overload` into the keyed `Warning` `effect-chain-over-budget` (args `$node`, `$owner`, re-used while visible) and `Event::AutoBypassed` into `chain.mark_auto_bypassed(slot)` + `effect-chain-auto-bypassed` (C5); `tick` dismisses `effect-chain-over-budget` by key once `RtShared::over_budget()` clears (C6)
- [X] T085 [P] `crates/modplayer-core/src/notifications.rs`: `KEY_EFFECT_CHAIN_OVER_BUDGET`, `KEY_EFFECT_CHAIN_AUTO_BYPASSED`
- [X] T086 `crates/modplayer-core/tests/controller_effects.rs`: `overload_event_raises_one_keyed_warning`, `auto_bypassed_event_marks_model`, `warning_clears_when_rt_reports_clean_window`
- [X] T087 `crates/modplayer-core/src/effects/view.rs`, `src/controller.rs`: wire `ChainView.nodes[i].cost_pct`/`total_cost_pct`/`over_budget`/`overload_count` from live `RtShared` reads (completing C9); wire `MeterSnapshot` from the new meter/spectrum atomics
- [X] T088 [P] `crates/modplayer-ui/src/widgets/chain_meters.rs` (new file), `src/widgets/mod.rs`: `level_pair` (peak+RMS bars with hold/integration, reusing `peak_meter`'s dB scale) and `spectrum` (64-bar log-frequency widget)
- [X] T089 `crates/modplayer-ui/src/effects_view.rs`: panel header wired to live whole-chain CPU %, overload counter, over-budget badge, pre/post `level_pair`s, and the 64-band spectrum; per-row "auto-bypassed (over budget)" label variant; repaint every 16 ms while the panel is open and playing
- [X] T090 `crates/modplayer-ui/tests/effects_view.rs`: `auto_bypassed_label_shown_from_view_flag`, `over_budget_badge_and_counter_follow_view`, `meters_and_spectrum_render_from_snapshot` — pins US4 AS2/AS3/AS4/AS5, FR-011, SC-008
- [X] T091 `crates/modplayer-ui/tests/accessibility.rs`, `tests/fluent_keys.rs` extended for meters, spectrum, badge, counter, auto-bypassed label
- [X] T092 `locales/en-US/effects.ftl`: remaining keys — `effects-overloads`, `effects-over-budget-badge`, `effects-pre`, `effects-post`, `effects-peak`, `effects-rms`, `effects-spectrum`, `effects-auto-bypassed`, `effect-chain-over-budget`, `effect-chain-auto-bypassed`, `effects-no-time-stretch`

**Checkpoint**: every acceptance scenario in the spec is automatically verifiable; the panel is feature-complete.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: the constitution's boundary, benchmark and soak obligations, a full accessibility/i18n sweep, the release-only measurements, and the manual scenario sign-off.

- [X] T093 [P] `modplayer/tests/decoded_store_boundary.rs::effects_crate_exposes_no_sample_sink` — scans the public API of `modplayer-effects` and `modplayer-engine` for `Write`/`File`/`TcpStream`/`&[f32]`-returning items — pins FR-013, SC-009
- [X] T094 [P] `crates/modplayer-effects/benches/nodes.rs` — criterion group per node kind, 128/256/1024-frame blocks at 44.1 kHz, both stretch modes, formant on/off
- [X] T095 [P] `crates/modplayer-effects/benches/reference_chain.rs` — SC-003's pitch + stretch + 8-band EQ at 128 frames, reporting `ns/render` against the 2.9 ms period
- [X] T096 `crates/modplayer-engine/tests/soak.rs::reference_chain_soak_60s` (CI) — renders the reference chain continuously with parameter drags/reorders/bypasses, asserts `assert_no_alloc` and stable `peak_rss` samples every 10 s
- [X] T097 `crates/modplayer-engine/tests/soak.rs::reference_chain_soak_24h` (`#[ignore = "manual"]`, `MODPLAYER_SOAK_HOURS=24`) and `::reference_chain_under_half_core` (`#[ignore = "manual"]`, release) — pins NFR-2.7, SC-003
- [X] T098 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, `scripts/check-license-headers.sh` (add the SPDX header to every new file from this feature); fix anything red
- [X] T099 [P] Full `crates/modplayer-ui/tests/accessibility.rs` and `tests/fluent_keys.rs` sweep confirming every Effect Chain control is enumerated and every `effects.ftl`/`controls.ftl` key is used — pins FR-015, NFR-7.1 (verified green: `effect_chain_controls_expose_accessible_names_and_states`, `effect_chain_meters_spectrum_and_overload_controls_expose_accessible_names`, `eq_filter_stereo_controls_expose_accessible_names_and_states` in accessibility.rs; `no_unused_keys_in_playback_and_settings_ftl` and `every_shell_nav_and_notification_key_resolves` in fluent_keys.rs cover every `effects.ftl`/`controls.ftl` key both directions — no gaps found, nothing to fix)
- [X] T100 Execute the release measurements on reference hardware and record them in `specs/008-effect-chain-and-built-in-nodes/quickstart.md`'s Release measurements table (`cargo bench -p modplayer-effects`, `reference_chain_under_half_core`, 24 h soak RSS drift) — DEVIATION: no dedicated reference hardware was available in this sandboxed session; `cargo bench -p modplayer-effects -- reference_chain`/`nodes` and `reference_chain_under_half_core` were run for real on the implementing agent's dev host (results recorded in quickstart.md, reference chain at ≈1.4 % of the 1.45 ms budget — a wide enough margin to expect it holds on slower hardware, but not itself a sign-off). The 24 h soak (`reference_chain_soak_24h`) could not be run to completion in this session (infeasible: no host can be held 24 h here); it was verified to compile and execute correctly in release instead (smoke run at `MODPLAYER_SOAK_HOURS=0`), and the 60 s CI soak passed clean. The real 24 h run and a repeat on the project's actual reference hardware are still owed before SC-003/NFR-2.7 sign-off.
- [X] T101 Execute manual scenarios M1–M14 from `quickstart.md` per the constitution's Manual Scenario Sign-Off recipe; record pass/deviation + evidence path for each — **executed 2026-09-19** (macOS 12.6 x86_64, debug build, toolchain 1.95.0, the maintainer's live signed-in Premium account — re-signed-in twice during the walk, real audio hardware "LG HDR WFHD", track *Billie Jean* / *Beat It*; driven by Quartz `CGEventPost` + `screencapture` per the constitution's recipe; helper scripts `drive.py`/`locate.py` and every PNG named below live in `target/manual-walk/`, gitignored). **Recipe deviation worth keeping**: the app must be launched from an Aqua-session process (`osascript -e 'tell application "Terminal" to do script "…/target/debug/modplayer"'`) — launched from the agent's own `Background` launchd session it can never become the key app, so synthetic keystrokes land in whatever *is* frontmost (here: the maintainer's Chrome); and every rebuild of the binary re-triggers the "modplayer wants to use your confidential information" Keychain prompt, which only the maintainer can answer. Results:
  - **M1 PASS** (`M1_a_after_E`, `M1_b_after_E_again`, `M1_c_after_header_toggle`, `M1_d_after_skip`): opens/closes/opens; still open after Skip forward to the next queue item.
  - **M2 DEVIATION (spec)** (`M2_b_t0`…`M2_k_t60`, `M2_pos_pair*.png`): ratio change is immediate and the tempo drops, but the playhead advances at ≈ 0.46 × wall at 55 % and ≈ 0.33 × at 45 % (expected = ratio; unity baseline confirmed 1.0 × via track roll-over). Root cause: research R8.1's periodic Player re-seek (`follow_tempo_drift`, one `SourceCommand::Seek` whenever predicted drift ≥ 500 ms) targets the *lead-subtracted* published position on a streaming (`Ring`-feed) source, and each seek's `Reposition` marker rewinds the RT cursor by roughly the buffered lead/ring depth (~100 ms) — measured loss matches one rewind per re-seek. Inside an armed loop (M7, `Store` feed, rule 4 ignores `Reposition`) the rate is exact, which pins the mechanism. **Fixed the same day** (second commit): `rt.rs::fill` pops the ring in lockstep on the `Store` feed too, so the receiver is paced by the engine's consumption in both feeds and R8.1's predicted drift never exists — `follow_tempo_drift` and `last_tempo_reseek_at` removed, rule C7 rewritten, `controller_effects.rs::player_is_never_reseeked_for_tempo_drift` pins the absence. **Re-run on hardware** (`M2r_pair.png`): 24 s of track in 60.4 s wall at 40 % = **0.40 ×** — exact.
  - **M3 PASS after fix** (`app_crash_M3.log`, `M3_strip.png`, `M3_pos_pair.png`): the first attempt **aborted the whole process** the moment formant was switched on at +7 st — `panicked at core/src/num/f32.rs: min > max, or either was NaN. min = NaN, max = NaN` on the audio thread, "Rust panics must be rethrown, aborting". Cause: `lpc.rs::resynthesize_with`'s step clamp bounded the per-sample *step* (≤ 2·|last|) but not the magnitude, so a resonant mismatched synthesis filter grew to `inf`, whose bound `inf − inf` is `NaN`, and `f32::clamp` panics on `NaN` limits. Fixed: white-noise correction + bandwidth expansion (γ = 0.98) in `LpcState::analyze`, an absolute output ceiling + non-finite reset in the synthesis filter, non-finite input dropped in the whitening filter; regression tests `lpc.rs::unstable_target_envelope_never_panics_and_stays_bounded` (reproduces the exact production panic without the guard), `::non_finite_input_is_dropped_not_propagated`, `::mismatched_envelopes_on_a_pure_tone_stay_bounded`, `::non_finite_window_leaves_envelope_unchanged`. Re-run: formant on for 20 s, process alive, tempo 30 s / 30.35 s wall (unchanged). Same walk also found the **pre-chain meter flickering to −60 dB / peak == rms** while a stretch stage is engaged (its pull plan asks the source for 0 or 1 frames on some renders and the meter measured each render alone) — fixed with `processor.rs::LevelAccumulator` (publishes once ≥ one output block of source frames is seen), pinned by `effects_meters.rs::pre_level_is_steady_under_an_engaged_stretch_stage`.
  - **M4 PASS** (`M4_a_crop`, `M4_b_crop`, `M4_c_separated`, `M4_d_strip`): adjacent pair shows identical split figures (1 %/1 %, 2 %/2 %); with the EQ dropped between them each stretch stage reports its own figure and the chain total rises (5 % → 7 %). **UI deviation found and fixed**: the EQ row laid its 8 bands end-to-end on one line, running off the right edge of a 1 560 px window from band 4 (bands 5–8 unreachable) — `effects_view.rs` now stacks the bands vertically inside the row.
  - **M5 PASS** (`M5_strip.png`): 25 % → Quality + "Quality mode auto-switched"; back to 100 % → Performance, note cleared; +12 st → Quality + note; 0 st → Performance, cleared.
  - **M6 PASS after two fixes** (`M6_strip2.png`, `M6_d_hold_minus_no_node.png`): `-` twice → 80 %; waveform click + `+` zooms (viewport halves) and tempo stays 80 %; node removed, `-` held 1.5 s → exactly one "Add a Time Stretch node to change tempo with the + / - keys" Info. First attempt stepped tempo instead of zooming for **two independent reasons**: (1) a pointer click on either waveform seeked but never took keyboard focus (egui only focuses a `Sense::click_and_drag()` rect on `Tab`) — `waveform/input.rs` now requests focus on click/drag-start, pinned by `effects_view.rs::click_on_waveform_takes_focus_so_plus_zooms`; (2) a typed `+` is `Shift`+`=` (logical `Plus`, `Shift` held) and the claim matcher compared raw modifiers against the waveform's `plain(Plus)` claim, so the dispatcher's own layout-normalised lookup won — `actions.rs::focused_widget_owns_event` now matches the same normalised chord, pinned by `actions.rs::shift_equals_plus_on_focused_waveform_never_steps_tempo` (the prior tests only ever sent bare `Equals`).
  - **M7 PASS** (`M7_positions.png`, `M7_last_crop.png`): 5.26 s region (A 1:46.564 / B 1:51.823) at 45 % → wraps 11.8 s apart (expected 5.26/0.45 = 11.7 s), "Looping indefinitely", markers unmoved, position advances smoothly between wraps; seam click judged via `effects_position.rs`/`loop_seam.rs` (no loopback capture available).
  - **M8 PASS** (`M8_strip.png`): reorder by drag, bypass (CPU figure → 0 %), remove — each reflected immediately, Overloads stayed 0. **UI deviation found and fixed**: Now Playing had no scroll region, so two EQs (or any ≥ ~7-node chain) pushed the "Add node…" row, master volume and Queue panel off the bottom of the window with no way to reach them — the panel now sits in a capped `ScrollArea` (`now_playing.rs`, `EFFECTS_PANEL_RESERVED_HEIGHT`), which M10 needed anyway.
  - **M9 PASS** (`M9_strip.png`, `M9_g_after_down.png`): `Tab` to the Time-stretch handle, `↑` moves it to position 1, `↓` (focus retained) moves it back. Note for a11y follow-up: reaching the second row's handle took 61 `Tab`s because the EQ row's 32 band widgets precede it.
  - **M10 PASS** (`M10_a_after_17.png`): 16 nodes added; the 17th `Add` is refused inline ("Chain is full — remove a node first"), playback unaffected, panel scrolls.
  - **M11 PASS** (`M11_d_crop`, `M11_d_hdr`, `M11_g_crop`, `M11_k_crop`, `M11_stereo_strip`, `M11_invert_strip2`): Gain +20 → displays 12.0 dB, Mute → post −60 dB with pre unchanged; EQ band 5 +6 dB / Low shelf, band 8 30 000 → displays 19845 Hz; Filter Low-pass 1000 Hz, resonance 1.5 → displays 1.000; Stereo width 0 / 2 clamped, Phase invert disabled until Mono sum, Channel swap; mono sum + invert on the centred mix drops post from −0.4 to −9.9 dB peak.
  - **M12 PASS** (`M12_strip.png`, `M12_a_spec.png`): Gain −20 dB → post = pre − 26 dB at 50 % master; master 100 % → pre − 20; bypass → post == pre; master 50 → pre − 6; pre followed none of it; spectrum live; chain/per-node figures follow add/remove/bypass. **UI deviation found and fixed**: the spectrum drew linear magnitude, so real music (−20…−40 dBFS per band) filled ≈ 1–10 % of the box even at a clipping post level — `chain_meters.rs` now draws a 60 dB log scale (`spectrum_bar_height`, unit-tested).
  - **M13 PASS** (`M13_b_crop.png`, `M13_a_crop.png`, `M13_e_crop.png`): six pitch-shift nodes at +7 st with formant at the Performance preset (debug build) → Chain CPU 129 %, "Overloads: 529" and climbing, "Effect chain over budget" badge, one Warning "Effect chain over budget: Pitch shift (host) is the costliest node", playback continued, no row auto-bypassed; bypassing the six → 1 %, badge and warning cleared on their own, counter held (1 511). **Doc deviation fixed**: quickstart's "or set `MODPLAYER_EFFECTS_BURN_NS`" named an env var that was never wired into the binary (the hook is `Processor::debug_set_burn_ns`, test-only) — quickstart and the two doc comments now say so.
  - **M14 PASS for the chain, DEVIATION (pre-existing, out of slice) for audio** (`29_crop.png`, `M14_c_crop.png`, `M14_d_crop.png`, `30_pair.png`, `31_pair.png`, `32_crop.png`): Balanced → Performance and Performance → Balanced both rebuilt the stream with all 16 nodes, their order, +7 st / Formant / Quality / Bypass states intact. But after either rebuild the position froze and both meters read −60 dB; Pause/Play showed "Buffering…" forever. Cause is 003's receiver: `ConnectSource::attach` builds fresh sample/marker rings whose producers are only handed to the worker by `Initialize`, which the controller sends once per session and `handle_initialize` ignores while a worker exists — so every re-attach after registration (any preset/device change, device fallback) leaves the new RT starving. Not 008 code (unchanged since `36b4b44`). **Fixed the same day** (second commit, receiver crate): `swap.rs::RingSwap` — `attach()` parks the new worker-side ring ends whenever a worker already runs (seeding the new marker ring with a `Reattach { store }` marker so the playing track's `DecodedStore` is adopted at once), and the `RingSink`/command loop adopt them at the top of their next write/iteration, crediting the abandoned ring's unread frames to the consumed clock so markers stay on time; tests `sink.rs::parked_producer_is_adopted_and_unread_frames_credited`, `rt.rs::reattach_marker_adopts_store_without_moving_cursor_or_track_seq`, `swap.rs::*`. **Re-run on hardware** (`M14r_pair.png`, `M14r_p_crop.png`): Balanced → Performance with a 40 % time-stretch node — meters and spectrum live straight after the rebuild, position 1:19 → 1:22 over the next 8 s, node and its parameters intact.
  - **Maintainer request during the walk**: the waveform playhead now paints in `strong_text_color` (1.5 px) instead of the bars' own colour (`waveform/paint.rs`) — visible in every screenshot from `19_np.png` on.

---

## Dependencies & Execution Order

- **Setup (Phase 1)** → no dependencies.
- **Foundational (Phase 2)** → depends on Setup; blocks every user story (T005–T028 must all be done — every later phase runs a real `Processor`/`ChainModel`).
- **US1 (Phase 3)** → depends on Foundational only. Fully engine/core/action-level; no UI panel dependency.
- **US2 (Phase 4)** → depends on Foundational; its panel renders whichever kinds already exist, so in practice it follows US1 (reuses pitch-shift/time-stretch for its own row/reorder tests), but adds no code US1 depends on.
- **US3 (Phase 5)** → depends on Foundational (Gain) and extends the same `effects_view.rs`/`rt/slot.rs` files US2 created, so it follows US2 in this list, though its DSP kernels (T064–T068) have no dependency on US1 or US2 and could be built in parallel by a second contributor.
- **US4 (Phase 6)** → depends on Foundational's cost plumbing and, for the full reference-chain scenarios, on US1's stretch stage and US3's EQ; follows US1–US3.
- **Polish (Phase 7)** → depends on all four user stories (benches cover every node kind; the reference-chain soak needs pitch + stretch + EQ; the manual scenarios exercise the whole app).

### Parallel Opportunities

- Foundational: T009, T010, T011, T012, T013 (distinct files: consts/smooth/crossfade/gain/cost) can run in parallel once T005–T008 (catalog) land.
- US1: T029 (`lpc.rs`) and T040 (`loop_math.rs`) are independent of the rest of the phase and of each other.
- US3: T064 (`biquad.rs`) then T065/T066 (EQ/filter, both depend only on biquad) and T067 (`stereo.rs`, independent of biquad) — three kernels effectively parallel once T064 lands.
- US4: T077 (spectrum) and T078 (RtShared atomics) are independent files and can run in parallel; T085 (notification keys) and T088 (chain_meters widget) are likewise independent of the task immediately before them.
- Polish: T093, T094, T095, T099 touch disjoint files/crates and can run in parallel.

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 (Setup) → Phase 2 (Foundational) → Phase 3 (US1).
2. **STOP and VALIDATE**: `cargo test --workspace` green for `catalog`, `mode_rule`, `smooth`, `crossfade`, `nodes`, `chain_rt`, `stretch`, `effects_pipeline`, `effects_transitions`, `effects_position`, `effects_latency`, `controller_effects`, `actions` covering everything through T050. This alone proves the headline "slow down / transpose without breaking the other dimension" capability, verifiable end to end with no UI.
3. Demo via the automated latency/position tests (SC-001, SC-011); a human demo needs Phase 4's panel too, since there is no other way to add a node interactively yet.

### Incremental Delivery

1. Setup + Foundational → substrate ready (Gain proves the pipeline).
2. + US1 → pitch/time-stretch correctness, position, Player-follow, tempo keys (MVP, engine/core-verified).
3. + US2 → the Effect Chain panel; the product becomes interactively usable end to end for the kinds built so far.
4. + US3 → the full six-kind catalog; every tone-shaping tool works and is on-screen.
5. + US4 → meters, spectrum, overload/auto-bypass; the spec's every acceptance scenario is now automatically verifiable.
6. + Polish → benches, soak, boundary check, full accessibility/i18n sweep, manual sign-off (M1–M14).

### Parallel Team Strategy

- One person: Foundational, then US1 → US2 → US3 → US4 → Polish in order (each phase's file edits build on the previous one's, e.g. `effects_view.rs` and `rt/slot.rs` grow across US2/US3/US4).
- Two people after Foundational: Person A takes US1 (engine/core/DSP for pitch+stretch); Person B starts US3's DSP kernels (T064–T068, no dependency on US1) in parallel, then both converge on US2's panel once US1 lands, then split US4's engine-side (meters/overload) vs UI-side (chain_meters widget) work.

---

## Notes

- [P] tasks touch different files with no ordering dependency on each other.
- Every module task's description names the exact test(s) it must make pass (FR-018's tests are mandatory, not optional, for this feature).
- Any PR touching `crates/modplayer-engine/` or `crates/modplayer-effects/` needs a real-time safety note and the engine maintainer's sign-off (Constitution I / Governance) — this applies to nearly every task in Phases 2–6.
- Commit after each task or logical group; verify the named test(s) actually fail before the task's implementation and pass after.
- `crates/modplayer-effects/src/rt/slot.rs`, `src/rt/chain.rs`, `crates/modplayer-ui/src/effects_view.rs`, and `crates/modplayer-engine/src/processor.rs` are each touched by tasks in several phases — those edits are inherently sequential (never mark them `[P]` against each other) even though they sit in different phases.

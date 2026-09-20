# Contract: Effects service (core model + controller façade)

**Crates**: `modplayer-core` (`src/effects/{mod,model,view}.rs`,
`src/controller.rs`, `src/notifications.rs`, `src/actions/catalog.rs`),
`modplayer-effects` (`catalog.rs` — consumed, not modified here).
Implements FR-002, FR-003 (data), FR-004, FR-008, FR-010 (clamp return),
FR-012 (notification half), FR-014, FR-016, FR-017; research R8, R10,
R11, R15. No real-time code: everything here runs on the UI thread and
talks to the engine only through `Command`s (`push_command_retrying`) and
`RtShared` reads.

## 1. `modplayer_core::effects::ChainModel` (pure; see data-model.md §2)

```rust
impl ChainModel {
    pub fn new(source_rate: u32) -> Self;
    pub fn add(&mut self, kind: NodeKind, owner: NodeOwner) -> Result<(NodeId, Vec<Command>), ChainError>;
    pub fn remove(&mut self, id: NodeId) -> Result<Vec<Command>, ChainError>;
    pub fn move_to(&mut self, id: NodeId, index: usize) -> Result<Vec<Command>, ChainError>;
    pub fn move_by(&mut self, id: NodeId, delta: i8) -> Result<Vec<Command>, ChainError>;
    pub fn set_bypass(&mut self, id: NodeId, bypassed: bool) -> Result<Vec<Command>, ChainError>;
    /// Returns the *clamped* value the engine will apply (SC-005), plus commands.
    pub fn set_param(&mut self, id: NodeId, param: ParamId, requested: f32) -> Result<(f32, Vec<Command>), ChainError>;
    pub fn set_mode(&mut self, id: NodeId, mode: QualityMode) -> Result<Vec<Command>, ChainError>;
    pub fn set_source_rate(&mut self, rate: u32) -> Vec<Command>;      // FR-014 re-clamp + re-send
    pub fn mark_auto_bypassed(&mut self, slot: u8);                    // from Event::AutoBypassed
    pub fn first_time_stretch(&self) -> Option<NodeId>;
    pub fn node_by_slot(&self, slot: u8) -> Option<&NodeModel>;
    pub fn replay(&self) -> Vec<Command>;                              // research R11
    pub fn nodes(&self) -> &[NodeModel];
    pub fn capacity(&self) -> usize;                                   // MAX_NODES
}
```

Rules:

| # | Rule | Test |
|---|---|---|
| G1 | `add` beyond `MAX_NODES` → `Err(ChainError::Full)`; the model and slot set are unchanged | `add_17th_is_refused_without_corruption` |
| G2 | Ids are monotonic and never reused; slots are reused (lowest free) | `ids_never_reused_slots_are` |
| G3 | `set_param` clamps with `catalog::clamp(kind, param, requested, source_rate)` and stores the clamped value; `Err(WrongKind)` for a param the kind lacks | proptest `set_param_returns_and_stores_clamped` |
| G4 | On `semitones`/`ratio` changes, `mode_after_value_change` runs on the clamped value; a resulting mode change emits a second `ChainSetParam` for `mode`; `set_mode` applies `mode_after_user_set` | `auto_switch_flags_and_reverts`, `user_forced_performance_persists_until_next_excursion` |
| G5 | `set_bypass(id, false)` clears `auto_bypassed` | `manual_unbypass_clears_auto_flag` |
| G6 | `move_by` at either end is `Ok` with no commands | `move_by_at_ends_is_noop` |
| G7 | `set_source_rate` re-clamps only `nyquist_clamped` params and emits a command per value that actually changed | `rate_change_reclamps_only_frequencies` |
| G8 | `replay()` reproduces the RT state from scratch: per node in order `ChainInsert`, `ChainSetParam` for every param that differs from its default, `ChainSetBypass` if bypassed | `replay_is_complete_and_ordered` |
| G9 | `first_time_stretch` ignores bypass and returns the earliest in order | `first_time_stretch_ignores_bypass` |

## 2. `PlaybackController` façade

```rust
pub fn chain(&self) -> &ChainModel;
pub fn chain_view(&self) -> ChainView;                 // model + RtShared costs/flags (data-model.md §2.4)
pub fn chain_meters(&self) -> MeterSnapshot;           // RtShared reads (data-model.md §2.5)
pub fn chain_add_node(&mut self, kind: NodeKind) -> Result<NodeId, ChainError>;   // owner = Host
pub fn chain_remove_node(&mut self, id: NodeId) -> Result<(), ChainError>;
pub fn chain_move_node(&mut self, id: NodeId, to_index: usize) -> Result<(), ChainError>;
pub fn chain_move_node_by(&mut self, id: NodeId, delta: i8) -> Result<(), ChainError>;
pub fn chain_set_bypass(&mut self, id: NodeId, bypassed: bool) -> Result<(), ChainError>;
pub fn chain_set_param(&mut self, id: NodeId, param: ParamId, value: f32) -> Result<f32, ChainError>;
pub fn chain_set_mode(&mut self, id: NodeId, mode: QualityMode) -> Result<(), ChainError>;
pub fn tempo_step(&mut self, direction: i8);            // FR-017
```

Rules:

| # | Rule | Test (`tests/controller_effects.rs`) |
|---|---|---|
| C1 | Every façade call applies the model op and pushes its commands with `push_command_retrying` (never dropped; queue-full retried on `tick`) | `commands_are_pushed_in_model_order` |
| C2 | `chain_set_param` returns the model's clamped value; the UI displays *that* | `set_param_returns_clamped` |
| C3 | `tempo_step(d)`: with a first time-stretch node, `chain_set_param(id, ratio, current + 0.10·d)` (clamped, FR-008 runs); without one, raise `effects-no-time-stretch` (Info) **only if not already visible** and change nothing | `tempo_step_moves_first_time_stretch_by_ten_points`, `tempo_step_clamps_at_bounds`, `tempo_step_without_node_notifies_once` |
| C4 | `open_stream_on` success path: after the 006 loop re-push, `chain.set_source_rate(new_rate)` (dropping its commands — the replay covers them) then push every `chain.replay()` command | `stream_rebuild_replays_chain`, `rate_change_rebuild_reclamps_eq` |
| C5 | `drain_engine_events`: `Event::Overload { costliest_slot, .. }` → `overload_count` mirrored from `RtShared`; if `effect-chain-over-budget` is not visible, raise it (Warning, args `$node` = kind label key, `$owner` = owner label) and set `over_budget_notified`; `Event::AutoBypassed { slot }` → `chain.mark_auto_bypassed(slot)` + raise `effect-chain-auto-bypassed` | `overload_event_raises_one_keyed_warning`, `auto_bypassed_event_marks_model` |
| C6 | `tick`: if `over_budget_notified && !shared.over_budget()` → `dismiss_by_key("effect-chain-over-budget")`, clear the flag | `warning_clears_when_rt_reports_clean_window` |
| C7 | `tick` (research R8.1, amended 2026-09-19): a non-unity `advance_rate` sends **no** `SourceCommand::Seek` — the receiver is paced by the engine's consumption in both of its feeds, so its position never drifts; the former predicted-drift re-seek rewound the real-time cursor on every send (quickstart M2) | `player_is_never_reseeked_for_tempo_drift`, `no_reseek_at_unity` |
| C8 | `drain_source_events` (research R8.2): `SourceEvent::EndOfTrack` while `advance_rate < 1.0` and `engine_position < len − drift_bound` → `end_of_track_pending = true`, not mirrored; `tick` mirrors it once the threshold is crossed. While `advance_rate > 1.0`: when `engine_position ≥ len − one_buffer` and not looping, mirror `Input::EndOfTrack` itself, set `engine_ended_track_seq`, and swallow the Player's next `EndOfTrack` until `TrackStarted` | `end_of_track_deferred_at_half_speed`, `engine_ends_track_first_at_double_speed_and_player_event_is_swallowed`, `unity_behaviour_unchanged` (re-runs 003/006 end-of-track tests) |
| C9 | `chain_view().nodes[i].cost_pct` = `RtShared::node_cost(slot)` while `transport == Playing`, else `0.0`; `total_cost_pct` = Σ; `over_budget` and `overload_count` from `RtShared` | `view_reports_zero_cost_when_not_playing` |
| C10 | `clear_for_sign_out` and `shutdown` leave the chain untouched (session-scoped, not account-scoped); a new `PlaybackController` starts with an empty chain | `chain_survives_sign_out_and_is_empty_at_construction` |

## 3. Notifications (data-model.md §6)

Keys added to `modplayer_core::notifications`:
`KEY_EFFECT_CHAIN_OVER_BUDGET`, `KEY_EFFECT_CHAIN_AUTO_BYPASSED`,
`KEY_EFFECTS_NO_TIME_STRETCH`. Strings in `locales/en-US/effects.ftl`
(see [ui-effect-chain.md](ui-effect-chain.md) §6).

## 4. Action catalog delta (research R15; 007 contracts/action-registry.md)

- `HostAction::ToggleEffectChain` appended to `HostAction::ALL` and
  `CATALOG` (`[ActionDef; 45]`): id `host.nav.toggle_effect_chain`, label
  key `action-nav-toggle-effect-chain`, category Navigation, scope
  `NowPlaying`, `repeats_while_held: false`, `enabled_by_default: true`,
  defaults `["E"]`.
- `TempoStepUp` / `TempoStepDown`: `enabled_by_default: true`; ids,
  scope, repeat and bindings unchanged.
- 007's tests re-pointed: catalog length 45; `shipped_defaults_are_conflict_free`
  must still pass (`E` unused elsewhere); `no_continuous_actions_in_this_slice`
  renamed `all_actions_are_triggers` (still true — the tempo steps are
  repeating triggers, not continuous actions).
- `ActionRegistry` seeded from settings applies user overrides for the
  two tempo actions exactly as for any other (their `[keybindings]`
  entries, if any, were valid while disabled — 007 FR-012).

Tests: `modplayer-core tests/actions.rs::catalog_has_45_entries_and_effects_enabled`,
`::toggle_effect_chain_defaults_to_e_in_now_playing`.

## 5. Errors

`ChainError` (`thiserror`): `Full`, `UnknownNode`, `WrongKind`. Refusal
keys for the UI: `Full → "effects-chain-full"`; the other two are
programming errors surfaced as debug assertions in the UI layer and
ignored in release (a stale id after a remove simply does nothing).

# Contract: Focus arbitration (core)

**Crate**: `modplayer-core` (`src/plugins/focus.rs` new, `src/plugins/
{host,apply,view,mod}.rs`, `src/controller.rs`, `src/settings/
{model,store}.rs`, `src/actions/catalog.rs`; `tests/focus_arbiter.rs`
new, `tests/controller_transport_focus.rs` new, `tests/
{controller_plugins_permissions,controller_plugins_lifecycle,settings,
actions}.rs` extended). Implements FR-001–FR-007, FR-010–FR-012,
FR-014 (delivery side); research R1, R3, R4, R5, R7, R11, R12, R13.
Types in [data-model.md](../data-model.md) §1–§2.

## 1. Arbiter rules (pure; `tests/focus_arbiter.rs`)

| # | Rule | Spec |
|---|---|---|
| A1 | Exactly one holder: `holder ∈ {Host, Plugin(p)}`; the arbiter is the only writer of `FocusToken` (via `PluginHost::apply_focus_changes`). | FR-001 |
| A2 | `request(p)` never fails and never returns a refusal to the caller; it records `p` in FIFO `pending` unless `p` is the holder or already pending (no-op, position kept). | FR-003, FR-011 |
| A3 | Under `FirstRequestWins`, `request(p)` grants immediately iff `holder == Host ∧ !host_locked_for_track ∧ pending.is_empty()`; otherwise it is recorded like any other. | FR-005 |
| A4 | Under `Manual` and `AutoOnInteraction`, `request(p)` never grants. | FR-005 |
| A5 | `give(p)`: revoke the current plugin holder (if any) → grant `p` → drop `p` from `pending`; no re-evaluation against earlier requesters; under `FirstRequestWins` clears `host_locked_for_track`. `give(holder)` is a no-op. | FR-008, Clarifications |
| A6 | `local_host_action()` revokes to Host only under `AutoOnInteraction` and only when a plugin holds focus; `take_back()` revokes under every policy and, under `FirstRequestWins`, sets `host_locked_for_track`. | FR-002, FR-005, FR-006 |
| A7 | `release(p)`: holder → `vacate(p, Voluntary)`; pending → withdraw; neither → no-op. Always `Ok`. | FR-004, FR-011 |
| A8 | `vacate(p, kind)`: holder → Host (`Revoked` emitted for `Voluntary` only), `p` dropped from `pending`; then under `FirstRequestWins ∧ !host_locked_for_track` → `Granted(pending.first())`. Never a refill after `take_back`. | FR-006, FR-007 |
| A9 | `on_track_changed()`: `FirstRequestWins` only → holder → Host (`Revoked`), `pending` cleared, lock cleared. Other policies: no-op. | FR-006 |
| A10 | `set_policy(x)` changes nothing but `policy`; `pending` survives and is judged by the new policy from the next event on. | FR-005, edge case |
| A11 | Every returned `Vec<FocusChange>` lists all `Revoked` before any `Granted`. | FR-014 |
| A12 | `PluginId`s that are not running (the caller passes `running: false` or `remove_plugin`) are never granted and are purged from `pending`. | FR-011 |

Named tests: `manual_never_auto_grants`, `auto_never_grants_on_request`,
`first_wins_grants_first_only`, `first_wins_later_request_pending`,
`first_wins_track_change_resets_holder_and_queue`,
`first_wins_release_refills_earliest`, `first_wins_fault_refills_earliest`,
`first_wins_take_back_locks_until_track_change`,
`give_revokes_then_grants_in_order`, `give_to_holder_is_noop`,
`request_twice_keeps_position`, `release_while_pending_withdraws`,
`release_by_non_holder_is_noop`, `local_action_revokes_only_under_auto`,
`policy_switch_keeps_holder_and_queue`, `revoked_precede_granted`,
plus a `proptest` `single_holder_invariant` over random operation
sequences (Constitution VIII: state machine over a host primitive).

## 2. Controller integration (`tests/controller_transport_focus.rs`, on `FakeBackend` + `SyntheticSource` with `MODPLAYER_PLUGIN_FIXTURES`)

| # | Rule | Spec |
|---|---|---|
| C1 | `plugins::apply` runs each request under `TransportActor::Plugin(id)`; `RequestFocus` → `focus_request`, `ReleaseFocus` → `focus_release`, both `Ok(Response::Ok)`. The eight focus-gated requests re-check `arbiter.holder() == Plugin(id)` and return `Refusal::no_focus()` otherwise, before any controller call. | FR-001, FR-003, R12 |
| C2 | `note_local_transport_action()` runs at the start of `dispatch(Input::{Play,Pause,Stop,Seek,SkipForward,SkipBack})` and on `Ok` from `arm_loop`/`disarm_loop`, only when `transport_actor == LocalUser`. `Effect::ApplyPendingTransferCommand` and `shutdown`/sign-out's `stop()` run under `Remote`. `Input::RemoteCommand` never triggers it. | FR-002, FR-002a |
| C3 | `apply_focus_changes` sets `FocusToken` to the final holder first, then sends events to each named handle in order; a missing handle is skipped. A `Revoked` for an auto-policy host action is therefore enqueued before the same `dispatch`'s `PlayStateChanged`. | FR-014, R4 |
| C4 | `PluginHost::stop(id, ..)` order: `apply_focus_changes(arbiter.vacate(id, Fault))` → `disarm_if_owned_by` → `remove_transient_owned_by` → `orphan_owned_by` → `Control::Unloading`. Persisted markers untouched. | FR-007 |
| C5 | Non-fault transitions never call into `TrackMarkers`. | FR-007 |
| C6 | `fan_out_plugin_playback_events`'s `TrackChanged` branch calls `on_track_changed()` and applies the changes before fanning `TrackChanged` out. | FR-006, R7 |
| C7 | `set_focus_policy` persists through `persist_settings`; `launch()` seeds the arbiter from `AudioSettings.focus_policy`; holder = Host and `pending` empty at construction. | FR-012 |
| C8 | `focus_give(id)` is ignored unless `id` is a current `TransportFocusView` row; `focus_take_back()` with `holder == Host` is a no-op. | FR-008 |
| C9 | `transport_focus_view()` rows = `enabled ∧ lifecycle ∈ {Loading, Active} ∧ grants ∋ transport.control`, sorted by name; `holder`/`request_order` from the arbiter. | FR-008 |
| C10 | `plugin_restart`/`plugin_enable` never touch the arbiter (the plugin re-requests). | Clarifications |

Named tests (each asserts via `transport_focus_view()`, the fixtures'
`debug_probe` logs and `markers()`):

- `non_holder_seek_is_refused_no_side_effect` — focus-a holds (via
  `focus_give`), focus-b's `seek` → `no_focus`, position unchanged
  (SC-001, US1-1).
- `host_side_recheck_refuses_after_revoke` — inject an admitted
  `Request::Seek` envelope for a plugin that no longer holds focus
  (C1/R12).
- `local_loop_toggle_applies_and_returns_focus_under_auto` — `L` path
  (`toggle_current_loop`) with a plugin holder: region toggled, holder =
  Host, plugin log shows `focus_revoked` before `loop_*` (US1-2).
- `local_action_keeps_holder_under_manual_and_first_wins` (SC-006).
- `remote_pause_applies_without_focus_change` — `Input::RemoteCommand
  (Pause)` and a `PendingTransferCommand` path: playback paused, holder
  unchanged, plugin log shows `play_state_changed` only (US1-3).
- `suspended_holder_returns_to_host_and_disarms_loop` — focus-a holds
  with an armed transient region; drive a suspension (`debug_inject`
  `RuntimeEvent::Suspended`); after one `tick`: holder Host, armed
  region `None`, persisted markers unchanged (US1-4, SC-003).
- `disabled_holder_returns_to_host_and_disarms_loop` (US1-5).
- `manual_two_requests_both_pending_until_give` (US2-1).
- `auto_give_focus_grants_immediately` (US2-2).
- `first_wins_a_then_b_over_twenty_tracks` — 20 `TrackStarted` cycles
  alternating request order (SC-005, US2-3/4).
- `policy_switch_leaves_holder` (US2-5).
- `first_wins_take_back_no_refill` (US2-6), `first_wins_release_refills`
  (US2-7).
- `policy_persists_holder_does_not` — write, rebuild controller from the
  same `SettingsStore`, assert policy restored, holder Host, no rows
  requesting (US2-8, SC-007).
- `give_to_never_requested_plugin_delivers_granted` (edge case).
- `release_while_pending_withdraws_request` (edge case).
- `pending_plugin_disabled_leaves_queue_and_panel` (FR-011).
- `restarted_plugin_is_observer` (C10).
- `non_fault_transitions_keep_loop_armed` (C5).
- `event_order_revoked_before_granted_on_plugin_to_plugin` (FR-014).
- `toggle_transport_panel_action_in_catalog` (in `tests/actions.rs`:
  id, label key, scope, default `T`, no default conflict).
- `settings_round_trip_focus_policy` + proptest strategy extension (in
  `tests/settings.rs`; unknown value → default + warning).

Rewritten 009 tests (research R10): `request_focus_is_recorded_never_
refused`, the flood rate-limit test (first-wins + `focus_give`),
`queue_write_ignores_focus` (explicit `focus_give`).

## 3. Non-goals (this contract)

No broadcast event; no per-track persistence of holder; no Performance
Mode indicator; no hot-reload retention; no MIDI origin (004's job).

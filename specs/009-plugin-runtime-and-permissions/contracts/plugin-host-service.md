# Contract: Plugin Host service (core) and controller façade

Crate: `modplayer-core` (`src/plugins/`, deltas in `markers/`, `effects/`,
`notifications.rs`, `controller.rs`). Requirement ids: FR-003, FR-005,
FR-010–FR-014, FR-017–FR-019, FR-021, FR-023, FR-024, FR-026, FR-027;
DM-10, DM-11; EC-6.x; Constitution II, III, VIII, X. Decisions:
[../research.md](../research.md) R2, R8–R12, R14, R16, R17, R19.

## 1. Controller façade

| Method | Behaviour |
|---|---|
| `PlaybackController::new(..)` | constructs `PluginHost::discover(fixtures = env MODPLAYER_PLUGIN_FIXTURES == "1")`; every valid bundled record starts `enabled = true` and is loaded in `launch()` |
| `launch()` | after the existing device/tone steps: `plugins.load_all_enabled()` |
| `set_waker(f)` | stores `Arc<dyn Fn() + Send + Sync>`; the host's request channel calls it after each `send` (UI installs `ctx.request_repaint`) |
| `plugins_view() -> PluginsView` | rows sorted by `name` (case-insensitive), gauges read from `PluginGauges`, `None` for CPU/memory unless `Active` |
| `plugin_enable(id)` | `Disabled → Loading` (fresh thread/state); no-op for `Invalid`; never prompts |
| `plugin_disable(id)` | `stop(id, Disable)` then `enabled = false`; from `Suspended` the `unloading` is still delivered to a fresh-less context? No — a suspended context is already gone (RT8 ran at suspension), so disable only flips the flag and dismisses the notice |
| `plugin_restart(id)` | only from `Suspended`: dismiss the notice, `Suspended → Loading`; the session suspension counter is **not** reset |
| `plugin_log() -> &PluginLog` | the 1 000-entry ring |
| `clear_for_sign_out()` | additionally: for every plugin, flush and drop the in-memory track scope and delete `<plugin-state>/<id>/tracks/`; `state.plugin` untouched (FR-014) |
| `shutdown()` | first `stop(all running, Shutdown)` and wait ≤ 250 ms for `Exited`s, then the existing sequence |
| `tick()` | new first steps: `drain_plugin_requests()`, `drain_plugin_runtime_events()`, `reap_plugin_threads()`; new last steps: `publish_plugin_snapshot_if_changed()`, `fan_out_revision_events()` |

## 2. Lifecycle rules (L)

- **L1 Discovery** (FR-003): `bundled::packages()` always; `bundled::
  fixtures()` only when `MODPLAYER_PLUGIN_FIXTURES=1`; each package parses
  through `manifest::validate` → `PluginRecord` (`Invalid` rows keep their
  reason; `enabled = false`, `health = None`).
- **L2 PluginId** (R10): `PluginIdTable` interns identifiers in sorted
  order at discovery; ids are stable for the session; owner strings found
  in marker files are interned on load.
- **L3 Load**: `Loading` spawns `PluginHandle` with `Grants::from_bundled`,
  a fresh `PluginStateStore` (plugin scope loaded from disk; track scope
  empty), the shared `FocusToken`, `PlaybackSnapshot`, `PluginSnapshot`
  and the writer. If a current track exists, a `TrackChanged` is queued
  before anything else so the plugin sees the current track after
  `ready_ack`.
- **L4 Ready**: `RuntimeEvent::Ready` ⇒ `Active`, `health = Ok`,
  `abort_window.clear()`, `warning_until = None`, `chain.readopt(id)`
  (bump chain revision), dismiss `plugin-suspended:<identifier>` if
  present.
- **L5 Abort accounting** (FR-010): every `HandlerAborted` pushes `now`
  into `abort_window`, evicts entries older than 60 s, and when the
  window holds ≥ 3 sets `warning_until = now + 5 min`; the error text is
  logged at `error` with target `plugin:<identifier>`.
- **L6 Suspension** (FR-011): `RuntimeEvent::Suspended{cause}` ⇒
  `lifecycle = Suspended{cause}`, `suspensions_this_session += 1`,
  core-side teardown (L7), `raise_keyed(Warning, "plugin-suspended",
  [$plugin, $cause], [RestartPlugin(id), DisablePlugin(id)],
  "plugin-suspended:<identifier>")`. If the counter reached 3:
  `plugin_disable(id)` and `raise_keyed(Warning, "plugin-auto-disabled",
  [$plugin], [], "plugin-auto-disabled:<identifier>")`, replacing the
  suspension notice.
- **L7 Core-side teardown** (FR-012 order): (a) `focus.release_if(id)`;
  (b) `markers.disarm_if_owned_by(Plugin(id))` (loop engine command as the
  host `disarm_loop` does; `loop_disarmed{by: identifier}` fan-out);
  (c) `markers.remove_transient_owned_by(Plugin(id))` (revision bump);
  (d) timers: nothing to do (thread-local); (e) `chain.orphan_owned_by(id)`
  (revision bump; nodes keep running with last parameters; the RT is not
  touched). No UI contributions exist to remove.
- **L8 Reap**: `reap_plugin_threads()` joins handles whose
  `is_finished()`; `Draining → Disabled` on `Exited` (or after the join
  deadline at shutdown).
- **L9 Uninstall**: no method exists; `PluginRow.can_uninstall` is always
  `false` (FR-013).
- **L10 Fixture-only probe**: `Request::DebugProbe` is answered only when
  `fixtures_enabled`, else `invalid_state`/`invalid_argument`.

## 3. Request application (A) — after `Gateway::admit` passed

| Request | Applies via | Refusals (per-call) |
|---|---|---|
| `Play/Pause/Toggle/SkipNext/SkipPrevious` | existing `play/pause/skip_forward/skip_back` | `invalid_state`/`no_track` when `transport_enabled()` is false |
| `Seek{ms}` | `seek(Duration)` | `no_track` |
| `ArmLoop{region}` | owner check → `arm_loop(region)`; `MarkerError::{RegionIncomplete, RegionTooShort}` map to `invalid_state`/`region_*`; disarms any other armed region (006 FR-008) | `not_found`, `not_owner` |
| `DisarmLoop` | `armed_region_owner() == Plugin(id)` → `disarm_loop()` | `invalid_state`/`nothing_armed` |
| `Queue*` | existing `queue_reorder/queue_remove/queue_play_next/queue_play_next_track` (R21) | `not_found` |
| `ListMarkers/ListChain/QueueList` | never reach core (snapshot); listed for completeness | – |
| `CreateMarker` | `markers.add_point_owned(pos, Plugin(id), transient)` | `no_track`, `invalid_state`/`marker_limit` |
| `Move/Rename/Recolor/DeleteMarker` | `owner_of(id) == Plugin(id)` → host method | `not_found`, `not_owner` |
| `CreateLoopRegion` | `new_loop_region_owned(a, b, owner, transient)` (2 markers) | `marker_limit` |
| `SetCue{slot,pos}` | `set_cue_owned` — empty slot creates (owner = plugin); own cue moves; other owner ⇒ `not_owner` | `invalid_argument` |
| `CreateNode{kind, suggested}` | `NodeKind::parse(kind)`; `chain.add_at(kind, Plugin(id), resolve_position(suggested))` + engine commands | `chain_full`, `unsupported_kind` |
| `SetParam/ScheduleParam/SetBypass/RemoveNode` | owner check → `chain_set_param/chain_set_bypass/chain_remove_node`; `bypass(false)` clears `auto_bypassed` (008 G5) | `not_found`, `not_owner` |
| `DebugProbe` | forwarded to the plugin's thread as `Control::Probe` (fixtures only) | `invalid_argument` |

All applies set `last_marker_actor = Plugin(id)` where markers change so
the coalesced `marker_changed` carries the right actor (host paths reset it
to `Host`).

## 4. Event fan-out (E)

| Trigger (controller) | Event | To |
|---|---|---|
| `dispatch(Input::TrackStarted)` / queue current change | `TrackChanged{track}` (+ `PlaybackSnapshot.track_generation += 1`) | `playback.observe` |
| intent change (Playing/Paused/Stopped) | `PlayStateChanged` (+ snapshot intent) | `playback.observe` |
| any `QueueChange` | `QueueChanged{items}` | `playback.observe` |
| `markers.revision` changed since last tick | `MarkerChanged{actor, revision}` | `markers.read` |
| `arm_loop`/`disarm_loop` (host or plugin), `Event::LoopWrapped` | `LoopArmed/LoopDisarmed/LoopWrapped` | `markers.read` |
| `chain.revision` changed (add/remove/move/bypass/auto-bypass/orphan/readopt) | `EffectChainChanged{chain}` | `audio.effects` |
| `Ready` | `ReadyAck` | that plugin |
| `stop(id, reason)` | `Unloading{reason}` | that plugin |

`position` and `meter` are produced on the plugin thread (RT6/RT10) — the
controller only keeps `PlaybackSnapshot` current.

## 5. Notifications (FR-011)

| Key | Severity | Args | Actions | Dedupe |
|---|---|---|---|---|
| `plugin-suspended` | Warning | `$plugin` (name), `$cause` (localised cause) | Restart, Disable | `plugin-suspended:<identifier>` |
| `plugin-auto-disabled` | Warning | `$plugin` | – | `plugin-auto-disabled:<identifier>` |

`NotificationAction::RestartPlugin(id)` / `DisablePlugin(id)` are handled
by `modplayer-ui/src/notifications.rs` by calling the façade.

## 6. Tests (`crates/modplayer-core/tests/`)

`plugins_manifest_discovery.rs`: `fixtures_only_with_env`,
`invalid_fixture_listed_not_loaded`, `rows_sorted_by_name`, `no_uninstall`.
`controller_plugins_lifecycle.rs`: `ready_ack_is_first_event`,
`never_ready_suspended_with_restart`, `hang_fixture_suspended_audio_continues`
(FakeBackend rendering on a second thread; asserts no missed callback and
`transport_enabled()` throughout), `three_suspensions_auto_disable`,
`restart_keeps_session_counter`, `disable_runs_teardown_in_order`
(focus → loop → transient markers → orphan), `suspended_readopts_on_ready`,
`shutdown_delivers_unloading_and_bounds_wait`, `sign_out_clears_track_state_only`,
`warning_after_three_aborts_and_clears_after_5min` (injected clock).
`controller_plugins_permissions.rs`: `matrix_every_request_without_its_permission_is_denied_with_no_effect`
(iterates `RequestKind::ALL` against the observer fixture and snapshots
models before/after), `not_owner_vs_not_found`, `arm_loop_permission_before_focus`,
`request_focus_contention_invalid_state`, `queue_write_ignores_focus`,
`rate_limit_1000_seeks`, `chain_full_and_marker_limit`,
`set_cue_ownership`, `transient_marker_lifetime`, `host_edit_keeps_owner_and_notifies`.
`markers_model.rs` (+): `owner_roundtrip_in_file`, `transient_never_encoded`,
`remove_transient_owned_by`. `notifications.rs` (+): `raise_keyed_replaces`.
`effects_model.rs` (+): `add_at_resolves_before_after_index_or_appends`,
`orphan_and_readopt`.

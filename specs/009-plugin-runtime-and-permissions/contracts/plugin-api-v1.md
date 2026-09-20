# Contract: Plugin API v1.0 (the Luau-facing API object)

Source of truth: `crates/modplayer-capability-gateway/api/v1.toml` (research
R6). This document is the human rendering the plan commits to; the
generated reference `docs/plugin-api/v1.md` must match it and the runtime
table-for-table (Constitution IX). Requirement ids: FR-007, FR-008,
FR-015–FR-022, FR-025, FR-028.

## 1. Loading and `ready()`

The entry script runs once inside a sandboxed Luau state with these
globals only: the safe standard library (`string`, `table`, `math`,
`bit32`, `utf8`, `pairs`, `ipairs`, `next`, `select`, `tostring`,
`tonumber`, `type`, `pcall`, `error`, `assert`, `unpack`) and one table,
**`api`**, shaped by the plugin's grants: every namespace of this slice
(`playback`, `transport`, `queue`, `markers`, `effects`, `state.plugin`,
`state.track`, `timers`, `log`) is always present so that a call the
plugin was not granted returns `nil, { code = "permission_denied",
reason = "not_granted" }` (spec US2 acceptance 1) rather than a Lua
error; `api.granted` lists the granted permission names and
`api.version` is `{ major = 1, minor = 0 }`. Methods that belong to a
later API minor (`define_section`, `serialize_state`, `ui.*`, …) are
absent, so calling them is an ordinary Lua error inside the handler.

```lua
api.on("track_changed", function(ev) ... end)   -- register a handler (one per event; later call replaces)
api.ready()                                     -- must be called within 5 s of load; before it no event is delivered
```

`api.ready()` returns `true` once; later calls return `false, refusal`
(`invalid_state`/`not_ready` is *not* used here — repeated `ready()` is
harmless). The first event after `ready()` is always `ready_ack`.

## 2. Result convention (FR-008)

Every request returns either `ok, value` or `nil, refusal` where
`refusal = { code = "<one of six>", reason = "<snake_case detail>",
message = "<sentence>" }`. A request never throws. Passing a wrong
argument type/shape yields `invalid_state`/`invalid_argument` (not a Lua
error) so a plugin can always inspect what went wrong.

Check order: permission → transport focus → rate limit → per-call
validation (ownership, ids, capacity, state). A refused call has no host
side effect and consumes no rate quota unless it reached the rate check.

## 3. Requests

Positions are milliseconds (integers); ids are integers from the host.
"Focus" = the calling plugin currently holds transport focus.

| Namespace / call | Requires | Focus | Category | Returns / refusals |
|---|---|---|---|---|
| `api.playback.subscribe_position(rate_hz)` | `playback.observe` | – | – | `ok`; rate clamped to 1..60 (default 10 if never called) |
| `api.playback.state()` | `playback.observe` | – | – | `{ state, position_ms, track }` snapshot (no RPC) |
| `api.transport.play()` / `pause()` / `toggle()` | `transport.control` | yes | transport | `ok` |
| `api.transport.seek(position_ms)` | `transport.control` | yes | transport | `ok`; `invalid_state`/`no_track` |
| `api.transport.skip_next()` / `skip_previous()` | `transport.control` | yes | transport | `ok` |
| `api.transport.request_focus()` | `transport.control` | – | transport | `ok`; `invalid_state`/`focus_held` when another plugin holds it |
| `api.transport.release_focus()` | `transport.control` | – | transport | `ok` always |
| `api.transport.arm_loop(region_id)` | `markers.write` | yes | transport | `ok`; `permission_denied`/`not_owner`; `not_found`; `invalid_state`/`region_incomplete` or `region_too_short` |
| `api.transport.disarm_loop()` | `markers.write` | yes | transport | `ok`; `invalid_state`/`nothing_armed` |
| `api.queue.list()` | `queue.write` | – | transport | `{ items = { {id, track, title, index, is_current}, … } }` |
| `api.queue.move(item_id, to_index)` | `queue.write` | – | transport | `ok`; `not_found` |
| `api.queue.remove(item_id)` / `play_next(item_id)` | `queue.write` | – | transport | `ok`; `not_found` |
| `api.queue.add(track_id)` | `queue.write` | – | transport | `ok, item_id`; `not_found` (research R21) |
| `api.markers.list()` | `markers.read` | – | markers | `{ markers = { MarkerInfo… }, armed = region_id or nil }` (all owners) |
| `api.markers.create(position_ms, { name=, transient= })` | `markers.write` | – | markers | `ok, marker_id`; `invalid_state`/`marker_limit`, `no_track` |
| `api.markers.move(id, position_ms)` / `rename(id, name)` / `recolor(id, color)` / `delete(id)` | `markers.write` | – | markers | `ok`; `permission_denied`/`not_owner`; `not_found` |
| `api.markers.create_loop(a_ms, b_ms, { transient= })` | `markers.write` | – | markers | `ok, region_id` (two markers, owned); `invalid_state`/`marker_limit` |
| `api.markers.set_cue(slot, position_ms)` | `markers.write` | – | markers | `ok, marker_id`; `permission_denied`/`not_owner` when the slot holds another owner's cue; `invalid_state`/`invalid_argument` for slot ∉ 1..8 |
| `api.effects.list_chain()` | `audio.effects` | – | effects | `{ nodes = { NodeInfo… } }` (all owners) |
| `api.effects.create_node(kind, { before=, after=, index= })` | `audio.effects` | – | effects | `ok, node_id`; `invalid_state`/`chain_full`, `unsupported_kind` |
| `api.effects.set_param(node_id, param, value)` / `schedule_param(node_id, param, value, at_ms)` | `audio.effects` | – | effects | `ok`; `permission_denied`/`not_owner`; `not_found` |
| `api.effects.bypass(node_id, bool)` / `remove_node(node_id)` | `audio.effects` | – | effects | `ok`; `permission_denied`/`not_owner`; `not_found` |
| `api.state.plugin.get(key)` / `set(key, value)` / `remove(key)` | `state.plugin` | – | state | `value or nil`; `budget_exceeded`/`storage_cap`, `key_too_long`, `value_too_large` |
| `api.state.track.get(key)` / `set(key, value)` / `remove(key)` | `state.track` | – | state | as above; `invalid_state`/`no_track` when no current track |
| `api.timers.set_timeout(ms)` / `set_interval(ms)` | – | – | timers | `ok, handle`; `invalid_state`/`timer_limit` (257th) or `invalid_argument` (< 1 ms) |
| `api.timers.schedule_at_position(position_ms)` | – | – | timers | `ok, handle`; `timer_limit` |
| `api.timers.clear(handle)` | – | – | timers | `ok`; `not_found` |
| `api.log.info(msg)` / `warn(msg)` / `error(msg)` | – | – | – | `ok` (tagged console entry) |
| `api.debug_probe(name)` | – (fixture flag only) | – | – | fixture-defined JSON value; absent when fixtures are off |

`schedule_param` in this slice applies at `at_ms` through the same
next-buffer ramp as `set_param` (the host converts it into a
`schedule_at_position` timer + `set_param`); it exists so 012/013 code
does not change when 008's ramp scheduling gains an absolute-time form.

## 4. Events

Handlers receive one table argument. Delivery is in order on the plugin's
own scheduler; a slow handler delays only that plugin's later events.

| Event | Requires | Payload |
|---|---|---|
| `ready_ack` | – | `{ api = {major, minor}, granted = { "playback.observe", … }, capabilities = { … , "timers" } }` |
| `unloading` | – | `{ reason = "disable" \| "suspend" \| "shutdown" }` — 4 ms handler budget; state writes issued here are committed within 200 ms |
| `track_changed` | `playback.observe` | `{ track = { id, title, artists, duration_ms } or nil }` — `api.state.track` already holds this track's entries |
| `position` | `playback.observe` | `{ position_ms }` at the subscribed rate, only while the position changes (once after a seek/loop wrap while paused) |
| `play_state_changed` | `playback.observe` | `{ state = "playing" \| "paused" \| "stopped" }` |
| `queue_changed` | `playback.observe` | `{ items = { … } }` |
| `marker_changed` | `markers.read` | `{ actor = "host" \| "<identifier>" \| "me", revision }` — coalesced per host tick; call `markers.list()` for the new state |
| `loop_armed` / `loop_disarmed` | `markers.read` | `{ region, by }` / `{ by }` |
| `loop_wrapped` | `markers.read` | `{ region, wraps }` |
| `effect_chain_changed` | `audio.effects` | `{ nodes = { NodeInfo… } }` — order, bypass/auto-bypass, orphan flags, any actor |
| `meter` | `audio.meter` | `{ pre = { peak_l, peak_r, rms_l, rms_r }, post = { … }, spectrum = { 64 numbers } }` at ~30/s |
| `timer` | – | `{ handle }` |
| `position_reached` | – | `{ handle, position_ms }` (actual position at the buffer boundary that crossed the target) |

Not delivered this slice (FR-028): `focus_granted`, `focus_revoked`,
`permission_changed`, `performance_mode_changed`, `connectivity_changed`,
`analysis_ready`, `action_invoked`, `settings_changed`, `midi_message`.
Absent methods this slice (calling them is a Lua error): `define_section`,
`serialize_state`, `restore_state`, every `ui.*`, `network.*`, `files.*`.

## 5. Budgets visible to the plugin

- 4 ms CPU per handler (abort, plugin continues; counts toward the
  3-in-60 s `warning`); 10 % of one core per rolling second and 64 MB heap
  (suspend); 10 MB storage (`budget_exceeded`); 100 calls/s per category
  (`rate_limited`); 256 pending timers; 1 024 queued inbound events.
- A handler that throws is aborted; the error text is logged to the
  plugin's console with the identifier and level `error`.

## 6. Ownership rules

A plugin owns what it created: markers, loop regions (both endpoint
markers), cue slots it set, effect nodes. It may read everything
(`markers.list`, `effects.list_chain` show `owner`), mutate only its own
(`permission_denied`/`not_owner` otherwise), and an id that no longer
exists is `not_found`. The host user may edit or delete anything the
plugin owns; the plugin then sees `marker_changed { actor = "host" }` or
`effect_chain_changed`. Non-transient markers survive the plugin;
transient ones vanish on track change and on the plugin's unload.
Orphaned nodes are re-adopted (owner = plugin again, ids unchanged) when
the same identifier calls `ready()` again.

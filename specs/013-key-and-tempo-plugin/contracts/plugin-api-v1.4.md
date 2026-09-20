# Plugin API 1.4 — delta over 1.3 (013-key-and-tempo-plugin)

Additive minor (Constitution IX). Everything in
[009 plugin-api-v1.md](../../009-plugin-runtime-and-permissions/contracts/plugin-api-v1.md),
[011 plugin-api-v1.2.md](../../011-plugin-ui-contributions/contracts/plugin-api-v1.2.md)
and [012 plugin-api-v1.3.md](../../012-section-loop-plugin/contracts/plugin-api-v1.3.md)
still holds; only the items below change or are added. Source of truth:
`crates/modplayer-capability-gateway/api/v1.toml` (`minor = 4`);
`docs/plugin-api/v1.md` is regenerated from it.

## 1. Version and capabilities

- `ready_ack.api = { major = 1, minor = 4 }`.
- A manifest declaring `api = "1.0"` … `"1.3"` still loads; `"1.4"` is
  accepted; `"1.5"` is refused exactly as `"1.4"` was under 1.3.
- No permission is added. `audio.effects` gates every item below.
- No request or event is added or renamed.

## 2. Result convention (unchanged)

`value, nil` on success; `nil, refusal` on refusal; an admitted call
never raises a Lua error — including the new argument forms below (a
wrong Lua type is an `invalid_state`/`invalid_argument` refusal).

## 3. Requests

### 3.1 `api.effects.set_param(node_id, param, value)` (~) — `audio.effects`

| Argument | 1.3 | 1.4 |
|---|---|---|
| `param` | integer parameter id | integer id **or** the parameter's wire name (§5) |
| `value` | number | number, **or** boolean for a `boolean`-shaped parameter, **or** an enum name string for an `enum`-shaped parameter |

Refusals (additive): `invalid_state`/`invalid_argument` — unknown wire
name for this node's kind, an enum name not in the parameter's value
list, a boolean/string value for a `number` parameter, or a non-integer
/non-string `param`. Existing refusals unchanged (`permission_denied`/
`not_owner`, `not_found`). A numeric `param` and `value` behave exactly
as in 1.3, including the id-out-of-range refusal.

Setting a `quality_mode` parameter (by id or by name) is an explicit
mode choice: it runs 008 FR-008 rule 3 and clears that node's
`auto_switched` flag (research R3) — previously a numeric write to the
mode id left the flag untouched, which 1.4 treats as a host defect, not
a behaviour to preserve.

### 3.2 `api.effects.schedule_param(node_id, param, value, at_ms)` (~)

Same argument widening as §3.1; still applied through the next-buffer
ramp (009 §3 note).

### 3.3 `api.effects.list_chain()` (~) — `audio.effects`

Each `NodeInfo` gains:

```lua
params = {                     -- every parameter of this node's kind, by wire name
  semitones = -2.0,            -- number: current clamped *target* (never mid-ramp)
  formant = false,             -- boolean
  quality_mode = "performance" -- enum name
},
auto_switched = false          -- 008 FR-008's flag (pitch_shift/time_stretch); false for other kinds
```

Read locally from the plugin snapshot as today (no RPC); the snapshot
is republished on the same host tick as the change.

## 4. Events

### 4.1 `effect_chain_changed { nodes }` (~) — `audio.effects`

Fires, coalesced to at most one per host tick and carrying the whole
chain (each node as in §3.3), whenever **any** of the following changes
by **any** actor: a node is added/removed/moved/bypassed/auto-bypassed/
orphaned/re-adopted (1.3 behaviour), **or** a node's parameter target or
quality mode changes — a plugin's `set_param`/`schedule_param` taking
effect, an Effect Chain panel edit, the host's `tempo_step` action, an
008 FR-008 auto-switch or revert, or an 008 FR-014 rate-change
recomputation. A write that leaves the clamped value unchanged fires
nothing.

Delivery fix (research R4): the payload is now built field-by-field
like `list_chain()`; under 1.3 a chain containing a plugin-owned node
made the event abort the receiving handler.

## 5. Parameter wire names (documentary `[[node_kind]]` in `v1.toml`)

| kind | parameters (id order) |
|---|---|
| `pitch_shift` | `semitones` (number −12..12), `formant` (boolean), `quality_mode` (enum `performance`\|`quality`) |
| `time_stretch` | `ratio` (number 0.25..2.0), `quality_mode` (enum) |
| `gain` | `level` (number −60..12), `mute` (boolean) |
| `filter` | `mode` (enum `high_pass`\|`low_pass`), `cutoff` (number 20..20000, Nyquist-clamped), `resonance` (number 0..1) |
| `stereo_tools` | `width` (0..2), `balance` (−1..1), `mono_sum`, `phase_invert`, `channel_swap` (booleans) |
| `equalizer` | `band<n>_freq`, `band<n>_gain` (−24..24), `band<n>_q` (0.1..10), `band<n>_type` (enum `peak`\|`low_shelf`\|`high_shelf`) for `n` = 1..8 |

The catalog (`crates/modplayer-effects/src/catalog.rs`) is the runtime
authority; `core/tests/effects_model.rs::wire_names_match_api_schema`
asserts the two agree.

## 6. Manifest

`api = "1.4"` is the only manifest-visible change.

## 7. Change request (Constitution IX — copied into the PR body)

**What**: plugin API 1.3 → 1.4, additive. `NodeInfo` gains `params`
(wire name → current clamped target value: number / boolean / enum
name) and `auto_switched`; `set_param`/`schedule_param` additionally
accept the wire name for `param` and boolean/enum-name values;
`effect_chain_changed` additionally fires (coalesced per tick) on
parameter-target and mode changes by any actor; a documentary
`[[node_kind]]` table enters the schema and the generated reference.
**Why**: a bundled plugin that owns effect nodes must keep its panel and
its per-track memory equal to what its nodes hold after the host's own
`+`/`-` action (008 FR-017 targets "any owner") or an Effect Chain panel
edit (J-2 steps 6→8; 013 FR-006/FR-007/SC-009); 1.3 exposes no
parameter value to plugins and fires the event only on structural
changes. **Compatibility**: no permission, request, event, refusal code
or wire field is removed or re-typed; every 1.0–1.3 fixture and Section
Loop load and pass unchanged; numeric `set_param` arguments keep their
exact 1.3 semantics. **Also fixed**: `effect_chain_changed` was
undeliverable for a chain containing a plugin-owned node (serde
newtype-variant error in the scheduler) — regression test added.
**Docs**: `docs/plugin-api/v1.md` regenerated by
`MODPLAYER_UPDATE_API_REFERENCE=1 cargo test -p modplayer-capability-gateway --test api_reference`.

## 8. Named tests

| Test | Crate / file | Proves |
|---|---|---|
| `api_version_is_1_4` | gateway `tests/api_reference.rs` | schema minor = 4; reference regenerated and matches |
| `reference_lists_node_kind_params` | gateway `tests/api_reference.rs` | "Node parameters" section rendered from `[[node_kind]]` |
| `manifest_api_1_4_accepted_1_5_refused` | gateway `tests/manifest.rs` | version gate moved by exactly one minor |
| `set_param_accepts_name_bool_and_enum` | runtime `tests/bindings.rs` | `("semitones", -2)`, `("formant", true)`, `("quality_mode", "quality")` produce `ParamRef::Name`/`ParamArg::{Number,Bool,Name}` |
| `set_param_numeric_form_unchanged` | runtime `tests/bindings.rs` | `(0, -2.0)` → `ParamRef::Id(0)`, `ParamArg::Number` |
| `set_param_bad_lua_type_is_refusal_not_error` | runtime `tests/bindings.rs` | a table argument → `nil, {code="invalid_state", reason="invalid_argument"}` |
| `list_chain_carries_params_and_auto_switched` | runtime `tests/bindings.rs` | snapshot projection reaches Lua as typed values |
| `effect_chain_changed_with_plugin_owned_node_reaches_handler` | runtime `tests/bindings.rs` | R4 regression |
| `wire_names_match_api_schema` | core `tests/effects_model.rs` | catalog ⇄ `v1.toml` `[[node_kind]]` |
| `wire_name_round_trip_every_kind` | core `tests/effects_model.rs` | `param_by_wire_name(param_wire_name(id)) == id` for every kind/param |
| `set_param_bumps_revision_only_on_change` | core `tests/effects_model.rs` | changed value → +1; same value → +0; mode auto-switch → +1 |
| `set_param_on_mode_id_delegates_to_set_mode` | core `tests/effects_model.rs` | `auto_switched` cleared, `mode_state` and `params` agree |
| `apply_set_param_by_name_and_enum` / `apply_set_param_unknown_name_refused` | core `tests/controller_plugins_permissions.rs` | `apply.rs` resolution and refusals |
| `tempo_step_fans_out_effect_chain_changed_with_ratio` | core `tests/controller_effects.rs` | host `+`/`-` → one event carrying `params.ratio` |
| `panel_edit_fans_out_effect_chain_changed` | core `tests/controller_effects.rs` | `chain_set_param`/`chain_set_mode` from the UI path → event with new value / `auto_switched=false` |
| `param_changes_coalesce_to_one_event_per_tick` | core `tests/controller_effects.rs` | three writes, one tick, one event |
| `effect_chain_changed_params_are_targets_not_ramp` | core `tests/controller_effects.rs` | value in the event equals the clamped request immediately, before the 20 ms ramp completes |
| `legacy_1_3_fixtures_still_load` | core `tests/plugins_manifest_discovery.rs` | every fixture and Section Loop `Active` under 1.4 |

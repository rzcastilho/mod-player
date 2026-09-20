# Data Model: Key & Tempo Bundled Plugin and Getting Started Panel

**Feature**: 013-key-and-tempo-plugin | **Date**: 2026-09-20 |
**Sources**: [spec.md](spec.md) Key Entities, [research.md](research.md)

Three layers: the API 1.4 delta in the gateway/runtime (§1), the host
model delta in `modplayer-core`/`modplayer-effects`/`modplayer-ui` (§2),
and the plugin package itself (§3). §4 lists what tests observe.

## 1. Gateway / runtime (API 1.4)

### 1.1 Schema (`crates/modplayer-capability-gateway/api/v1.toml`)

```toml
[api_version]
major = 1
minor = 4            # was 3 — additive (R1)

# New, documentary (rendered into docs/plugin-api/v1.md; not consumed by
# build.rs). Order = ParamId order = catalog order (R2).
[[node_kind]]
name = "pitch_shift"
params = [
  { name = "semitones",    shape = "number",  range = "-12..12" },
  { name = "formant",      shape = "boolean" },
  { name = "quality_mode", shape = "enum",    values = ["performance", "quality"] },
]
[[node_kind]]
name = "time_stretch"
params = [
  { name = "ratio",        shape = "number",  range = "0.25..2.0" },
  { name = "quality_mode", shape = "enum",    values = ["performance", "quality"] },
]
[[node_kind]]
name = "gain"
params = [ { name = "level", shape = "number", range = "-60..12" }, { name = "mute", shape = "boolean" } ]
[[node_kind]]
name = "filter"
params = [
  { name = "mode",      shape = "enum",   values = ["high_pass", "low_pass"] },
  { name = "cutoff",    shape = "number", range = "20..20000 (Nyquist-clamped)" },
  { name = "resonance", shape = "number", range = "0..1" },
]
[[node_kind]]
name = "stereo_tools"
params = [
  { name = "width",        shape = "number", range = "0..2" },
  { name = "balance",      shape = "number", range = "-1..1" },
  { name = "mono_sum",     shape = "boolean" },
  { name = "phase_invert", shape = "boolean" },
  { name = "channel_swap", shape = "boolean" },
]
[[node_kind]]
name = "equalizer"
params = [  # band1_… band8_
  { name = "band<n>_freq", shape = "number", range = "20..20000 (Nyquist-clamped)" },
  { name = "band<n>_gain", shape = "number", range = "-24..24" },
  { name = "band<n>_q",    shape = "number", range = "0.1..10" },
  { name = "band<n>_type", shape = "enum",   values = ["peak", "low_shelf", "high_shelf"] },
]
```

Existing `[[request]]`/`[[event]]` rows are unchanged (`SetParam`,
`ScheduleParam`, `EffectChainChanged` keep their names, permission and
category). Ranges above are documentary; `catalog::clamp` remains the
authority (a core test asserts each range string against the catalog's
`min`/`max`).

### 1.2 Read-model additions (`gateway/src/request.rs`)

```rust
/// A parameter's current target value as a plugin sees it (API 1.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ParamValue { Number(f64), Bool(bool), Name(String) }

pub struct NodeInfo {
    pub id: NodeId, pub kind: String, pub owner: OwnerInfo,
    pub bypassed: bool, pub auto_bypassed: bool, pub orphaned: bool, pub index: usize,
    /// API 1.4: wire name -> current clamped *target* value (never mid-ramp).
    pub params: BTreeMap<String, ParamValue>,
    /// API 1.4: 008 FR-008's flag for pitch_shift/time_stretch; false otherwise.
    pub auto_switched: bool,
}
```

`NodeInfo` derives `PartialEq`, so `Eq` is dropped (f64) — no caller
relies on `Eq` (grep: only `PartialEq` comparisons in tests).

### 1.3 `Request` variant change (internal DTO, not wire-visible)

```rust
pub enum ParamRef  { Id(u8), Name(String) }          // set_param's `param`
pub enum ParamArg  { Number(f32), Bool(bool), Name(String) }  // set_param's `value`

Request::SetParam      { node: NodeId, param: ParamRef, value: ParamArg }
Request::ScheduleParam { node: NodeId, param: ParamRef, value: ParamArg, at_ms: u64 }
```

Every existing construction site (`runtime/src/bindings/effects.rs`,
`runtime/tests/bindings.rs`, `core/tests/*.rs` `call()` helpers) is
updated mechanically; a numeric `param`/`value` from Lua maps to
`Id`/`Number` and behaves exactly as 1.3.

### 1.4 Lua surface (`runtime/src/bindings/effects.rs`, `bindings/mod.rs`, `scheduler.rs`)

| Surface | 1.3 | 1.4 |
|---|---|---|
| `api.effects.set_param(node, param, value)` | `param: integer`, `value: number` | `param: integer \| string`, `value: number \| boolean \| string`; any other Lua type ⇒ `invalid_state`/`invalid_argument` (never a Lua error) |
| `api.effects.schedule_param(node, param, value, at_ms)` | as above | as above |
| `api.effects.list_chain().nodes[i]` | 7 fields | + `params` (table keyed by wire name) + `auto_switched` |
| `effect_chain_changed { nodes }` | structural changes only; **broken for plugin-owned nodes (R4)** | built with `node_info_to_lua` (R4); also fires on parameter/mode changes (R3), ≤ 1 per host tick |

`node_info_to_lua` sets `params` as a Lua table: `Number` → number,
`Bool` → boolean, `Name` → string.

## 2. Host model delta

### 2.1 `modplayer-effects::catalog` (control-side data only, R2)

```rust
pub fn param_wire_name(kind: NodeKind, id: ParamId) -> Option<&'static str>;
pub fn param_by_wire_name(kind: NodeKind, name: &str) -> Option<ParamId>;
/// `Some` only for `Unit::Combo` discrete params; index order = value order.
pub fn enum_names(kind: NodeKind, id: ParamId) -> Option<&'static [&'static str]>;
/// Shape rule for `NodeInfo.params` (research R2).
pub enum WireShape { Number, Bool, Enum }
pub fn wire_shape(kind: NodeKind, id: ParamId) -> Option<WireShape>;
```

Invariants (tests): wire names are unique per kind; every catalog param
has one; `param_by_wire_name(kind, param_wire_name(kind, id)) == Some(id)`;
`enum_names(..).len() == count` for every Combo param; the set of names
equals `v1.toml`'s `[[node_kind]]` table.

### 2.2 `ChainModel` (`core/src/effects/model.rs`, R3)

| Method | Change |
|---|---|
| `set_param(id, param, requested)` | if `param == mode_param_id(kind)` ⇒ `return self.set_mode(id, QualityMode::from_index(requested))`; after clamp, `revision += 1` iff the stored value changed or `mode_state` changed |
| `set_mode(id, mode)` | `revision += 1` iff `mode_state` changed (mode **or** `auto_switched`) |
| `set_source_rate(rate)` | `revision += 1` iff ≥ 1 command emitted |
| new `param_value(&NodeModel, pos) -> ParamValue` (or in controller) | projection by `wire_shape` |

`revision`'s doc comment becomes "bumped on every structural,
ownership, parameter-target or mode change". No RT command changes.

### 2.3 `PlaybackController` (`core/src/controller.rs`)

- One `fn node_info(index, &NodeModel, ids) -> GatewayNodeInfo` replaces
  the two inline projections; fills `params`/`auto_switched`.
- `getting_started_dismissed: bool` field (loaded from settings);
  `pub fn getting_started_dismissed(&self) -> bool`;
  `pub fn dismiss_getting_started(&mut self)` → `persist_settings`.

### 2.4 `plugins::apply` (`core/src/plugins/apply.rs`)

`SetParam`/`ScheduleParam` arms: resolve `ParamRef` → `ParamId`
(`Name` via `param_by_wire_name(kind_of(node), name)`), `ParamArg` →
`f32` (`Bool` → 0/1, valid only for `WireShape::Bool`; `Name` via
`enum_names` index, valid only for `WireShape::Enum`; `Number` for any
shape as today). Failures: unknown node ⇒ `not_found` (unchanged);
unknown name / wrong-shape value ⇒ `invalid_state`/`invalid_argument`
with a message naming the parameter.

### 2.5 Settings (`core/src/settings/model.rs`, `store.rs`, R10)

```rust
pub struct AudioSettings { …, /// `[onboarding] getting_started_dismissed` (013 FR-018)
                               pub getting_started_dismissed: bool, … }
```

Raw TOML section `[onboarding]` with `getting_started_dismissed: bool`
(`#[serde(default)]`); absent ⇒ `false`. Included in `PartialEq` and the
existing round-trip tests (`core/tests/settings.rs`, `persist.rs`).
Device-scoped: untouched by sign-out/revocation (only account records
are removed, as for `disclosure`).

### 2.6 Links (`core/src/links.rs`)

`pub const GETTING_STARTED_TUTORIAL_URL: &str =
"https://github.com/rzcastilho/mod-player/blob/main/docs/plugin-tutorial.md";`

### 2.7 Bundled registration (`core/src/plugins/bundled.rs`)

`packages() = vec![section_loop(), key_tempo()]`; `key_tempo()` embeds
`plugins/bundled/org.modplayer.key-tempo/{plugin.toml, main.luau,
README.md}`, `fixture: false`, `resources: &[]`.

### 2.8 Getting Started card (`ui/src/getting_started.rs`, R10)

```rust
pub enum GettingStartedOutcome { None, OpenTutorial, Dismiss }
pub fn show(ui: &mut Ui) -> GettingStartedOutcome
```

Content (Fluent keys in `locales/en-US/app.ftl`): `getting-started-title`,
`getting-started-section-loop` ("Section Loop — Drop A and B around a
passage and drill it hands-free — I A, O B, L loop, [ / ] nudge"),
`getting-started-key-tempo` ("Key & Tempo — Transpose or slow a song
without changing the rest — + / - tempo step, panel controls for key"),
`getting-started-tutorial` ("Open plugin tutorial"),
`getting-started-dismiss` ("Dismiss"). Drawn by `App::show_library`
above `library_view::show` while `!controller.getting_started_dismissed()`
and `library_detail.is_none()`.

## 3. The plugin package (`plugins/bundled/org.modplayer.key-tempo/`)

### 3.1 Manifest (`plugin.toml`)

```toml
identifier     = "org.modplayer.key-tempo"
name           = "Key & Tempo"
description    = "Transpose a song or change its tempo without changing the rest."
version        = "1.0.0"
api            = "1.4"
author         = "ModPlayer project"
license        = "MIT OR Apache-2.0"
source         = "bundled"
entry          = "main.luau"
default_locale = "en-US"

[[permissions.required]]  permission = "audio.effects"    justification = "Creates and drives its one pitch-shift and one time-stretch node; observes every change to them."
[[permissions.required]]  permission = "ui.panel"         justification = "Registers the Key & Tempo panel."
[[permissions.required]]  permission = "ui.shortcuts"     justification = "Registers its 8 trigger actions (tempo up/down default + / -)."
[[permissions.required]]  permission = "state.track"      justification = "Stores and restores a track's key/tempo when 'Remember for this track' is on."
[[permissions.required]]  permission = "playback.observe" justification = "Reacts to track changes and reads the current track to restore or reset its settings."
```

(Each `[[permissions.required]]` is a multi-line table in the real
file; shown inline for brevity.)

### 3.2 Session-only script state (`main.luau`, never persisted)

| Field | Type | Meaning |
|---|---|---|
| `S.pitch`, `S.stretch` | node id or nil | own nodes, re-found on every `effect_chain_changed` |
| `S.key` (int −12..12), `S.cents` (int −50..50), `S.tempo` (int 25..200), `S.formant` (bool), `S.quality` (bool = OR of both nodes) | mirrored view | rewritten from `params` (R5) |
| `S.sent` | `{ semitones, key, cents }` or nil | last composed value sent; FR-006 tie-break |
| `S.step` | int 1..50, default 10 | configured tempo step |
| `S.remember` | bool | ≡ current track has a `settings` entry |
| `S.keep_across` | bool, default false | session-wide mode (FR-011) |
| `S.restored` | bool | badge condition for the current track |
| `S.last_written` | table or nil | last `settings` table written (skip identical writes) |
| `S.node_removed` | bool | a user removed one of the nodes while Active (status text) |

### 3.3 Per-track entry (`api.state.track`, key `settings`)

```lua
{ key = -2, cents = 0, tempo = 100, formant = false, quality = "performance" }
```

Validation on read (field-by-field, never refused): `key` integer in
−12..12 else 0; `cents` integer in −50..50 else 0; `tempo` integer in
25..200 else 100; `formant` boolean else false; `quality` one of
`"performance"`/`"quality"` else `"performance"`. Existence of the key
**is** "remember for this track". Removed by `toggle_remember` → off.
Account-scoped and cleared on sign-out by 009's own store rule.

### 3.4 Panel `main` (title `@panel_title`) — widgets in order

| id | kind | label | range / initial | interaction |
|---|---|---|---|---|
| `key` | slider | `@key` | −12..12 step 1, 0 | `S.key = v`; send composed |
| `cents` | slider | `@cents` | −50..50 step 1, 0 | `S.cents = v`; send composed |
| `tempo` | slider | `@tempo` | 25..200 step 1, 100 | `set_param(stretch, "ratio", v/100)` |
| `step` | slider | `@step` | 1..50 step 1, 10 | `S.step = v`; `step_note` text |
| `formant` | toggle | `@formant` | false | `set_param(pitch, "formant", v)` |
| `quality` | toggle | `@quality` | false | `set_param(both, "quality_mode", v and "quality" or "performance")` |
| `remember` | toggle | `@remember` | false | `toggle_remember(v)` |
| `keep_across` | toggle | `@keep_across` | false | `S.keep_across = v` |
| `restored` | text | `@restored` | `""` | badge / status line |
| `step_note` | text | `@step_note` | `""` | `@step_note_text` while `S.step ~= 10` |
| `key_up`, `key_down` | button (plain, R6) | `@key_up` / `@key_down` | — | `key_step(±1)` |
| `tempo_up`, `tempo_down` | button (plain, R6) | `@tempo_up` / `@tempo_down` | — | `tempo_step(±S.step)` |
| `reset_key`, `reset_tempo` | button (plain, R6) | `@reset_key` / `@reset_tempo` | — | `reset_key()` / `reset_tempo()` |

Composed semitone value: `S.key + S.cents / 100`, sent as
`set_param(S.pitch, "semitones", v)`; the host clamps to [−12, 12].

### 3.5 Actions (`ui.shortcuts`, 8 trigger actions)

| id | label | default binding |
|---|---|---|
| `key_up` / `key_down` | `@action_key_up` / `@action_key_down` | — |
| `tempo_up` / `tempo_down` | `@action_tempo_up` / `@action_tempo_down` | `Plus` / `Minus` (conflict with host on a fresh install, R7) |
| `reset_key` / `reset_tempo` | `@action_reset_key` / `@action_reset_tempo` | — |
| `toggle_remember` / `toggle_keep_across_tracks` | `@action_toggle_remember` / `@action_toggle_keep_across` | — |

Fully-namespaced ids: `org.modplayer.key-tempo.<id>`.

### 3.6 String table keys (`[strings.en-US]`)

`panel_title`, `key`, `cents`, `tempo`, `step`, `formant`, `quality`,
`remember`, `keep_across`, `restored`, `restored_text` ("Restored from
this track's memory"), `step_note`, `step_note_text` ("+ / - still steps
by the default 10% until rebound in Settings › Controls"),
`node_removed_text` ("Effect node removed — disable and re-enable Key &
Tempo to recreate it"), `key_up`, `key_down`, `tempo_up`, `tempo_down`,
`reset_key`, `reset_tempo`, and the eight `action_*` labels — 28 keys.

### 3.7 State transitions (per track)

```text
                 toggle_remember(on)  ──►  entry written {current values}
 [no entry] ◄──  toggle_remember(off) ───  [entry]  ──(any mirrored change)──► entry rewritten
     │                                        │
 track_changed:                           track_changed:
   keep_across off → apply defaults          apply entry, remember=on, restored=true (badge)
   keep_across on  → leave values
   remember=off, restored=false
```

## 4. Host-side test observables (no plugin probes)

- `controller.chain().nodes()` — kinds, owners, adjacency, `params`,
  `mode_state`, `orphaned`.
- Plugin panel registry (`controller.plugin_panels()` / 011's registry
  API used by `controller_plugin_ui.rs`) — widget values and the two
  `text` contents.
- `PluginStateStore` for `org.modplayer.key-tempo` — presence and
  content of `settings` for a track id.
- `ActionRegistry` — the 8 actions, `Plus`/`Minus` conflict flags.
- `SettingsStore::load().settings.getting_started_dismissed`.
- Runtime handle event counts — one `effect_chain_changed` per tick with
  ≥ 1 change (coalescing).

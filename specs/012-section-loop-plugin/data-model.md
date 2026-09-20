# Data Model: Section Loop Bundled Plugin

**Feature**: 012-section-loop-plugin | **Date**: 2026-09-20 |
**Inputs**: [spec.md](spec.md) § Key Entities, [research.md](research.md)

Three layers: (1) the API 1.3 additions in the gateway/runtime crates,
(2) the host-model delta in `modplayer-core`, (3) the plugin package
itself — its manifest, its session-only script state and its panel/
overlay/action declarations. Nothing new is persisted anywhere: every
durable fact is 006's `TrackMarkers` file, unchanged in shape.

## 1. Gateway / runtime (API 1.3)

### 1.1 Schema (`api/v1.toml`)

| Entry | Value |
|---|---|
| `[api_version]` | `major = 1`, `minor = 3` |
| `[[request]]` | `name = "SetLoopEndpoint"`, `namespace = "markers"`, `method = "set_loop_endpoint"`, `requires = "markers.write"`, `needs_focus = false`, `category = "markers"` |
| `[[request]]` | `name = "SetLoopRepeat"`, `namespace = "markers"`, `method = "set_loop_repeat"`, `requires = "markers.write"`, `needs_focus = false`, `category = "markers"` |

No permission, event or refusal-code change. `ready_ack.api` reports
`"1.3"`; a manifest `api = "1.3"` loads; every `1.0`–`1.2` manifest still
loads (`ApiRange.min_minor ≤ 3`).

### 1.2 `Request` variants (`gateway/src/request.rs`)

```rust
SetLoopEndpoint { region: Option<RegionId>, which: LoopEndpoint, position_ms: u64 }
SetLoopRepeat   { region: RegionId, repeat: RepeatArg }
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopEndpoint { A, B }                    // wire: "a" | "b"

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum RepeatArg { Times(u16), Infinite }       // wire: 1..=1000 | "infinite"
```

Validation (in the Lua binding, before the RPC — the `SetCue` slot
precedent): `which` ∉ {"a","b"} → `invalid_state`/`invalid_argument`;
`repeat` not an integer in `1..=1000` and not `"infinite"` →
`invalid_state`/`invalid_argument`; `position_ms` must be a non-negative
integer (mlua `u64` coercion; a negative/non-integer is a Lua-side
argument error exactly as for `create_loop`).

### 1.3 `Response` and read-model additions

```rust
Response::Markers { markers: Vec<MarkerInfo>, armed: Option<RegionId>, regions: Vec<RegionInfo> }
Response::LoopEndpoint { region: RegionId, marker: MarkerId }   // set_loop_endpoint's `ok, region_id, marker_id`
```

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegionInfo {
    pub id: RegionId,
    pub owner: OwnerInfo,          // owner of `a`, else of `b` (research R2)
    pub a: Option<MarkerId>,
    pub b: Option<MarkerId>,
    pub repeat: RepeatArg,         // serialises as number | "infinite"
    pub armed: bool,
}
```

`PluginSnapshot` (`runtime/src/handle.rs`) gains `regions:
Vec<RegionInfo>`; `bindings/mod.rs` `RequestKind::ListMarkers` copies it
into the response; `response_to_lua` emits `regions` as a 1-based array
of tables `{ id, owner = {kind, ...}, a, b, repeat, armed }` (`a`/`b`
absent when `nil`, mirroring `MarkerInfo.region`).

### 1.4 Lua surface (`runtime/src/bindings/markers.rs`)

| Call | Args | Returns |
|---|---|---|
| `api.markers.set_loop_endpoint(region_id \| nil, which, position_ms)` | `(number?, "a"\|"b", integer)` | `{ region = id, marker = id }, nil` or `nil, err` |
| `api.markers.set_loop_repeat(region_id, repeat)` | `(number, integer 1..=1000 \| "infinite")` | `true, nil` or `nil, err` |
| `api.markers.list()` | — | `{ markers = {...}, armed = id\|nil, regions = {...} }` |

## 2. Host model delta (`modplayer-core`)

### 2.1 `TrackMarkers` (`markers/model.rs`)

```rust
pub fn set_loop_endpoint_owned(
    &mut self,
    region: Option<RegionId>,   // None ⇒ new_loop_region()
    which_a: bool,
    pos: u64,                   // frames, clamped to len_frames
    owner: Owner,
) -> Result<(RegionId, MarkerId), MarkerError>
```

Rules (all inherited from the private `set_loop_endpoint` body it
replaces): `NotFound` for an unknown `region`; creates the endpoint if
absent (`LimitReached` at 64 markers) else moves it; new markers get
`owner`, `transient = false`, `default_name_for(kind)`, colour 0; sets
`current_region = Some(region)`; runs `maybe_swap_region_endpoints`;
`resort`; `dirty`; `revision += 1`. `set_loop_a`/`set_loop_b` become
`self.set_loop_endpoint_owned(self.current_region, true/false, pos,
Owner::Host)`.

Invariants (proptest targets, Constitution VIII): after any sequence of
`set_loop_endpoint_owned` on one region, `a.pos ≤ b.pos` whenever both
exist; both endpoints of a region share an owner; marker count never
exceeds 64; positions ≤ `len_frames`; the region id is stable across
swaps.

### 2.2 `PlaybackController` façade (`controller.rs`)

```rust
pub(crate) fn plugin_set_loop_endpoint(&mut self, region: Option<RegionId>, which_a: bool, position_ms: u64, owner: Owner) -> Result<(RegionId, MarkerId), MarkerError>
// ms → frames via ms_to_frames; recommit_if_armed(region) after a move (existing helper); last_marker_actor = owner
```

`set_loop_repeat(region, RepeatCount)` already exists and is reused
unchanged (re-commits an armed region without resetting wraps).

### 2.3 `plugins::apply` arms

| Request | Checks (in order) | Effect | Response |
|---|---|---|---|
| `SetLoopEndpoint{region: Some(r)}` | `require_owner(region_owner(r))` → `not_found` / `not_owner` | `plugin_set_loop_endpoint(Some(r), ..)` | `LoopEndpoint{region, marker}` |
| `SetLoopEndpoint{region: None}` | `require_track` (`no_track`) | creates region + endpoint | `LoopEndpoint{..}` |
| `SetLoopRepeat{region, repeat}` | `require_owner(region_owner(region))` | `set_loop_repeat(region, RepeatCount::{Infinite, Times(n)})` | `Ok` |

`marker_refusal` maps `LimitReached → invalid_state/marker_limit`,
`NoTrack → invalid_state/no_track`, `NotFound → not_found`, `NotOwner →
permission_denied/not_owner` (all existing).

### 2.4 `resolve_string` extension (research R9)

`UpdateWidget { value: WidgetValue::Text(s) }` and every `AddOverlays`
primitive with a `text` (labels) pass through `resolve_string(manifest,
&s)` before validation. Unchanged rule: only a string starting with `@`
is looked up; unknown key → rendered literally + console warning.

### 2.5 Snapshot publication

`publish_plugin_snapshot_if_changed` additionally maps
`TrackMarkers::regions()` → `Vec<RegionInfo>` inside the same
`markers_changed` branch (owner via `owner_info(owner_of(a or b))`,
`repeat` via `RepeatCount → RepeatArg`).

### 2.6 Bundled registration

`plugins::bundled::packages()` returns `[section_loop()]` — identifier
`org.modplayer.section-loop`, `fixture: false`, `resources: &[]`,
`include_str!` of `plugin.toml`/`main.luau`/`README.md`.

## 3. The plugin package

### 3.1 Manifest (`plugins/bundled/org.modplayer.section-loop/plugin.toml`)

| Field | Value |
|---|---|
| `identifier` | `org.modplayer.section-loop` |
| `name` | `Section Loop` |
| `description` | Drop A and B around a passage and drill it hands-free. |
| `version` | `1.0.0` |
| `api` | `1.3` |
| `author` | `ModPlayer project` |
| `license` | `MIT OR Apache-2.0` |
| `source` | `bundled` |
| `entry` | `main.luau` |
| `default_locale` | `en-US` |
| `[[permissions.required]]` ×7 | `playback.observe`, `transport.control`, `markers.read`, `markers.write`, `ui.panel`, `ui.overlay`, `ui.shortcuts` — each with a one-line `justification` (FR-003) |
| `[[permissions.optional]]` ×1 | `analysis.read` — "Reserved for beat snapping in a later update; not called." |
| `[strings.en-US]` | every key in §3.5 |

### 3.2 Session-only script state (`main.luau`, never persisted)

| Field | Type | Meaning / reset |
|---|---|---|
| `S.region` | region id or nil | the plugin-owned region on the current track; from `relist()` |
| `S.a`, `S.b` | marker id or nil | endpoint ids of `S.region`; from `relist()` |
| `S.a_ms`, `S.b_ms` | integer or nil | endpoint positions (overlay + nudge base) |
| `S.cues[1..8]` | `{ id, ms, owner_is_me }` or nil | slot occupancy of **any** owner (jump targets); `owner_is_me` gates `set_cue_n` UX |
| `S.active` | `"a"` \| `"b"` \| nil | FR-006 pointer; set by `set_a`/`set_b`/`nudge_*`; nil on `track_changed`; cleared when its endpoint disappears |
| `S.armed` | bool | mirror of `loop_armed`/`loop_disarmed` for `S.region` |
| `S.pending_repeat` | `RepeatArg` or nil | slider value committed while no region exists; applied on next region creation, then nil |

State transitions:

```
(no region) --set_a/set_b--> (incomplete: one endpoint) --set_b/set_a--> (complete)
(complete) --toggle_loop [focus granted, armable]--> (complete, armed)
(complete, armed) --toggle_loop | host L | repeat exhausted | endpoint deleted | disable/suspend--> (complete | incomplete, disarmed)
(any) --clear_markers--> (no region)          -- host removes the empty region
(any) --track_changed--> state rebuilt from markers.list()
```

### 3.3 Panel `main` (title `@panel_title`) — widgets in order

| id | kind | label key | notes |
|---|---|---|---|
| `set_a` | `button` | `@set_a` | plain button (research R11); `panel_interaction` → `set_a()` |
| `set_b` | `button` | `@set_b` | plain button → `set_b()` |
| `loop` | `toggle` | `@loop` | value driven only by `loop_armed`/`loop_disarmed`; interaction → `toggle_loop(value)` |
| `repeat` | `slider` | `@repeat` | `min 0 max 1000 step 1 value 0`; `0 ⇔ "infinite"`; interaction → `set_repeat(value)` |
| `markers` | `marker_list` | `@markers` | host-rendered, own markers only, row select = host seek |
| `snap` | `toggle` | `@snap` | interaction → `update_widget("main","snap",false)` |
| `snap_note` | `label` | `@snap_note` | `text = "@snap_note_text"` |
| `status` | `text` | `@status` | `text = ""`; last refusal `message` or a plugin hint; cleared on the next success |

### 3.4 Actions (`ui.shortcuts`, 23) and overlays

| short id | kind | default binding | handler |
|---|---|---|---|
| `set_a`, `set_b` | trigger | `I`, `O` (flagged vs host on a fresh install) | `set_a()`, `set_b()` |
| `toggle_loop` | trigger | `L` (flagged) | `toggle_loop(not S.armed)` |
| `nudge_earlier`, `nudge_later` | trigger | `[`, `]` | `nudge(-10)`, `nudge(+10)` |
| `set_cue_1..8` | trigger | unbound | `set_cue(n)` |
| `jump_cue_1..8` | trigger | unbound | `jump_cue(n)` |
| `toggle_snap` | trigger | unbound | no-op |
| `clear_markers` | trigger | unbound | `clear_markers()` |

Overlay primitive ids (≤ 21): `a_line`, `b_line`, `ab_region`,
`a_label`, `b_label`, `cue_<n>_dot`, `cue_<n>_label` (n = 1..8, only for
set slots). Rebuilt by `relist()` with `clear_overlays()` +
`add_overlays(list)`.

### 3.5 String table keys (`[strings.en-US]`)

`panel_title`, `set_a`, `set_b`, `loop`, `repeat`, `markers`, `snap`,
`snap_note`, `snap_note_text` ("Needs beat analysis — coming in a later
update"), `status`, `status_no_region` ("Set A and B in Section Loop
first"), `status_no_focus` ("Needs transport focus — give Section Loop
focus in the Transport panel (T)"), `status_cue_owned_1..8` ("Cue n
belongs to the host — move or delete it in the Markers panel"),
`label_a` ("A"), `label_b` ("B"), `cue_label_1..8` ("Cue n"), and one
`action_<short id>` label per action (23). Total ≈ 55 keys.

## 4. Host-side test observables (no plugin probes)

| Fact | Read from |
|---|---|
| Region/endpoints/owner/repeat/armed | `controller.markers().regions()` + `owner_of` |
| Focus holder | `controller.plugins().arbiter().holder()` |
| Loop toggle / slider / Status text | `controller.plugins().ui().panels()` widget state for `PanelKey(section-loop, "main")` |
| Overlay ids present | `controller.plugin_overlay_layers()` (011 read model) |
| Flagged chords | `ActionRegistry` conflicts for `PluginActionId(section-loop.set_a)` |

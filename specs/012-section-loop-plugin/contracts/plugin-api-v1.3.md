# Contract: Plugin API 1.3 — loop endpoints and repeat count

**Feature**: 012-section-loop-plugin | **Supersedes** (additively):
[009 plugin-api-v1.md](../../009-plugin-runtime-and-permissions/contracts/plugin-api-v1.md) §3,
[010 plugin-api-v1.1.md](../../010-transport-focus/contracts/plugin-api-v1.1.md),
[011 plugin-api-v1.2.md](../../011-plugin-ui-contributions/contracts/plugin-api-v1.2.md) |
**Schema**: `crates/modplayer-capability-gateway/api/v1.toml` (the
regenerated `docs/plugin-api/v1.md` is the authoritative rendering; this
file is the human-authored design it must match)

## 1. Version and capabilities

- `api_version` = **1.3** (additive minor; every `api = "1.0"`–`"1.2"`
  manifest still loads).
- No new permission. Both new requests are gated by the existing
  `markers.write`; the `regions` read rides on `markers.read`.
- `ready_ack.capabilities` unchanged.

## 2. Result convention (unchanged)

Every call returns `value, nil` or `nil, { code, reason, message }`.
Check order for the new calls: **permission → rate limit (`markers`
bucket) → argument validation (plugin thread) → ownership/track state
(host)**. Neither call needs transport focus (`no_focus` is never
produced — they are region metadata, like `move`).

## 3. Requests

### 3.1 `api.markers.set_loop_endpoint(region_id | nil, which, position_ms)` — `markers.write`

The plugin-side twin of the host's own `I`/`O` (006 FR-006).

```lua
local r, err = api.markers.set_loop_endpoint(nil, "a", 62000)     -- new caller-owned region holding only A
local r2, err = api.markers.set_loop_endpoint(r.region, "b", 70000) -- completes it (or moves B if present)
-- r = { region = <region_id>, marker = <marker_id> }
```

| Arg | Type | Rule |
|---|---|---|
| `region_id` | integer or `nil` | `nil` creates a new region owned by the caller with only that endpoint; otherwise must be a region the caller owns |
| `which` | `"a"` \| `"b"` | anything else → `invalid_state`/`invalid_argument` |
| `position_ms` | integer ≥ 0 | clamped by the host to the track end |

Semantics: on an own region, **creates** the named endpoint if absent
(subject to the 64-marker limit) or **moves** it if present. The host's
swap rule applies (if A would land after B their roles swap — the
region id is unchanged, the marker ids keep their positions and swap
kinds); the created marker is `transient = false`, named "A"/"B", colour
0, owner = caller. The region becomes the host's current region (so the
user's own `L` can arm it). An armed region whose endpoint moves is
re-committed at the next buffer boundary without resetting wraps.

| Refusal | reason | when |
|---|---|---|
| `permission_denied` | `not_owner` | `region_id` names a region another owner's endpoint belongs to |
| `not_found` | — | `region_id` names no region on the current track |
| `invalid_state` | `no_track` | no current track |
| `invalid_state` | `marker_limit` | creating an endpoint at 64 markers (moving is never refused) |
| `invalid_state` | `invalid_argument` | bad `which` |

### 3.2 `api.markers.set_loop_repeat(region_id, repeat)` — `markers.write`

```lua
local ok, err = api.markers.set_loop_repeat(r.region, 4)          -- release after 4 wraps
local ok, err = api.markers.set_loop_repeat(r.region, "infinite") -- default
```

| Arg | Type | Rule |
|---|---|---|
| `region_id` | integer | an own region |
| `repeat` | integer `1..=1000` \| `"infinite"` | exact — never clamped; `0`, `1001`, `2.5`, `"forever"` → `invalid_state`/`invalid_argument` |

Returns `true`. If the region is armed the new count applies at the
next buffer boundary without resetting the wrap counter (006 FR-011a);
the host releases the loop after exactly `n` wraps and emits
`loop_disarmed`. Refusals: `permission_denied`/`not_owner`,
`not_found`, `invalid_state`/`invalid_argument`, `invalid_state`/`no_track`.

### 3.3 `api.markers.list()` (~) — `markers.read`

The result gains a `regions` array (all owners, all regions on the
current track); `markers` and `armed` are unchanged.

```lua
local m = api.markers.list()
-- m.markers : as 1.0
-- m.armed   : region id or nil, as 1.0
-- m.regions : { { id = 3, owner = { kind = "plugin", plugin = "org.modplayer.section-loop" },
--                 a = 7, b = 8, repeat = "infinite", armed = false }, ... }
--   a / b   : marker id, absent while that endpoint does not exist (incomplete region)
--   repeat  : integer 1..=1000 or the string "infinite"
--   owner   : owner of a, else of b (both endpoints always share an owner)
```

Served locally from the plugin snapshot (no RPC), refreshed on every
marker revision change — arming, disarming, repeat edits and endpoint
edits all bump the revision.

## 4. Events

None added or changed. `marker_changed`, `loop_armed`, `loop_disarmed`,
`loop_wrapped` fire for the new calls' effects exactly as for the
host's own edits (actor `plugin`).

## 5. Manifest

No change. A plugin using either call declares `api = "1.3"`.

## 6. Change request (Constitution IX — copied into the PR body)

**Change**: API 1.2 → 1.3. Adds `markers.set_loop_endpoint` and
`markers.set_loop_repeat` (both `markers.write`, `markers` rate bucket,
no focus) and a `regions` array in `markers.list()`'s result.
**Why**: FR-13.1 obliges the bundled Section Loop plugin to drive two
host primitives — an incomplete (A-only/B-only) loop region and a
region's repeat count (DM-7) — that the 1.2 `markers` surface cannot
express; reimplementing them in script would violate Constitution III.
**Compatibility**: additive; no existing request, event, payload field
or refusal changes meaning; `create_loop` remains the one-shot
two-endpoint constructor. **Deprecations**: none. **Migration**: none.
**Reference**: regenerated `docs/plugin-api/v1.md`. **Real-time
safety**: N/A — both calls reach the engine only through the existing
buffer-boundary recommit path.

## 7. Named tests

- `modplayer-capability-gateway/tests/api_reference.rs::reference_is_current` (regenerated 1.3)
- `modplayer-capability-gateway/tests/gateway.rs::{set_loop_endpoint_requires_markers_write, set_loop_repeat_requires_markers_write, loop_endpoint_calls_never_need_focus}`
- `modplayer-plugin-runtime/tests/bindings.rs::{set_loop_endpoint_which_validated, set_loop_repeat_range_exact, list_markers_exposes_regions}`
- `modplayer-core/tests/markers_model.rs::{set_loop_endpoint_owned_creates_a_only_region, set_loop_endpoint_owned_completes_and_swaps, set_loop_endpoint_owned_moves_existing, host_set_loop_a_b_unchanged, endpoint_owned_proptest}`
- `modplayer-core/tests/controller_markers.rs::{plugin_endpoint_not_owner, plugin_endpoint_marker_limit, plugin_repeat_recommits_armed_without_wrap_reset, snapshot_regions_track_arm_and_repeat}`

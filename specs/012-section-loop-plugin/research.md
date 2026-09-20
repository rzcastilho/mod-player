# Research: Section Loop Bundled Plugin

**Feature**: 012-section-loop-plugin | **Date**: 2026-09-20 | **Input**: [spec.md](spec.md)

Every decision below was taken against the shipped 006/009/010/011
code in this worktree (paths cited), not against the specs alone. No
`NEEDS CLARIFICATION` remained in the Technical Context after this
pass; headless assumptions are marked **(A)** and echoed in
plan.md § Complexity Tracking.

## R1. Plugin API 1.3 is a schema-first, two-request bump

**Decision**: `crates/modplayer-capability-gateway/api/v1.toml` moves
`[api_version] minor = 3` and gains exactly two `[[request]]` entries —
`SetLoopEndpoint` (`markers.set_loop_endpoint`, `requires =
"markers.write"`, `needs_focus = false`, `category = "markers"`) and
`SetLoopRepeat` (`markers.set_loop_repeat`, same gating). No new
permission, event, refusal code or rate bucket. The third FR-017 change
(`regions` in `markers.list()`) is a **response-shape** addition, which
the schema does not model per request (it lists requests/events only),
so it is documented in the human-authored contract and in the regenerated
`docs/plugin-api/v1.md` prose exactly as 009 documented `markers.list()`'s
`markers`/`armed` fields.

**Rationale**: Constitution IX — one definition, generated enums;
`build.rs` emits `RequestKind::{SetLoopEndpoint, SetLoopRepeat}` and the
exhaustive `match`es in `bindings/mod.rs::dispatch`, `request.rs`,
`apply.rs::apply` fail to compile until every arm exists (the 011 R1
mechanism, verified in `apply.rs` header comment). `tests/api_reference.rs`
regenerates the reference; the PR body carries the change request
(`.github/PULL_REQUEST_TEMPLATE.md` already has the checkbox).

**Alternatives considered**: (a) script-side "pending A" + `create_loop`
once both exist — rejected by the spec's clarify pass (Constitution III;
A-only region must persist and render like the host's own `I`). (b)
`create_loop(a, nil)` overloading — rejected: changes an existing call's
argument contract (not purely additive) and cannot express "move the
existing endpoint". (c) Counting `loop_wrapped` in script and calling
`disarm_loop()` — rejected: cannot guarantee SC-005 on short regions and
needs focus for the disarm.

## R2. Host model: one owned endpoint helper, reused by the host's own I/O

**Decision**: `TrackMarkers::set_loop_endpoint` (private,
`markers/model.rs:629`) is generalised into
`set_loop_endpoint_owned(region: Option<RegionId>, is_a: bool, pos: u64,
owner: Owner) -> Result<(RegionId, MarkerId), MarkerError>`. The existing
`set_loop_a`/`set_loop_b` call it with `region = self.current_region`
(creating one when `None`, as today) and `Owner::Host`, so host behaviour
is byte-for-byte unchanged (their tests in `controller_markers.rs` and
`markers_model.rs` are the regression net). The plugin path passes an
explicit region (or `None` ⇒ `new_loop_region()` — the same allocation
`set_loop_a` uses) and `Owner::Plugin(id)`. Swap (`maybe_swap_region_
endpoints`), end-clamp (`pos.min(len_frames)`), the 64-marker limit and
`revision += 1` all come for free from the shared body. A plugin-created
endpoint is `transient = false` always (FR-013) and named by
`default_name_for(kind)` like the host's ("A"/"B"), so the host's Markers
panel lists it identically.

**Region ownership**: a region has no owner field; ownership derives from
its endpoints (`apply.rs::region_owner` already reads "either endpoint" —
`new_loop_region_owned` creates both with one owner). The new helper keeps
this invariant: when the region already has an endpoint, the caller must
own it (`require_owner(region_owner(..))` in `apply.rs`), and the new
endpoint is created with the same owner. A region that is created by the
plugin's `nil` call and then loses its only endpoint through the host UI
is removed by `delete`'s existing empty-region cleanup
(`model.rs:836-841`), so no owner-less region can exist.

**`current_region` side effect (A)**: the plugin path sets `current_region
= Some(region)` exactly as the host path does (the model's `I8` rule:
"a region endpoint sets current_region"). This means the host's own `L`
can then arm Section Loop's region — precisely the edge case the spec
lists ("the user arms Section Loop's region with the host's L") and 009's
clarify entry ("the user may arm a plugin's region"). Rejected: leaving
`current_region` untouched — would make a plugin region unreachable from
the host's keyboard, contradicting Constitution X's user-override rule.

## R3. `set_loop_repeat` reuses the controller's existing recommit path

**Decision**: `apply.rs` arm: `require_owner(region_owner(..))` →
`controller.set_loop_repeat(region, RepeatCount)` (already exists,
`controller.rs:2963`; re-commits an armed region **without** resetting
wraps — exactly 006 FR-011a "applies at the next buffer boundary"). The
wire type is a new gateway enum `RepeatArg { Infinite, Times(u16) }`; the
Lua binding accepts a number or the string `"infinite"`; a number outside
`1..=1000`, a non-integer, or any other string is refused
`invalid_state`/`invalid_argument` by the binding **before** the RPC (the
same place `SetCue` validates its slot range today, `apply.rs:355`). The
model's `times_clamped` never gets to clamp a plugin value (validation
is exact, not clamping — matches 011's "never clamped" slider rule).

**Alternatives**: `0` meaning infinite on the wire — rejected: the
DM-7 domain is `1..=1000 | infinite`; `0` is a panel-slider convention
(011 has no "infinite" slider stop) and stays in the plugin script.

## R4. `regions` rides in the existing snapshot, not a new RPC

**Decision**: `PluginSnapshot` (`plugin-runtime/src/handle.rs:76`) gains
`regions: Vec<RegionInfo>`; `Response::Markers` gains `regions`;
`controller.rs::publish_plugin_snapshot_if_changed` builds it from
`TrackMarkers::regions()` (already public) in the same revision-gated
block (`arm`/`disarm`/`set_repeat`/endpoint edits all bump `revision`,
verified in `model.rs:889-955`, so `armed`/`repeat` are never stale).
`RegionInfo { id, owner: OwnerInfo, a: Option<MarkerId>, b:
Option<MarkerId>, repeat: RepeatInfo, armed: bool }` derives `Serialize`;
`RepeatInfo` serialises as an integer or the string `"infinite"` (serde
`untagged`), which `mlua`'s `to_value` turns into a Lua number/string
directly (the `MarkerInfo` precedent in `request.rs` header). Owner of a
region = owner of `a`, else `b` (R2).

**Rationale**: `markers.list()` is served locally from the snapshot with
no RPC (009 R3; `bindings/mod.rs:257`); a second call or an RPC for
regions would double the read path for nothing. Purely additive on the
Lua side (`result.regions` beside `result.markers`/`result.armed`).

## R5. Package layout, embedding and licence

**Decision**: `plugins/bundled/org.modplayer.section-loop/` (the
`plugins/bundled/README.md` convention is `<identifier>/`) containing
`plugin.toml`, `main.luau`, `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`
(verbatim copies of the workspace root files). `bundled.rs::packages()`
returns one `BundledPackage { fixture: false, resources: &[] }` via the
existing `include_str!` trio; `build_records` already gives a
`Source::Bundled` record `enabled = manifest.is_ok()` and
`Grants::from_bundled` (pre-approved), so "enabled by default" and
"no approval sheet" need no new code. Manifest `api = "1.3"`, `license =
"MIT OR Apache-2.0"`, `source = "bundled"`, no `icon`/`glyphs`
(Constitution X — the plugin needs none; the host's generic glyph is
used for the panel header), `default_locale = "en-US"`, `[strings.en-US]`
table (R9).

**Consequence to handle**: `packages()` is no longer empty, so two
existing tests that assert an empty plugin list without fixtures must be
updated: `modplayer-core/tests/plugins_manifest_discovery.rs::
fixtures_only_with_env` (expects `records().is_empty()` without the env)
and `modplayer-ui/tests/plugins_view.rs::empty_state_without_fixtures`.
Both become "exactly Section Loop without fixtures"; the 009 FR-023 empty
state stays implemented for the (now theoretical) zero-package build and
is exercised by a `PluginHost` constructed from an explicit empty package
list in the discovery test. Every other test that builds a
`PlaybackController` will now also spawn Section Loop's thread; its
manifest declares no `state.*` permission, so no state directory is
needed and no file is written — the tasks phase includes a full-workspace
run to confirm no assertion on plugin counts/focus holder breaks.

## R6. Script architecture: one file, one state table, relist-on-event

**Decision**: `main.luau` (~350 lines, heavily commented as the living
example) with a single `local S = { region = nil, a = nil, b = nil, cues =
{}, active = nil, pending_repeat = nil, armed = false }` and these
handlers:

| Event | Action |
|---|---|
| `ready_ack` | `register_actions()` (23 `register_action` calls), `register_panel()`, then `relist()` (the plugin may be re-enabled mid-track, FR-015) |
| `track_changed` | `relist()` — rediscovers region/cues from `markers.list()`, resets `S.active`, redraws overlays, sets slider/toggle |
| `marker_changed` | `relist()` (coalesced by the host; the payload's `actor` is ignored except for the log line) |
| `loop_armed` | `S.armed = event.region == S.region`; `update_widget("main","loop", S.armed)` |
| `loop_disarmed` | `S.armed = false`; `update_widget("main","loop", false)` |
| `focus_granted` / `focus_revoked` | log only — no deferred arm (FR-008) |
| `panel_interaction` | `loop` → `toggle_loop(value)`; `repeat` → `set_repeat(value)`; `snap` → `update_widget(...,"snap",false)` |
| `action_invoked` | dispatch on the action's short name (strip the identifier prefix) |

`relist()` is the only reader of `markers.list()`; every mutating call
is followed by nothing — the host's `marker_changed` fan-out triggers the
re-list (FR-012's "never reinterpret the host's result"). Refusals set
the Status `text` widget; successes clear it. The `debug_probe` handler
is **not** used in the shipped plugin (A): every acceptance is observable
host-side (marker store, focus arbiter, `PanelRegistry` widget values,
`OverlayRegistry`), and a probe surface in the reference plugin would
teach third-party authors a fixture-only mechanism.

**Active marker (FR-006)**: `S.active ∈ {"a","b",nil}` set by `set_a`/
`set_b`/`nudge_*` on success; `relist()` on `track_changed` resets it to
`nil`; on `marker_changed` it is kept unless the pointed endpoint no
longer exists. Nudge target = `S.active` if its marker exists, else `b`,
else `a`, else no-op.

## R7. Playhead and focus reads are already local

**Decision**: `set_a`/`set_b`/`set_cue_n` read `api.playback.state().
position_ms` (local, `bindings/playback.rs`, no RPC). `toggle_loop` and
`jump_cue_n` call `api.transport.request_focus()` synchronously in the
handler — the 011 R16 flag is set by the scheduler around
`panel_interaction`/`action_invoked` handlers, so under the default
policy the grant lands before the following `arm_loop`/`seek` RPC is
drained (both RPCs are applied in arrival order on the controller thread,
`apply.rs` header; `request_focus` is applied first). Under manual/
first-request-wins the arm/disarm/seek returns `no_focus` and the script
reverts the toggle via `update_widget` and writes the Status hint. No new
host code.

## R8. Overlays: 21 stable ids, rebuilt from the list

**Decision**: ids `a_line`, `b_line`, `ab_region`, `a_label`, `b_label`,
`cue_<n>_dot`, `cue_<n>_label`. `relist()` computes the wanted set, calls
`clear_overlays()` then one `add_overlays(list)`; both are `ui`-bucket
calls (100/s) and `marker_changed` is coalesced per tick, so a burst of
host drags costs ≤ 2 calls per tick. `region` uses `color = "accent"`;
`glyph` uses the host glyph `dot` (must exist in 011's host glyph set —
verified: `HostGlyph` in `gateway/src/ui.rs`; if the name differs at
implementation time the task uses the host set's dot-equivalent and
records it). Positions are `position_ms` straight from `markers.list()`.

**Alternative**: `remove_overlays` per deleted id — rejected: two calls
plus bookkeeping for no user-visible gain; `clear`+`add` is idempotent.

## R9. Strings: `@key` table, per-slot keys, and two host-side resolution gaps closed

**Decision**: every plugin string lives under `[strings.en-US]` and is
referenced as `@key` (panel title, widget labels, action labels, the snap
explanation label, Status hints, overlay labels). Because string tables
have no interpolation (011 R17), the eight cue overlay labels and the
eight "Cue n belongs to the host…" hints are per-slot keys
(`cue_label_1..8`, `status_cue_owned_1..8`).

**Host change required**: `apply.rs::resolve_string` is applied today
only to `RegisterPanel`, `RegisterAction` and `RegisterSettings`
(`apply.rs:558-687`). 011's contract §5 says "*any* user-facing string
parameter may be `@key`", but `UpdateWidget { value: WidgetValue::Text }`
and `AddOverlays` `label.text` are not resolved. This feature extends
resolution to those two arms (core-only, ≈ 15 lines, no schema change,
existing `@`-prefix rule — a host refusal `message` echoed into Status
never starts with `@`). Rejected: keeping a duplicate Lua string table in
the script — contradicts FR-018 ("provided through the manifest string
table") and teaches the wrong pattern.

## R10. Test strategy: host-observable, fixture-env harness, no probes

**Decision**: a new `modplayer-core/tests/controller_section_loop.rs`
reuses `controller_plugin_ui.rs`'s harness (`fixture_controller`, `call`,
`invoke_plugin_action`, `plugin_panel_interaction`, `wait_active`) but
targets the bundled identifier; the fixtures env is set only for US3's
contention scenario (`focus-a` as the "other plugin"). Assertions read
`controller.markers()` (positions, owners, `regions()`), the focus
arbiter's holder, `plugins().ui().panels()` widget values (Loop toggle,
Status text, slider) and `OverlayRegistry` primitive ids. SC-008's
"no private API" check is `bundled_section_loop.rs::script_calls_only_
schema_requests`: a regex over `main.luau` for `api\.([a-z_.]+)\.
([a-z_]+)\(` whose `(namespace, method)` pairs must each match a
`[[request]]` in the generated schema (plus the `api.on`/`api.ready`/
`api.log.*` locals). Gateway tests cover the two new requests'
validation; model proptests cover `set_loop_endpoint_owned` swap/clamp
invariants (Constitution VIII "marker/loop arithmetic"). Manual scenarios
M1–M8 in quickstart.md run against the real app (Governance).

## R11. Keyboard conflicts on a fresh install are left exactly as 011 ships them

**Decision**: register `I`/`O`/`L` as defaults per FR-005 and do nothing
else; `ActionRegistry`'s tier rule (G13) flags the plugin's three chords
inactive and keeps the host's.

**Spec deviation (FR-004, recorded in plan.md Complexity Tracking)**:
FR-004 says the "Set A"/"Set B" buttons *target* actions `set_a`/`set_b`.
Verified in `actions/registry.rs:320`, `ActionRegistry::is_invocable`
returns `false` whenever **any** effective chord of the action is in the
conflict set, and 011's D3/G14 deliberately gates the panel-button
dispatch path on `is_invocable` too. On a fresh install `set_a`'s `I`
conflicts with the host, so an action-targeting button would be inert —
contradicting FR-016 and the spec's own assumption that "panel controls
remain fully operable throughout". Therefore the panel's "Set A"/"Set B"
are **plain buttons** (no `action` field) whose `panel_interaction` the
script routes to the same `set_a()`/`set_b()` functions the keyboard
actions call. The observable behaviour FR-004 asks for (a click sets A at
the playhead, from a fresh install) is preserved; only the wiring
differs. Rejected: changing 011's G14 gate — that rule is ratified and
its tests (`inactive_action_not_dispatched`) pin it. Tests assert both:
the three chords are flagged, and the panel buttons still work.

## R12. Nothing on the real-time path changes

**Decision**: no engine/effects crate edit. `set_loop_repeat` and
endpoint moves go through the controller's existing `recommit_if_armed`
→ `push_loop_engine_commands` path (buffer-boundary `Command`s); the
seam, wrap counting and finite-repeat release are already host-side
(006). PR real-time note: "N/A — no real-time path changes."

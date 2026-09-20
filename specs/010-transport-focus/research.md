# Research: Transport Focus Arbitration

**Feature**: 010-transport-focus | **Date**: 2026-09-19 | **Spec**: [spec.md](spec.md)

Phase 0 output. Every decision below was checked against the code on
this branch (009's gateway/runtime/core/ui as merged), the constitution
(v1.1.1) and the spec's Clarifications. Format per decision: Decision /
Rationale / Alternatives considered. No `NEEDS CLARIFICATION` remains in
[plan.md](plan.md)'s Technical Context.

## R1. Where the arbiter lives: `modplayer-core::plugins::focus`, not the gateway crate

**Decision**: A new pure state machine, `FocusArbiter`, in
`crates/modplayer-core/src/plugins/focus.rs` — `policy`, `holder`,
ordered `pending: Vec<PluginId>`, and the first-request-wins
`host_locked_for_track` flag — whose methods return a
`Vec<FocusChange>` describing what the controller must do (set the
shared token, deliver `focus_revoked`/`focus_granted`, nothing else).
The gateway crate's `FocusToken` (an `Arc<AtomicU16>`) stays exactly
where it is as the **read-side cell** every plugin thread's
`Gateway::admit` checks (`needs_focus`), but core becomes its **only
writer** through a new `FocusToken::set_holder(Option<PluginId>)`;
`try_acquire`/`release_if` are removed (no remaining callers —
Constitution X YAGNI).

**Rationale**: Constitution III names transport focus a host primitive
"owned by core crates"; the gateway crate only *enforces* the holder
(G2) and has no notion of policy, plugin lifecycle, tracks or the UI.
`modplayer-core` already owns the sibling models the arbiter reacts to
(`PluginRecord`/`Lifecycle`, `TrackMarkers`, the transport reducer's
`TrackChanged`). Keeping the arbiter pure (no channels, no controller
borrow) lets `tests/focus_arbiter.rs` cover the three policies' full
transition table without a plugin thread, mirroring 009's
`Gateway::admit` tests.

**Alternatives considered**: (a) grow `modplayer-capability-gateway::
focus` into the arbiter — rejected: the crate is dependency-free by
design (009 R2) and would need to know `Lifecycle`/track changes; (b)
an `Arc<Mutex<Arbiter>>` shared with plugin threads so `request_focus`
stays local — rejected: policy decisions need the controller's records
and must deliver events, so the call has to reach core anyway (R2).

## R2. `request_focus()`/`release_focus()` become RPCs to core

**Decision**: Delete the two local arms in
`modplayer-plugin-runtime::bindings::dispatch` (`TransportRequestFocus`,
`TransportReleaseFocus`) so both fall through to `_ => rpc(..)` like
every other host-applied request; `plugins::apply` handles them by
calling `controller.focus_request(plugin)` / `focus_release(plugin)`
and always answers `Ok(Response::Ok)` (FR-003/FR-004). The refusal
constructor `Refusal::focus_held()` and its `focus_held` reason are
removed (no remaining producer; the closed six-code set is unchanged).

**Rationale**: FR-003 makes the call's return value policy-independent
("always success") and moves the real outcome into events that only
core can deliver. The RPC path already exists, is woken through
`set_waker` (009 R3) and costs one UI frame — acceptable for a call
whose result is asynchronous by definition.

**Alternatives considered**: keeping the CAS on the plugin thread and
"notifying" core after the fact — rejected: under manual/auto policies
the CAS must *not* grant, so the plugin thread cannot decide anything
locally.

## R3. Distinguishing local user actions from plugin and remote ones: a controller-scoped `TransportActor`

**Decision**: `PlaybackController` gains a private
`transport_actor: TransportActor` (`LocalUser` by default, `Plugin(
PluginId)`, `Remote`) and a scoped setter used in exactly two places:
`plugins::apply::drain_plugin_requests` wraps each `dispatch` in
`Plugin(id)`, and `apply_effects`'s `Effect::ApplyPendingTransferCommand`
arm (the only internal path that calls `play/skip/seek` on behalf of a
remote controller) wraps in `Remote`. `Input::RemoteCommand` is already
a distinct reducer input and never calls those methods. The auto-policy
revoke hook (`note_local_transport_action`) runs when the actor is
`LocalUser` at the entry of `dispatch(Input::{Play, Pause, Stop, Seek,
SkipForward, SkipBack})` and inside `arm_loop`/`disarm_loop` (after the
model mutation succeeds). Because every UI pointer control (transport
buttons, seek slider, waveform click-to-seek via `seek_frames`, loop
toggle via `toggle_current_loop`, cue jump buttons via `jump_to_cue`)
and every 007 keyboard action ends in one of those methods, FR-002a's
whole set is covered without touching a single UI call site; volume,
marker, cue-set, queue, navigation, chain and tempo paths never reach
them, so they cannot revoke focus. `shutdown()`/sign-out's internal
`stop()` calls are wrapped in `Remote` too (they are not user
interactions and the plugins are being torn down anyway).

**Rationale**: The spec's Assumptions say the controller already
distinguishes `RemoteCommand` from local actions; the only gap was
"plugin vs. local", and the plugin path has exactly one entry point
(`drain_plugin_requests`). A single hook in the controller is
compiler-checked (the `Input` match is exhaustive) and survives new UI
surfaces (011's plugin-contributed widgets will call the same methods
and, as spec Clarifications intend, count as local interactions).

**Alternatives considered**: (a) explicit `controller.note_user_action()`
calls from each UI site — rejected: 10+ sites, easy to miss, untestable
from core; (b) an `actor` parameter on every transport method — rejected:
changes 30+ signatures across 003–009 tests for a value that is
`LocalUser` everywhere but two places.

## R4. Event ordering on an auto-policy host action

**Decision**: `note_local_transport_action` applies the arbiter's
`revoke_to_host` result *before* `transport::reduce` runs: it sets the
token, then `handle.send_event(FocusRevoked{holder: Host})` straight to
the demoted plugin's inbox. The resulting `play_state_changed` (fanned
out at the end of the same `dispatch`) or `loop_armed/disarmed` (fanned
out by `fan_out_revision_events` on the next `tick`) therefore lands
later on the same FIFO channel — FR-014's order holds structurally.
Plugin→plugin transitions deliver `focus_revoked` to the outgoing
handle before `focus_granted` to the incoming one, in one
`apply_focus_changes` loop.

**Rationale**: 009's `PluginHandle` inbox is a single `SyncSender`
per plugin (RT1); ordering within one sender is guaranteed. No new
sequencing machinery is needed.

**Alternatives considered**: batching focus events into
`fan_out_revision_events` — rejected: they would then trail the
`play_state_changed` that `dispatch` emits immediately.

## R5. Policy persistence: `[transport] focus_policy` in `settings.toml`

**Decision**: `AudioSettings` (the app's settings struct, 002–007) gains
`focus_policy: FocusPolicy` (default `AutoOnInteraction`); `RawSettings`
gains `#[serde(default)] transport: RawTransport { focus_policy:
String }` with the wire values `"manual" | "auto_on_interaction" |
"first_request_wins"`. Parsing an unknown value falls back to the
default with the same `InvalidField` warning path 007 uses for an
unknown theme/buffer preset; writes go through `persist_settings`
(atomic `.tmp → rename`). The holder and pending queue are never
serialized (FR-012).

**Rationale**: Reuses the one settings file, its atomic-write contract
and its test suite (`tests/settings.rs` proptest round trip gains the
new field). FR-012 names `settings.toml` explicitly.

**Alternatives considered**: a separate `transport.toml` — rejected
(second file, second writer, no benefit).

## R6. Plugin API 1.0 → 1.1: two events, additive

**Decision**: `api/v1.toml` sets `[api_version] minor = 1` and adds
```toml
[[event]]
name = "FocusGranted"
requires = "transport.control"
payload = ["holder"]

[[event]]
name = "FocusRevoked"
requires = "transport.control"
payload = ["holder"]
```
`HostEvent::FocusGranted { holder: OwnerInfo }` /
`FocusRevoked { holder: OwnerInfo }` are added to the gateway crate's
enum (and `kind()`), `scheduler::event_to_lua` renders `holder` with the
existing `owner_to_string` (`"host"` or the identifier — never `"me"`,
since these events are delivered directly to a handle, bypassing the
fan-out's self-substitution), `docs/plugin-api/v1.md` is regenerated by
`tests/api_reference.rs`, and the PR carries the Constitution IX change
request (text in [contracts/plugin-api-v1.1.md](contracts/plugin-api-v1.1.md)
§4). Manifests declaring `api = "1.0"` remain compatible (`ApiRange::
min_minor` ≤ 1). 009 contract's "not delivered this slice" line for
these two events is superseded.

**Rationale**: Constitution IX (minor = add only; single schema; docs
generated). `OwnerInfo` already encodes exactly `host | <identifier>`.

**Alternatives considered**: a broadcast `focus_changed` to every
`transport.control` plugin — rejected by spec Clarifications (only the
two parties are notified).

## R7. First-request-wins "track change" trigger = the existing `TrackChanged` fan-out

**Decision**: `fan_out_plugin_playback_events` already decides when
`HostEvent::TrackChanged` is emitted (009 T070); the same branch now
also calls `arbiter.on_track_changed()` and applies its changes
(holder → host, queue cleared, `host_locked_for_track = false`) **only
when the policy is `FirstRequestWins`**. Under the other two policies
the branch is a no-op for focus.

**Rationale**: Spec Clarifications define a track change as "every
emission of the `track_changed` event"; reusing the trigger makes the
two definitions identical by construction (including whatever 009
decided about same-track repeats).

**Alternatives considered**: hooking `Input::TrackStarted` — rejected:
it fires on source reloads that 009 deliberately does not surface as
`track_changed`.

## R8. Panel read model: `TransportFocusView`, built per frame

**Decision**: `PlaybackController::transport_focus_view() ->
TransportFocusView { policy, holder: FocusHolderRow (Host | Plugin{id,
name}), rows: Vec<FocusRow { id, name, holds: bool, request_order:
Option<usize> }> }` in `plugins/view.rs` next to `PluginsView`. Rows =
every record that is `enabled`, whose `lifecycle` is `Loading | Active`,
and whose `grants.holds(Permission::TransportControl)`; sorted by name
like `PluginsView`. `request_order` is 1-based position in
`arbiter.pending` (only still-listed plugins count — a pending entry for
a plugin that just got suspended is removed by `stop`, R11).

**Rationale**: Mirrors 008/009's "read model per panel" pattern; the UI
never touches records or the arbiter directly.

**Alternatives considered**: exposing the arbiter — rejected (UI would
need the id table and lifecycle filter too).

## R9. Two new fixture plugins: `focus-a` and `focus-b`

**Decision**: Add `plugins/fixtures/focus-a` and `focus-b`
(identifiers `org.modplayer.fixture.focus-a` / `focus-b`, permissions
`playback.observe` + `transport.control` + `markers.write`, `markers.read`):
each calls `request_focus()` in `ready_ack`, logs
`focus_granted holder=<…>` / `focus_revoked holder=<…>` /
`play_state_changed` as they arrive, and on `focus_granted` issues one
`seek(1000)` and (focus-a only) creates + arms a transient loop region;
both answer `debug_probe` with their event log. Embedded through
`bundled::fixtures()` like the existing eight (ten fixtures total).

**Rationale**: SC-001/SC-005/SC-006 need two competing
`transport.control` plugins whose *event* history can be asserted;
`wellbehaved`/`flood` do not record focus events and their scripted
flows assume an immediate grant. Real Luau fixtures are also what the
manual scenarios drive (Governance › Manual Scenario Sign-Off).

**Alternatives considered**: injecting `RpcEnvelope`s only
(`debug_requests_sender`) — kept for arbiter/apply tests, but it cannot
prove the `no_focus` refusal on a real plugin thread or the Lua-side
event payload.

## R10. Impact on 009's existing tests and fixtures

**Decision**: Rewrite, don't delete:
- `controller_plugins_permissions::request_focus_contention_invalid_state`
  → `request_focus_is_recorded_never_refused` (two requests → both
  pending under the default policy, `Ok` both times, holder = host).
- the flood rate-limit test: set `FirstRequestWins` and give the flood
  fixture focus via `focus_give` before triggering, instead of clearing
  the token by hand.
- `queue_write_ignores_focus`: give focus to the holder candidate with
  `focus_give` instead of relying on a CAS win.
- `wellbehaved` fixture script unchanged: under the default policy its
  `request_focus` step still logs `ok`, its `arm_loop` step now logs
  `no_focus` until the user gives it focus (documented in its README and
  in quickstart M6); `arm_loop_permission_before_focus` still holds (G2
  order unchanged).
- Gateway crate tests referring to `focus_held` are dropped with the
  constructor; `admit_checks_focus_before_rate` keeps using
  `set_holder`.

**Rationale**: FR-003 explicitly supersedes the 009 placeholder; the
tests encoded that placeholder.

## R11. Fault vs. non-fault focus loss

**Decision**: `PluginHost::stop(id, reason, ..)` (disable, suspend,
shutdown) keeps its 009 teardown order — it now calls
`arbiter.vacate(id, Vacancy::Fault)` (holder → host, pending entry
removed, first-request-wins refill from the earliest still-pending
*running* plugin) and still runs `markers.disarm_if_owned_by` /
`remove_transient_owned_by` / `chain.orphan_owned_by`. Every non-fault
transition (`focus_give`, `focus_take_back`, auto-policy host action,
`release_focus`, first-wins track reset) goes through the arbiter only
and never touches `TrackMarkers` (FR-007 second half). `release_focus`
from the holder is `Vacancy::Voluntary` (refill under first-wins);
`release_focus` while pending withdraws the request (FR-011).
`focus_take_back` is not a vacancy: it sets `host_locked_for_track`
under first-wins (no refill until the next track change or a
`focus_give`).

**Rationale**: FR-006/FR-007 and the Clarifications spell this table
out; keeping the loop teardown inside `stop` means the existing 009
lifecycle tests keep proving it.

## R12. Host-side `no_focus` re-check for the eight focus-gated requests

**Decision**: `plugins::apply::dispatch` adds `require_focus(controller,
plugin)?` to `Play/Pause/Toggle/Seek/SkipNext/SkipPrevious/ArmLoop/
DisarmLoop` (returns `Refusal::no_focus()` when `arbiter.holder() !=
Plugin(plugin)`).

**Rationale**: G2's admission runs on the plugin thread; a revoke can
land between admission and application (one UI frame). FR-001 promises
zero side effects for a non-holder, so core validates again — 009's
contract already reserves "per-call validation host-side".

**Alternatives considered**: relying on the atomic alone — rejected:
the race window is real (RPC queue) and the check is O(1).

## R13. "Within the same second" (FR-007, SC-003)

**Decision**: No timer. `drain_plugin_runtime_events` (runs every
`tick`, ≤ 33 ms cadence, and is followed by `wake()`) processes
`RuntimeEvent::Suspended` → `stop` → arbiter vacate in the same call;
the panel reads `transport_focus_view()` every frame. The automated
test asserts holder = host and loop disarmed immediately after the
`tick` that drained the suspension; the manual scenario reads the
screenshot within one second.

## R14. Panel placement and toggle: mirror 008 exactly

**Decision**: `crates/modplayer-ui/src/transport_view.rs` with
`panel_open_id()` = `Id::new("now-playing-transport-open")`,
`toggle_transport_panel(ctx)`, `show(ui, controller)`; a
`selectable_label` "Transport" in the Now Playing header row beside
"Queue"/"Effects"; drawn after the Effect Chain panel and before the
Queue panel when open. New `HostAction::ToggleTransportPanel`
(`host.nav.toggle_transport_panel`, Navigation, trigger, Now Playing
scope, `repeats_while_held: false`, default `["T"]`) appended to the
007 catalog; `T` is unbound today (verified: only `L`, `Q`, `E` are
single-letter defaults in that scope). Strings in a new
`locales/en-US/transport.ftl` (+1 key in `controls.ftl`).

**Rationale**: FR-013 says "mirrors 008"; the temp-memory flag already
survives track changes and is per-viewer.

## R15. Restart / re-enable never restores focus or a request

**Decision**: `PluginHost::spawn` does not touch the arbiter; `stop`
already removed the plugin from `holder`/`pending`. A restarted plugin
therefore starts as an observer and must `request_focus()` again
(joining the end of the queue). Hot-reload retention is
002-developer-mode's.

## R16. i18n scope

**Decision**: `locales/en-US/transport.ftl` only, keys prefixed
`transport-` (+ `action-nav-toggle-transport-panel` in `controls.ftl`);
the `fluent_keys` test guards every key. No `pt-BR` directory exists in
the repository yet (001–009 shipped `en-US` only); this slice follows
the established precedent and keeps all strings externalised so the
pt-BR pass remains a pure resource-file addition.

## Open items resolved without escalation (headless)

| Item | Chosen | Rejected |
|---|---|---|
| Does a no-op host action (e.g. `L` with no region, cue jump on an empty slot, `play` with no track) revoke focus under auto? | **No** — the hook fires only when the transport reducer receives a real `Input` (`play`/`pause`/… always dispatch one, so they revoke even if the reducer ignores them; `arm_loop`/`disarm_loop` revoke only on `Ok`; `jump_to_cue` on an empty slot never reaches `seek_frames`). Documented in contracts/focus-arbitration.md A6. | Revoking on every invocation regardless of effect — would let an accidental key press on an empty slot demote a plugin for nothing. |
| `focus_give` to the current holder | no-op, no events | re-sending `focus_granted` — noise |
| `focus_give`/`focus_take_back` while no plugin is running | `take_back` no-op; `give` to a non-listed id is ignored | — |
| Policy change with a pending queue | queue preserved, re-evaluated only on the next event (spec edge case) | immediate re-evaluation |

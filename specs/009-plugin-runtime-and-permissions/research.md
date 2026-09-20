# Research: Plugin Runtime, Sandbox, and Permission Gateway

**Feature**: 009-plugin-runtime-and-permissions | **Date**: 2026-09-19 | **Spec**: [spec.md](spec.md)

Every decision below was taken headlessly against the spec (no open
`[NEEDS CLARIFICATION]` after the clarify pass), the constitution (v1.1.0)
and the code as it exists after 008 (`41651c9`, 101/101 tasks). Evidence
lines cite files in this worktree. Where the spec fixes a number, the
number is taken as-is (spec Assumptions: "the plan phase may not change
them without recording the deviation"); no deviation was needed.

---

## R1. Scripting runtime: Luau via `mlua` 0.12 (Constitution II ADR)

**Decision**: the plugin language is **Luau**, embedded through
`mlua = "0.12"` with features `["luau", "send", "serialize"]` (no
`luau-jit`). The ADR is written at
[docs/adr/0001-plugin-runtime-luau.md](../../docs/adr/0001-plugin-runtime-luau.md);
a PATCH amendment of the constitution linking it from Principle II's
`TODO(WASM_RUNTIME_DECISION)` is a task of this feature.

**Rationale** (verified against the vendored `mlua-0.12.1/src/state.rs`):

- *Preemptive handler interruption* (FR-009, FR-011, Clarifications "the
  runtime MUST be able to preemptively interrupt a handler at its CPU budget
  regardless of whether the plugin code cooperates"): `Lua::set_interrupt`
  installs a callback the Luau VM invokes "at any function call or at any
  loop iteration"; returning `Err` from it unwinds the running handler as an
  ordinary `mlua::Error`. A `while true do end` is therefore aborted within
  one interrupt check of the 4 ms deadline. No script cooperation is
  involved.
- *Native memory cap* (FR-009 "enforced by the runtime's allocator so an
  over-cap allocation fails inside the plugin, never the host"):
  `Lua::set_memory_limit(bytes)` makes the allocator return
  `Error::MemoryError` at the allocation that would cross the limit;
  `Lua::used_memory()` gives the live heap figure for the plugin list.
- *Sandbox by design*: `Lua::sandbox(true)` (Luau-only) freezes globals and
  builtins; `Lua::new_with(StdLib, LuaOptions)` loads only the safe subset
  (`STRING | TABLE | MATH | BIT32 | UTF8`; no `io`, `os`, `debug`,
  `package`, no `require`). Luau exists to run untrusted scripts in a
  multi-tenant host, which is exactly PL-1.2/NFR-4.3's threat model.
- *Isolation and threading*: one `Lua` state per plugin, `Send` under the
  `send` feature, owned by that plugin's own OS thread (R3). Two states
  share nothing (FR-004).
- *JSON-shaped values* for state and event payloads through the
  `serialize` feature (`serde_json::Value ⇄ LuaValue`), which also bounds
  what a plugin can store (FR-014 "JSON-serializable").
- *Licensing and build*: MIT (mlua, mlua-sys, luau0-src); the `luau`
  feature auto-vendors and compiles Luau's C++ through `cc`, available on
  the ubuntu/macos/windows CI images already used by `ci.yml`; MSRV 1.88 ≤
  the workspace's 1.95. One new runtime dependency, justified under
  Constitution X because `std` has no interpreter and the runtime is a
  constitution-named component.
- *Safety surface*: mlua's public API is safe; both new crates keep
  `#![forbid(unsafe_code)]`. The constitution's "plugin runtime" unsafe
  allowance is not needed.

**Alternatives considered**:

- *WebAssembly via `wasmtime` with fuel/epoch interruption* — strongest
  isolation and deterministic fuel, but ≈ 50 transitive crates plus
  Cranelift compile time on every CI run, and plugin authors would need a
  compile toolchain: the spec's "small scripted plugins" and 002-developer-
  mode's hot reload argue for a script that is a text file in a folder.
  Retained as the documented fallback in the ADR if Luau ever fails to
  build on a target platform.
- *Rhai* — pure Rust and `on_progress` gives operation-granularity
  interruption, but it has no per-engine heap cap (only element-count
  limits); a global-allocator shim would count the host's allocations too.
  Fails Constitution II's "support execution budgets and memory caps
  natively".
- *Lua 5.4 via mlua* — same crate, but no `set_interrupt`; `set_hook`
  with `every_nth_instruction` works but Luau's `sandbox()` and its
  read-only builtins are absent, so sandboxing would be hand-rolled.

## R2. Crate layout: two constitution-named crates plus a core service

**Decision**: add `crates/modplayer-capability-gateway` and
`crates/modplayer-plugin-runtime` (Constitution VII's "plugin-runtime" and
"capability-gateway" components), keep the plugin *host service* (registry,
lifecycle, list view, request application against the controller's
authoritative models) inside `modplayer-core` as `src/plugins/`, and put
the Plugins section in `modplayer-ui/src/plugins_view.rs`.

- **gateway** (std + `serde`/`serde_json`/`toml`/`thiserror`; no
  workspace crate deps): the single API definition (R6), the permission
  catalog, the manifest parser/validator (R7), `Refusal`, the
  `Request`/`Response`/`HostEvent` types, `Grants`, the per-category
  `RateLimiter` (R13), the storage-capped `PluginStateStore` (R9) and the
  budget constants. Everything a plugin may ask for is a `Request`
  variant; `Gateway::admit` runs permission → focus → rate limit and is
  the only path a `Request` can take toward the host (Constitution II
  "single Capability Gateway").
- **runtime** (gateway + mlua + `modplayer-engine` for `RtShared`/
  `PositionClock`): `PluginContext` (Lua state, sandbox, memory limit,
  interrupt), `Budget` accounting (R4), the per-plugin `Scheduler` thread
  (R5), the Lua API object bound from the generated request table (R6),
  timers (R5), and `RuntimeEvent` reporting to the host.
- **core** `src/plugins/`: `PluginHost` (discovery of bundled packages,
  `PluginRecord` state machine, grants, focus holder, suspension counter,
  health windows, notifications, request application, event fan-out,
  `PluginsView`), plus the marker/effects ownership deltas in the existing
  models.

Dependency graph after this feature: `runtime → {gateway, engine}`;
`core → {gateway, runtime, engine, effects, …}`; `ui → {core, gateway
(permission names for Fluent keys), …}`; `gateway → {}` (no workspace
crate). `modplayer/tests/single_dependent.rs` (only the receiver's consumer
depends on the receiver crate) is unaffected.

**Alternatives considered**: one `modplayer-plugins` crate — violates the
constitution's component list and would let the runtime reach the
controller; gateway inside core — the manifest validator and API schema
would then be untestable without the whole controller and un-reusable by
the future registry tooling (001-mvp/011).

## R3. Threading and the Gateway RPC model

**Decision**: every enabled plugin runs on its **own OS thread**
(`plugin:<identifier>`) that owns its `Lua` state and scheduler (FR-004
"own scheduler and timers"). Requests that need the controller's
authoritative state are **synchronous RPCs**: the plugin thread runs
`Gateway::admit` locally (permission, focus, rate limit — all of whose
state is shared atomically with core), then sends
`(PluginId, Request, reply: SyncSender<Response>)` on a bounded
`std::sync::mpsc::sync_channel` and blocks on the reply. The controller
drains that queue at the top of `PlaybackController::tick()`
(`drain_plugin_requests`), validates against its models (ownership, ids,
capacity), applies, and replies. A new `PlaybackController::set_waker`
hook (mirroring `set_clock`) lets the UI install
`egui::Context::request_repaint`, so a queued request wakes the UI thread
immediately instead of waiting for the next 33 ms ticker frame; tests
install no waker and tick manually.

- The plugin's **CPU clock is paused while it waits** for a reply
  (spec FR-009: CPU "includes the synchronous part of Gateway calls …
  excluding asynchronous host work they trigger"): the wait duration is
  added to the handler deadline and subtracted from the aggregate sample
  (R4).
- A reply that does not arrive within **1 s** (UI thread blocked, e.g. a
  Windows title-bar drag) returns `invalid_state`/`host_busy` and is
  logged; the controller drops the stale reply sender harmlessly.
- Requests that need **no** controller state never RPC: timers, `position`
  subscription rate, `state.*` get/set (the store is owned by the plugin
  thread, R9), `request_focus`/`release_focus` (an atomic in core shared
  with the gateway, R12), `list_chain`/`list_markers` reads (served from
  the controller's last published snapshot, `Arc<ArcSwap>`-free: a
  `Mutex<Snapshot>` written by core after every change).

**Rationale**: the controller is the single authority for shadow state
(`controller.rs` module doc) and allocates every id a plugin must get back
synchronously (`TrackMarkers::next_marker_id`, `ChainModel::add` →
`NodeId`), so "mirror + fire-and-forget apply" cannot return
`create_marker`'s id without duplicating the allocators; running handlers
on the UI thread would jank frames by up to 100 ms/s per plugin (the very
budget the spec allows) and violate the spirit of FR-004.

**Alternatives considered**: shared `Arc<Mutex<TrackMarkers/ChainModel>>`
between controller and gateway — a 3 500-line controller refactor touching
006/007/008 code paths for no user-visible gain; a dedicated "gateway
thread" that owns the models — same refactor plus a second authority.

## R4. Budget enforcement

**Decision** (numbers fixed by the spec — FR-009, FR-010, FR-011, FR-006):

| Budget | Mechanism |
|---|---|
| 4 ms per handler | Before each handler call the scheduler stores `deadline = now + 4 ms` in an `AtomicU64` (nanos since context start). The `set_interrupt` callback compares `Instant::now()` against it and returns `Err(BudgetExceeded)` when passed. Cost per check ≈ 25 ns (one monotonic clock read); Luau calls it at loop back-edges and calls. RPC waits push the deadline forward by the wait length. |
| 10 % of one core over 1 s | A `VecDeque<(Instant, Duration)>` of handler durations (RPC waits excluded); after every handler completion or abort, evict samples older than 1 s and suspend if the sum > 100 ms. |
| 64 MB heap | `Lua::set_memory_limit(64 * 1024 * 1024)`; any `Error::MemoryError` surfacing from a handler suspends the plugin (`cause: Memory`). `used_memory()` is sampled after every handler into the `PluginGauges` atomics for the list. |
| 10 MB storage | `PluginStateStore` (R9) accounts the serialized size of all keys+values across both scopes; a `set` that would exceed it (or 256-byte key / 1 MB value) returns `budget_exceeded`/`storage_cap` and leaves the store unchanged. |
| `ready()` within 5 s | The scheduler's first wake deadline; on expiry with no `ready()` → suspend (`cause: DidNotStart`). |
| 200 ms persistence window on `unloading` | After the `unloading` handler returns/aborts, the thread hands its dirty store to the writer and waits ≤ 200 ms for the write acknowledgement before dropping the Lua state (R17). |

Handler aborts (exception, deadline, failed track-state restore) are
reported as `RuntimeEvent::HandlerAborted { cause }`; core's
`PluginRecord` keeps the 3-in-60 s → `warning` → clear-after-5-min window
(FR-010) with the controller's injectable clock.

**Alternatives considered**: counting Luau instructions instead of wall
time — deterministic but not what the spec measures ("wall-clock time
during which the plugin's context is executing"); `luau-jit` — faster but
interrupt points in generated code are the same, and JIT adds a moving
target to the memory accounting.

## R5. Scheduler, events and the position/meter pump

**Decision**: the plugin thread's loop is `recv_timeout(inbox, next_wake)`
where `next_wake = min(next timer, next position tick, next meter tick,
ready deadline)`. Inbound items are `Inbound::Event(HostEvent)` from core
(bounded `sync_channel(1024)`, `try_send`; a full inbox — only possible
while a handler is stuck — drops the event with a log line, the plugin is
about to be suspended anyway), `Inbound::Control` (`Unloading{reason}`,
`Stop`) and the thread's own timers.

- **`position`** is produced *on the plugin thread* by reading
  `Arc<RtShared>` through `modplayer_engine::PositionClock::now(&shared,
  rate)` at the subscribed rate (default 10/s, clamped 1–60; FR-016),
  delivered only when the value differs from the last delivered one (so
  nothing while paused, once after a seek/loop wrap while paused). Intent
  and source rate come from a core-written `Arc<PlaybackSnapshot>` of
  atomics. Jitter at the requested rate is the scheduler's wait accuracy:
  ≤ 1 ms on macOS/Linux (asserted by a test), 1–15.6 ms on Windows
  depending on the process timer resolution — recorded as a risk; if the
  Windows manual run exceeds NFR-1.9's 5 ms, raising the timer resolution
  belongs in the audio-io platform adapter, not in this crate.
- **`meter`** is sampled the same way from `RtShared::pre_level/post_level/
  spectrum` at the UI's cadence (33 ms, `ticker.rs` `TICK_INTERVAL`) for
  `audio.meter` holders (FR-020).
- **`schedule_at_position(p)`**: kept in the plugin's timer set; every
  position sample checks `pos ≥ p` and fires once with the actual
  position; a track-generation change (from `PlaybackSnapshot`) cancels
  all position timers silently (FR-022).
- Controller-originated events (`track_changed`, `play_state_changed`,
  `queue_changed`, `marker_changed`, `loop_*`, `effect_chain_changed`,
  `ready_ack`, `unloading`) are pushed by core's `fan_out(event)` filtered
  by each plugin's grants (FR-028); ordering per plugin is the inbox order.
- **`track_changed` with state restored first** (FR-021): the plugin
  thread, on dequeuing `TrackChanged`, loads the per-track file for that
  track into the store under a 4 ms deadline (the read is host code with a
  deadline check between read and parse); on overrun the load is dropped,
  a `HandlerAborted{cause: RestoreTimeout}` is reported and the event is
  delivered with no per-track state.

**Alternatives considered**: a single shared "plugin clock" thread pushing
positions to every plugin — breaks "a slow handler delays only that
plugin's subsequent events" unless each plugin still has its own queue,
so it only adds a thread; controller-driven `position` at frame cadence —
30 Hz max and frame-timed jitter, cannot meet 60/s ≤ 5 ms.

**Windows jitter figure (T118, quickstart.md M6)**: not yet measured.
Every implementation session on this feature (US1–US4, T080/T090/T103/
T112) ran in a headless, non-interactive sandbox with no display and no
Windows host, so quickstart.md's M6 could only be exercised through
`hang`/`noready`-fixture automated proxies covering the *suspension*
half of M6 — never the well-behaved fixture's position-probe log this
risk needs, and never on Windows. `position_jitter_under_5ms`
(`modplayer-plugin-runtime/tests/scheduler.rs`, T098) is itself `#[cfg(
not(windows))]` and so has never run against a Windows scheduler either.
This risk therefore remains open pending a maintainer session on real
Windows hardware: run `MODPLAYER_PLUGIN_FIXTURES=1` on Windows, enable
the `wellbehaved` fixture, read its position-probe log (M8), record the
observed jitter here, and confirm whether it stays within NFR-1.9's
5 ms — raising the timer resolution in the audio-io platform adapter if
it does not.

## R6. One API definition (Constitution IX)

**Decision**: `crates/modplayer-capability-gateway/api/v1.toml` is the
single machine-readable definition: the 25-entry permission catalog
(name, category, explanation Fluent key), every request (name, Lua
namespace, rate category, required permission, `needs_focus`, arguments,
result), every event (name, required permission, payload fields), the
refusal codes and the API version (`1.0`). A `build.rs` parses it with
`toml` and emits `generated.rs` (`Permission`, `RequestKind`, `EventKind`
enums with `requires()`, `category()`, `needs_focus()`, `name()` tables)
into `OUT_DIR`. The runtime binds the Lua API object by iterating the
generated `RequestKind::ALL` table, so a request cannot exist in the
schema without a binding and vice-versa (an exhaustive `match` in the
runtime's dispatcher is the compile-time check). The API reference
`docs/plugin-api/v1.md` is generated by
`tests/api_reference.rs` in the gateway crate, which rewrites the file
under `MODPLAYER_UPDATE_API_REFERENCE=1` and otherwise fails if the
committed file differs — docs and runtime cannot diverge (PL-9.5).

**Alternatives considered**: a Rust `const` table as the source of truth —
readable by Rust only, so the reference would be hand-written; JSON schema
with a code generator crate — a build dependency for a 200-line TOML.

## R7. Manifest format and validation

**Decision**: a plugin package is a folder with `plugin.toml`, the entry
script (`main.luau` by default; `entry` overrides), `README.md`, and any
extra scripts/resources. `plugin.toml` is parsed with `toml` (workspace
dep) into `ManifestDto` then validated into `Manifest` (see
[contracts/manifest.md](contracts/manifest.md)). Versions are hand-parsed
`major.minor.patch`; the API range is a caret-style `api = "1.0"` meaning
`≥ 1.0, < 2.0`. Validation reasons are a closed `ManifestError` enum
rendered through Fluent (`manifest-error-*`), one sentence each (FR-002).
`proptest` covers parsing/validation round-trips (Constitution VIII).

**Alternatives considered**: JSON manifests — TOML is already the
project's config format (`settings.toml`, `account.toml`) and is friendlier
to hand-author; the `semver` crate — in the tree transitively but a full
parser for a three-integer field is more than needed.

## R8. Bundled source and fixture packages

**Decision**: bundled packages are **embedded** in the binary. Package
folders live in the repository under `plugins/bundled/<identifier>/` (the
folder shape FR-001 requires, editable and readable as files) and are
compiled in through `include_str!` by `modplayer-core/src/plugins/
bundled.rs` into `BundledPackage { identifier, manifest_toml, entry,
readme }`. The eight fixture packages (FR-003, research R18) live under
`plugins/fixtures/<name>/` and are embedded the same way but only
*discovered* when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch (any
build, as the spec says). With no packages discovered the list is empty
(FR-023 empty state). 012/013 add their packages to `plugins/bundled/`.

**Alternatives considered**: reading a `plugins/` folder next to the
executable at runtime — needs install-time packaging on three platforms and
a filesystem read path that the sideload slice (001-mvp/011) will design
properly; embedding the fixtures only under `cfg(debug_assertions)` — the
spec says "any build".

## R9. Plugin state store and persistence

**Decision**: `PluginStateStore` (gateway crate) holds
`plugin: BTreeMap<String, serde_json::Value>` and
`track: Option<(TrackId-string, BTreeMap<…>)>` with running serialized-size
accounting. On disk: `<data_local_dir>/ModPlayer/plugin-state/<hex(identifier)>/plugin.json`
and `…/tracks/<hex(track_id)>.json`, overridable with
`MODPLAYER_PLUGIN_STATE_DIR` (tests), encoded like 006's files
(`markers::store::encode_track_id` hex convention, pretty JSON), written
by one host-wide writer thread with the `.tmp` → `sync_all` → `rename`
pattern and a 500 ms debounce per file (mirrors `markers::store::
spawn_writer`, 006 research R11). `state.plugin` survives sign-out;
`clear_for_sign_out` deletes every plugin's `tracks/` directory and clears
the in-memory track scope (FR-014). Note: 006's implementation *keeps*
marker files on sign-out (`clear_for_sign_out` comment, controller.rs:
"markers are not account data"); the spec's FR-014 rule for `state.track`
stands on its own and is applied regardless.

**Alternatives considered**: one JSON file per plugin holding both scopes
— sign-out would have to rewrite every file; SQLite — a new dependency for
≤ 10 MB per plugin of key-value data.

## R10. Marker ownership, `not_owner`, transient markers

**Decision**: `markers::model::Owner` gains `Plugin(PluginId)` where
`PluginId` is `modplayer_effects::PluginId(u16)` (already the effect-node
owner type, DM-17) so both ownership fields use one session-stable id.
Core keeps a `PluginIdTable` interner (identifier ⇄ `PluginId`) seeded
from the registry in identifier order; the persisted marker DTO's `owner`
string (already `"host"`-defaulted, `store.rs:131`) stores the identifier,
and unknown identifiers at load are interned too, so such markers are
simply owned by a plugin that is not running (host may edit, no plugin may
touch: `not_owner`). `Marker.transient` (already present, always `false`)
becomes real: `store::encode` skips transient markers and the regions
whose endpoints are transient; `TrackMarkers::remove_transient_owned_by`
runs on owner unload, and a track change drops the model anyway.
`MarkerError` gains `NotOwner`; plugin-path controller methods
(`plugin_marker_*`) check ownership before delegating to the untouched
host methods. `marker_changed` is emitted per `tick()` from a
`TrackMarkers::revision` counter plus a `last_marker_actor` field the
plugin paths set (host paths leave it `Host`), coalescing a burst into one
event per tick.

**Alternatives considered**: a string owner in the model — `Owner` is
`Copy` and lives inside every `Marker`, and 006's tests construct it by
value; per-mutation events from 15 host methods — a choke point is less
error-prone and PL-5 does not require one event per operation.

## R11. Effect-node ownership, suggested position, orphaning

**Decision**: `NodeOwner::Plugin(PluginId)` is used for real. `ChainModel`
gains `add_at(kind, owner, index)`, `orphan_owned_by(owner)`,
`readopt(owner)`, `owned_by(owner)`; `NodeModel.orphaned` (already present)
is set/cleared by those. The suggested position (`Index(n)` |
`Before(kind)` | `After(kind)`) resolves to an index at insertion; an
absent kind appends (FR-019). Auto-bypass for non-host owners already
exists on the RT (008 R10) and needs no engine change. `effect_chain_
changed` is emitted per tick from a `ChainModel::revision` counter.
`list_chain()` and `list_markers()` are served from a `PluginSnapshot`
(`Mutex`) that core rewrites whenever either revision changes, so those
reads never RPC.

**Alternatives considered**: refusing `create_node` when the named kind is
absent — the spec chose append; deleting orphans on disable — FR-012 says
never remove.

## R12. Transport focus (single global holder)

**Decision**: `PluginHost.focus: Arc<AtomicU16>` (`0` = host, `n` =
`PluginId(n-1)`). `request_focus` CAS `0 → n` succeeds immediately, any
other value → `invalid_state`/`focus_held`; `release_focus` stores `0`
(always succeeds). Every other transport mutation checks the atomic
equals the caller (`no_focus`). Suspend/disable stores `0`. The host's own
transport actions ignore the atomic entirely (FR-017; Constitution X).
010 replaces the atomic's policy, not its shape.

## R13. Rate limiter

**Decision**: `RateLimiter` in the gateway crate: five categories, each a
`VecDeque<Instant>` capped at 100; `admit(category, now)` evicts entries
older than 1 s and refuses the 101st (`rate_limited`) without recording
it. It runs on the plugin thread after permission and focus checks
(FR-025). Category membership comes from the generated `RequestKind::
category()` table (R6).

## R14. Notifications keyed per plugin

**Decision**: `Notification` gains `dedupe_key: Option<String>`;
`NotificationCenter::raise_keyed(severity, message_key, args, actions,
dedupe_key)` dismisses any live notification with the same dedupe key
before pushing (the spec's "a repeat suspension replaces the same key").
`NotificationAction` gains `RestartPlugin(PluginId)` and
`DisablePlugin(PluginId)` (`Copy` preserved). Keys: `plugin-suspended`
with dedupe `plugin-suspended:<identifier>` and args `{ $plugin, $cause }`,
`plugin-auto-disabled` with dedupe `plugin-auto-disabled:<identifier>`.

## R15. Plugin list UI

**Decision**: `modplayer-ui/src/plugins_view.rs` replaces
`shell::plugins_placeholder` for `Section::Plugins`, rendering
`controller.plugins_view()` rows (sorted by name in core) as one row per
plugin: name, version, source, enabled checkbox (accessible name
`plugins-enable-toggle` with `$plugin`), health badge, permission summary
(the catalog explanation strings joined with the locale list separator),
CPU `xx %` of the 10 % share and `used MB / 64 MB` (or `—`), plus
"invalid manifest: <reason>" for invalid rows; an empty-state label when
there are no rows; no uninstall control anywhere (FR-013). Strings in a
new `locales/en-US/plugins.ftl` (≈ 80 keys incl. the 25 permission
explanations and the manifest reasons). Follows the Queue/Effects panel
conventions (rows keyed by id, `fluent_keys.rs`/`accessibility.rs` tests).

## R16. Health, suspension counter, auto-disable, Restart

**Decision**: `PluginRecord` (core) owns `health`, `suspensions_this_
session` (in-memory, +1 on every Active→Suspended, reset only by process
start), `abort_window: VecDeque<Instant>` (60 s) and `warning_until`
(5 min), all driven by `RuntimeEvent`s drained in `tick()` and by the
controller's injectable clock. The third suspension in a session calls
`disable()` and raises `plugin-auto-disabled`. `Restart` and re-enable
create a fresh `PluginContext` on a fresh thread with the same `PluginId`;
`Ready` sets `health = Ok`, resets the abort window, readopts orphaned
nodes (R11).

## R17. Teardown ordering (FR-012) and shutdown

**Decision**: `PluginHost::stop(id, reason)` is one core-side function:

1. send `Control::Unloading { reason }` to the plugin thread (non-blocking);
2. immediately, on the core side and in this order: release focus if held
   (R12), disarm the plugin's armed region, remove its transient markers
   (revision bump → `marker_changed`), orphan its nodes (revision bump →
   `effect_chain_changed`); timers die with the thread (they are thread-
   local) so "cancel timers" needs no message;
3. the plugin thread runs the `unloading` handler under the 4 ms budget,
   flushes dirty state to the writer and waits ≤ 200 ms for the ack, drops
   the Lua state and exits; core keeps the `JoinHandle` and reaps finished
   threads in `tick()`. A thread that is stuck in a native wait is
   detached at `shutdown()` after the same 200 ms + one handler budget.

`PlaybackController::shutdown()` calls `stop(_, Shutdown)` for every
Active/Loaded plugin and waits ≤ 250 ms for all threads before proceeding
with the existing shutdown sequence (spec Clarifications: "the same 200 ms
window before the host proceeds").

## R18. Fixture plugins

**Decision**: eight Luau packages under `plugins/fixtures/` (the spec's
seven "at minimum" plus a flood fixture so the rate-limit scenario is
self-driven): `org.modplayer.fixture.observer` (`playback.observe` only;
counts every event, records position timestamps, and on `ready_ack`
calls `api.transport.seek(0)` to log the expected `permission_denied`),
`…wellbehaved` (all nine operable permissions; drives the full US3 cycle
starting at `ready_ack`), `…flood` (`transport.control`; requests focus
and issues 1 000 `seek`s on its first `play_state_changed(playing)`),
`…hang` (`while true do end` in its `play_state_changed` handler),
`…throw` (`error("boom")` on every `position`), `…leak` (appends 1 MB
strings to a table per `position`), `…noready` (never calls `ready()`),
`…invalid` (`required = ["teleport.everywhere"]`). A `debug_probe(name)`
request (schema-listed, answered only under the fixture flag, never
permission-gated) lets automated tests read a fixture's internal
counters (event counts, last refusal, position timestamps) without a UI;
the manual scenarios read the same figures from the console.

## R19. Logging ("the plugin's console")

**Decision**: the `log` façade (0.4, already in `Cargo.lock` through eframe
and librespot; MIT OR Apache-2.0) becomes a direct dependency of core and
the runtime; every plugin-tagged entry goes through `log::{info,warn,
error}!(target: "plugin:<identifier>", …)` **and** into
`modplayer_core::plugins::PluginLog`, a bounded (1 000-entry) in-memory
ring that tests and the future console UI (002-developer-mode/002) read.
The binary installs a ≤ 30-line stderr `log::Log` at `Info` level so the
manual scenarios can read the app log. No new crate.

## R20. Testing strategy (Constitution VIII)

**Decision**: named tests are listed in each contract and in
[quickstart.md](quickstart.md). Highlights: gateway proptests for manifest
parsing and state serialization; runtime tests for hang-abort within the
budget, aggregate-share suspension, memory suspension, never-ready,
timers, rate limiting; core controller tests (FakeBackend + SyntheticSource)
for the permission matrix, ownership refusals, teardown ordering, three-
suspensions auto-disable, sign-out/shutdown; an engine-continuity test
that renders through the `FakeBackend` on a second thread while the hang
fixture is suspended and asserts no missed callback; UI offscreen tests
for the list, empty state, accessibility and Fluent keys; manual scenarios
M1–M12 executed by the implementing agent (Governance › Manual Scenario
Sign-Off).

## R21. Queue writes from a plugin

**Decision** (assumption; the spec lists "view, reorder, add to, and
remove" without an add source): `queue.add(track_id)` resolves the id
through the current queue's existing `TrackRef`s and the library index
(`LibraryIndex`) and returns `not_found` when neither knows it; `queue.
move`, `queue.remove`, `queue.play_next` take queue item ids from
`queue.list()`/`queue_changed`. No catalog fetch is issued on a plugin's
behalf (no `library.*` capability in this slice).

## R22. Existing debug-toggle and directory conventions reused

- Env toggles: `MODPLAYER_PLUGIN_FIXTURES=1` (discovery, any build),
  `MODPLAYER_PLUGIN_STATE_DIR` (store location; mirrors
  `MODPLAYER_TRACK_STATE_DIR`).
- Directories: `directories::ProjectDirs::from("", "ModPlayer", "ModPlayer")
  .data_local_dir()` as `analysis/`, `track-state/`, `library/` already do.
- Fluent: one new `plugins.ftl` under `locales/en-US/`, registered like
  `effects.ftl`.
- Section rendering: `App::update`'s `Section::Plugins` arm (`app.rs:512`).

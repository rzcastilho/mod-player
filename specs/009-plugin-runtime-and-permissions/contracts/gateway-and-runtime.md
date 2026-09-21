# Contract: Capability Gateway and Plugin Runtime (Rust-side)

Crates: `modplayer-capability-gateway`, `modplayer-plugin-runtime`.
Requirement ids: FR-004, FR-006–FR-012, FR-014, FR-016, FR-020–FR-022,
FR-025, FR-026, FR-028; PL-1.2, PL-3, PL-5.1, PL-8; NFR-2.3, NFR-4.3;
Constitution I, II, VII, VIII, IX. Types are in
[../data-model.md](../data-model.md) §1–§2; decisions in
[../research.md](../research.md) R1–R6, R9, R13.

## 1. Gateway rules (G)

- **G1 Single path**: the only way a plugin's request reaches the host is
  `Gateway::admit(kind, now)` → `Ok` → either a local capability (timers,
  state, focus, snapshot reads) or an `RpcEnvelope` on the host's request
  channel. No other channel exists between a plugin thread and core.
- **G2 Check order**: `admit` evaluates permission (`requires()` ∈ grants,
  else `permission_denied`/`not_granted`), then focus (`needs_focus()` ⇒
  `FocusToken::holder() == Some(me)`, else `no_focus`), then the rate
  limiter for `category()` (101st in a rolling second ⇒ `rate_limited`).
  Per-call validation happens after `admit`, host-side for RPC calls.
- **G3 Refusals are values**: every failure is `Err(Refusal)`; the crate
  has no `panic!`, `unwrap`, `expect`, `unreachable!` outside tests
  (`#![deny(clippy::unwrap_used, clippy::expect_used)]`, `#![forbid(unsafe_code)]`).
- **G4 Grants are immutable** for the life of a `Gateway` (bundled: fixed
  at load); the type has no setter.
- **G5 Rate window**: `RateLimiter::admit` evicts timestamps older than
  1 s before counting, records only admitted calls, and is per plugin per
  category; the five categories are the generated
  `RequestKind::category()` values.
- **G6 Storage cap**: `PluginStateStore::set` rejects before mutating when
  `key.len() > 256` (`key_too_long`), `to_vec(value).len() > 1 MiB`
  (`value_too_large`) or `used_bytes − old + new > 10 MiB`
  (`storage_cap`); all three are `budget_exceeded`. Values must be JSON
  (`serde_json::Value`, no NaN/inf — rejected as `invalid_argument`).
- **G7 Persistence**: `encode` output is deterministic (sorted keys,
  pretty JSON); the writer applies `.tmp → write_all → sync_all → rename`
  and never leaves a partial file; a `WriteJob` with an `ack` sender is
  acknowledged after the rename.
- **G8 Schema is the truth** (Constitution IX): `build.rs` fails the
  build if `api/v1.toml` names a permission a request requires that is not
  in the catalog, a category outside the five, or an event with an unknown
  permission; `tests/api_reference.rs` fails when `docs/plugin-api/v1.md`
  is stale.

## 2. Runtime rules (RT)

- **RT1 One thread, one state**: `PluginHandle::spawn(package, grants,
  deps)` creates an OS thread named `plugin:<identifier>` that owns the
  `mlua::Lua`; the state never leaves the thread; the handle exposes only
  the `inbox`, the `gauges` and the `JoinHandle`.
- **RT2 Sandbox at creation**: `Lua::new_with(STRING|TABLE|MATH|BIT32|
  UTF8, LuaOptions::new().catch_rust_panics(true))`, `set_memory_limit
  (64 MiB)`, `set_interrupt(budget check)`, install `api`, then
  `sandbox(true)` so globals/builtins are read-only before the entry
  script runs. No `require`, `loadstring`, `io`, `os`, `debug`.
- **RT3 Handler invocation**: `run_handler(name, payload)` arms a 4 ms
  budget of the plugin thread's own *CPU time* (FR-009), calls the Lua
  function, and maps the result: `Ok` → sample recorded;
  `Err(RuntimeError|CallbackError)` with the budget marker →
  `HandlerAborted{Deadline}`; any other `Err` →
  `HandlerAborted{Exception(text)}` (logged at `error`);
  `Err(MemoryError)` → `Suspended{Memory}`. The Lua state is reused after
  a Deadline/Exception abort (the error unwound cleanly). Mechanically the
  interrupt callback compares a cheap wall-clock trigger (`now + 4 ms`,
  pushed out by any RPC wait per RT7); only once that has passed does it
  read the thread CPU clock, and it re-arms the trigger from the CPU still
  unspent rather than aborting a handler that was merely descheduled or
  blocked. Time off-CPU is never charged.
- **RT4 Aggregate share**: after every handler completion or abort, its
  thread-CPU duration is recorded; `samples` older than 1 s are evicted;
  if the sum exceeds 100 ms → `Suspended{CpuShare}` (or `Hang` when the
  last sample was itself a Deadline abort) and the thread proceeds to RT8
  with reason `Suspend`.
- **RT5 Ready gate**: no `HostEvent` is dispatched to handlers before
  `api.ready()`; inbound events before it are held (up to the inbox cap);
  if `ready()` has not been called 5 s after context creation →
  `Suspended{DidNotStart}`, thread exits without `unloading`. On
  `ready()` the thread emits `RuntimeEvent::Ready` and dispatches
  `ready_ack` first.
- **RT6 Scheduler wait**: `recv_timeout(min(next timer, next position
  tick, next meter tick, ready deadline))`; position/meter samples read
  `RtShared` on this thread (research R5) and never touch the RT thread's
  data structures other than the published atomics.
- **RT7 RPC**: an admitted RPC sends `RpcEnvelope` and blocks on
  `reply.recv_timeout(1 s)`; the wait length is added to the handler
  deadline and excluded from the aggregate sample; timeout ⇒
  `invalid_state`/`host_busy`. The host's reply is `Result<Response,
  Refusal>`.
- **RT8 Unloading**: on `Control::Unloading{reason}` (or a self-initiated
  suspend) the thread runs the `unloading` handler under RT3, sends a
  `WriteJob` per dirty scope with an `ack`, waits ≤ 200 ms for the acks,
  drops the Lua state, emits `RuntimeEvent::Exited` and returns. Pending
  timers die with the thread.
- **RT9 Timers**: `set_timeout/set_interval` ≥ 1 ms; ≤ 256 pending across
  all three kinds (`invalid_state`/`timer_limit`); `clear(handle)` on an
  unknown or already-fired handle ⇒ `not_found`; `schedule_at_position`
  fires once when a position sample is `≥ target` (payload = the sampled
  position); a `track_generation` change cancels all position timers
  silently; timers fire only in `Active`.
- **RT10 Position subscription**: rate clamped to `1..=60`, default 10;
  an event is delivered only when the sampled position differs from the
  last delivered one; scheduler wait accuracy target ≤ 1 ms on
  macOS/Linux (test `position_jitter_under_5ms`), Windows measured
  manually (research R5 risk).
- **RT11 Track state restore**: on `HostEvent::TrackChanged{track}` the
  thread first loads the new track's scope into the store, then delivers
  `track_changed`. The previous track scope is flushed (dirty ⇒
  `WriteJob`) before the load. Because the writer is asynchronous, the
  thread keeps the last few flushed track scopes in memory and restores
  from that record when the same track comes straight back, so a return
  never depends on the write having landed. Otherwise the bytes come from
  `<tracks>/<hex(track)>.json`; a file larger than `budgets.storage`
  (which the store enforces on every write, so it cannot be the store's
  own) is refused unread ⇒ `HandlerAborted{RestoreTimeout}` and an empty
  track scope. A read that merely took long is never discarded after the
  fact — that would only lose state without bounding anything.
- **RT12 Faults never escape**: the thread body is wrapped in
  `std::panic::catch_unwind`; a panic (which `mlua` with
  `catch_rust_panics` already converts) becomes `Suspended{Hang}` +
  `Exited`. A `JoinHandle` that finishes with `Err` is treated the same by
  core. Nothing in either crate touches the audio thread (Constitution I:
  the runtime reads `RtShared` atomics only).
- **RT13 Gauges**: after every handler the thread stores
  `used_memory()` and the trailing-1 s CPU sum as `permille of the 100 ms
  share` into `PluginGauges`; the list reads them lock-free.

## 3. Core-side apply contract (what the controller promises the runtime)

- **C1** `drain_plugin_requests()` runs first in `tick()`, handles every
  queued `RpcEnvelope` in arrival order, and always replies (a dropped
  reply is a bug caught by `rpc_always_replied`).
- **C2** Per-call validation uses core's models; ownership before
  existence when both fail? No — an id that does not exist is
  `not_found` even if the caller owns nothing (spec FR-008: "an identifier
  that does not or no longer exists MUST return not_found"); ownership of
  an existing id ⇒ `not_owner`.
- **C3** After any RPC that mutated markers or the chain, the controller
  bumps the model revision; `tick()` republishes `PluginSnapshot` and
  fans out `marker_changed`/`effect_chain_changed` once per tick.
- **C4** `fan_out(event)` filters by `EventKind::requires()` against each
  Active plugin's grants and `try_send`s; a full inbox drops the event and
  logs once per plugin per second.
- **C5** `RuntimeEvent`s are drained in `tick()` after requests
  (`drain_plugin_runtime_events`), updating `PluginRecord` (R16) and
  raising notifications (R14).

## 4. Tests (named; Constitution VIII "plugin isolation (crash, hang,
memory), permission enforcement")

gateway crate — `tests/gateway.rs`: `admit_checks_permission_before_focus`,
`admit_checks_focus_before_rate`, `refused_calls_consume_no_quota`,
`rate_limit_101st_in_window`, `rate_limit_window_slides`;
`tests/state_store.rs` (+ proptest `state_roundtrip`):
`set_rejects_long_key`, `set_rejects_large_value`, `cap_is_atomic`,
`encode_is_deterministic`, `writer_is_atomic_and_acks`;
`tests/api_reference.rs`: `reference_is_current`.

runtime crate — `tests/isolation.rs`: `infinite_loop_handler_aborts_within_budget`
(asserts abort ≤ 4 ms + 2 ms slack and the state is reusable),
`repeated_hang_suspends_within_one_second`, `memory_bomb_suspends_plugin_not_host`,
`exception_aborts_handler_only`, `never_ready_suspends_after_5s` (injected clock),
`panic_in_binding_is_contained`; `tests/scheduler.rs`: `events_in_order`,
`slow_handler_delays_only_own_events`, `position_rate_clamped_and_coalesced`,
`position_silent_while_paused`, `position_jitter_under_5ms` (macOS/Linux),
`schedule_at_position_fires_once_with_actual_position`,
`track_change_cancels_position_timers`, `timer_limit_257`,
`clear_unknown_timer_not_found`, `rpc_wait_excluded_from_cpu`,
`rpc_timeout_is_host_busy`, `unloading_writes_committed_within_200ms`,
`track_state_restored_before_track_changed`, `restore_timeout_delivers_event_anyway`.

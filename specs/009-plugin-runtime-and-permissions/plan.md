# Implementation Plan: Plugin Runtime, Sandbox, and Permission Gateway

**Branch**: `009-plugin-runtime-and-permissions` | **Date**: 2026-09-19 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/009-plugin-runtime-and-permissions/spec.md`

## Summary

Let small scripted plugins act on the music while it plays without ever
being able to take the player down. A plugin is a folder (`plugin.toml`
manifest, `main.luau` entry script, readme) whose manifest is validated
against the 25-entry permission catalog before anything runs; in this
slice the only source is **bundled** (embedded in the binary, enabled by
default, permissions pre-approved) and no production plugin ships — eight
fixture plugins appear only under `MODPLAYER_PLUGIN_FIXTURES=1`. Each
enabled plugin runs a **Luau** state (`mlua` 0.12, research R1, ADR 0001)
on its **own OS thread** with a sandboxed standard library, a 64 MB
allocator-enforced heap cap, a 4 ms per-handler CPU budget enforced by
Luau's interrupt hook, a 10 %-of-one-core rolling share, 10 MB of
key-value storage, and its own scheduler and timers; nothing it does ever
touches the audio thread. Every request goes through one **Capability
Gateway** (permission → transport focus → rate limit → per-call
validation) and returns success or a `{ code, reason, message }` refusal
from a closed six-code set. Capabilities: observe playback (coalesced
`position` up to 60/s), control transport and queue under a single
global focus holder, own and edit markers/loops/cues (`transient`
markers, `not_owner` for others'), create and parameterize own effect
nodes (suggested position honored on insertion; orphaned, never removed,
on unload; re-adopted on `ready()`), meters, plugin- and per-track state
(restored before `track_changed`), and timers including
`schedule_at_position`. A misbehaving plugin is suspended within its
budget window with a keyed non-blocking notice (Restart / Disable), three
suspensions in a session auto-disable it, and the Plugins section lists
name, version, source, enabled, health, permission summary and live
CPU/memory with a one-action enable/disable and no uninstall.

Technical approach (details in [research.md](research.md)): two new
constitution-named crates — `modplayer-capability-gateway` (the single
machine-readable API definition `api/v1.toml` generating the permission
catalog, request/event tables and the reference doc; manifest validator
with proptests; `Refusal`, `Grants`, `RateLimiter`, `PluginStateStore`)
and `modplayer-plugin-runtime` (Luau context, budgets, per-plugin
scheduler thread, Lua API binding, timers, position/meter pump reading
`RtShared`) — plus a `plugins` service in `modplayer-core` (discovery,
lifecycle state machine, health/suspension accounting, request
application against the controller's authoritative models over a
synchronous RPC channel woken through a new `set_waker` hook, event
fan-out, notifications, list view) and a `plugins_view` in the UI. The
marker `Owner` and effect `NodeOwner` fields gain real plugin values, the
marker store learns to skip transient markers, and notifications gain a
per-plugin dedupe key.

Headless assumptions (each with its rejected alternative in Complexity
Tracking): Luau over WASM/Rhai; synchronous RPC to the controller rather
than shared models; embedded bundled packages; an eighth "flood" fixture;
`queue.add` resolving through the queue and library index only; the
`log` façade plus an in-memory ring as "the console".

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–008; plugin scripts are Luau (`mlua` 0.12 vendored build, C++ via `cc`)

**Primary Dependencies**: existing eframe/egui 0.36 (+accesskit), cpal 0.18, rtrb 0.4, fluent-templates 0.15, serde/serde_json 1, toml 0.9, directories 6, thiserror 2, proptest 1 (dev). **New**: `mlua = "0.12"` with features `luau`, `send`, `serialize` (runtime crate only; MIT; research R1) and the `log = "0.4"` façade (already in `Cargo.lock` transitively; core, runtime, binary; research R19). No other new crate.

**Storage**: per-plugin key-value files under `<data_local_dir>/ModPlayer/plugin-state/<hex(identifier)>/{plugin.json, tracks/<hex(track)>.json}` (`MODPLAYER_PLUGIN_STATE_DIR` override), pretty JSON, atomic `.tmp → sync → rename`, 500 ms debounce, 10 MB cap per plugin (research R9); marker files (006) now carry plugin owner identifiers and never contain transient markers; no settings or secure-store change

**Testing**: `cargo test --workspace` — gateway unit/proptests (manifest, state serialization, admission, rate limiter), runtime isolation/scheduler tests with real Luau states (hang, memory, exception, never-ready, timers, jitter), core controller tests on `FakeBackend` + `SyntheticSource`/`ScriptedHost` using the embedded fixture plugins and `debug_probe`, UI offscreen `egui::Context` tests, `single_dependent.rs` and `decoded_store_boundary.rs` unchanged; API-reference staleness test (Constitution IX); manual scenarios M1–M12 in [quickstart.md](quickstart.md); CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers) on ubuntu / macos / windows

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical plugin API and behaviour (Luau interpreter, no JIT); one known platform variance risk (scheduler wait accuracy on Windows for the ≤ 5 ms position jitter, research R5) measured in M6

**Project Type**: Desktop application — Cargo workspace grows from 11 to **13 crates** (`crates/modplayer-capability-gateway`, `crates/modplayer-plugin-runtime`)

**Performance Goals**: a hanging handler is aborted ≤ 4 ms + interrupt granularity after its deadline and a looping plugin suspended ≤ 1 s (SC-001); audio callbacks unaffected by any plugin state (zero missed callbacks in the continuity test, SC-001/SC-006/SC-009); `position` delivered at the requested rate 1–60/s with ≤ 5 ms jitter (NFR-1.9) on macOS/Linux; Gateway RPC round-trip ≤ one UI frame when idle (woken repaint, ≈ 1–3 ms), 100 admitted calls/s/category with the 101st refused in O(1); plugin list gauges refreshed every 500 ms; interrupt check cost ≈ 25 ns per Luau loop iteration/call

**Constraints**: Constitution I — neither new crate touches the engine crate or the RT thread; the runtime reads `RtShared` atomics only; Constitution II — one Gateway path, refusals are values, no `panic!`/`unwrap`/`expect` outside tests, `#![forbid(unsafe_code)]` in both new crates (mlua's API is safe); Constitution V — meters reach plugins as numbers (`RtShared` levels/spectrum), never samples; Constitution IX — `api/v1.toml` is the single source, reference doc generated and checked; spec-fixed numbers (4 ms / 10 % / 64 MB / 10 MB / 5 s / 200 ms / 100 per s / 10 per s default / 3-in-60 s → 5 min / 256 timers / 256 B keys / 1 MB values) unchanged; every new control keyboard-operable and accessibly named; strings externalised (`plugins.ftl`); no `ui.*`, `network`, `files.*`, approval sheet, focus arbitration policy, sideload or registry

**Scale/Scope**: gateway ≈ 2 100 LOC (schema + build.rs 300, generated tables, manifest 450, refusal/request/event types 450, grants/limiter/focus 200, state store + writer 450, tests 600 incl. proptests); runtime ≈ 2 600 LOC (context/sandbox 350, budgets 250, scheduler + timers + pump 700, Lua bindings 900, tests 700); core ≈ 2 300 LOC (`plugins/` host/record/view/bundled/log 1 300, controller delta 500, markers/effects/notifications deltas 300, tests 1 400); ui ≈ 450 LOC + 300 test; 8 fixture packages ≈ 40 Luau lines each; 1 new `.ftl` (≈ 80 keys); 1 ADR; 1 generated API reference; 25 permissions, ≈ 40 requests (30 RPC + local state/timer/log/subscription calls), 14 events in the schema

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | **Yes** (indirectly) | ✅ PASS | No engine or effects-crate change: plugin nodes use 008's existing `NodeOwner::Plugin` path and commands; the runtime never calls into the RT thread — it reads `RtShared` atomics (`PositionClock`, levels, spectrum) exactly as the UI does (research R5). Plugin code runs on plugin threads only; Gateway applies run on the controller (UI) thread and reach the engine only through the existing `Command` SPSC queue at buffer boundaries. `tests/realtime.rs` stays untouched and green; the continuity test renders through `FakeBackend` while the hang fixture is suspended (FR-004, FR-026). |
| II. Plugins Are Guests (non-negotiable) | **Yes** | ✅ PASS | One `Gateway::admit` path (permission → focus → rate limit) in a dedicated crate; refusals are `Err(Refusal)` values (G3), never panics; per-plugin CPU (4 ms handler via `set_interrupt`, 10 %/1 s aggregate), memory (`set_memory_limit`, allocator-enforced), storage (10 MB) budgets; sandboxed Luau with no ambient authority (RT2); a fault (exception, hang, memory, panic in a binding) is contained to that plugin thread (RT3, RT12) and becomes a logged abort, a suspension or a refusal (FR-026). The runtime choice is recorded with rationale in [docs/adr/0001-plugin-runtime-luau.md](../../docs/adr/0001-plugin-runtime-luau.md) **before** the crate is implemented; linking it from the constitution's TODO is a PATCH task. |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | Plugins only compose host primitives: markers/loops/cues (006 `TrackMarkers`), effect nodes (008 `ChainModel`/catalog), transport/queue (003), per-track state (this slice's store); no DSP is exposed to scripts (`audio.process` out of scope), and `create_node` only instantiates 008's built-in kinds. |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No source crate is touched; every automated test runs on `SyntheticSource`/`ScriptedHost` + `FakeBackend`; `single_dependent.rs` unchanged. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | **Yes** | ✅ PASS | `meter` events carry only the `f32` gauges already published in `RtShared` (peak/RMS/64 bands); no request or event type contains sample data; `decoded_store_boundary.rs` gains `plugin_crates_expose_no_sample_sink` over both new crates' public items. The plugin state store is JSON key-value only (no audio, so unencrypted per spec FR-014). |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | Plugins never see credentials (no `network`, `files.*`, `library.*` capability; the sandbox has no `io`/`os`; the `SecureStore` is unreachable from both new crates by dependency graph). Bundled packages are embedded in the signed binary, so signature/digest verification of downloaded packages (NFR-4.4) is deferred with the registry slice, not weakened. No telemetry; the console is a local log. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | Two new crates for two constitution-named components; `#![forbid(unsafe_code)]` and `deny(clippy::unwrap_used, expect_used)` in both; `thiserror` errors (`ManifestError`, `StoreError`, `RuntimeError`); doc examples on public items (`Manifest::parse`, `Gateway::admit`, `PluginStateStore::set`); MSRV unchanged (mlua needs 1.88); `cargo deny check` covers mlua/mlua-sys/luau0-src (MIT) and `log` (MIT OR Apache-2.0); CI matrix unchanged, C++ toolchain present on all three images; licence headers on every new file. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Test-first for the public behaviour named in the contracts; **plugin isolation (crash, hang, memory) and permission enforcement** tests are explicit (`isolation.rs`, `controller_plugins_permissions.rs::matrix_…`); proptests for manifest parsing and state serialization (`round_trip`, `state_roundtrip`); jitter test for NFR-1.9; every bug found in the manual walk ships with a regression test; manual scenarios M1–M12 executed by the implementing agent (Governance › Manual Scenario Sign-Off). No new real-time crate, so no new criterion bench/soak obligation. |
| IX. One Plugin API Definition | **Yes** | ✅ PASS | `crates/modplayer-capability-gateway/api/v1.toml` is the single machine-readable definition; `build.rs` generates the manifest validator's catalog and the runtime call checker's tables; `tests/api_reference.rs` regenerates/checks `docs/plugin-api/v1.md` so docs and runtime cannot diverge; API version 1.0, minor additions only later (`define_section`, `serialize_state` explicitly absent). Every API change in a PR requires the written change request the constitution asks for (the PR template note is a task). |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No trait with a single implementor (the runtime is concrete; `Request`/`HostEvent` are enums); no feature flag (the fixture toggle is a runtime env var like `MODPLAYER_LIBRARY_FIXTURE`); each new crate states why `std`/existing deps are insufficient (an interpreter and a constitution-named boundary); behaviour and API identical across platforms (interpreter, no JIT; the Windows jitter risk is measured, not designed around); every list control keyboard-operable with an accessible name; strings in `plugins.ftl`. **User override**: the host user's transport actions ignore plugin focus, `L` re-arms over a plugin loop, any plugin is disabled in one click from the list or the notice (FR-024, SC-008). |
| Governance: engine/gateway/runtime sign-off | **Yes** | ✅ PASS | `crates/modplayer-capability-gateway/` and `crates/modplayer-plugin-runtime/` are Capability Gateway / Plugin Runtime code and require the area maintainer's sign-off on every PR (GOV-3.2); no engine change is planned. Requirement ids (FR-, SC-, PL-, DM-, EC-, NFR-, AR-, C-) are referenced throughout spec, plan, contracts and test names. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no `unsafe`, no feature
flag, no trait, one runtime dependency that is itself the constitution-
sanctioned runtime choice (recorded in ADR 0001), two crates that are
constitution-named components; the RT path is untouched; the single-
gateway, values-not-panics and native-budget rules are structural (G1–G3,
RT2–RT4, RT12); the API schema is the one source of truth; every headless
assumption (Complexity Tracking) sits outside the non-negotiable
principles.

## Project Structure

### Documentation (this feature)

```text
specs/009-plugin-runtime-and-permissions/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R22 with evidence from the code and mlua sources
├── data-model.md        # Phase 1: gateway/runtime/core types, lifecycle state machines, model deltas, on-disk formats
├── quickstart.md        # Phase 1: automated gates (named suites), fixture launch, manual scenarios M1–M12
├── contracts/
│   ├── plugin-api-v1.md         # the Luau-facing API object: requests, events, refusal shape, ownership rules
│   ├── manifest.md              # package layout, plugin.toml schema, validation rules and reasons, grants
│   ├── gateway-and-runtime.md   # Rust-side gateway rules G1–G8, runtime rules RT1–RT13, core apply contract C1–C5, tests
│   ├── plugin-host-service.md   # controller façade, lifecycle rules L1–L10, request application, event fan-out, notifications, tests
│   └── ui-plugins.md            # Plugins section layout, notifications, accessibility, Fluent keys, tests
├── checklists/requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)

docs/adr/0001-plugin-runtime-luau.md   # Constitution II ADR (written in this phase)
docs/plugin-api/v1.md                  # generated API reference (created by the gateway crate's test task)
```

### Source Code (repository root)

Existing layout (001–008) is kept; `+` marks new files, `~` modified files.

```text
Cargo.toml                                   ~ workspace.dependencies += mlua 0.12 (luau, send, serialize), log 0.4
deny.toml                                    ~ comment noting mlua/mlua-sys/luau0-src (MIT) if `cargo deny check` needs it
docs/adr/0001-plugin-runtime-luau.md         + ADR (this phase)
docs/plugin-api/v1.md                        + generated reference (tests/api_reference.rs)
plugins/
├── bundled/                                 + (empty this slice; 012/013 add packages)
└── fixtures/                                + eight packages: observer, wellbehaved, flood, hang, throw, leak, noready, invalid
    └── <name>/{plugin.toml, main.luau, README.md}
locales/en-US/
├── plugins.ftl                              + list, health, permissions ×25, manifest errors ×8, notifications (≈ 80 keys)
└── app.ftl                                  ~ − placeholder-plugins
crates/
├── modplayer-capability-gateway/            + NEW CRATE (Constitution VII "capability-gateway"; research R2, R6)
│   ├── Cargo.toml                           + deps: serde, serde_json, toml, thiserror; build: toml; dev: proptest
│   ├── build.rs                             + parses api/v1.toml → OUT_DIR/generated.rs (Permission, RequestKind, EventKind tables)
│   ├── api/v1.toml                          + THE API DEFINITION: catalog, requests, events, refusal codes, version
│   ├── src/lib.rs                           + forbid(unsafe_code); pub mod api, manifest, refusal, request, event, grants, limiter, focus, state, budgets
│   ├── src/api.rs                           + include!(generated.rs) + ApiVersion, HOST_CAPABILITIES
│   ├── src/manifest.rs                      + Manifest, ManifestDto, Version, ApiRange, SuggestedPosition, ManifestError, parse/validate
│   ├── src/refusal.rs                       + RefusalCode, Refusal (constructors per reason), PluginResult
│   ├── src/request.rs                       + Request, Response, MarkerInfo, NodeInfo, QueueItemInfo, OwnerInfo, id newtypes
│   ├── src/event.rs                         + HostEvent, UnloadReason, PlayState, TrackInfo, LevelInfo
│   ├── src/grants.rs                        + GrantState, PermissionGrant, Grants
│   ├── src/limiter.rs                       + RateCategory, RateLimiter
│   ├── src/focus.rs                         + FocusToken
│   ├── src/gateway.rs                       + Gateway::admit (permission → focus → rate)
│   ├── src/state/{mod,store,paths,writer}.rs + PluginStateStore, PluginStatePaths, StateWriter
│   ├── src/budgets.rs                       + Budgets::DEFAULT
│   └── tests/{manifest.rs, gateway.rs, state_store.rs, api_reference.rs}
├── modplayer-plugin-runtime/                + NEW CRATE (Constitution VII "plugin-runtime"; research R1, R3–R5)
│   ├── Cargo.toml                           + deps: modplayer-capability-gateway, modplayer-engine, mlua, serde_json, log
│   ├── src/lib.rs                           + forbid(unsafe_code); pub mod context, budget, scheduler, bindings, timers, pump, handle, events
│   ├── src/context.rs                       + PluginContext::new (StdLib subset, memory limit, interrupt, api table, sandbox), load entry
│   ├── src/budget.rs                        + BudgetState, deadline/interrupt check, samples window, PluginGauges
│   ├── src/bindings/{mod,playback,transport,queue,markers,effects,state,timers,log}.rs + Lua ↔ Request/Refusal, exhaustive over RequestKind::ALL
│   ├── src/timers.rs                        + TimerSet, TimerHandle
│   ├── src/pump.rs                          + position/meter sampling from RtShared + PlaybackSnapshot
│   ├── src/scheduler.rs                     + thread loop: inbox, ready gate, handlers, RPC, unloading/draining
│   ├── src/handle.rs                        + PluginHandle::spawn, Inbound, Control, RuntimeDeps, RpcEnvelope
│   ├── src/events.rs                        + RuntimeEvent, AbortCause, SuspendCause, PlaybackSnapshot
│   └── tests/{isolation.rs, scheduler.rs, bindings.rs}
├── modplayer-core/
│   ├── Cargo.toml                           ~ + modplayer-capability-gateway, modplayer-plugin-runtime, log
│   ├── src/lib.rs                           ~ pub mod plugins; re-exports (PluginsView, PluginRow, Health, PluginId…)
│   ├── src/plugins/{mod,host,record,bundled,view,log,apply,fanout}.rs + PluginHost, PluginRecord/Lifecycle, embedded packages, PluginsView, PluginLog, request application, event fan-out
│   ├── src/markers/model.rs                 ~ Owner::Plugin, MarkerError::NotOwner, revision, *_owned methods, transient removal, disarm_if_owned_by
│   ├── src/markers/store.rs                 ~ owner identifier string, skip transient on encode, intern on load
│   ├── src/effects/model.rs                 ~ revision, add_at, orphan_owned_by, readopt, owned_by, resolve_position
│   ├── src/notifications.rs                 ~ dedupe_key, raise_keyed, dismiss_by_dedupe, RestartPlugin/DisablePlugin, 2 keys
│   ├── src/controller.rs                    ~ plugins field + façade, set_waker, tick() drains, dispatch/queue/intent hooks → fan-out, PlaybackSnapshot upkeep, sign-out/shutdown, last_marker_actor
│   └── tests/{plugins_manifest_discovery.rs +, controller_plugins_lifecycle.rs +, controller_plugins_permissions.rs +, markers_model.rs ~, markers_store.rs ~, notifications.rs ~, controller_effects.rs ~}
├── modplayer-ui/
│   ├── Cargo.toml                           ~ + modplayer-capability-gateway (Permission::explanation_key)
│   ├── src/lib.rs                           ~ pub mod plugins_view
│   ├── src/plugins_view.rs                  + Plugins section (rows, toggle, health, permissions, gauges, empty state)
│   ├── src/app.rs                           ~ Section::Plugins → plugins_view::show; install waker (ctx.request_repaint) on the controller
│   ├── src/shell.rs                         ~ − plugins_placeholder
│   ├── src/notifications.rs                 ~ RestartPlugin/DisablePlugin buttons
│   └── tests/{plugins_view.rs +, accessibility.rs ~, fluent_keys.rs ~}
└── modplayer/
    ├── Cargo.toml                           ~ + log
    ├── src/main.rs                          ~ stderr log::Log (Info) install; nothing else (fixture env read in core)
    └── tests/decoded_store_boundary.rs      ~ + plugin_crates_expose_no_sample_sink
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
(one crate per architectural component, Constitution VII) and add **two
crates**, `crates/modplayer-capability-gateway` and
`crates/modplayer-plugin-runtime`, the two plugin-subsystem components the
constitution names (AR-16 Plugin Runtime, AR-17 Capability Gateway;
research R2). The gateway crate has no workspace dependency so its API
definition, manifest validator and state store are testable alone and
reusable by the later registry tooling; the runtime crate depends on the
gateway and on `crates/modplayer-engine` (for `RtShared`/`PositionClock`
reads only); `crates/modplayer-core` hosts the plugin service beside the
`markers/`, `effects/` and `queue.rs` models it composes; the UI section
lives in `crates/modplayer-ui/src/plugins_view.rs` next to the
Queue/Effects panels it mirrors. Plugin packages live in a top-level
`plugins/` folder (`bundled/`, `fixtures/`) embedded at compile time, and
the ADR in `docs/adr/`. The dependency graph becomes
`modplayer → {ui, core, audio-io, account, secure-store, audio-source-connect}`;
`ui → {core, gateway, audio-io, engine, effects, account, secure-store, audio-source}`;
`core → {gateway, runtime, engine, effects, audio-io, audio-source, audio-source-synthetic}`;
`runtime → {gateway, engine}`; `gateway → {}`. The engine, effects,
source, receiver, audio-io, account and secure-store crates are not
modified.

## Design notes that tasks must respect

1. **Schema first** (R6, Constitution IX): `api/v1.toml` and `build.rs`
   land before any binding or validator; the runtime's dispatcher is an
   exhaustive `match` over `RequestKind`, so adding a request without a
   binding fails to compile; `docs/plugin-api/v1.md` is regenerated by
   the test, never hand-edited.
2. **ADR before crate** (Constitution II): `docs/adr/0001-plugin-runtime-
   luau.md` exists now; the first runtime task links it from the
   constitution (PATCH bump, Sync Impact Report regenerated).
3. **One thread per plugin, one state per thread** (RT1); the `Lua` value
   never crosses a channel; core sees only `PluginHandle`.
4. **Check order is fixed** (G2): permission → focus → rate limit on the
   plugin thread, then per-call validation host-side; refused calls
   consume no quota and do no host work.
5. **CPU is wall-clock minus RPC waits** (R4, RT7); the aggregate window
   is re-evaluated after every handler; memory breaches suspend
   immediately.
6. **`ready()` gates everything** (RT5): no handler runs before it; the
   first event after it is `ready_ack`; 5 s without it suspends with
   `did_not_start`.
7. **Teardown order is FR-012's** (L7): focus → loop → transient markers
   → (timers die with the thread) → orphan nodes; `unloading` runs under
   4 ms, writes commit within 200 ms; nodes are never removed by the host.
8. **Re-adopt on `ready()`** (L4): orphaned nodes whose owner id matches
   return to the plugin with the same `NodeId`s.
9. **Ownership before nothing, existence first** (C2): an id that does
   not exist is `not_found`; an existing id of another owner is
   `permission_denied`/`not_owner`; the host UI is never ownership-gated.
10. **Coalesced change events** (R10, R11, C3): `marker_changed` and
    `effect_chain_changed` are one per tick from revision counters;
    `PluginSnapshot` is republished in the same tick so a plugin that
    calls `list_*` right after the event sees the new state.
11. **Position/meter are produced on the plugin thread** (R5, RT10):
    default 10/s, clamped 1–60, delivered only when changed; controller
    keeps `PlaybackSnapshot` (intent, source rate, track generation)
    current.
12. **Fixtures are real packages** (R8, R18): eight folders under
    `plugins/fixtures/`, embedded with `include_str!`, discovered only
    when `MODPLAYER_PLUGIN_FIXTURES=1`; `debug_probe` answers only then.
13. **Notifications are keyed per plugin** (R14): `raise_keyed` replaces
    a live notice with the same dedupe key; Restart/Disable actions call
    the façade; `plugin-auto-disabled` has no actions.
14. **Sign-out clears `state.track` only** (R9, FR-014); shutdown waits
    ≤ 250 ms for all plugin threads (R17).
15. **Spec numbers are constants in one place** (`Budgets::DEFAULT`,
    `RateLimiter::LIMIT`, `MAX_TIMERS`, store bounds) and are asserted
    by tests against the spec's values.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. Headless assumptions and plan-level decisions
are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Luau via `mlua` 0.12 as the runtime (R1, ADR 0001) | Preemptive `set_interrupt`, allocator-enforced `set_memory_limit`, `sandbox(true)`, one state per thread — every Constitution II requirement natively; text scripts fit "small scripted plugins" and hot reload. | `wasmtime` — ≈ 50 crates, Cranelift build time, plugins need a compiler; Rhai — no per-engine heap cap; Lua 5.4 — no interrupt hook or sandbox mode. |
| Two new crates (gateway, runtime) instead of one (R2) | Constitution VII names both components; the gateway (API schema, validator, store) must be testable and reusable without a Lua VM or the controller. | One `modplayer-plugins` crate — collapses the boundary the constitution draws between "enforces permissions" and "executes scripts". |
| Synchronous RPC from plugin threads to the controller, woken by `set_waker` (R3) | The controller is the single shadow-state authority and allocates `MarkerId`/`NodeId`; a plugin must get ids back synchronously; the UI thread is woken immediately so latency is a frame, not the 33 ms ticker. | Shared `Arc<Mutex<…>>` models — a 3 500-line controller refactor across 006–008 paths; handlers on the UI thread — up to 100 ms/s of jank per plugin by design. |
| CPU clock paused during RPC waits; 1 s RPC timeout → `invalid_state`/`host_busy` (R3, R4) | The spec excludes asynchronous host work from a plugin's CPU; a blocked UI thread (window drag on Windows) must not hang a plugin forever. | Counting the wait — punishes plugins for host latency; no timeout — an uninterruptible native wait inside a handler. |
| Position/meter sampled on the plugin thread from `RtShared` (R5) | Meets 60/s ≤ 5 ms jitter and "a slow handler delays only its own events" without a controller-cadence dependency. | Controller-pushed `position` — ≤ 30 Hz, frame-timed jitter; a shared clock thread — an extra thread that still needs per-plugin queues. |
| `api/v1.toml` + `build.rs` + reference-staleness test (R6) | Constitution IX demands one machine-readable definition generating validator, call checker and reference. | A Rust `const` table as truth — reference doc would be hand-written and drift. |
| Bundled/fixture packages embedded with `include_str!` from `plugins/` (R8) | "Shipped with the host" on three platforms without an install-time file layout; folders remain real files for FR-001 and for 002-developer-mode. | Reading a folder next to the executable — packaging work that belongs to the sideload/registry slice. |
| Eighth "flood" fixture (R18) | Makes the 1 000-seeks acceptance scenario self-driven and repeatable in tests and manual runs; the spec's list is "at minimum". | Triggering the flood through a developer keybinding — a UI surface the spec excludes. |
| Ungranted namespaces present on `api`, returning `permission_denied` (contract §1) | Spec US2 acceptance 1 requires the refusal, not a Lua error; also lets a plugin discover its grants uniformly. | Absent namespaces — a `nil` call error is not a structured refusal. |
| `PluginId(u16)` interner shared by marker and node owners; owner identifier strings in marker files (R10) | Keeps `Owner`/`NodeOwner` `Copy` (006/008 code constructs them by value) while persisting a stable identity across sessions. | String owners in the models — breaks `Copy` and 006's tests; numeric ids in files — unstable across launches. |
| `marker_changed`/`effect_chain_changed` coalesced per tick from revision counters (R10, R11) | One choke point instead of instrumenting 15 host methods; PL-5 does not require per-operation events. | Per-mutation emission — easy to miss a path; per-mutation payload diff — more data for no consumer this slice. |
| `queue.add(track_id)` resolves via queue + library index only (R21) | The spec names "add to" without a source and no `library.*` capability exists this slice. | A catalog fetch on the plugin's behalf — network work outside the plugin's permissions. |
| `log` façade + in-memory `PluginLog` ring + stderr logger in the binary (R19) | The spec's "console" is "the host application log" and none exists; `log` is already in the lock file; the ring feeds 002-developer-mode's console. | `eprintln!` only — untestable and unreadable by the future console UI; `tracing`/`env_logger` — new crates for a 30-line need. |
| Windows position-jitter test skipped, figure recorded in M6 (R5) | Windows default timer resolution can exceed 5 ms; raising it belongs to a platform adapter, not this crate. | Asserting on Windows in CI — flaky; silently accepting — hides an NFR-1.9 gap. |
| `state.track` cleared on sign-out even though 006 keeps marker files (R9) | FR-014 states the rule for plugin track state on its own; the spec's aside about 006 does not change 006. | Keeping it — contradicts FR-014 and the Retention summary. |

# Tasks: Plugin Runtime, Sandbox, and Permission Gateway

**Input**: Design documents from `/specs/009-plugin-runtime-and-permissions/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included. The feature's own contracts (`contracts/*.md`) name every test file and test function as a required deliverable per Constitution VIII ("plugin isolation and permission enforcement tests are explicit"), so this is not the template's optional case.

**Organization**: Tasks are grouped by user story (spec.md priorities P1–P4) after a Setup and a Foundational phase. Requirement ids (FR-, RT-, G-, L-, C-, E-, R-) are cited for traceability; they are not part of the checklist syntax.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an incomplete task — safe to run in parallel
- **[Story]**: US1 / US2 / US3 / US4, per spec.md priority
- Every task names an exact file path from plan.md's Project Structure

## Path Conventions

Existing 11-crate Cargo workspace under `crates/`, growing to 13
(`modplayer-capability-gateway`, `modplayer-plugin-runtime`). Plugin
packages live under top-level `plugins/`. See plan.md § Project Structure
for the full tree.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: workspace plumbing so the two new crates exist and compile empty before any real code lands.

- [X] T001 Add `mlua = "0.12"` (features `luau`, `send`, `serialize`) and `log = "0.4"` to `workspace.dependencies` in `Cargo.toml`; add both new crates to `[workspace.members]`.
- [X] T002 Create `crates/modplayer-capability-gateway/` skeleton: `Cargo.toml` (deps `serde`, `serde_json`, `toml`, `thiserror`; build-dep `toml`; dev-dep `proptest`), `src/lib.rs` with `#![forbid(unsafe_code)]`, `#![deny(clippy::unwrap_used, clippy::expect_used)]` and empty `pub mod api; pub mod manifest; pub mod refusal; pub mod request; pub mod event; pub mod grants; pub mod limiter; pub mod focus; pub mod state; pub mod budgets; pub mod gateway;` stubs, plus licence header.
- [X] T003 Create `crates/modplayer-plugin-runtime/` skeleton: `Cargo.toml` (deps `modplayer-capability-gateway`, `modplayer-engine`, `mlua`, `serde_json`, `log`), `src/lib.rs` with `#![forbid(unsafe_code)]` and empty `pub mod context; pub mod budget; pub mod scheduler; pub mod bindings; pub mod timers; pub mod pump; pub mod handle; pub mod events;` stubs, plus licence header.
- [X] T004 [P] Add `modplayer-capability-gateway`, `modplayer-plugin-runtime`, `log` to `crates/modplayer-core/Cargo.toml`.
- [X] T005 [P] Add `modplayer-capability-gateway` to `crates/modplayer-ui/Cargo.toml` (for `Permission::explanation_key`).
- [X] T006 [P] Add `log` to `crates/modplayer/Cargo.toml`.
- [X] T007 [P] Add a `deny.toml` comment noting `mlua`/`mlua-sys`/`luau0-src` (MIT) and `log` (MIT OR Apache-2.0) are covered by the existing licence allow-list.
- [X] T008 Write `docs/adr/0001-plugin-runtime-luau.md` (Constitution II ADR: Luau via `mlua` 0.12, rationale and rejected alternatives per research.md R1), then link it from `.specify/memory/constitution.md` Principle II's `TODO(WASM_RUNTIME_DECISION)` with a PATCH version bump and a regenerated Sync Impact Report comment.
- [X] T009 [P] Create `plugins/bundled/` (empty this slice, with a short `README.md` explaining the layout FR-001 requires) and `plugins/fixtures/` directories per research.md R8.
- [X] T010 [P] Create `locales/en-US/plugins.ftl` with every key from data-model.md §4 (`plugins-title`, `plugins-empty`, 8 `plugins-col-*`, `plugins-source-bundled`, `plugins-health-ok|warning|suspended`, `plugins-enable-toggle`, `plugins-invalid-manifest`, `plugins-cpu`, `plugins-memory`, `plugins-dash`, `plugins-list-separator`, `permission-<name>` ×25, `manifest-error-*` ×8, `plugin-suspended`, `plugin-suspended-cause-hang|cpu-share|memory|did-not-start`, `plugin-auto-disabled`, `notification-action-restart-plugin`, `notification-action-disable-plugin`); remove the `placeholder-plugins` key from `locales/en-US/app.ftl`.

**Checkpoint**: both new crates compile empty; workspace builds; strings file exists.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the one Capability Gateway path (Constitution II), the sandboxed runtime scheduler, and the core plugin host/lifecycle skeleton that every user story runs on top of. No story-specific fixture, request-application, or UI code lands here — only what all four stories need to exist at all.

**⚠️ CRITICAL**: No user story work can begin (or be meaningfully tested) until this phase is complete.

### Schema (Constitution IX — must land before any binding or validator)

- [X] T011 `crates/modplayer-capability-gateway/api/v1.toml`: author the 25-entry permission catalog (name, category, explanation Fluent key, `operable` flag for the 9 this slice implements) per data-model.md §1.1 and FR-015.
- [X] T012 `crates/modplayer-capability-gateway/api/v1.toml`: add every `RequestKind` (name, Lua namespace/path, `requires`, `needs_focus`, rate `category`) for transport/queue/markers/effects/state/timers/`debug_probe` per contracts/plugin-api-v1.md §3.
- [X] T013 `crates/modplayer-capability-gateway/api/v1.toml`: add every `EventKind` (name, `requires`, payload field names) per contracts/plugin-api-v1.md §4, plus the 6 refusal codes and `API_VERSION = 1.0`.
- [X] T014 `crates/modplayer-capability-gateway/build.rs`: parse `api/v1.toml` with `toml`, emit `OUT_DIR/generated.rs` (`Permission`, `RequestKind`, `EventKind` enums + `requires()`/`category()`/`needs_focus()`/`lua_path()`/`name()`/`ALL` tables); **fail the build** (G8) if a request/event names a permission or category outside the catalog.
- [X] T015 `crates/modplayer-capability-gateway/src/api.rs`: `include!(generated.rs)`, `ApiVersion` struct, `HOST_CAPABILITIES` const (9 operable permission names + `"timers"`).

### Manifest (FR-001, FR-002)

- [X] T016 [P] `crates/modplayer-capability-gateway/src/manifest.rs`: `PluginIdentifier`, `Version`, `ApiRange`, `SuggestedPosition`, `ManifestDto`, `Manifest`, `ManifestError`, `parse()`/`validate()` implementing the 8 ordered rules of contracts/manifest.md §3.
- [X] T017 [P] `crates/modplayer-capability-gateway/tests/manifest.rs`: `round_trip` (proptest), `rejects_unknown_permission` (proptest), `network_rules`, `duplicate_permission`, `missing_field_names_the_field`, `malformed_identifier_cases`.

### Refusal, request/event types, grants, limiter, focus, budgets

- [X] T018 [P] `crates/modplayer-capability-gateway/src/refusal.rs`: `RefusalCode`, `Refusal`, `PluginResult<T>`.
- [X] T019 [P] `crates/modplayer-capability-gateway/src/request.rs`: `Request`, `Response`, `MarkerInfo`, `NodeInfo`, `QueueItemInfo`, `OwnerInfo`, and the `MarkerId`/`RegionId`/`NodeId`/`QueueItemId` newtypes.
- [X] T020 [P] `crates/modplayer-capability-gateway/src/event.rs`: `HostEvent`, `UnloadReason`, `PlayState`, `TrackInfo`, `LevelInfo`.
- [X] T021 [P] `crates/modplayer-capability-gateway/src/grants.rs`: `GrantState`, `PermissionGrant`, `Grants` (`from_bundled`, `holds`, `granted`, `rows`).
- [X] T022 [P] `crates/modplayer-capability-gateway/src/limiter.rs`: `RateCategory` (5 categories), `RateLimiter` (rolling 1 s window, cap 100 per category, per G5).
- [X] T023 [P] `crates/modplayer-capability-gateway/src/focus.rs`: `FocusToken` (`AtomicU16`-backed `holder`/`try_acquire`/`release_if`, R12).
- [X] T024 [P] `crates/modplayer-capability-gateway/src/budgets.rs`: `Budgets` struct + `DEFAULT` const (4 ms handler, 100 ms/1 s share, 64 MiB, 10 MiB, 5 s ready, 200 ms unload, 1 s RPC timeout, 256 timers, 1024 inbox).
- [X] T025 `crates/modplayer-capability-gateway/src/gateway.rs`: `Gateway { plugin, grants, focus, limiter }`, `admit(kind, now)` implementing G2's fixed order (permission → focus → rate limit).
- [X] T026 `crates/modplayer-capability-gateway/tests/gateway.rs`: `admit_checks_permission_before_focus`, `admit_checks_focus_before_rate`, `refused_calls_consume_no_quota`, `rate_limit_101st_in_window`, `rate_limit_window_slides`.

### Plugin state store (DM-12, FR-014, FR-021)

- [X] T027 [P] `crates/modplayer-capability-gateway/src/state/paths.rs`: `PluginStatePaths` (`MODPLAYER_PLUGIN_STATE_DIR` override, else `data_local_dir/ModPlayer/plugin-state`), `plugin_file`/`track_file`/`clear_tracks`.
- [X] T028 [P] `crates/modplayer-capability-gateway/src/state/writer.rs`: `StateWriter` thread, `WriteJob { path, bytes, ack }`, `.tmp → write_all → sync_all → rename`, 500 ms debounce per path (G7).
- [X] T029 `crates/modplayer-capability-gateway/src/state/store.rs`: `PluginStateStore` (`get`/`set`/`remove`/`load_track`/`encode`), `MAX_KEY_BYTES`/`MAX_VALUE_BYTES`/`STORAGE_CAP`, `PluginStateEntry` (G6).
- [X] T030 `crates/modplayer-capability-gateway/tests/state_store.rs` (+ proptest `state_roundtrip`): `set_rejects_long_key`, `set_rejects_large_value`, `cap_is_atomic`, `encode_is_deterministic`, `writer_is_atomic_and_acks`.
- [X] T031 `crates/modplayer-capability-gateway/tests/api_reference.rs`: generate/check `docs/plugin-api/v1.md` against `api/v1.toml` (`reference_is_current`; regenerates under `MODPLAYER_UPDATE_API_REFERENCE=1`); commit the initial generated `docs/plugin-api/v1.md`.
- [X] T032 `crates/modplayer-capability-gateway/src/lib.rs`: wire every `pub mod`, add doc examples on `Manifest::parse`, `Gateway::admit`, `PluginStateStore::set` (Constitution VII).

### Runtime crate: sandbox, budgets, scheduler, bindings dispatch

- [X] T033 `crates/modplayer-plugin-runtime/src/context.rs`: `PluginContext::new` — `Lua::new_with(STRING|TABLE|MATH|BIT32|UTF8, LuaOptions::new().catch_rust_panics(true))`, `set_memory_limit(64 MiB)`, `set_interrupt` budget check, install the `api` table, `sandbox(true)`, load the entry script (RT2).
- [X] T034 [P] `crates/modplayer-plugin-runtime/src/budget.rs`: `BudgetState` (deadline `AtomicU64`, epoch, trailing-1s sample `VecDeque`), `PluginGauges`, the interrupt-check closure and the aggregate-share evaluator (RT3, RT4).
- [X] T035 [P] `crates/modplayer-plugin-runtime/src/events.rs`: `RuntimeEvent`, `AbortCause`, `SuspendCause`, `PlaybackSnapshot`.
- [X] T036 [P] `crates/modplayer-plugin-runtime/src/timers.rs`: `TimerSet`, `TimerHandle` (≤ 256 pending across all kinds, 1 ms minimum, position timers keyed by `track_generation`) (RT9 structure).
- [X] T037 [P] `crates/modplayer-plugin-runtime/src/pump.rs`: position/meter sampling primitives reading `Arc<RtShared>` + `PlaybackSnapshot` (RT5/RT6 plumbing; delivery policy filled in US3).
- [X] T038 [P] `crates/modplayer-plugin-runtime/src/handle.rs`: `PluginHandle::spawn`, `Inbound`, `Control`, `RuntimeDeps`, `RpcEnvelope`.
- [X] T039 `crates/modplayer-plugin-runtime/src/scheduler.rs`: the thread loop — `recv_timeout(inbox, next_wake)`, ready gate (RT5), `run_handler` invocation mapping `Ok`/`Deadline`/`Exception`/`MemoryError` (RT3), aggregate-share re-evaluation after every handler (RT4), RPC send/`recv_timeout(1s)` (RT7), `Control::Unloading` draining with the 200 ms write-ack wait (RT8), `catch_unwind` fault containment (RT12), gauge publication (RT13).
- [X] T040 `crates/modplayer-plugin-runtime/src/bindings/mod.rs`: the exhaustive `match` dispatcher over `RequestKind::ALL` — calls `Gateway::admit`, then routes to a local capability (timers/state/focus/snapshot) or sends an `RpcEnvelope` and blocks for the reply.
- [X] T041 [P] `crates/modplayer-plugin-runtime/src/bindings/playback.rs`: `subscribe_position`/`state()` Lua↔Request mapping (local, no RPC).
- [X] T042 [P] `crates/modplayer-plugin-runtime/src/bindings/transport.rs`: `play`/`pause`/`toggle`/`seek`/`skip_next`/`skip_previous`/`request_focus`/`release_focus`/`arm_loop`/`disarm_loop` Lua↔Request mapping.
- [X] T043 [P] `crates/modplayer-plugin-runtime/src/bindings/queue.rs`: `list`/`move`/`remove`/`play_next`/`add` Lua↔Request mapping.
- [X] T044 [P] `crates/modplayer-plugin-runtime/src/bindings/markers.rs`: `list`/`create`/`move`/`rename`/`recolor`/`delete`/`create_loop`/`set_cue` Lua↔Request mapping.
- [X] T045 [P] `crates/modplayer-plugin-runtime/src/bindings/effects.rs`: `list_chain`/`create_node`/`set_param`/`schedule_param`/`bypass`/`remove_node` Lua↔Request mapping.
- [X] T046 [P] `crates/modplayer-plugin-runtime/src/bindings/state.rs`: `state.plugin`/`state.track` `get`/`set`/`remove` (local `PluginStateStore` calls, no RPC).
- [X] T047 [P] `crates/modplayer-plugin-runtime/src/bindings/timers.rs`: `set_timeout`/`set_interval`/`schedule_at_position`/`clear` (local `TimerSet` calls).
- [X] T048 [P] `crates/modplayer-plugin-runtime/src/bindings/log.rs`: `log.info`/`warn`/`error` → the `log` façade + `RuntimeEvent::Log`.
- [X] T049 `crates/modplayer-plugin-runtime/src/lib.rs`: wire every `pub mod`.

### Core plugin host skeleton and model deltas

- [X] T050 `crates/modplayer-core/src/plugins/bundled.rs`: `BundledPackage`, `packages()`/`fixtures()` via `include_str!` from `plugins/bundled/` and `plugins/fixtures/`, `fixtures()` gated on `MODPLAYER_PLUGIN_FIXTURES == "1"` (L1).
- [X] T051 `crates/modplayer-core/src/plugins/mod.rs`: `PluginId` (re-export of `modplayer_effects::PluginId`), `PluginIdTable` interner, `Source`, `Health`, `Lifecycle`, `PluginRecord`, and the FR-005 lifecycle-transition functions.
- [X] T052 `crates/modplayer-core/src/plugins/host.rs`: `PluginHost` (`discover`, request/event channels, `PluginSnapshot`, `focus`, `paths`, `writer`, `fixtures_enabled`, `waker`).
- [X] T053 `crates/modplayer-core/src/plugins/apply.rs`: `drain_plugin_requests()` skeleton (C1: dispatch table over every `RequestKind`, always replies) with `Play`/`Pause`/`Toggle`/`Seek`/`SkipNext`/`SkipPrevious` implemented (`invalid_state`/`no_track` per contracts/plugin-host-service.md §3); every other variant returns a placeholder `invalid_state` refusal until its story task fills it in.
- [X] T054 `crates/modplayer-core/src/plugins/fanout.rs`: `fan_out(event)` filtering by `EventKind::requires()` against each Active plugin's grants, `try_send` with drop+log-once-per-second-per-plugin (C4).
- [X] T055 `crates/modplayer-core/src/plugins/log.rs`: `PluginLog` (1000-entry ring), `LogEntry`; hook into the `log` façade with `target: "plugin:<identifier>"` (R19).
- [X] T056 `crates/modplayer-core/src/plugins/view.rs`: `PluginRow`, `PluginsView` structs (empty mapping; real population is a US4 task).
- [X] T057 `crates/modplayer-core/src/lib.rs`: `pub mod plugins;` and re-export `PluginsView`, `PluginRow`, `Health`, `PluginId`, `Refusal`, etc. used by `modplayer-ui`.
- [X] T058 `crates/modplayer-core/src/markers/model.rs`: `Owner::Plugin(PluginId)` (stays `Copy`), `MarkerError::NotOwner`, `TrackMarkers.revision`, `add_point_owned`, `new_loop_region_owned`, `set_cue_owned`, `owner_of`, `armed_region_owner`, `remove_transient_owned_by`, `disarm_if_owned_by`.
- [X] T059 `crates/modplayer-core/src/markers/store.rs`: owner DTO string (`"host"` | `"<identifier>"`), interning unknown identifiers on load, `encode()` skips transient markers/regions.
- [X] T060 `crates/modplayer-core/src/effects/model.rs`: `ChainModel.revision`, `add_at(kind, owner, index)`, `orphan_owned_by`, `readopt`, `owned_by`, `resolve_position(&SuggestedPosition)`.
- [X] T061 `crates/modplayer-core/src/notifications.rs`: `Notification.dedupe_key`, `NotificationAction::RestartPlugin`/`DisablePlugin`, `NotificationCenter::raise_keyed`/`dismiss_by_dedupe`, `KEY_PLUGIN_SUSPENDED`/`KEY_PLUGIN_AUTO_DISABLED` consts.
- [X] T062 `crates/modplayer-core/src/controller.rs`: add `plugins: PluginHost`, `last_marker_actor: Owner`, `set_waker(f)`; stub `plugins_view()`/`plugin_enable`/`plugin_disable`/`plugin_restart`/`plugin_log`/`plugins_mut()`; wire `launch()` → `plugins.load_all_enabled()`, `tick()` → `drain_plugin_requests()`/`drain_plugin_runtime_events()`/`reap_plugin_threads()` first and `publish_plugin_snapshot_if_changed()`/`fan_out_revision_events()` last.
- [X] T063 `crates/modplayer/src/main.rs`: install a ≤ 30-line stderr `log::Log` at `Info` level (R19); no other change (fixture env read stays in core).
- [X] T064 `crates/modplayer/tests/decoded_store_boundary.rs`: add `plugin_crates_expose_no_sample_sink` asserting neither new crate's public items expose sample data (Constitution V).
- [X] T065 `crates/modplayer/tests/single_dependent.rs`: re-run and confirm unchanged after the new dependency graph (Constitution IV).

**Checkpoint**: both new crates compile with real logic; `modplayer-core` builds with a `plugins` module wired into `PlaybackController`; no story-specific fixture or UI code exists yet. `cargo test --workspace` is green for everything landed so far.

---

## Phase 3: User Story 1 - A misbehaving plugin never takes the player down (Priority: P1) 🎯 MVP

**Goal**: hang / throw / leak / never-ready fixtures are contained — audio and transport never stop, the user gets a keyed Restart/Disable notice, and three suspensions in a session auto-disable the plugin.

**Independent Test**: enable a bundled plugin instrumented to hang in an event handler; verify audio and transport continue uninterrupted, a notice names the plugin with Restart/Disable, and after three such episodes the plugin auto-disables.

- [X] T066 [P] [US1] `plugins/fixtures/hang/{plugin.toml, main.luau, README.md}`: requires `playback.observe`; `while true do end` inside its `play_state_changed` handler (research R18).
- [X] T067 [P] [US1] `plugins/fixtures/throw/{plugin.toml, main.luau, README.md}`: requires `playback.observe`; `error("boom")` on every `position` event.
- [X] T068 [P] [US1] `plugins/fixtures/leak/{plugin.toml, main.luau, README.md}`: requires `playback.observe`; appends a 1 MB string to a table on every `position` event.
- [X] T069 [P] [US1] `plugins/fixtures/noready/{plugin.toml, main.luau, README.md}`: minimal manifest, entry script that never calls `api.ready()`.
- [X] T070 [US1] `crates/modplayer-core/src/plugins/fanout.rs` + `src/controller.rs`: wire `TrackChanged`/`PlayStateChanged`/`QueueChanged`/`ReadyAck`/`Unloading` event triggers (E table rows 1–3, 7–8) into `dispatch`/queue/intent hooks and `stop()`.
- [X] T071 [US1] `crates/modplayer-core/src/plugins/host.rs`: `drain_plugin_runtime_events()` implementing L4 (`Ready` → `Active`, health `Ok`, clear abort window, dismiss suspension notice) and L5 (abort-window accounting: push on every `HandlerAborted`, evict > 60 s, `warning_until = now + 5 min` at the 3rd, controller-injectable clock).
- [X] T072 [US1] `crates/modplayer-core/src/plugins/host.rs`: L6 suspension handling (`RuntimeEvent::Suspended{cause}` → `Suspended`, `suspensions_this_session += 1`, teardown, notice; the 3rd suspension calls `plugin_disable` and raises `plugin-auto-disabled`).
- [X] T073 [US1] `crates/modplayer-core/src/plugins/host.rs`: `PluginHost::stop(id, reason)` implementing L7's fixed order — release focus, disarm the plugin's armed region, remove its transient markers (revision bump), orphan its nodes (revision bump) — and L8 `reap_plugin_threads()`.
- [X] T074 [US1] `crates/modplayer-core/src/controller.rs`: complete `plugin_enable`/`plugin_disable`/`plugin_restart`/`shutdown()`/`clear_for_sign_out()` (Disabled ⇄ Loading ⇄ Active ⇄ Suspended ⇄ Draining per data-model.md §3.1; suspension counter resets only on process launch; `shutdown()` stops every running plugin and waits ≤ 250 ms).
- [X] T075 [US1] `crates/modplayer-core/src/notifications.rs` usage in `controller.rs`: raise `plugin-suspended` (Restart/Disable actions, dedupe `plugin-suspended:<identifier>`) on every L6 suspension and `plugin-auto-disabled` (no actions) on the 3rd; dismiss `plugin-suspended:<identifier>` on L4 `Ready`.
- [X] T076 [P] [US1] `crates/modplayer-plugin-runtime/tests/isolation.rs`: `infinite_loop_handler_aborts_within_budget` (abort ≤ 4 ms + 2 ms slack, state reusable), `repeated_hang_suspends_within_one_second`, `memory_bomb_suspends_plugin_not_host`, `exception_aborts_handler_only`, `never_ready_suspends_after_5s` (injected clock), `panic_in_binding_is_contained`.
- [X] T077 [P] [US1] `crates/modplayer-plugin-runtime/tests/scheduler.rs` (US1 subset): `events_in_order`, `slow_handler_delays_only_own_events`, `rpc_wait_excluded_from_cpu`, `rpc_timeout_is_host_busy`, `unloading_writes_committed_within_200ms`.
- [X] T078 [US1] `crates/modplayer-core/tests/controller_plugins_lifecycle.rs`: `ready_ack_is_first_event`, `never_ready_suspended_with_restart`, `hang_fixture_suspended_audio_continues` (`FakeBackend` rendering on a second thread; asserts zero missed callbacks and `transport_enabled()` throughout), `three_suspensions_auto_disable`, `restart_keeps_session_counter`, `disable_runs_teardown_in_order`, `suspended_readopts_on_ready`, `shutdown_delivers_unloading_and_bounds_wait`, `sign_out_clears_track_state_only`, `warning_after_three_aborts_and_clears_after_5min` (injected clock).
- [X] T079 [US1] `crates/modplayer-core/tests/notifications.rs` (+): `raise_keyed_replaces`.
- [X] T080 [US1] Run quickstart.md manual scenarios M3 (hang isolation), M4 (restart/auto-disable), M5 (memory/throw fixtures), M6's suspension half (never-ready), M12 (shutdown); record results/deviations in quickstart.md.

**Checkpoint**: User Story 1 is fully functional and independently testable — a hanging/throwing/leaking/never-ready plugin is contained, notified, restartable and auto-disables, with zero audio interruption.

---

## Phase 4: User Story 2 - A plugin can only do what it was granted permission to do (Priority: P2)

**Goal**: every ungranted request is refused with no side effect; an invalid manifest never loads; ownership and rate limits are enforced.

**Independent Test**: enable a plugin that requests only `playback.observe`, have it call `transport.control`-only `seek`, verify `permission_denied` and no side effect; separately, load a plugin whose manifest names an unknown permission and verify it is listed invalid, never loaded.

- [X] T081 [P] [US2] `plugins/fixtures/observer/{plugin.toml, main.luau, README.md}`: `playback.observe` only; on `ready_ack` calls `api.transport.seek(0)` and logs the resulting `permission_denied`/`not_granted`.
- [X] T082 [P] [US2] `plugins/fixtures/invalid/{plugin.toml, main.luau, README.md}`: `required = ["teleport.everywhere"]` (a permission outside the catalog).
- [X] T083 [P] [US2] `plugins/fixtures/flood/{plugin.toml, main.luau, README.md}`: `transport.control`; requests focus then issues 1 000 `seek` calls on its first `play_state_changed(playing)`.
- [X] T084 [US2] `crates/modplayer-core/src/plugins/apply.rs`: implement `RequestFocus`/`ReleaseFocus` (R12 CAS on the shared `FocusToken`) and `ArmLoop`/`DisarmLoop` (ownership + focus check, `RegionIncomplete`/`RegionTooShort` → `invalid_state`/`region_*`, disarms any other armed region per 006 FR-008).
- [X] T085 [US2] `crates/modplayer-core/src/plugins/apply.rs`: implement `MoveMarker`/`RenameMarker`/`RecolorMarker`/`DeleteMarker` and `SetCue` with `owner_of(id) == Plugin(id)` checks → `not_owner`/`not_found` (C2).
- [X] T086 [US2] `crates/modplayer-core/src/plugins/apply.rs`: implement `SetParam`/`ScheduleParam`/`SetBypass`/`RemoveNode` with `owned_by(id)` checks → `not_owner`/`not_found`.
- [X] T087 [US2] `crates/modplayer-core/src/controller.rs` + `src/plugins/fanout.rs`: wire `MarkerChanged` (from `TrackMarkers.revision`), `LoopArmed`/`LoopDisarmed`/`LoopWrapped`, and `EffectChainChanged` (from `ChainModel.revision`) into `fan_out_revision_events()` (C3, E table rows 4–6), so a refused call can be asserted to raise none of them.
- [X] T088 [US2] `crates/modplayer-core/tests/plugins_manifest_discovery.rs`: `fixtures_only_with_env`, `invalid_fixture_listed_not_loaded`, `rows_sorted_by_name`, `no_uninstall`.
- [X] T089 [US2] `crates/modplayer-core/tests/controller_plugins_permissions.rs` (US2 subset): `matrix_every_request_without_its_permission_is_denied_with_no_effect` (iterates `RequestKind::ALL` against the observer fixture, snapshots models before/after), `not_owner_vs_not_found`, `arm_loop_permission_before_focus`, `request_focus_contention_invalid_state`, `rate_limit_1000_seeks`, `set_cue_ownership`.
- [X] T090 [US2] Run quickstart.md manual scenarios M2 (fixture list, invalid row reads "invalid manifest: …teleport.everywhere…"), M7 (permission refusal), M9 (rate limit); record results.

**Checkpoint**: User Stories 1 and 2 both work independently — least-privilege enforcement and invalid-manifest rejection are in place alongside fault isolation.

---

## Phase 5: User Story 3 - A bundled plugin observes and acts on the music while it plays (Priority: P3)

**Goal**: the well-behaved fixture completes a full behavior cycle — position/meter subscriptions, transport+queue, markers/loop, effect nodes, per-track state, position timers — exactly as declared.

**Independent Test**: enable a fixture holding every operable permission; drive it through subscribe-position, request-focus, arm-own-loop, create-and-parameterize-a-node-at-a-suggested-position, read-a-meter-tick, persist-and-recall-per-track-state, schedule-a-position-timer, and verify each works exactly as declared.

- [X] T091 [P] [US3] `plugins/fixtures/wellbehaved/{plugin.toml, main.luau, README.md}`: all 9 operable permissions; on `ready_ack` subscribes to `position` at 30/s, requests focus, creates and arms a loop region, creates a `pitch_shift` node `before` `time_stretch`, sets one of its parameters, writes `state.track`, schedules a `schedule_at_position` timer — logging each step (research R18).
- [X] T092 [US3] `crates/modplayer-core/src/plugins/apply.rs`: implement `QueueList`/`QueueMove`/`QueueRemove`/`QueuePlayNext`/`QueueAdd` (resolves via the current queue's `TrackRef`s and the library index, `not_found` otherwise — R21).
- [X] T093 [US3] `crates/modplayer-core/src/plugins/apply.rs`: implement `CreateMarker`/`CreateLoopRegion` (owned, `transient` flag honored, `marker_limit`) and `CreateNode` (`resolve_position`, `chain_full`/`unsupported_kind`).
- [X] T094 [US3] `crates/modplayer-core/src/plugins/apply.rs`: implement `ListMarkers`/`ListChain`/`QueueList` as `PluginSnapshot` reads (no RPC) and `DebugProbe` forwarding to `Control::Probe`, gated on `fixtures_enabled` (`invalid_state`/`invalid_argument` otherwise — L10).
- [X] T095 [US3] `crates/modplayer-plugin-runtime/src/pump.rs` + `src/bindings/playback.rs`: finish position delivery (rate clamped `[1,60]`, default 10, coalesced to latest, delivered only while changing or once after a seek/loop-wrap while paused) and `meter` delivery at the UI's cadence for `audio.meter` holders.
- [X] T096 [US3] `crates/modplayer-plugin-runtime/src/scheduler.rs`: finish RT11 (on `TrackChanged`, load `tracks/<hex>.json` into the store under a 4 ms deadline; overrun ⇒ discard + `HandlerAborted{RestoreTimeout}`, deliver `track_changed` anyway) and RT9's `schedule_at_position` firing plus track-generation cancellation.
- [X] T097 [US3] `crates/modplayer-core/src/effects/model.rs` + `apply.rs`: `bypass(node, false)` clears `auto_bypassed` on a plugin's own node exactly as the user's re-enable does (008 G5); `effect_chain_changed` carries current orphan flags.
- [X] T098 [P] [US3] `crates/modplayer-plugin-runtime/tests/scheduler.rs` (US3 subset): `position_rate_clamped_and_coalesced`, `position_silent_while_paused`, `position_jitter_under_5ms` (`#[cfg(not(windows))]`), `schedule_at_position_fires_once_with_actual_position`, `track_change_cancels_position_timers`, `timer_limit_257`, `clear_unknown_timer_not_found`, `track_state_restored_before_track_changed`, `restore_timeout_delivers_event_anyway`.
- [X] T099 [P] [US3] `crates/modplayer-plugin-runtime/tests/bindings.rs`: every `RequestKind` round-trips Lua ↔ `Request`/`Response`; an ungranted namespace call returns `nil, { code = "permission_denied", reason = "not_granted" }` rather than a Lua error; `api.granted`/`api.version`/`api.capabilities` shape; `api.ready()` returns `true` once then `false, refusal` after.
- [X] T100 [US3] `crates/modplayer-core/tests/controller_plugins_permissions.rs` (US3 subset): `queue_write_ignores_focus`, `chain_full_and_marker_limit`, `transient_marker_lifetime`, `host_edit_keeps_owner_and_notifies`.
- [X] T101 [P] [US3] `crates/modplayer-core/tests/markers_model.rs` (+): `owner_roundtrip_in_file`, `transient_never_encoded`, `remove_transient_owned_by`.
- [X] T102 [P] [US3] `crates/modplayer-core/tests/effects_model.rs` (+): `add_at_resolves_before_after_index_or_appends`, `orphan_and_readopt`.
- [X] T103 [US3] Run quickstart.md manual scenarios M8 (well-behaved cycle), M10 (disable teardown / re-adopt on restart), M11 (per-track state across track A→B→A and sign-out); record results.

**Checkpoint**: User Stories 1–3 are all independently functional — safe, permission-checked, and now behaviorally complete.

---

## Phase 6: User Story 4 - The user sees every installed plugin and controls it in one action (Priority: P4)

**Goal**: the Plugins section lists every plugin with live health/CPU/memory, lets the user enable/disable in one action, and never offers uninstall.

**Independent Test**: launch with `MODPLAYER_PLUGIN_FIXTURES=1`, open Plugins; verify every column and live gauges, disable one plugin and verify its teardown runs, verify no uninstall control exists; launch without the variable and verify the empty state.

- [X] T104 [US4] `crates/modplayer-core/src/plugins/view.rs`: real `PluginsView`/`PluginRow` mapping from `PluginRecord`s — sorted by name (case-insensitive), gauges from `PluginGauges`, "—" (`None`) for CPU/memory unless `Active`, `invalid_reason`, granted permissions in catalog order, `can_uninstall = false` always (FR-023).
- [X] T105 [US4] `crates/modplayer-ui/src/plugins_view.rs`: the Plugins section — column table from `controller.plugins_view().rows`, enable checkbox (`plugins-enable-toggle` with `$plugin`, immediate `plugin_enable`/`plugin_disable`, inert for `Invalid` rows), health badge + dot, permission summary joined with `plugins-list-separator`, `plugins-cpu`/`plugins-memory`/`plugins-dash`, invalid-manifest row rendering, `plugins-empty` when no rows (contracts/ui-plugins.md §2).
- [X] T106 [US4] `crates/modplayer-ui/src/app.rs`: `Section::Plugins` calls `plugins_view::show` instead of `shell::plugins_placeholder`; install the controller's waker as `ctx.request_repaint`; request a repaint every 500 ms while the section is visible so gauges stay live.
- [X] T107 [US4] `crates/modplayer-ui/src/shell.rs`: remove `plugins_placeholder` (rail entry `nav-plugins`/`action-nav-plugins` unchanged); confirm `placeholder-plugins` is gone from `app.ftl` (T010).
- [X] T108 [US4] `crates/modplayer-ui/src/notifications.rs`: render `RestartPlugin(id)`/`DisablePlugin(id)` as `notification-action-restart-plugin`/`notification-action-disable-plugin`, calling `plugin_restart`/`plugin_disable`.
- [X] T109 [P] [US4] `crates/modplayer-ui/tests/plugins_view.rs`: `empty_state_without_fixtures`, `rows_show_every_column_sorted_by_name`, `invalid_row_shows_reason_and_inert_toggle`, `toggle_disables_in_one_action`, `suspended_row_shows_dash_gauges`, `no_uninstall_control`, `notification_actions_call_facade`.
- [X] T110 [P] [US4] `crates/modplayer-ui/tests/accessibility.rs`: `plugins_section_controls_named`.
- [X] T111 [P] [US4] `crates/modplayer-ui/tests/fluent_keys.rs`: `plugins_ftl_keys_used_exist`.
- [X] T112 [US4] Run quickstart.md manual scenario M1 (empty state) and re-verify M2's columns/live gauges visually; record results.

**Checkpoint**: all four user stories are independently functional and demonstrable end to end.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: the automated gates quickstart.md requires before manual sign-off, plus the process note Constitution IX calls for.

- [X] T113 [P] `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean across every file touched by this feature.
- [X] T114 [P] `cargo deny check` passes with the `mlua`/`mlua-sys`/`luau0-src`/`log` entries from T007.
- [X] T115 [P] `scripts/check-license-headers.sh` passes on every new file (both new crates, `plugins/`, `docs/adr/0001-…`, `locales/en-US/plugins.ftl`).
- [X] T116 `cargo test --doc` passes for the doc examples on `Manifest::parse`, `Gateway::admit`, `PluginStateStore::set`, `PluginHandle::spawn`.
- [X] T117 Add a PR-template checklist item in `.github/PULL_REQUEST_TEMPLATE.md` (or, if templated per-area, a note in `docs/adr/0001-plugin-runtime-luau.md`) that any `api/v1.toml` change requires the written change request Constitution IX asks for.
- [X] T118 Record the observed Windows position-jitter figure (research.md R5 risk) from M6 into `research.md`, noting whether it stays within NFR-1.9's 5 ms.
- [X] T119 Full quickstart.md automated-gate run — `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, `scripts/check-license-headers.sh` — green on ubuntu / macos / windows CI; final sign-off note in quickstart.md.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup; blocks every user story. Internally: schema (T011–T015) before manifest/refusal/request/event/grants/limiter/focus/budgets (T016–T024) before `Gateway::admit` (T025–T026); state store (T027–T031) can run alongside T016–T024; runtime crate (T033–T049) depends on the gateway crate's types (T018–T024) but not on the state store; core skeleton (T050–T065) depends on both crates' public types.
- **User Stories (Phase 3–6)**: all depend on Foundational; each is independently testable once Foundational is done, but they are numbered and best executed in priority order (P1 → P2 → P3 → P4) because later stories' `apply.rs`/`fanout.rs` edits touch the same files US1/US2 already modified.
- **Polish (Phase 7)**: depends on all four user stories.

### User Story Dependencies

- **US1 (P1)**: no dependency on US2–US4; its fixtures only need `playback.observe`.
- **US2 (P2)**: independently testable from US1, but its `apply.rs`/`fanout.rs` edits land in the same files US1 edited (T070, T053) — do not parallelize US1 and US2 file edits.
- **US3 (P3)**: same file-sharing caveat with US1/US2's `apply.rs`/`fanout.rs`/`effects/model.rs` edits.
- **US4 (P4)**: reads `PluginsView`/`PluginRow` (T056) populated for real in T104; otherwise UI-only files, parallel-safe with the others once Foundational is done.

### Within Each User Story

- Fixture packages (`[P]`) before the tests that exercise them.
- `apply.rs`/`fanout.rs`/model edits before the tests that assert their behavior.
- Runtime scheduler/binding edits before the runtime tests that assert them.
- Manual scenarios last, after their story's automated tests are green.

### Parallel Opportunities

- All Setup tasks marked `[P]` (T004–T007, T009–T010).
- Within Foundational: T016–T024 (manifest/refusal/request/event/grants/limiter/focus/budgets — 8 separate files) in parallel; T027–T028 in parallel; T034–T038 (runtime crate's independent modules) in parallel; T041–T048 (per-namespace bindings — 8 separate files) in parallel.
- Within each user story, the `[P]`-marked fixture packages and the `[P]`-marked test files.
- US4's UI tests (T109–T111) in parallel with each other.

---

## Parallel Example: Foundational bindings

```bash
# Launch all per-namespace Lua binding files together (T041–T048):
Task: "playback.rs bindings in crates/modplayer-plugin-runtime/src/bindings/playback.rs"
Task: "transport.rs bindings in crates/modplayer-plugin-runtime/src/bindings/transport.rs"
Task: "queue.rs bindings in crates/modplayer-plugin-runtime/src/bindings/queue.rs"
Task: "markers.rs bindings in crates/modplayer-plugin-runtime/src/bindings/markers.rs"
Task: "effects.rs bindings in crates/modplayer-plugin-runtime/src/bindings/effects.rs"
Task: "state.rs bindings in crates/modplayer-plugin-runtime/src/bindings/state.rs"
Task: "timers.rs bindings in crates/modplayer-plugin-runtime/src/bindings/timers.rs"
Task: "log.rs bindings in crates/modplayer-plugin-runtime/src/bindings/log.rs"
```

## Parallel Example: User Story 1 fixtures

```bash
# Launch all four US1 fixture packages together (T066–T069):
Task: "hang fixture in plugins/fixtures/hang/"
Task: "throw fixture in plugins/fixtures/throw/"
Task: "leak fixture in plugins/fixtures/leak/"
Task: "noready fixture in plugins/fixtures/noready/"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational (the single Gateway path, sandboxed scheduler, and core lifecycle skeleton — CRITICAL, blocks everything).
3. Complete Phase 3: User Story 1.
4. **STOP and VALIDATE**: hang/throw/leak/never-ready fixtures are isolated, notified, restartable, auto-disable at 3 — audio never stops (SC-001–SC-003, SC-009).

### Incremental Delivery

1. Setup + Foundational → gateway/runtime/core skeleton compiles and passes its own crate tests.
2. Add US1 → fault isolation demonstrable (MVP).
3. Add US2 → least-privilege enforcement and invalid-manifest rejection demonstrable.
4. Add US3 → the full plugin behavior cycle (position, markers, effects, state, timers) demonstrable.
5. Add US4 → the user-facing Plugins section and one-action enable/disable demonstrable.
6. Polish → CI gates green on all three platforms; manual scenario sign-off complete.

## Notes

- `[P]` tasks touch different files with no unmet dependency.
- `[Story]` maps a task to its user story for traceability back to spec.md.
- Some `apply.rs`/`fanout.rs`/model-delta files are edited by more than one story's tasks in sequence (US1 → US2 → US3); those specific tasks are **not** parallel with each other even without an explicit note, because they share a file.
- Constitution VIII treats plugin isolation and permission-enforcement tests as non-negotiable, so every named test in contracts/ is a task here, not an optional extra.
- Spec-fixed numeric constants (4 ms, 10 %, 64 MB, 10 MB, 5 s, 200 ms, 100/s, 10/s default, 3-in-60s→5min, 256 timers, 256 B/1 MB state bounds) are asserted by tests against `Budgets::DEFAULT`/`RateLimiter`/`TimerSet`/`PluginStateStore` constants (design note 15) — do not retune them without recording a deviation per the spec's Assumptions section.

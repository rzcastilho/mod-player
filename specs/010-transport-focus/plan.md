# Implementation Plan: Transport Focus Arbitration

**Branch**: `feature/010-transport-focus` | **Date**: 2026-09-19 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/010-transport-focus/spec.md`

## Summary

Replace 009's placeholder single-holder CAS (`request_focus()` →
`invalid_state`/`focus_held`) with real transport-focus arbitration:
at any moment the host or exactly one plugin holds focus; only the
holder's `play/pause/toggle/seek/skip_*/arm_loop/disarm_loop` apply,
everyone else gets `no_focus`; the user's own commands — host UI,
keyboard, or a remote controller on the account — always win. A plugin
asks with `request_focus()` (always `ok`, recorded as pending) and the
user's global policy decides: **manual** (only "Give focus" grants),
**auto on interaction** (default; "Give focus" grants, a *local* host
transport action returns focus to the host), or **first request wins**
(per track; the first requester is granted, vacancies refill from the
queue, a user "Take back" locks the host until the next track). The
outgoing holder gets `focus_revoked{holder}` and the incoming one
`focus_granted{holder}` (plugin API 1.0 → 1.1, additive). Disable,
suspension or crash of the holder returns focus to the host immediately
and disarms its loop (markers untouched); every non-fault transition
leaves the loop alone. A Transport panel in Now Playing (`T` / header
toggle) shows the holder, every enabled `transport.control` plugin with
its request order, one-click "Give focus"/"Take back" and the policy
selector, persisted in `settings.toml`.

Technical approach ([research.md](research.md)): a pure `FocusArbiter`
state machine in `modplayer-core::plugins::focus` returning
`FocusChange`s that the controller applies (write the shared
`FocusToken`, send events straight to the two affected plugin handles);
`request_focus`/`release_focus` become ordinary RPCs to core; a
controller-scoped `TransportActor` (LocalUser / Plugin / Remote) makes
the auto-policy revoke hook fire from the six transport `Input`s and
`arm_loop`/`disarm_loop` only for local user actions, covering every
keyboard and pointer path without touching UI call sites; the
first-wins reset piggybacks on the existing `TrackChanged` fan-out;
`PluginHost::stop` keeps 009's teardown order with a `vacate(Fault)`
first; two new Luau fixtures (`focus-a`, `focus-b`) make the contention
scenarios testable and manually drivable; a `transport_view.rs` panel
mirrors 008's Effect Chain panel. Headless assumptions are listed in
Complexity Tracking.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–009; fixture scripts in Luau (`mlua` 0.12)

**Primary Dependencies**: existing only — eframe/egui 0.36 (+accesskit), fluent-templates 0.15, serde/serde_json 1, toml 0.9, thiserror 2, mlua 0.12 (runtime crate), log 0.4, proptest 1 (dev). **No new crate** (Constitution X).

**Storage**: `settings.toml` gains `[transport] focus_policy` under 007's atomic-write contract (research R5); the holder and the request queue are session-only and never written (FR-012). No change to marker files, plugin state files or the secure store.

**Testing**: `cargo test --workspace` — new `focus_arbiter.rs` (pure state machine + proptest single-holder invariant), `controller_transport_focus.rs` (end to end on `FakeBackend` + `SyntheticSource` with the `focus-a`/`focus-b` fixtures and `debug_probe`), `transport_view.rs` (offscreen `egui::Context`); rewritten 009 focus tests; gateway API-reference test regenerated for 1.1; runtime binding/scheduler tests for the RPC routing and Lua payloads; manual scenarios M1–M8 in [quickstart.md](quickstart.md). CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers) on ubuntu / macos / windows.

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical behaviour, shortcut (`T`) and plugin API on all three; no platform-specific code.

**Project Type**: Desktop application — the 13-crate Cargo workspace is unchanged (no new crate; changes in `modplayer-core`, `modplayer-capability-gateway`, `modplayer-plugin-runtime`, `modplayer-ui`, plus fixtures and locales).

**Performance Goals**: arbitration adds zero work to the audio thread and O(1)–O(n_plugins) work per transport action on the controller thread (n ≤ 10 fixtures); a user transport command's control-to-audio latency is unchanged (NFR-1.1, SC-002 — the hook is an enum compare and at most one channel send before the same `reduce`); focus return on a fault is reflected in the panel on the next frame after the `tick` that drained the suspension (≤ 33 ms + frame, SC-003's "within one second"); `request_focus()` round trip = one woken UI frame (009 R3).

**Constraints**: Constitution I — no engine/effects crate change, nothing on the RT path; Constitution II — one Gateway path unchanged (permission → focus → rate), refusals are values, `#![forbid(unsafe_code)]` everywhere touched; Constitution III — focus is a core-owned host primitive (arbiter in core, not in the gateway crate or scripts); Constitution IX — the two events and the version bump are made once in `api/v1.toml`, reference regenerated, written change request in the PR; Constitution X — no new trait, no feature flag, every panel control keyboard-operable and named, strings externalised (`transport.ftl`), user "Take back" in one action under every policy; spec-fixed semantics (policy table in [data-model.md](data-model.md) §1.6) unchanged; out of scope: plugin-contributed UI (011), MIDI (004), Performance Mode indicator (004/003), hot-reload retention (002), presets that snapshot focus (006/003), broadcast focus events.

**Scale/Scope**: core ≈ 900 LOC (`plugins/focus.rs` 300 incl. unit tests, view 60, host/apply/controller deltas 250, settings 60, catalog 20, tests 900 across `focus_arbiter.rs` + `controller_transport_focus.rs` + rewrites); gateway ≈ 60 LOC delta (schema, event enum, token, refusal) + regenerated reference; runtime ≈ 40 LOC delta + 60 test; ui ≈ 250 LOC (`transport_view.rs`) + 300 test; 2 fixture packages (≈ 60 Luau lines each); 1 new `.ftl` (14 keys) + 1 key; 1 new `HostAction`; 2 new events; API 1.1.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No (indirectly) | ✅ PASS | No change to `modplayer-engine`/`modplayer-effects`; the arbiter runs on the controller (UI) thread, plugin threads only *read* the `FocusToken` atomic exactly as in 009; loop disarm on a fault reuses 009's `disarm_if_owned_by` → existing `Command` SPSC path at buffer boundaries. `tests/realtime.rs` untouched. No "real-time safety" PR note needed (engine crate not modified). |
| II. Plugins Are Guests (non-negotiable) | **Yes** | ✅ PASS | `Gateway::admit`'s permission → focus → rate order is unchanged (G2); `no_focus` stays an `Err(Refusal)` value; the host re-validates focus at application time (R12) so a plugin can never move the playhead without focus; a faulting holder is torn down by 009's containment and focus returns to the host in the same `tick` (FR-007). No budget, sandbox or isolation change. |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | The arbiter, policy and holder live in `modplayer-core` (R1); plugins only `request_focus()`/`release_focus()` and react to two events; no script can decide or observe arbitration beyond its own grant/revoke. |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No source crate touched; every automated test runs on `SyntheticSource`/`ScriptedHost` + `FakeBackend`; `single_dependent.rs` unchanged. The remote-controller path is exercised through the existing `Input::RemoteCommand` reducer input, not a live receiver. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | The feature adds no request or event carrying sample data (payloads are `holder` strings); `decoded_store_boundary.rs` unchanged and still green. |
| VI. Security and Privacy by Default | No | N/A | No credential, network, file or telemetry surface is added; the settings key is a plain enum string; plugins still cannot reach the secure store. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate; `#![forbid(unsafe_code)]` and `clippy::unwrap_used/expect_used` denies already apply in every touched crate; `thiserror` unchanged (no new error type — `InvalidField` gains a variant); doc examples on the new public items (`FocusArbiter::request`, `FocusPolicy::parse`, `FocusToken::set_holder`); `cargo deny` unaffected (no dependency change); licence headers on every new file. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Test-first on the contracts' named tests; **permission enforcement** (`no_focus` with zero side effect, host-side re-check) and **plugin isolation** (holder suspension → host + loop disarm, audio unaffected) are explicit; proptest for the arbiter's single-holder invariant and the `settings.toml` round trip (state serialization); the control-to-audio budget test from 003/005 stays green with the hook in `dispatch`; manual scenarios M1–M8 executed by the implementing agent (Governance › Manual Scenario Sign-Off); every manual deviation ships a regression test. |
| IX. One Plugin API Definition | **Yes** | ✅ PASS | `api/v1.toml` is the single edit point: `minor = 1`, two `[[event]]` entries; `build.rs` regenerates `EventKind`, the scheduler's exhaustive `event_to_lua` match fails to compile until both arms exist, `tests/api_reference.rs` regenerates `docs/plugin-api/v1.md`; a written change request (contracts/plugin-api-v1.1.md §4) goes in the PR. Additive only — every 1.0 manifest still loads. 009 contract's "not delivered" line is superseded as 009 FR-027 anticipated. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No trait, no feature flag, no crate; `FocusArbiter` is a concrete struct; behaviour and `T` identical on all platforms; every panel element keyboard-operable with an accessible name (contracts/ui-transport-panel.md §2); strings in `transport.ftl`. **User override**: "Take back" is one click/one key path under every policy, user transport commands apply regardless of holder, and the auto policy hands focus back on any local transport action (FR-002, FR-008, SC-004). |
| Governance: engine/gateway/runtime sign-off | **Yes** | ✅ PASS | `crates/modplayer-capability-gateway/` (schema, event enum, token) and `crates/modplayer-plugin-runtime/` (binding routing, scheduler payloads) are modified → area-maintainer sign-off on the PR (GOV-3.2); no engine change. Requirement ids (FR-, SC-, PL-, EC-, NFR-, C-) are referenced in spec, plan, contracts and test names. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no crate, trait, flag
or `unsafe`; the RT path is untouched; the single gateway path and
values-not-panics rules are preserved and strengthened by the host-side
re-check; the arbiter is core-owned (III); the API change is additive
and schema-first (IX); the user's one-action override is structural
under all three policies (X).

## Project Structure

### Documentation (this feature)

```text
specs/010-transport-focus/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R16 with evidence from the 009 code
├── data-model.md        # Phase 1: FocusPolicy/FocusArbiter/FocusChange, transition table, view, settings, API delta, fixtures
├── quickstart.md        # Phase 1: automated gates (named suites), fixture launch, manual scenarios M1–M8
├── contracts/
│   ├── focus-arbitration.md     # core rules A1–A12, controller rules C1–C10, named tests
│   ├── plugin-api-v1.1.md       # API delta: request semantics, two events, change request text, runtime rules
│   └── ui-transport-panel.md    # panel placement/toggle, layout, a11y names, Fluent keys, tests
├── checklists/          # (created by /speckit-checklist, if run)
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)

docs/plugin-api/v1.md    # regenerated for API 1.1 by the gateway crate's test
```

### Source Code (repository root)

Existing layout (001–009) is kept; `+` marks new files, `~` modified
files, `−` removed items.

```text
locales/en-US/
├── transport.ftl                              + 14 keys (contracts/ui-transport-panel.md §3)
└── controls.ftl                               ~ + action-nav-toggle-transport-panel
plugins/fixtures/
├── focus-a/{plugin.toml, main.luau, README.md} + requests focus, seeks + arms loop on grant, logs events, hangs on 3rd play_state_changed
├── focus-b/{plugin.toml, main.luau, README.md} + requests focus, seeks on grant, seeks without focus → no_focus logged
└── wellbehaved/README.md                      ~ note: arm_loop reports no_focus until the user gives focus
docs/plugin-api/v1.md                          ~ regenerated (API 1.1)
crates/
├── modplayer-capability-gateway/
│   ├── api/v1.toml                            ~ minor = 1; + FocusGranted, FocusRevoked events
│   ├── src/event.rs                           ~ + HostEvent::{FocusGranted, FocusRevoked}{holder: OwnerInfo}; kind()
│   ├── src/focus.rs                           ~ + set_holder; − try_acquire, release_if, clear
│   ├── src/refusal.rs                         ~ − focus_held()
│   └── tests/{gateway.rs ~, api_reference.rs ~}
├── modplayer-plugin-runtime/
│   ├── src/bindings/mod.rs                    ~ − local RequestFocus/ReleaseFocus arms (fall through to rpc)
│   ├── src/bindings/transport.rs              ~ doc comments only
│   ├── src/scheduler.rs                       ~ event_to_lua: + FocusGranted/FocusRevoked → { holder }
│   └── tests/{bindings.rs ~, scheduler.rs ~}
├── modplayer-core/
│   ├── src/lib.rs                             ~ re-exports FocusPolicy, FocusHolder, TransportFocusView, FocusRow
│   ├── src/plugins/mod.rs                     ~ pub mod focus; re-exports
│   ├── src/plugins/focus.rs                   + FocusPolicy, FocusHolder, FocusArbiter, FocusChange, Vacancy (+ unit tests)
│   ├── src/plugins/view.rs                    ~ + TransportFocusView, FocusRow, from_records_and_arbiter
│   ├── src/plugins/host.rs                    ~ + arbiter field/accessors, apply_focus_changes; stop() vacates first
│   ├── src/plugins/apply.rs                   ~ RequestFocus/ReleaseFocus → controller; require_focus on 8 arms; actor scope in drain
│   ├── src/plugins/bundled.rs                 ~ + focus-a, focus-b packages (10 fixtures)
│   ├── src/controller.rs                      ~ TransportActor + with_transport_actor; note_local_transport_action hook in dispatch/arm_loop/disarm_loop; Remote scope for ApplyPendingTransferCommand and shutdown/sign-out stop(); on_track_changed in fan_out_plugin_playback_events; façade: transport_focus_view, focus_policy, set_focus_policy, focus_give, focus_take_back, focus_request, focus_release; launch() seeds policy
│   ├── src/settings/model.rs                  ~ AudioSettings.focus_policy, RawTransport, InvalidField::FocusPolicy
│   ├── src/settings/store.rs                  ~ read/write [transport]
│   ├── src/actions/catalog.rs                 ~ + HostAction::ToggleTransportPanel (T, Now Playing)
│   └── tests/{focus_arbiter.rs +, controller_transport_focus.rs +, controller_plugins_permissions.rs ~, controller_plugins_lifecycle.rs ~, settings.rs ~, actions.rs ~}
├── modplayer-ui/
│   ├── src/lib.rs                             ~ pub mod transport_view
│   ├── src/transport_view.rs                  + panel_open_id, toggle_transport_panel, show (holder, policy combo, take back, rows, empty state)
│   ├── src/now_playing.rs                     ~ header "Transport" toggle; draw panel after Effects, before Queue
│   ├── src/actions.rs                         ~ invoke: ToggleTransportPanel
│   └── tests/{transport_view.rs +, actions.rs ~, now_playing.rs ~, accessibility.rs ~, fluent_keys.rs ~}
└── modplayer/                                 (unchanged)
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
with one crate per architectural component (Constitution VII) and add
**no crate**: the arbiter is a host-primitive model and therefore lives
in `crates/modplayer-core/src/plugins/focus.rs` beside the `markers/`,
`effects/` and `plugins/` models it composes (Constitution III,
research R1); the API delta is confined to
`crates/modplayer-capability-gateway/api/v1.toml` and its generated
consumers (Constitution IX); the runtime crate only loses two local
arms and gains two payload renderers; the panel lives in
`crates/modplayer-ui/src/transport_view.rs` next to the
`effects_view.rs`/`plugins_view.rs` panels it mirrors; fixture packages
go in the existing top-level `plugins/fixtures/` embedded through
`bundled.rs`. The dependency graph is unchanged from 009
(`core → {gateway, runtime, engine, effects, …}`, `ui → {core, gateway,
…}`, `runtime → {gateway, engine}`, `gateway → {}`); the engine,
effects, source, receiver, audio-io, account, secure-store and binary
crates are not modified.

## Design notes that tasks must respect

1. **Schema first** (R6, Constitution IX): the `api/v1.toml` bump and
   the two `[[event]]` entries land before any Rust event arm; the
   exhaustive `HostEvent::kind()` and `event_to_lua` matches then force
   the arms; `docs/plugin-api/v1.md` is regenerated by the test, never
   hand-edited; the PR body carries contracts/plugin-api-v1.1.md §4.
2. **Core is the only token writer** (R1, RT-F2): `FocusToken::
   set_holder` is called from `PluginHost::apply_focus_changes` only;
   the CAS helpers are deleted, not deprecated.
3. **Arbiter is pure** (A1–A12): no channels, no records, no markers;
   it takes `PluginId`s and returns `FocusChange`s; `tests/focus_
   arbiter.rs` covers the whole table before the controller wiring.
4. **Fixed apply order** (C3): token → `Revoked` events → `Granted`
   events; `Fault` vacancies emit no `Revoked`.
5. **Actor scope has two non-default sites** (R3, C2):
   `drain_plugin_requests` (`Plugin(id)`) and
   `Effect::ApplyPendingTransferCommand` + shutdown/sign-out `stop()`
   (`Remote`); the hook checks `LocalUser` only. Adding a third internal
   caller of a transport method must wrap it.
6. **Hook placement is exact** (C2, FR-002a): entry of `dispatch` for
   the six transport `Input`s; after `Ok` in `arm_loop`/`disarm_loop`;
   nowhere else (volume, markers, cue-set, queue, chain, tempo,
   navigation and the panel's own buttons never revoke).
7. **Teardown order is 009's L7 plus one step** (C4): vacate → disarm
   owned loop → remove transient markers → orphan nodes → `Unloading`.
8. **Track reset shares the `TrackChanged` trigger** (R7, C6): no
   second definition of "track change".
9. **Panel reads a view** (R8, C9): the UI never sees records or the
   arbiter; rows are filtered in core.
10. **Policy is a setting, holder is not** (R5, C7): only
    `AudioSettings.focus_policy` is written; construction always starts
    at `Host`/empty.
11. **Rewrite, don't delete, 009's focus tests** (R10): the three named
    tests keep their intent under the new semantics.
12. **Fixtures are real packages** (R9): embedded via `include_str!`,
    visible only under `MODPLAYER_PLUGIN_FIXTURES=1`; focus-a's
    deliberate hang is documented in its README.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. Headless assumptions and plan-level
decisions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Arbiter in `modplayer-core`, gateway keeps only the atomic read cell (R1) | Constitution III makes focus a core-owned primitive; policy needs records, lifecycle and track events the gateway crate must not know. | Growing `gateway::focus` — breaks the crate's dependency-free design (009 R2) and puts policy in the enforcement layer. |
| `request_focus`/`release_focus` routed as RPCs (R2) | Under manual/auto a request must be recorded without granting; only core can deliver the asynchronous grant. | Keeping the plugin-thread CAS — cannot express "recorded but not granted". |
| Controller-scoped `TransportActor` with two non-default sites (R3) | Distinguish local user actions from plugin RPCs and remote/transfer commands at one compiler-checked point covering all keyboard and pointer paths. | Per-UI-site `note_user_action()` calls — 10+ sites, untestable from core, easy to miss; an `actor` parameter on every transport method — 30+ signature changes across 003–009 tests. |
| Host-side `no_focus` re-check on the eight focus-gated requests (R12) | Closes the admission→application race (one UI frame) so FR-001's "no side effect" is exact. | Trusting the atomic alone — a revoke can land after admission. |
| `Fault` vacancies send no `focus_revoked` (A8) | The plugin is suspended/disabled; its thread is exiting or gone (009 RT8) — an event would be dropped or wake a thread mid-teardown. | Sending anyway — inconsistent delivery, no consumer. |
| No-op host actions do not revoke (research "Open items") | A key press with nothing to act on should not demote a plugin; the reducer receives no `Input` for empty cue jumps and `arm_loop`/`disarm_loop` return `Err`. | Revoking on every invocation — surprising demotions for nothing. |
| Two new fixtures rather than extending `wellbehaved`/`flood` (R9) | Contention needs two `transport.control` plugins whose event history is assertable and manually drivable; existing fixtures assume an immediate grant. | Envelope injection only — cannot prove `no_focus` on a real plugin thread or the Lua payload. |
| focus-a hangs on its third `play_state_changed` (data-model §5) | Makes the "holder suspended → host within a second" manual scenario reproducible from the transport without a debug keybinding. | Relying on the flood fixture's CPU share — machine-dependent, not reproducible. |
| `T` as the default chord (R14) | Spec FR-013; verified unbound in the current catalog. | — |
| `en-US` resources only (R16) | Precedent 001–009; no `pt-BR` directory exists yet; all strings externalised so the pt-BR pass is additive. | Creating `pt-BR` for one panel — partial locale with 9 untranslated features. |
| Policy change never re-evaluates the pending queue (A10) | Spec edge case: the next qualifying event decides. | Immediate re-evaluation — could grant focus as a side effect of a settings change. |

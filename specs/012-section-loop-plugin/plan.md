# Implementation Plan: Section Loop Bundled Plugin

**Branch**: `feature/012-section-loop-plugin` | **Date**: 2026-09-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/012-section-loop-plugin/spec.md`

## Summary

Ship the first bundled plugin, **Section Loop** (`org.modplayer.section-
loop`): a Luau package under `plugins/bundled/` that a musician uses to
drop A and B around a passage and drill it hands-free. It is written
only against the public plugin API — the same `api.*` surface a
third-party author sees — so it doubles as living documentation, and it
ships under the host's `MIT OR Apache-2.0` licence with its own licence
copies. It declares `playback.observe`, `transport.control`,
`markers.read`, `markers.write`, `ui.panel`, `ui.overlay`, `ui.shortcuts`
(required) and `analysis.read` (optional, unused); registers 23 trigger
actions (`I`/`O`/`L`/`[`/`]` defaults); registers one panel (Set A, Set
B, Loop, Repeat count slider, marker list, inert Snap-to-beat +
explanation, Status); draws A/B lines, an accent region, labels and cue
dots as overlays; and keeps every marker in the host's per-track store,
so markers survive restarts and the plugin being disabled, and the
host's own marker UI keeps editing them.

Because the shipped `markers` API cannot express an A-only region or a
repeat count, the feature also ships plugin API **1.3** — an additive
minor bump of exactly `markers.set_loop_endpoint`,
`markers.set_loop_repeat` and a `regions` array in `markers.list()`
([contracts/plugin-api-v1.3.md](contracts/plugin-api-v1.3.md) §6 is the
Constitution IX change request).

Technical approach ([research.md](research.md)): schema-first bump in
`api/v1.toml` (R1); one owned endpoint helper in `TrackMarkers` reused by
the host's own `I`/`O` (R2); `set_loop_repeat` through the existing
recommit path (R3); `regions` in the existing local snapshot (R4);
package embedded via `bundled::packages()` (R5); a single-file script
with a relist-on-event state table and no debug probes (R6); local
playhead/focus reads (R7); 21 stable overlay ids rebuilt per list (R8);
`@key` strings with two host-side resolution gaps closed (R9);
host-observable tests (R10); plain panel buttons because the 011
conflict gate would make action-targeting buttons inert on a fresh
install (R11); nothing on the real-time path (R12).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–011; the plugin itself is Luau (`mlua` 0.12 runtime, 009 ADR-0001)

**Primary Dependencies**: existing only — mlua 0.12 (runtime crate), serde/serde_json 1, toml 0.9, thiserror 2, eframe/egui 0.36 (+accesskit), proptest 1 (dev), regex-free string scanning in the SC-008 test (std only). **No new crate** and no new feature flag (Constitution X).

**Storage**: none new. A/B and cue markers are 006's per-track marker file (`TrackMarkers`, `transient = false`), unchanged in shape; the plugin declares no `state.*` permission and writes no file. Session-only script memory: active-marker pointer, last-listed ids, un-applied repeat value.

**Testing**: `cargo test --workspace` — new `bundled_section_loop.rs` + `controller_section_loop.rs` (core, end to end on `FakeBackend` + `SyntheticSource` driving the real bundled package through `invoke_plugin_action`/`plugin_panel_interaction`), extensions to gateway `api_reference.rs`/`gateway.rs`, runtime `bindings.rs`, core `markers_model.rs` (incl. proptest)/`controller_markers.rs`/`plugins_manifest_discovery.rs`, ui `plugin_panels.rs`/`plugins_view.rs`; manual scenarios M1–M9 in [quickstart.md](quickstart.md). CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers) on ubuntu / macos / windows.

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical plugin, shortcuts and behaviour on all three; no platform-specific code.

**Project Type**: Desktop application — the 13-crate Cargo workspace is unchanged (no new crate; changes in `modplayer-capability-gateway`, `modplayer-plugin-runtime`, `modplayer-core`, `modplayer-ui` tests, plus the new package under `plugins/bundled/` and the regenerated API reference).

**Performance Goals**: zero work on the audio thread (no engine/effects change; seam, wrap count and finite release remain 006's host-side real-time logic — FR-009). Per user action: ≤ 3 RPCs (`request_focus` + `arm_loop`, or one marker call) drained in one controller tick; per `marker_changed` tick: one local `markers.list()` + `clear_overlays` + `add_overlays` (≤ 21 primitives) + ≤ 2 `update_widget` — well under the `ui` bucket's 100/s and the `markers` bucket. SC-001's 10-second drill is bounded by the user's clicks, not the plugin.

**Constraints**: Constitution I — no RT-path edit, PR note "N/A"; II — every call through `Gateway::admit`, refusals are values, faults contained by 009; III — the plugin only composes host primitives (the whole reason for API 1.3 instead of script-side state); VI — no network/credential/asset surface (no icon, no glyphs); VII — `forbid(unsafe_code)`, no `unwrap`, doc examples on the new public model fn; VIII — proptest on the endpoint helper, permission-enforcement and isolation cases, manual sign-off; IX — one schema edit, regenerated reference, written change request; X — no trait/flag/crate, every panel control keyboard-operable and named, strings in the manifest table, the user can disable the plugin / take focus back / rebind any chord in one action. Out of scope: beat snapping, "loop last N beats", MIDI, per-marker nudge actions, host `nudge_step_ms` honouring, plugin-manager detail page, hot-cue hold.

**Scale/Scope**: gateway ≈ 60 LOC (`v1.toml` +2 requests, `request.rs` DTOs/variants, `build.rs` untouched) + 40 test; runtime ≈ 80 LOC (`bindings/markers.rs` two calls, `bindings/mod.rs` response/`regions` conversion, `handle.rs` field) + 60 test; core ≈ 220 LOC (`markers/model.rs` helper refactor 60, `controller.rs` façade + snapshot 60, `plugins/apply.rs` 2 arms + resolve_string 50, `plugins/bundled.rs` 15, `lib.rs` re-exports) + ≈ 900 test; plugin package ≈ 350 Luau lines + manifest (≈ 55 string keys) + readme + 2 licence copies; ui ≈ 0 LOC + 60 test; API 1.3 (2 requests, 0 events); `docs/plugin-api/v1.md` regenerated; `plugins/bundled/README.md` updated.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No | ✅ PASS | No change to `modplayer-engine`/`modplayer-effects`. The two new requests reach the engine only through the controller's existing `recommit_if_armed` → buffer-boundary `Command` path (R3/R12); seam and wrap counting stay host-side (FR-009). PR real-time note: "N/A — no real-time path changes." |
| II. Plugins Are Guests (non-negotiable) | **Yes** | ✅ PASS | Section Loop is an ordinary sandboxed Luau plugin with pre-approved bundled grants; both new calls are `markers.write`-gated, rate-limited in the `markers` bucket, owner-checked in `apply.rs` (`not_owner`/`not_found` values, never panics); a handler throw/hang is contained by 009's budget (contract L3); disable/suspend releases focus and the loop via 010 FR-007 with no plugin code. |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | The plugin never holds marker/loop state of its own: A-only regions, repeat counts and `regions[]` are added to the **host** API precisely so the script composes primitives instead of reimplementing them (R1); every view the script has is rebuilt from `markers.list()` (contract A12/E1). |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No source crate touched; all automated tests run on `SyntheticSource` + `FakeBackend`. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | No request, event, overlay or widget carries sample data; `decoded_store_boundary.rs` unchanged. |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | No network, files, clipboard or credential permission; no icon/glyph assets; plugin strings are `@key` lookups rendered as text (011 FR-022); the package is embedded at build time like every bundled package (NFR-4.4 inheritance). |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate; `forbid(unsafe_code)` and the `unwrap`/`expect` denies apply in every touched crate; new public `TrackMarkers::set_loop_endpoint_owned` carries a doc example; refusals via existing `Refusal` constructors; SPDX headers on new `.rs` files; `cargo deny` unaffected (no dependency change). |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Test-first on the contracts' named tests; **permission enforcement** (`markers.write` required, no focus needed) and **plugin isolation** (disable mid-loop, handler containment) suites; proptest on `set_loop_endpoint_owned` (marker/loop arithmetic); loop-seam tests untouched and still gate; manual scenarios M1–M9 executed by the implementing agent (Governance), M9 under VoiceOver. |
| IX. One Plugin API Definition | **Yes** | ✅ PASS | `api/v1.toml` is the single edit point (`minor = 3`, 2 `[[request]]`); generated `RequestKind` forces every dispatch arm; `tests/api_reference.rs` regenerates `docs/plugin-api/v1.md`; written change request in contracts/plugin-api-v1.3.md §6 goes in the PR body. Additive only. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No trait, flag or crate; one script file; identical behaviour on all platforms; every panel control keyboard-operable with an accessible name (SC-006); all strings externalised (`[strings.en-US]`); **user override**: the host's `I`/`O`/`L` keep winning on a fresh install (011 FR-011), the user can take focus back / disable the plugin / rebind any chord in one action; the host's Markers panel edits the plugin's markers at any time. |
| Governance: engine/gateway/runtime sign-off | **Yes** | ✅ PASS | `crates/modplayer-capability-gateway/` (schema, DTOs) and `crates/modplayer-plugin-runtime/` (bindings, snapshot) are modified → area-maintainer sign-off on the PR (GOV-3.2); no engine change. Requirement ids (FR-, SC-, PL-, EC-, NFR-, GOV-) referenced throughout spec, plan, contracts and test names. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no crate, trait,
flag or `unsafe`; the RT path is untouched; the API change is additive,
schema-first and owner-gated; the plugin holds no primitive state of
its own; the one spec-level deviation (plain panel buttons, R11) is a
wiring choice that *strengthens* the user-override guarantee rather
than weakening any principle.

## Project Structure

### Documentation (this feature)

```text
specs/012-section-loop-plugin/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R12 with evidence from the 006/009/010/011 code
├── data-model.md        # Phase 1: API 1.3 DTOs, host-model delta, package manifest/state/panel/actions/strings
├── quickstart.md        # Phase 1: automated gates (named suites), launch, manual scenarios M1–M9
├── contracts/
│   ├── plugin-api-v1.3.md       # API delta: 2 requests, list().regions, change request text, named tests
│   └── section-loop-plugin.md   # Package/registration/action/event/overlay/lifecycle rules P, G, A, E, O, L + named tests
├── checklists/          # (created by /speckit-checklist, if run)
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)

docs/plugin-api/v1.md    # regenerated for API 1.3 by the gateway crate's test
```

### Source Code (repository root)

Existing layout (001–011) is kept; `+` marks new files, `~` modified
files.

```text
plugins/
├── bundled/
│   ├── README.md                                  ~ lists Section Loop; drops "no production plugin yet"
│   └── org.modplayer.section-loop/
│       ├── plugin.toml                            + manifest: 7 required + 1 optional permission, api 1.3, [strings.en-US]
│       ├── main.luau                              + the plugin (living example; contracts/section-loop-plugin.md)
│       ├── README.md                              + what it does, keys, how it uses the API
│       ├── LICENSE-MIT                            + copy of /LICENSE-MIT
│       └── LICENSE-APACHE                         + copy of /LICENSE-APACHE
└── fixtures/                                      (unchanged; focus-b reused by US3 tests / M6)
docs/plugin-api/v1.md                              ~ regenerated (API 1.3)
crates/
├── modplayer-capability-gateway/
│   ├── api/v1.toml                                ~ minor = 3; + SetLoopEndpoint, SetLoopRepeat
│   ├── src/request.rs                             ~ LoopEndpoint, RepeatArg, RegionInfo; Request::{SetLoopEndpoint, SetLoopRepeat}; Response::{Markers.regions, LoopEndpoint}
│   └── tests/{api_reference.rs ~, gateway.rs ~}
├── modplayer-plugin-runtime/
│   ├── src/handle.rs                              ~ PluginSnapshot.regions
│   ├── src/bindings/markers.rs                    ~ set_loop_endpoint, set_loop_repeat (arg validation)
│   ├── src/bindings/mod.rs                        ~ dispatch arms (rpc); ListMarkers copies regions; response_to_lua regions/LoopEndpoint
│   └── tests/bindings.rs                          ~
├── modplayer-core/
│   ├── src/markers/model.rs                       ~ set_loop_endpoint_owned (pub), set_loop_a/b delegate
│   ├── src/markers/mod.rs                         ~ re-export if needed
│   ├── src/controller.rs                          ~ plugin_set_loop_endpoint; snapshot regions
│   ├── src/plugins/apply.rs                       ~ 2 arms; resolve_string on UpdateWidget text + overlay labels
│   ├── src/plugins/bundled.rs                     ~ packages() = [section_loop()]
│   └── tests/{bundled_section_loop.rs +, controller_section_loop.rs +, markers_model.rs ~, controller_markers.rs ~, plugins_manifest_discovery.rs ~}
├── modplayer-ui/
│   └── tests/{plugin_panels.rs ~, plugins_view.rs ~}
└── modplayer/                                     (unchanged)
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
with one crate per architectural component (Constitution VII) and add
**no crate**. The plugin is a data package, not Rust: it lives in the
existing top-level `plugins/bundled/` directory (created empty by 009
with a README that names this exact folder convention) and is embedded
through the existing `crates/modplayer-core/src/plugins/bundled.rs`
`packages()` hook, so discovery, grants, enabling and the Plugins list
need no new code. The API 1.3 delta follows 010/011's precedent: schema
in `crates/modplayer-capability-gateway/api/v1.toml`, DTOs beside the
other read-model types in `src/request.rs`, Lua glue in
`crates/modplayer-plugin-runtime/src/bindings/markers.rs`, and the host
mutation in `crates/modplayer-core/src/markers/model.rs` +
`src/plugins/apply.rs` (host primitives, Constitution III). No
`modplayer-ui` source changes are expected — the panel, overlays and
marker list are rendered by 011's existing widgets — only UI tests are
added. The dependency graph is unchanged (`core → {gateway, runtime,
…}`, `ui → {core, gateway}`, `runtime → {gateway, engine}`, `gateway →
{}`).

## Design notes that tasks must respect

1. **Schema first** (R1, Constitution IX): the `v1.toml` bump lands
   before any Rust arm; the exhaustive matches then force the arms;
   `docs/plugin-api/v1.md` is regenerated by the test, never hand-edited;
   the PR body carries contracts/plugin-api-v1.3.md §6.
2. **One endpoint helper** (R2): `set_loop_a`/`set_loop_b` must
   delegate to `set_loop_endpoint_owned`; the 006 host tests are the
   regression net and must pass unmodified.
3. **Validate on the plugin thread, own-check in core** (data-model §1.2/
   §2.3): `which`/`repeat` are validated in the binding before the RPC;
   ownership and track state in `apply.rs`.
4. **`regions` is snapshot data** (R4): no new RPC; refreshed by the
   existing revision gate.
5. **Plain panel buttons** (R11): "Set A"/"Set B" carry no `action`
   field; their `panel_interaction` and the keyboard actions call the
   same script functions.
6. **`request_focus()` is always the first call in `toggle_loop` /
   `jump_cue_n` handlers** (contract A11); no deferred arm on
   `focus_granted`.
7. **Relist, never reinterpret** (contract A12/E1): the script rebuilds
   its view from `markers.list()` on `track_changed`/`marker_changed`
   only; after a refusal it changes nothing itself.
8. **No debug probes in the shipped plugin** (R6): tests read host
   state (data-model §4).
9. **Every string is `@key`** (R9): including Status hints and overlay
   labels; `apply.rs` resolves `UpdateWidget` text and overlay label
   text — a host `message` echoed to Status is not `@`-prefixed and
   passes through.
10. **Two existing "empty plugin list" tests change meaning** (R5): they
    now expect exactly Section Loop without fixtures; the empty state is
    tested through an explicit empty package list.
11. **Licence copies are byte-identical** to the root files and a test
    asserts it (contract P1).
12. **Manual scenarios M1–M9 are executed by the implementing agent**
    (Governance); deviations become regression tests.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. Headless assumptions, the one spec-level
deviation and plan-level decisions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| **Spec deviation — FR-004 "button targeting action `set_a`/`set_b`" implemented as plain buttons routed in-script (R11)** | Verified: `ActionRegistry::is_invocable` returns false while any chord of the action is in conflict, and 011 D3/G14 gates panel-button dispatch on it; on a fresh install `I`/`O` conflict with the host, so the buttons would be inert — contradicting FR-016 and every "from a fresh install" acceptance scenario. | Relaxing 011's G14 gate — ratified rule pinned by `inactive_action_not_dispatched`; not this feature's to reopen. Shipping `set_a`/`set_b` unbound — contradicts FR-005's default bindings and the prompt. |
| API 1.3: two new `markers.write` requests + `list().regions` (R1) | Constitution III forbids script-side loop state; the 1.2 surface cannot create an A-only region or set a repeat count. | Script "pending A" + `create_loop` — no persisted/visible A-only region, and a `loop_wrapped` counter cannot guarantee SC-005. Overloading `create_loop(a, nil)` — changes an existing call's contract. |
| `regions` in the snapshot rather than a new request (R4) | `markers.list()` is local already; keeps one read path, zero RPC. | `api.markers.regions()` RPC — duplicates data and adds a frame of latency for nothing. |
| Refactor the host's private `set_loop_endpoint` into an owned helper (R2) | One body for host `I`/`O` and plugin A/B guarantees identical swap/clamp/limit/naming behaviour and one proptest. | A second plugin-only function — duplicated invariants that drift. |
| Plugin region sets `current_region` (A) | The model's I8 rule; lets the host's own `L` arm the plugin's region — the user-override the spec's Edge Cases and Constitution X require. | Leaving `current_region` alone — plugin region unreachable from the host keyboard. |
| `resolve_string` extended to `UpdateWidget` text and overlay labels (R9) | 011's contract says any user-facing string may be `@key` but the implementation covered three arms; FR-018 needs the other two. | A duplicate Lua string table in the script — contradicts FR-018 and teaches the wrong pattern. |
| Per-slot string keys for "Cue n" texts (R9) | String tables have no interpolation (011 R17). | Concatenation in script — the strings would no longer come from the manifest table. |
| No `debug_probe` in the shipped plugin (R6) | The reference plugin should not teach a fixture-only mechanism; every acceptance is host-observable. | A probe handler — simpler tests, worse example code. |
| Existing "empty list" tests re-targeted (R5) | A bundled package now always exists. | Gating Section Loop behind an env var — it would no longer be "bundled, enabled by default". |
| Package folder named by identifier (`org.modplayer.section-loop/`) (A) | `plugins/bundled/README.md` states `<identifier>/` literally. | Short name `section-loop/` — fixtures use short names, but the bundled README's own convention wins. |
| `repeat` wire domain `1..=1000 \| "infinite"`, slider `0` mapped in-script (R3) | DM-7 domain; 011 sliders have no "infinite" stop. | `0` = infinite on the wire — leaks a UI convention into the API. |
| Nudge step fixed at 10 ms (spec Clarifications) | No plugin API reads host settings. | A settings-read API — a separate wave. |

# Implementation Plan: Key & Tempo Bundled Plugin and Getting Started Panel

**Branch**: `013-key-and-tempo-plugin` | **Date**: 2026-09-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/013-key-and-tempo-plugin/spec.md`

## Summary

Ship the second bundled plugin, **Key & Tempo**
(`org.modplayer.key-tempo`): a Luau package under `plugins/bundled/`
that transposes a song by semitones (with fine cents) or changes its
tempo (25–200 %) without touching the other, and remembers a track's
settings when asked. It is written only against the public plugin API
and released under the host's `MIT OR Apache-2.0` licence with its own
licence copies. It declares `audio.effects`, `ui.panel`, `ui.shortcuts`,
`state.track`, `playback.observe`; creates one pitch-shift and one
time-stretch node adjacent in the chain; registers 8 trigger actions
(`tempo_up`/`tempo_down` default `+`/`-`); registers one panel (Key,
Fine tune, Tempo, Tempo step sliders; Formant, Quality, Remember, Keep
across tracks toggles; "Track memory" and "Shortcut note" text rows;
six buttons); and keeps one `settings` entry per remembered track in
the host's per-track store. The same slice adds the host-native,
dismissible **Getting started** card at the top of the Library view
(device-scoped flag, placeholder tutorial link) and confirms both
bundled plugins ship enabled with pre-approved grants.

Because the shipped API 1.3 exposes no node parameter values to plugins
and fires `effect_chain_changed` only on structural changes, the
feature also ships plugin API **1.4** — additive: `NodeInfo.params`
(wire name → clamped target value) and `NodeInfo.auto_switched`,
`set_param` accepting wire names and boolean/enum values, and
`effect_chain_changed` firing (coalesced per tick) on any parameter or
mode change by any actor ([contracts/plugin-api-v1.4.md](contracts/plugin-api-v1.4.md)
§7 is the Constitution IX change request). Research also found a latent
009 defect — `effect_chain_changed` was undeliverable for a chain with
a plugin-owned node — fixed here with a regression test (R4).

Technical approach ([research.md](research.md)): schema-first bump with
a documentary `[[node_kind]]` table (R1); wire names and enum names in
the effects catalog, resolved in `apply.rs`, numeric form unchanged
(R2); one `revision` bump site in `ChainModel` on changed targets/modes,
mode-id writes routed through `set_mode` (R3); scheduler payload built
field-by-field (R4); a mirror-the-host state machine with a single
`settings` key (R5); plain panel buttons because the 011 conflict gate
would make action-targeting buttons inert on a fresh install (R6, the
same deviation 012 recorded); `Plus`/`Minus` defaults that lose to the
host's fixed-step action (R7); 012's package/embedding/licence shape
(R8); `@key` strings everywhere (R9); a settings-file flag plus a
`links.rs` constant and an `App`-owned `open_url` for the card (R10);
host-observable tests on the real package (R11); nothing on the RT path
(R12); budget headroom (R13); orphan-reuse on restart/re-enable (R14).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–012; the plugin itself is Luau (`mlua` 0.12 runtime, 009 ADR-0001)

**Primary Dependencies**: existing only — mlua 0.12 (runtime crate), serde/serde_json 1, toml 0.9, thiserror 2, eframe/egui 0.36 (+accesskit), fluent-templates 0.15, proptest 1 (dev). **No new crate, no new feature flag, no new dependency edge** (Constitution X); the wire-name lookup lives in `modplayer-effects`, which `modplayer-core` already depends on.

**Storage**: (1) per-track plugin state — one `settings` key per remembered track in 009's account-scoped `state.track` store (≈ 90 bytes, well under the 1 MB value cap), written by the plugin only; (2) the per-user settings file gains `[onboarding] getting_started_dismissed` (device-scoped, atomic replace, untouched by sign-out); (3) session-only script memory (node ids, mirrored values, last-sent split, `keep_across`, `step`). Nothing new on disk for the effect chain (008 FR-002: empty on every launch).

**Testing**: `cargo test --workspace` — new `controller_key_tempo.rs` + `bundled_key_tempo.rs` (core, end to end on `FakeBackend` + `ScriptedHost` driving the real bundled package), extensions to gateway `api_reference.rs`/`manifest.rs`, runtime `bindings.rs` (incl. the R4 regression), core `effects_model.rs` (wire names ⇄ schema, revision rules, proptest on the projection's clamp/round-trip)/`controller_effects.rs`/`controller_plugins_permissions.rs`/`plugins_manifest_discovery.rs`/`settings.rs`/`persist.rs`, ui `library_view.rs`/`first_launch.rs`/`plugin_panels.rs`/`accessibility.rs`/`fluent_keys.rs`; manual scenarios M1–M10 in [quickstart.md](quickstart.md). CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers, reference regeneration diff) on ubuntu / macos / windows.

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical plugin, shortcuts, card and behaviour on all three; no platform-specific code.

**Project Type**: Desktop application — the 13-crate Cargo workspace is unchanged (no new crate; changes in `modplayer-capability-gateway`, `modplayer-plugin-runtime`, `modplayer-effects` (catalog tables only), `modplayer-core`, `modplayer-ui`, plus the new package under `plugins/bundled/` and the regenerated API reference).

**Performance Goals**: zero work on the audio thread (no engine/effects RT change; the 20 ms ramp and 5 ms crossfade stay 008's real-time logic — FR-015). Control side: one `Vec<NodeInfo>` projection (≤ 16 nodes × ≤ 32 params) per tick in which any parameter changed, coalesced to ≤ 1 `effect_chain_changed` per tick; per plugin `track_changed`: ≤ 1 store read + ≤ 5 `set_param` + ≤ 9 `update_widget`; per `effect_chain_changed`: 0 RPC reads, ≤ 9 `update_widget`, ≤ 1 store write — inside the `effects`/`ui` buckets (100/s). SC-001's "within 20 ms" is 008's existing ramp budget; the plugin adds one controller tick of latency for the panel mirror only, never for the audio.

**Constraints**: Constitution I — no RT-path edit, PR note "N/A"; II — every call through `Gateway::admit`, refusals are values, faults contained by 009; III — the plugin only parameterises host nodes (the whole reason for API 1.4 instead of script-side guesses); VI — no network/credential/asset surface; the tutorial URL is opened by the host from `App` only; VII — `forbid(unsafe_code)`, no `unwrap`, doc examples on the new public catalog fns; VIII — regression test for R4, proptest on the settings round trip and the projection, permission-enforcement and isolation cases, manual sign-off; IX — one schema edit, regenerated reference, written change request; X — no trait/flag/crate, every control keyboard-operable and named, strings externalised, the user can disable the plugin / remove its nodes / rebind any chord in one action. Out of scope: detected-key display (FR-13.2.5), MIDI, the tutorial content, absolute-time `schedule_param`.

**Scale/Scope**: gateway ≈ 40 LOC (`v1.toml` minor + `[[node_kind]]`, `request.rs` `ParamValue`/`ParamRef`/`ParamArg` + 2 `NodeInfo` fields, `api_reference.rs` section) + 40 test; effects catalog ≈ 120 LOC (wire/enum tables, 4 fns) + 60 test; runtime ≈ 80 LOC (`bindings/effects.rs` value parsing, `bindings/mod.rs` `params` table, `scheduler.rs` R4 fix) + 120 test; core ≈ 260 LOC (`effects/model.rs` revision/mode routing 40, `controller.rs` `node_info` helper + getting-started 60, `plugins/apply.rs` resolution 60, `settings/model.rs` 30, `links.rs` 5, `plugins/bundled.rs` 15, test-helper `call()` updates) + ≈ 1 100 test; ui ≈ 90 LOC (`getting_started.rs` 60, `app.rs` wiring 30) + 150 test; locales +5 keys; plugin package ≈ 420 Luau lines + manifest (28 string keys) + readme + 2 licence copies; `docs/plugin-api/v1.md` regenerated; `plugins/bundled/README.md` updated. API 1.4: 0 requests, 0 events, 2 `NodeInfo` fields, 2 argument widenings, 1 event-trigger widening.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No | ✅ PASS | No change to `modplayer-engine` or `modplayer-effects` RT code; the catalog additions are `const`/`static` tables read on the control thread (R12). `params` values come from `ChainModel` (control-side targets), never the audio thread (R3). `Command::ChainSetParam` and the ramp/crossfade are untouched; 008's latency and loop-seam tests still gate. PR real-time note: "N/A — no real-time path changes." |
| II. Plugins Are Guests (non-negotiable) | **Yes** | ✅ PASS | Key & Tempo is an ordinary sandboxed Luau plugin with pre-approved bundled grants; every call is `audio.effects`/`state.track`/`ui.*`-gated, rate-limited and owner-checked in `apply.rs` (`not_owner`/`not_found`/`invalid_argument` values, never panics); a handler throw/hang is contained by 009's budget (contract L3); disable/suspend orphans its nodes with no plugin code (L1). The 1.4 widening adds no permission and refuses bad arguments as values (api §2). |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | The plugin holds no DSP and no parameter truth of its own: `params` and the wider `effect_chain_changed` exist precisely so the script **mirrors** the host's nodes instead of guessing (R1/R5, contract E1); pitch/time DSP, clamping, auto-switch and ramps stay 008's. |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No source crate touched; all automated tests run on `ScriptedHost`/`SyntheticSource` + `FakeBackend`. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | No request, event, widget or setting carries sample data; `params` are scalar targets; `decoded_store_boundary.rs` unchanged. |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | No network, files, clipboard or credential permission for the plugin; no icon/glyph assets; strings are `@key` lookups rendered as text (011 FR-022); the package is embedded at build time (NFR-4.4 inheritance). The tutorial link is a host constant opened via `egui::Context::open_url` from `App` only (002 design note 10) — no plugin can open a URL. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate; `forbid(unsafe_code)` and the `unwrap`/`expect` denies apply in every touched crate; new public catalog fns carry doc examples; refusals via existing `Refusal` constructors; SPDX headers on new `.rs` files; `cargo deny` unaffected (no dependency change); the reference-regeneration diff check runs in CI. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Test-first on the contracts' named tests; the R4 defect ships with `effect_chain_changed_with_plugin_owned_node_reaches_handler`; **permission enforcement** (`set_param` by name still owner-gated) and **plugin isolation** (disable mid-shift, node removal, Section Loop coexistence) suites; proptest on the `[onboarding]` settings round trip (state serialization) and on the `params` projection (clamp ⇄ wire value); manual scenarios M1–M10 executed by the implementing agent (Governance), M10 under VoiceOver. |
| IX. One Plugin API Definition | **Yes** | ✅ PASS | `api/v1.toml` is the single edit point (`minor = 4`, `[[node_kind]]`); `tests/api_reference.rs` regenerates `docs/plugin-api/v1.md` and CI diffs it; `wire_names_match_api_schema` pins the catalog to the schema; written change request in contracts/plugin-api-v1.4.md §7 goes in the PR body. Additive only — every 1.0–1.3 fixture and Section Loop load unchanged (`legacy_1_3_fixtures_still_load`). |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No trait, flag or crate; one script file; identical behaviour on all platforms; every panel control and the card's two controls keyboard-operable with accessible names (SC-006, card §3); all strings externalised (`[strings.en-US]`, `app.ftl`); **user override**: the host's `+`/`-` keep winning on a fresh install (011 FR-011), the user can disable the plugin, remove or bypass its nodes in the Effect Chain panel (never fought — contract L4), and rebind any chord in one action. |
| Governance: engine/gateway/runtime sign-off | **Yes** | ✅ PASS | `crates/modplayer-capability-gateway/` (schema, DTOs) and `crates/modplayer-plugin-runtime/` (bindings, scheduler) are modified → area-maintainer sign-off on the PR (GOV-3.2); no engine change. Requirement ids (FR-, SC-, PL-, NFR-, GOV-, US-) referenced throughout spec, plan, contracts and test names. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no crate, trait,
flag or `unsafe`; the RT path is untouched; the API change is additive,
schema-first and owner-gated; the plugin holds no primitive state of
its own; the two spec-level deviations (plain panel buttons, R6;
`set_param` widened to accept names because the shipped API takes
numeric ids, R2) are wiring choices that keep the user-override
guarantee and the additive-only rule intact rather than weakening any
principle.

## Project Structure

### Documentation (this feature)

```text
specs/013-key-and-tempo-plugin/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R14 with evidence from the 008/009/011/012 code
├── data-model.md        # Phase 1: API 1.4 DTOs, catalog/model/controller/settings delta, package manifest/state/panel/actions/strings, card
├── quickstart.md        # Phase 1: automated gates (named suites), launch, manual scenarios M1–M10
├── contracts/
│   ├── plugin-api-v1.4.md       # API delta: NodeInfo.params/auto_switched, set_param widening, event trigger, wire names, change request, named tests
│   ├── key-tempo-plugin.md      # Package/registration/action/event/memory/lifecycle rules P, G, A, E, M, L + named tests
│   └── getting-started-card.md  # Card visibility/content/accessibility/persistence + bundled-default rules + named tests
├── checklists/          # (created by /speckit-checklist, if run)
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)

docs/plugin-api/v1.md    # regenerated for API 1.4 by the gateway crate's test
```

### Source Code (repository root)

Existing layout (001–012) is kept; `+` marks new files, `~` modified
files.

```text
plugins/
├── bundled/
│   ├── README.md                                  ~ lists Key & Tempo; drops the "013 will be added" note
│   ├── org.modplayer.section-loop/                (unchanged)
│   └── org.modplayer.key-tempo/
│       ├── plugin.toml                            + manifest: 5 required permissions, api 1.4, [strings.en-US] (28 keys)
│       ├── main.luau                              + the plugin (living example; contracts/key-tempo-plugin.md)
│       ├── README.md                              + what it does, keys, how it uses the API (incl. params mirroring)
│       ├── LICENSE-MIT                            + copy of /LICENSE-MIT
│       └── LICENSE-APACHE                         + copy of /LICENSE-APACHE
└── fixtures/                                      (unchanged; 1.0–1.3 manifests must still load)
docs/plugin-api/v1.md                              ~ regenerated (API 1.4, "Node parameters" section)
locales/en-US/app.ftl                              ~ + getting-started-* keys
crates/
├── modplayer-capability-gateway/
│   ├── api/v1.toml                                ~ minor = 4; + [[node_kind]] × 6
│   ├── src/request.rs                             ~ ParamValue, ParamRef, ParamArg; NodeInfo.params/auto_switched; SetParam/ScheduleParam fields
│   └── tests/{api_reference.rs ~, manifest.rs ~}
├── modplayer-effects/
│   ├── src/catalog.rs                             ~ param_wire_name, param_by_wire_name, enum_names, wire_shape (+ const tables)
│   └── src/lib.rs                                 ~ re-exports if needed
├── modplayer-plugin-runtime/
│   ├── src/bindings/effects.rs                    ~ set_param/schedule_param take mlua::Value param/value
│   ├── src/bindings/mod.rs                        ~ node_info_to_lua params/auto_switched, pub(crate)
│   ├── src/scheduler.rs                           ~ EffectChainChanged payload via node_info_to_lua (R4)
│   └── tests/bindings.rs                          ~
├── modplayer-core/
│   ├── src/effects/model.rs                       ~ revision bump on changed target/mode; mode-id → set_mode; rate bump
│   ├── src/controller.rs                          ~ node_info() helper; getting_started_dismissed(), dismiss_getting_started()
│   ├── src/plugins/apply.rs                       ~ ParamRef/ParamArg resolution + refusals
│   ├── src/plugins/bundled.rs                     ~ packages() = [section_loop(), key_tempo()]
│   ├── src/settings/model.rs                      ~ AudioSettings.getting_started_dismissed; [onboarding] raw section
│   ├── src/links.rs                               ~ GETTING_STARTED_TUTORIAL_URL
│   └── tests/{controller_key_tempo.rs +, bundled_key_tempo.rs +, effects_model.rs ~, controller_effects.rs ~,
│              controller_plugins_permissions.rs ~, plugins_manifest_discovery.rs ~, settings.rs ~, persist.rs ~,
│              controller_section_loop.rs ~, bundled_section_loop.rs ~ (two-package expectation)}
├── modplayer-ui/
│   ├── src/getting_started.rs                     + the card (show → GettingStartedOutcome)
│   ├── src/lib.rs                                 ~ mod getting_started
│   ├── src/app.rs                                 ~ show_library draws the card; OpenTutorial → open_url; Dismiss → controller
│   └── tests/{library_view.rs ~, first_launch.rs ~, plugin_panels.rs ~, accessibility.rs ~, fluent_keys.rs ~, plugins_view.rs ~}
└── modplayer/                                     (unchanged)
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
with one crate per architectural component (Constitution VII) and add
**no crate**. The plugin is a data package, not Rust: it lives in the
existing top-level `plugins/bundled/` directory beside
`org.modplayer.section-loop/` and is embedded through the existing
`crates/modplayer-core/src/plugins/bundled.rs` `packages()` hook, so
discovery, grants, enabling and the Plugins list need no new code. The
API 1.4 delta follows 010/011/012's precedent: schema in
`crates/modplayer-capability-gateway/api/v1.toml`, DTOs beside the
other read-model types in `src/request.rs`, Lua glue in
`crates/modplayer-plugin-runtime/src/bindings/effects.rs` +
`scheduler.rs`, and the host behaviour in
`crates/modplayer-core/src/effects/model.rs` + `src/plugins/apply.rs`
(host primitives, Constitution III). Wire-name tables sit in
`crates/modplayer-effects/src/catalog.rs` next to the parameter
definitions they name — the one place that already knows every kind's
parameters — and are read only by core. The Getting Started card is a
new `modplayer-ui` module wired from `App::show_library`, with its flag
in `modplayer-core`'s settings model and its URL in `links.rs`, exactly
where 002 put the disclosure flag and the status/upgrade URLs. The
dependency graph is unchanged (`core → {gateway, runtime, effects, …}`,
`ui → {core, gateway}`, `runtime → {gateway, engine}`, `gateway → {}`).

## Design notes that tasks must respect

1. **Schema first** (R1, Constitution IX): the `v1.toml` bump and
   `[[node_kind]]` table land before any Rust change; `docs/plugin-api/
   v1.md` is regenerated by the test, never hand-edited; the PR body
   carries contracts/plugin-api-v1.4.md §7.
2. **Numeric `set_param` keeps 1.3 semantics byte for byte** (R2): the
   `Id`/`Number` path is the old code; only `Name`/`Bool`/`Name` are new.
   Resolution happens in `apply.rs`, never in the runtime crate.
3. **One bump site** (R3): `ChainModel::set_param`/`set_mode`/
   `set_source_rate` bump `revision` only on an actual change; the
   controller's per-tick diff is untouched. Mode-id writes delegate to
   `set_mode`.
4. **Fix R4 before writing the plugin**: until `scheduler.rs` builds
   the payload with `node_info_to_lua`, no `effect_chain_changed` test
   with a plugin-owned node can pass.
5. **One `node_info()` projection** in the controller feeds both the
   snapshot and the event; `params` comes from `NodeModel.params` via
   `wire_shape`, `auto_switched` from `mode_state`.
6. **Plain panel buttons** (R6): the six buttons carry no `action`
   field; `panel_interaction` and `action_invoked` call the same
   functions. Keyboard actions stay registered with `Plus`/`Minus`.
7. **Mirror, never reinterpret** (contract E1/A12): widgets and the
   stored entry are rewritten only from `params`; the `S.sent`
   tie-break keeps the user's own key/cents split; after a refusal the
   script changes nothing itself.
8. **One `settings` key** (contract M1–M6): full table per write, dedup
   against `S.last_written`, field-by-field defaults on read, removed
   on remember-off, `keep_across` never persisted.
9. **Restart/re-enable never issues `set_param`** (G2 adopt path,
   SC-005); only the "neither node" path runs the track logic.
10. **Never recreate a removed node while Active** (A11/L4); the
    `restored` text explains; the next `ready()` recreates the missing
    member in the right position.
11. **`open_url` only from `App`** (R10, 002 design note 10); the card
    module returns an outcome and touches neither the controller nor
    the browser.
12. **Every string is `@key` / Fluent** (R9): including the badge, the
    step note, the node-removed message and the card; a host refusal
    `message` echoed to `restored` passes through unprefixed.
13. **Two existing "exactly Section Loop" expectations change** (R8):
    `packages()`, the Plugins list and discovery tests now expect both
    bundled plugins in declaration order.
14. **Licence copies are byte-identical** to the root files and a test
    asserts it (contract P1).
15. **Manual scenarios M1–M10 are executed by the implementing agent**
    (Governance); deviations become regression tests.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. Headless assumptions, the spec-level
deviations and plan-level decisions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| **Spec deviation — FR-003/FR-004 "button targeting action" implemented as plain buttons routed in-script (R6)** | Verified: `ActionRegistry::is_invocable` is false while any chord conflicts and `controller.rs:2755` gates panel-button dispatch on it; `tempo_up`/`tempo_down` ship on `Plus`/`Minus`, which conflict with the host's `TempoStepUp`/`TempoStepDown` on a fresh install, so the Tempo up/down buttons would be inert — contradicting US2-3/US2-5. Same deviation 012 recorded for Set A/Set B. | Relaxing 011 D3/G14 — ratified rule, not this feature's to reopen. Shipping `tempo_up`/`tempo_down` unbound — contradicts FR-004 and the prompt's default-shortcut list. |
| **Spec deviation — `set_param` widened to accept wire names and boolean/enum values; `params` keyed by catalog-defined wire names (R2)** | The spec writes `set_param(node, "semitones", …)` but the shipped binding takes a numeric `u8` id and there are no wire names anywhere; FR-020's "the same names `set_param` accepts" is only satisfiable by adding both. Additive: numeric arguments unchanged. | `params` indexed by numeric id — unreadable for EQ ids; a separate `set_param_by_name` request — a second method for one binding change. |
| API 1.4 with 0 new requests/events: `NodeInfo.params`/`auto_switched` + event-trigger widening (R1) | Constitution III forbids script-side guessing; 1.3 exposes no parameter value and fires the event only on structural changes, so the host's `+`/`-` and panel edits are invisible (J-2 6→8, SC-009). | Stale panel — contradicts FR-006/FR-007. Unbinding the host's `+`/`-` — contradicts 008 FR-017. A `get_params` RPC — adds a call and still misses host edits. |
| Revision bump only on a **changed** target/mode (R3) | Avoids a per-tick fan-out during a held slider and a plugin's own echo being "confirmed" twice. | Bump on every write — event storms with no information. |
| Mode-id `set_param` routed through `set_mode` (R3) | A numeric write to the mode id left `mode_state` stale (latent 009 inconsistency); FR-009 needs a user flip to clear `auto_switched`. | Leaving it — the model's two views of the mode disagree after any plugin mode write. |
| R4 scheduler fix inside this feature | `effect_chain_changed` is undeliverable for plugin-owned nodes; Key & Tempo's nodes are plugin-owned; no earlier test covered it. | A separate bug-fix PR first — blocks every US test here; the regression test lands with the fix either way. |
| `[[node_kind]]` documentary table + catalog⇄schema test (R1/R2) | Constitution IX: the reference must list wire names, and it cannot diverge from the runtime. | Hand-editing `v1.md` — forbidden; putting the catalog in the gateway crate — gateway has no dependencies by design. |
| Getting Started flag in the settings file, not an account record (R10) | 002 deletes account records on sign-out; a one-time onboarding aid must not re-show after every sign-in (spec clarify-6). | Account-scoped record — re-shows on every sign-in. |
| Card drawn from `App::show_library`, hidden in detail view (R10) | J-1 step 5 places it in the Library view; `open_url` must stay in `App`. | A shell-level banner — visible on every section, not "top of the Library view". |
| Package folder named by identifier (`org.modplayer.key-tempo/`) (R8) | `plugins/bundled/README.md` states `<identifier>/` literally; 012 did the same. | Short name — fixtures use short names, but the bundled README's convention wins. |
| No `debug_probe` in the shipped plugin (R11) | The reference plugin should not teach a fixture-only mechanism; every acceptance is host-observable. | A probe handler — simpler tests, worse example code. |
| `keep_across` is script memory only (spec clarify) | J-3 DP-3.4 calls it a session-wide mode; no `state.plugin` permission is declared. | Persisting via `state.plugin` — adds a permission and contradicts the source document. |
| `GETTING_STARTED_TUTORIAL_URL` as a plain constant (no `option_env!`) | The spec names one constant replaced when the tutorial ships. | A compile-time override like `STATUS_PAGE_URL` — a second knob nobody asked for (Constitution X). |

# Implementation Plan: Plugin UI Contributions — Panels, Overlays, Shortcuts, Settings, Notifications

**Branch**: `feature/011-plugin-ui-contributions` | **Date**: 2026-09-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/011-plugin-ui-contributions/spec.md`

## Summary

Let a plugin show the user something and give them controls — without
ever injecting markup, styles or scripts into the host. Five `ui.*`
surfaces become operable in plugin API **1.2** (additive minor):
`ui.panel` registers a flat list of host widgets (`label`, `button`,
`toggle`, `slider`, `knob`, `list`, `text`, `marker list`, `meter`) the
host renders, themes and makes keyboard-operable with an accessible
name from the plugin's label (an unlabeled widget is refused with its
`widgets[<i>] (<id>)` path); panels are attributed, docked or floated
inside Now Playing, closable (session) or disabled (persisted in
`settings.toml` `[plugin_panels]`), updatable via `update_widget`, and
report `panel_interaction`. `ui.overlay` draws lines/regions/labels/
glyphs in track-time ms that the host re-projects on every zoom/scroll
with zero plugin code. `ui.shortcuts` registers `<identifier>.<name>`
actions into 007's Action & Binding registry with an ownership tier
(host > bundled > community): a host-vs-plugin collision leaves only the
plugin's binding inactive, a same-tier collision leaves both inactive
until resolved; `action_invoked{source, value}` is delivered on
keyboard or panel-button dispatch. `ui.settings` renders a declarative
schema under Settings › Plugins, persisted by the host into a new
host-owned `Scope::Settings` of the plugin's own store and reported as
`settings_changed`. `ui.notify` posts attributed, non-blocking
notifications rate-limited to 6 per rolling 60 s. A `request_focus()`
made inside an interaction handler counts as a user interaction for
010's auto policy.

Technical approach ([research.md](research.md)): schema-first API bump
in `api/v1.toml` (R1); two new rate buckets in the existing limiter
(R2); closed DTO vocabularies + pure validators in the gateway crate,
stateful registries in `modplayer-core::plugins::ui` (R3); all `ui.*`
calls are RPCs except a local `get_settings` (R4); settings values are
a third store scope written only through `Control::SettingsWrite`
(R5); `ActionRegistry` re-keyed on `ActionId = Host | Plugin` with
tiered conflicts and dormant non-`host.` overrides (R6/R7); panels as
an egui `SidePanel` dock + constrained `Window`s (R8/R9); overlays
painted through 005/006's existing overlay callback (R10); PNG-only
assets decoded at discovery (R11); attributed notifications (R12);
dynamic settings search (R13); `[plugin_panels]` persistence (R14);
suspend-placeholder lifecycle (R15); interaction-origin focus flag
(R16); `@key` string tables (R17); six `ui-*` fixtures (R18).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–010; fixture scripts in Luau (`mlua` 0.12)

**Primary Dependencies**: existing only — eframe/egui 0.36 (+accesskit), fluent-templates 0.15, serde/serde_json 1, toml 0.9, thiserror 2, mlua 0.12 (runtime crate), log 0.4, image 0.25, proptest 1 (dev). **No new crate** (Constitution X); the workspace `image` dependency gains the `png` feature beside `jpeg` (research R11).

**Storage**: `settings.toml` gains `[plugin_panels."<identifier>/<panel>"]` (placement, geometry, disabled) under 007's atomic-write contract and retains non-`host.` `[keybindings]` entries dormant (R14, FR-010a); each plugin's state directory gains `settings.json` (a third `PluginStateStore` scope, same `StateWriter`, counted against the 10 MB cap; R5). Overlays, widget values, closed-panel state and the notify window are session-only. No change to marker files or the secure store.

**Testing**: `cargo test --workspace` — new `ui_validation.rs` (gateway, proptests), `plugin_ui_registry.rs` + `controller_plugin_ui.rs` (core, end to end on `FakeBackend` + `SyntheticSource` with six Luau fixtures via `debug_probe`), `plugin_panels.rs` + `plugin_overlays.rs` + `settings_plugins.rs` (ui, offscreen `egui::Context` with AccessKit assertions), extensions to `gateway.rs`, `state_store.rs`, `api_reference.rs` (1.2), runtime `bindings.rs`/`scheduler.rs`, core `actions.rs`/`settings.rs`/`controller_plugins_lifecycle.rs`, ui `controls.rs`/`notifications.rs`/`plugins_view.rs`/`accessibility.rs`/`fluent_keys.rs`; manual scenarios M1–M10 in [quickstart.md](quickstart.md). CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers) on ubuntu / macos / windows.

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical behaviour, widgets, shortcuts and plugin API on all three; no platform-specific code (a macOS `Control` chord as a plugin default is rejected → unbound, exactly as 007 already does for host captures).

**Project Type**: Desktop application — the 13-crate Cargo workspace is unchanged (no new crate; changes in `modplayer-capability-gateway`, `modplayer-plugin-runtime`, `modplayer-core`, `modplayer-ui`, plus fixtures, locales and the regenerated API reference).

**Performance Goals**: zero work on the audio thread (no engine/effects change); overlay re-projection is O(primitives) paint per frame with no controller call (SC-003; ≤ 500 × n plugins, n ≤ 16 fixtures); a `panel_interaction` reaches the plugin within one controller `tick` + one inbox hop (≤ 33 ms + frame, same path as 010's `focus_granted`); `update_widget` applies on the next frame; theme change re-renders every panel in the same frame with zero events (SC-004); `register_panel` validation is O(widgets) ≤ 100; registry `rebuild` stays O(total bindings) and runs only on mutation (007 G8), never per key event.

**Constraints**: Constitution I — no engine/effects crate change, nothing on the RT path; Constitution II — one Gateway path (permission → rate → validation → capacity), refusals are values, every cap spec-fixed, faults contained by 009's existing per-handler budget; Constitution III — widget/overlay/action/settings registries are core-owned, plugins only declare and react; Constitution VI — PNG-only assets, no URLs, no inline markup, plugin strings never interpreted (FR-022, NFR-4.7); Constitution IX — every API change made once in `api/v1.toml`, reference regenerated, written change request in the PR; Constitution X — no trait, no feature flag, no crate, every widget keyboard-operable and named, strings externalised (`plugins.ftl`), the user can close/disable any panel and rebind/remove any plugin shortcut in one action; spec-fixed numbers (data-model.md §1.2) unchanged; out of scope: second-display detach, developer console, permission approval sheet, MIDI, panel reordering, container widgets, community source, pt-BR resources.

**Scale/Scope**: gateway ≈ 700 LOC (`ui.rs` + `ui/limits.rs` 450 incl. validators, `request.rs`/`event.rs`/`manifest.rs`/`limiter.rs`/`state/*` deltas 250) + 400 test; runtime ≈ 350 LOC (`bindings/ui.rs` 200, scheduler/handle/context/budget deltas 150) + 250 test; core ≈ 2,100 LOC (`plugins/ui/{mod,panel,overlay,settings,strings,assets}.rs` 1,000, `actions/*` deltas 450, `plugins/{host,apply,view,focus,bundled}.rs` 300, `settings/*` 150, `notifications.rs` 40, controller façade 160) + 1,400 test; ui ≈ 1,900 LOC (`plugin_panels.rs` 700, `widgets/knob.rs` 120, `plugin_overlays.rs` 250, `plugin_assets.rs` 120, `settings/plugins.rs` 300, `settings/controls.rs` 120, `plugins_view.rs`/`notifications.rs`/`actions.rs`/`now_playing.rs`/`theme.rs` 290) + 1,100 test; 6 fixture packages (≈ 80 Luau lines each + 4 PNGs); `plugins.ftl` + 30 keys; API 1.2 (9 requests, 3 events); 16 fixtures total.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No (indirectly) | ✅ PASS | No change to `modplayer-engine`/`modplayer-effects`; every registry mutation runs on the controller (UI) thread inside `tick`/`drain_plugin_requests`; painting overlays and widgets is pure egui on the UI thread; the only transport effect this feature can cause — a `marker list` row seeking — reuses the existing user-command `seek` path. `tests/realtime.rs` untouched; no "real-time safety" PR note needed (engine crate not modified). |
| II. Plugins Are Guests (non-negotiable) | **Yes** | ✅ PASS | All nine calls go through `Gateway::admit` (permission → rate) then core validation (R2/R3); two new budget buckets (`ui` 100/s, `notify` 6/60 s) live in the same limiter; every refusal is a `Refusal` value in the closed six-code set (FR-024); no plugin-supplied bytes are ever interpreted (PNG decoded once at discovery by the `image` crate, strings rendered as text only — FR-022); a handler throw/hang on `panel_interaction`/`action_invoked`/`settings_changed` is contained by 009's existing budget/suspension path (FR-023, SC-007); a suspended plugin's panel becomes a placeholder, never a stale live control (FR-025). |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | Widgets, overlays, actions, settings pages and notifications are host primitives owned by `modplayer-core::plugins::ui` / `actions` / `notifications` (R3/R6/R12); plugins declare closed vocabularies and receive events; the marker list is 006's host marker model filtered by owner; settings persistence is the host's `StateWriter` (R5); no DSP, no rendering, no key handling in scripts. |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No source crate touched; every automated test runs on `SyntheticSource` + `FakeBackend`; `single_dependent.rs` unchanged. Track changes reach the overlay registry through the existing `TrackChanged` fan-out trigger, not the receiver. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | No request, event or view carries sample data; the `meter` widget receives a plugin-supplied scalar the plugin already had through `audio.meter`'s existing level summaries; `decoded_store_boundary.rs` unchanged and still green. |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | Icons/glyphs are PNG only, size- and dimension-capped, decoded from the signed/embedded package at discovery (NFR-4.4 inheritance), never fetched from a URL; SVG and inline path data are rejected (NFR-4.7); a plugin string is never a Fluent pattern, path, URL or markup; settings values are plugin-scoped JSON in the plugin's own store, unreachable by other plugins; no credential, network or telemetry surface is added. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate (`image` gains a feature only — `cargo deny` unaffected, no new transitive licence); `#![forbid(unsafe_code)]` and the `unwrap_used`/`expect_used` denies already apply in every touched crate; new error surface is `Refusal` values plus one `ManifestError::MalformedField{"glyphs"}` case and `InvalidField::PluginPanel` (existing `thiserror` enums); doc examples on new public items (`validate_panel`, `PluginActionId::parse`, `OverlayRegistry::add`, `PanelRegistry::register`); licence headers on every new file; `cargo test --doc` covers the examples. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Test-first on the contracts' named tests; **permission enforcement** (`permission_denied` first for every `ui.*` call; `notify` window semantics) and **plugin isolation** (handler throw during `panel_interaction` contained; suspension placeholder; overlays cleared on stop) are explicit suites; proptests for the id grammar/widget lists (manifest-adjacent parsing), `[plugin_panels]` round trip and `Scope::Settings` (state serialization); accessibility asserted through AccessKit on an offscreen context (SC-001, SC-004); manual scenarios M1–M10 executed by the implementing agent (Governance › Manual Scenario Sign-Off) with M1 under VoiceOver; every manual deviation ships a regression test. |
| IX. One Plugin API Definition | **Yes** | ✅ PASS | `api/v1.toml` is the single edit point: `minor = 2`, 5 permissions → operable, 9 `[[request]]`, 3 `[[event]]`; `build.rs` regenerates the enums, the exhaustive `dispatch`/`kind()`/`event_to_lua` matches force every arm, `tests/api_reference.rs` regenerates `docs/plugin-api/v1.md`; written change request in contracts/plugin-api-v1.2.md §6 goes in the PR. Additive only — every 1.0/1.1 manifest still loads; the string-table and asset manifest fields are optional. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No trait, no feature flag, no crate; registries are concrete structs; behaviour, widgets and chords identical on all platforms; every widget, panel control, settings field and Plugins-list control is keyboard-operable with an accessible name (contracts/ui-panels.md §3); host strings in `plugins.ftl`; every collection capped at a spec-fixed number; deliberately not built: detach, reordering, containers, plugin-set disabled state, MIDI (research R20). **User override**: Close/Disable any panel in one click, rebind or remove any plugin shortcut in the same map as host shortcuts, and the host's own binding always wins a cross-tier conflict with no user action (FR-006, FR-011, SC-008). |
| Governance: engine/gateway/runtime sign-off | **Yes** | ✅ PASS | `crates/modplayer-capability-gateway/` (schema, DTOs, validators, limiter, store scope, manifest) and `crates/modplayer-plugin-runtime/` (ui bindings, scheduler payloads, `SettingsWrite`, interaction flag) are modified → area-maintainer sign-off on the PR (GOV-3.2); no engine change. Requirement ids (FR-, SC-, PL-, EC-, NFR-, C-) are referenced in spec, plan, contracts and test names. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no crate, trait, flag
or `unsafe`; the RT path is untouched; the single gateway path and
values-not-panics rules hold for all nine calls with the check order
FR-024 fixes; every rendered surface is host-owned code over a closed
vocabulary (II/III/VI); the API change is additive and schema-first
(IX); the user's one-action override is structural for panels and
shortcuts (X).

## Project Structure

### Documentation (this feature)

```text
specs/011-plugin-ui-contributions/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R20 with evidence from the 007/009/010 code
├── data-model.md        # Phase 1: gateway DTOs/limits/validators, runtime deltas, ActionId/tiers, plugins::ui registries, views, settings, fixtures, transitions
├── quickstart.md        # Phase 1: automated gates (named suites), fixture launch, manual scenarios M1–M10
├── contracts/
│   ├── plugin-api-v1.2.md           # API delta: ui namespace (9 requests), 3 events, manifest additions, change request text, named tests
│   ├── ui-panels.md                 # PanelRegistry rules P1–P6, placement L1–L6, per-kind a11y/keys table, Fluent keys, named tests
│   ├── action-registry-plugins.md   # PluginActionId, tiers, registry rules G10–G15, dispatch D1–D4, persistence K1–K3, named tests
│   └── overlays-settings-notify.md  # overlay rules O1–O12, settings S1–S7, notify N1–N4, named tests
├── checklists/          # (created by /speckit-checklist, if run)
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)

docs/plugin-api/v1.md    # regenerated for API 1.2 by the gateway crate's test
```

### Source Code (repository root)

Existing layout (001–010) is kept; `+` marks new files, `~` modified
files.

```text
Cargo.toml                                        ~ image features += "png"
locales/en-US/
├── plugins.ftl                                   ~ + ~30 keys (contracts/ui-panels.md §4; settings/notify keys)
├── controls.ftl                                  ~ + plugin-group / tier-conflict phrasing
└── settings.ftl                                  ~ + settings-plugins-pages, search path
plugins/fixtures/
├── ui-panel/{plugin.toml, main.luau, README.md, icon.png}          + every widget kind; Take over / Register bad / Hang / Notify ×7 buttons; focus_me (Shift+K)
├── ui-shortcuts/{plugin.toml, main.luau, README.md}                + take_over (L), nudge (continuous, Shift+K), tab_bound (Tab)
├── ui-overlay/{plugin.toml, main.luau, README.md}                  + one primitive per kind per track; add_501st / bad_region / bad_icon probes
├── ui-settings/{plugin.toml, main.luau, README.md}                 + boolean/number/string/choice; logs settings_changed; get probe
├── ui-notify/{plugin.toml, main.luau, README.md}                   + post {level,text,n} probe
├── ui-icons/{plugin.toml, main.luau, README.md, icon.png, glyphs/{ok,big}.png} + valid icon, one valid + one over-limit glyph
└── README.md                                                       ~ lists the 16 fixtures
docs/plugin-api/v1.md                             ~ regenerated (API 1.2)
crates/
├── modplayer-capability-gateway/
│   ├── api/v1.toml                               ~ minor = 2; ui.* operable; + 9 requests (category ui / notify); + 3 events
│   ├── build.rs                                  ~ RateCategory gains Ui/Notify from category strings
│   ├── src/lib.rs                                ~ pub mod ui
│   ├── src/ui.rs                                 + UiId, WidgetKind/Spec/Value, OverlayPrimitive/Color/GlyphRef/HostGlyph, ActionSpec, FieldKind/SettingsField, NotifyLevel, validators
│   ├── src/ui/limits.rs                          + every spec-fixed cap (data-model.md §1.2)
│   ├── src/api.rs                                ~ HOST_CAPABILITIES += ui.*
│   ├── src/request.rs                            ~ + 9 Request variants; RequestFocus{interaction}; Response::Settings
│   ├── src/event.rs                              ~ + ActionSource; HostEvent::{PanelInteraction, ActionInvoked, SettingsChanged}; kind()
│   ├── src/manifest.rs                           ~ + icon, glyphs, default_locale, strings (DTO + validate rule 3)
│   ├── src/limiter.rs                            ~ 7 buckets with per-category (limit, window)
│   ├── src/state/{store,paths,writer}.rs         ~ Scope::Settings; settings.json; used_bytes()
│   └── tests/{ui_validation.rs +, gateway.rs ~, state_store.rs ~, manifest.rs ~, api_reference.rs ~}
├── modplayer-plugin-runtime/
│   ├── src/bindings/mod.rs                       ~ pub mod ui; install; dispatch arms (GetSettings local, rest rpc)
│   ├── src/bindings/ui.rs                        + api.ui.* Lua ↔ DTO conversion; local get_settings
│   ├── src/bindings/transport.rs                 ~ request_focus reads Shared.in_interaction_handler
│   ├── src/bindings/state.rs                     ~ doc: Settings scope never installed
│   ├── src/handle.rs                             ~ Control::SettingsWrite
│   ├── src/context.rs                            ~ Shared.{settings_schema, in_interaction_handler}
│   ├── src/budget.rs                             ~ PluginGauges.storage_used
│   ├── src/scheduler.rs                          ~ event_to_lua ×3; SettingsWrite → store.set then settings_changed; interaction flag around the two handlers; load/flush settings.json
│   └── tests/{bindings.rs ~, scheduler.rs ~}
├── modplayer-core/
│   ├── src/lib.rs                                ~ re-exports (ActionId, PluginActionId, OwnerTier, PluginPanelsView, …)
│   ├── src/actions/mod.rs                        ~ PluginActionId, ActionId, OwnerTier, ActionOwner::Plugin, PluginActionDef, ActionSource
│   ├── src/actions/registry.rs                   ~ re-keyed on ActionId; plugin defs; tiered rebuild; is_invocable; grouped rows
│   ├── src/actions/keymap.rs                     ~ plugin map + dormant set; adopt_dormant/park
│   ├── src/plugins/mod.rs                        ~ pub mod ui; PluginRecord.assets
│   ├── src/plugins/ui/mod.rs                     + PluginUi { panels, overlays, settings }; on_ready/on_stop/on_track_changed
│   ├── src/plugins/ui/panel.rs                   + Panel, WidgetState, PanelRegistry, PanelKey
│   ├── src/plugins/ui/overlay.rs                 + OverlayRegistry, OverlaySet, OverlayLayer
│   ├── src/plugins/ui/settings.rs                + SettingsRegistry, SettingsPage
│   ├── src/plugins/ui/strings.rs                 + @key resolution (R17)
│   ├── src/plugins/ui/assets.rs                  + PluginAssets, DecodedPng, load()
│   ├── src/plugins/view.rs                       ~ PluginPanelsView, PanelView, PanelBody, PluginSettingsView, MarkerListRow; PluginRow.panels
│   ├── src/plugins/host.rs                       ~ ui field; discover() loads assets; Ready → ui.on_ready + actions enable; stop() → ui.on_stop + actions disable/unregister
│   ├── src/plugins/apply.rs                      ~ 8 RPC arms (+ Notify); RequestFocus origin
│   ├── src/plugins/focus.rs                      ~ RequestOrigin on request()
│   ├── src/plugins/bundled.rs                    ~ BundledPackage.resources; + 6 fixture packages (16 total)
│   ├── src/controller.rs                         ~ façade (data-model.md §5.4); on_track_changed → ui; settings write for [plugin_panels]
│   ├── src/settings/model.rs                     ~ PanelPlacement, PanelPersisted, AudioSettings.plugin_panels, RawSettings, InvalidField::PluginPanel; dormant keybindings
│   ├── src/settings/store.rs                     ~ read/write [plugin_panels]
│   ├── src/settings_registry.rs                  ~ search_plugin_settings()
│   ├── src/notifications.rs                      ~ PluginAttribution; raise_attributed
│   └── tests/{plugin_ui_registry.rs +, controller_plugin_ui.rs +, actions.rs ~, settings.rs ~, controller_plugins_lifecycle.rs ~, controller_transport_focus.rs ~, notifications.rs ~}
├── modplayer-ui/
│   ├── src/lib.rs                                ~ pub mod plugin_panels, plugin_overlays, plugin_assets
│   ├── src/plugin_panels.rs                      + dock SidePanel, floated Windows, per-kind renderers, a11y, claims, header controls
│   ├── src/plugin_overlays.rs                    + paint(painter, space, layers, view_kind)
│   ├── src/plugin_assets.rs                      + TextureHandle cache; generic glyph
│   ├── src/widgets/mod.rs                        ~ pub mod knob
│   ├── src/widgets/knob.rs                       + rotary slider
│   ├── src/theme.rs                              ~ overlay_color(); host glyph painters
│   ├── src/now_playing.rs                        ~ dock + windows; overlay hook in overview/detail closures; plugin lane height
│   ├── src/shell.rs                              ~ floated windows drawn before notifications
│   ├── src/actions.rs                            ~ Invocation.action: ActionId; plugin arm in invoke()
│   ├── src/settings/plugins.rs                   + Plugins category screen: 009 placeholder content + per-plugin sub-pages + field renderers
│   ├── src/settings/controls.rs                  ~ plugin groups, tier-aware conflict text, greyed rows
│   ├── src/settings/mod.rs                       ~ pub mod plugins; Plugins arm → plugins::show; search merges plugin hits
│   ├── src/plugins_view.rs                       ~ per-panel Show/Hide + Enable/Disable
│   ├── src/notifications.rs                      ~ attribution icon + name
│   └── tests/{plugin_panels.rs +, plugin_overlays.rs +, settings_plugins.rs +, actions.rs ~, controls.rs ~, notifications.rs ~, plugins_view.rs ~, now_playing.rs ~, accessibility.rs ~, fluent_keys.rs ~}
└── modplayer/                                    (unchanged)
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
with one crate per architectural component (Constitution VII) and add
**no crate**. The closed vocabularies and their pure validators go in
`crates/modplayer-capability-gateway/src/ui.rs` beside `manifest.rs`
(the crate is the "runtime call checker" Constitution IX names and is
dependency-free, research R3); the stateful registries go in a new
`crates/modplayer-core/src/plugins/ui/` module beside `focus.rs`,
`host.rs` and `apply.rs` (host primitives, Constitution III); plugin
actions extend the existing `crates/modplayer-core/src/actions/`
service rather than a parallel registry (research R6); the egui side
lives in `crates/modplayer-ui/src/plugin_panels.rs` and
`plugin_overlays.rs` next to `effects_view.rs`/`transport_view.rs`,
with the dock and floated windows drawn from `now_playing.rs` like
008's and 010's panels; fixture packages go in the existing top-level
`plugins/fixtures/` embedded through `bundled.rs`. The dependency graph
is unchanged from 009/010 (`core → {gateway, runtime, engine, effects,
…}`, `ui → {core, gateway, …}`, `runtime → {gateway, engine}`,
`gateway → {}`); the engine, effects, source, receiver, audio-io,
account, secure-store and binary crates are not modified.

## Design notes that tasks must respect

1. **Schema first** (R1, Constitution IX): the `api/v1.toml` bump, the
   five `operable` flips, nine requests and three events land before any
   Rust arm; the exhaustive matches then force the arms; `docs/plugin-
   api/v1.md` is regenerated by the test, never hand-edited; the PR body
   carries contracts/plugin-api-v1.2.md §6.
2. **Validate in the gateway crate, mutate in core** (R3): every
   `Request` DTO is validated by a pure `validate_*` before
   `PluginUi` touches state; refusal messages come from the validator
   (`widgets[<i>] (<id>)`, `fields[<i>]`); capacity (`*_limit`) is
   checked in core after validation, on the prospective count.
3. **Check order is fixed** (FR-024): permission → rate → validation →
   capacity; `notify` consumes its slot at admission (N1).
4. **`get_settings` is local; every other `ui.*` is an RPC** (R4); the
   settings snapshot rides inside the `RegisterSettings` RPC.
5. **Settings writes flow host → plugin thread only** (R5): core never
   opens `settings.json`; `Control::SettingsWrite` applies the store set
   and *then* dispatches `settings_changed` in the same inbox item.
6. **One `ActionRegistry`** (R6): `HostAction` keeps its 46-slot fast
   path; plugin actions are a `BTreeMap` side; `rebuild` implements the
   tier rule G13; `resolve` never returns a flagged action.
7. **Interaction origin is a plugin-thread flag** (R16): set only around
   `panel_interaction`/`action_invoked` handlers, read only by
   `request_focus`, carried in the request — nowhere else.
8. **Direct-to-handle events** (R7/R9): `panel_interaction`,
   `action_invoked`, `settings_changed` bypass `FanOut` (the permission
   is implied by the registration that produced them).
9. **Lifecycle hooks are exactly three** (R15): `on_ready`,
   `on_stop(reason)`, `on_track_changed`; `PluginHost::stop`'s order is
   009 L7 + 010 vacate + `ui.on_stop` + `actions.set_plugin_enabled/
   unregister`.
10. **Overlays paint inside the existing callback** (R10, O5): no new
    layer; the plugin lane is reserved height in both views; `theme.rs`
    is the only file with colour mapping.
11. **No plugin string is ever interpreted** (FR-022): labels/titles/
    texts are drawn as text; `@key` resolution happens once in core at
    registration; assets are PNG decoded at discovery only.
12. **Persist only what the spec names** (R14): `[plugin_panels]`
    placement/geometry/disabled and dormant `[keybindings]`; closed
    state, widget values, overlays and the notify window are never
    written.
13. **Fixtures are real packages** (R18): embedded via `include_str!`/
    `include_bytes!`, visible only under `MODPLAYER_PLUGIN_FIXTURES=1`;
    every refusal path has a probe.
14. **Numbers are spec-fixed** (data-model.md §1.2): a change requires
    recording a deviation in the spec's Assumptions.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. Headless assumptions and plan-level
decisions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| DTOs + validators in the gateway crate, registries in core (R3) | Dependency-free, proptest-able grammar checks (VIII, IX) while state composes with records/markers/actions (III). | Everything in core — loses the isolated validator surface; everything in the gateway — the crate would need `PluginRecord`/`TrackMarkers`. |
| `Scope::Settings` as a third store scope written via `Control::SettingsWrite` (R5) | One store, one 10 MB cap, one atomic writer, one thread owning the file; `settings_changed` ordered after the value. | Core-owned `settings.json` — two writers per directory and split cap accounting; `state.plugin` reserved keys — FR-018 forbids reachability. |
| `ActionRegistry` re-keyed on `ActionId` with a dynamic plugin side (R6) | Conflicts are cross-owner by definition; 007 FR-016 reserved this extension; keeps the 46-slot host fast path and every 007 test. | Parallel plugin registry — two indexes per key event, conflicts computed twice; `HostAction::Plugin(String)` — breaks `Copy` and the fixed array. |
| Tier rule: single top-tier holder stays unflagged (G13) | Prompt acceptance (host `L` keeps working, plugin inactive) and EC-6.8 (same tier: both inactive) read together. | Always-symmetric flagging — the host's loop toggle would stop working because a plugin asked for `L`. |
| `is_invocable` gates the panel-button path on conflicts too (D3/G14) | FR-008 names "the FR-011 conflict/FR-013 enabled gate" for buttons; FR-012 defines an inactive action as flagged **or** not Active. | Health-only gate for buttons — a flagged action would fire from its panel but not its key, an inconsistency the map cannot explain. |
| `get_settings` served locally (R4) | Values already sit on the plugin thread; avoids an RPC frame for a frequently polled read. | RPC — duplicates values in core and adds latency for nothing. |
| Dock as `SidePanel::right` inside Now Playing, floated as constrained `Window`s (R8) | egui provides order, clamp, resize and focus groups natively; mirrors 008/010's Now Playing-scoped panels. | Custom docking layout — Constitution X. |
| Overlay colours mapped from `Visuals` in `theme.rs` (O8) | Closed token set, theme-following, no new colour literals outside `theme.rs` (005 rule). | Plugin-supplied RGB — PL-7.1 forbids styles. |
| `image` gains the `png` feature (R11) | PNG is the only accepted format; the crate is already a dependency. | New `png` crate — Constitution X (no new crate). |
| Dormant non-`host.` keybindings never warned (K1) | A plugin's override would otherwise be dropped on every launch (007 FR-013's intent is that customisations survive). | Warn-and-keep — noise on every launch for a normal state. |
| Search via a dynamic complement to the `const DESCRIPTORS` (R13) | 001's search is `&'static` by construction and referenced by its tests. | Making descriptors dynamic — churn across 001/007 tests for no user-visible gain. |
| `marker list` seeks as a host user command with no event (Clarifications) | FR-3.3.2: a user seek needs no plugin permission and must not move focus. | Firing `panel_interaction` — the plugin would learn about a host-owned action it did not cause. |
| Overlays not exposed to AccessKit (spec Assumptions) | A `list`/`marker list` widget is the accessible channel; per-primitive nodes would flood the tree with 500 × n items. | Per-primitive accessible nodes — deferred to a later slice via the waveform's description, no contract change needed. |
| `en-US` resources only (R17) | Precedent 001–010; the `@key` mechanism ships so a locale slice needs no plugin contract change. | Creating `pt-BR` for this feature — a partial locale with ten untranslated features. |
| Six fixtures rather than one (R18) | Refusal tests need plugins *without* a grant; permissions differ per surface; manual scenarios need isolated behaviour. | One mega-fixture — cannot prove `permission_denied` per surface. |
| Panel geometry writes coalesced to one per 500 ms (L3) | Dragging a floated window produces per-frame rect changes; settings.toml is an atomic full-file write. | Write per frame — hundreds of file writes per drag. |

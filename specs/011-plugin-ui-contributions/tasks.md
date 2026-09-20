---

description: "Task list for 011-plugin-ui-contributions"
---

# Tasks: Plugin UI Contributions — Panels, Overlays, Shortcuts, Settings, Notifications

**Branch**: `feature/011-plugin-ui-contributions` | **Input**: `/specs/011-plugin-ui-contributions/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included. Constitution VIII ("Test What the NFRs Promise") and every contract's own **Named tests** section make automated tests a mandatory deliverable of this feature, not an option — each test task below cites the exact suite/case names contracts/plugin-api-v1.2.md, contracts/ui-panels.md, contracts/action-registry-plugins.md and contracts/overlays-settings-notify.md already name.

**Organization**: Tasks are grouped by user story (US1–US5, spec.md priorities P1–P5) so each story is independently implementable and testable once Setup + Foundational are done. Gateway/runtime plumbing is shared identically by all five `ui.*` surfaces (plan.md's dependency graph: `gateway → {}`, `runtime → gateway`, `core → {gateway, runtime}`, `ui → {core, gateway}`) and is therefore Foundational, not story-scoped.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1–US5, mapped to spec.md's five priorities
- Every task names its exact file path(s)
- **`*(reqs: …)*`**: every task ends with the spec.md requirement IDs it implements or verifies (`FR-`, `SC-`, `NFR-`) and, where a Constitution principle is the driver, that principle — Governance requirement-id traceability. Research `R-n` and contract rule ids (`G`, `D`, `O`, `N`) cited inline are design pointers, not the traceability anchor.

## Path Conventions

Existing 13-crate Cargo workspace under `crates/`; no new crate (plan.md, Constitution X). Paths below are exactly plan.md's Project Structure.

---

## Phase 1: Setup

**Purpose**: Confirm the baseline and the one dependency change the whole feature needs.

- [X] T001 Confirm `cargo test --workspace` is green on `feature/011-plugin-ui-contributions` before any change (baseline gate; no file change) *(reqs: SC-001–SC-008 baseline; Constitution VIII)*
- [X] T002 Add the `png` feature to the workspace `image` dependency (research R11; Constitution X — no new crate) in `Cargo.toml` *(reqs: FR-014a)*

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The schema-first API bump (Constitution IX, plan.md design note 1) and the two dependency-free crates (`modplayer-capability-gateway`, `modplayer-plugin-runtime`) that every one of the five `ui.*` surfaces rides on identically, plus the core-crate plumbing (`PluginUi` skeleton, controller façade, exhaustive dispatch arms) that keeps `Request`/`HostEvent` exhaustive matches compiling once the schema exists.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Gateway crate (`modplayer-capability-gateway`)

- [X] T003 Bump `api/v1.toml`: `minor = 2`, flip `ui.panel`/`ui.overlay`/`ui.shortcuts`/`ui.settings`/`ui.notify` to `operable = true`, add the 9 `[[request]]` entries and 3 `[[event]]` entries (contracts/plugin-api-v1.2.md §1–§4) in `crates/modplayer-capability-gateway/api/v1.toml` *(reqs: FR-003, FR-007, FR-010, FR-014, FR-017, FR-018, FR-019, FR-024, FR-026)*
- [X] T004 Update `build.rs` so the generated `RateCategory` gains `Ui`/`Notify` from the new category strings (depends on T003) in `crates/modplayer-capability-gateway/build.rs` *(reqs: FR-020, FR-020a)*
- [X] T005 [P] Add every spec-fixed constant (data-model.md §1.2) in `crates/modplayer-capability-gateway/src/ui/limits.rs` *(reqs: FR-003, FR-010, FR-014a, FR-015, FR-017, FR-019, FR-020, FR-020a)*
- [X] T006 Add closed vocabularies `UiId`, `WidgetKind`/`ListItem`/`WidgetSpec`/`WidgetValue`, `OverlayColor`/`OverlayPrimitive`/`GlyphRef`/`HostGlyph`, `ActionKindSpec`/`ActionSpec`, `FieldKind`/`SettingsField`, `NotifyLevel` (depends on T005) in `crates/modplayer-capability-gateway/src/ui.rs` *(reqs: FR-001, FR-002, FR-002a, FR-010, FR-014, FR-017, FR-019, FR-022)*
- [X] T007 Add pure validators `validate_panel`, `validate_update`, `validate_primitives`, `validate_action`, `validate_schema`, `validate_notify`, `validate_stored` (data-model.md §1.3 table; depends on T006) in `crates/modplayer-capability-gateway/src/ui.rs` *(reqs: FR-001, FR-002, FR-002a, FR-003, FR-007, FR-010, FR-014, FR-015, FR-017, FR-018, FR-019, FR-024)*
- [X] T008 [P] Wire `pub mod ui;` in `crates/modplayer-capability-gateway/src/lib.rs` *(reqs: FR-022)*
- [X] T009 [P] Add the five `ui.*` names to `HOST_CAPABILITIES` in `crates/modplayer-capability-gateway/src/api.rs` *(reqs: FR-024)*
- [X] T010 Extend `Request`/`Response`: 9 new `Request` variants, `RequestFocus { interaction }`, `Response::Settings` (depends on T006) in `crates/modplayer-capability-gateway/src/request.rs` *(reqs: FR-003, FR-007, FR-010, FR-014, FR-017, FR-018, FR-019, FR-026)*
- [X] T011 [P] Extend `event.rs`: `ActionSource`, `HostEvent::{PanelInteraction, ActionInvoked, SettingsChanged}`, `kind()` (depends on T006) in `crates/modplayer-capability-gateway/src/event.rs` *(reqs: FR-007, FR-012, FR-018)*
- [X] T012 [P] Extend `manifest.rs`: `icon`, `glyphs`, `default_locale`, `strings` fields, Rule 3 `MalformedField{"glyphs"}` in `crates/modplayer-capability-gateway/src/manifest.rs` *(reqs: FR-014a, FR-021)*
- [X] T013 Extend `limiter.rs`: 7 rate buckets, per-category `(limit, window)`, `Ui` 100/1s, `Notify` 6/60s (depends on T004) in `crates/modplayer-capability-gateway/src/limiter.rs` *(reqs: FR-020, FR-020a, FR-024)*
- [X] T014 Extend `src/state/{store,paths,writer}.rs`: `Scope::Settings`, `settings.json`, `used_bytes()` in `crates/modplayer-capability-gateway/src/state/{store,paths,writer}.rs` *(reqs: FR-018)*
- [X] T015 [P] New `tests/ui_validation.rs`: `unlabeled_widget_names_path`, `duplicate_widget_id`, `panel_widget_limit_101`, `slider_bounds`, `list_item_bounds`, `overlay_region_order`, `overlay_unknown_icon`, `schema_default_out_of_range`, `notify_text_201`, `id_grammar_proptest` (depends on T007) in `crates/modplayer-capability-gateway/tests/ui_validation.rs` *(reqs: FR-001, FR-002, FR-002a, FR-003, FR-014, FR-017, FR-019, FR-024, SC-001)*
- [X] T016 [P] Update `tests/gateway.rs`: `notify_window_six_per_minute`, `notify_refused_at_validation_still_counts`, `ui_category_101st_in_window` (depends on T013) in `crates/modplayer-capability-gateway/tests/gateway.rs` *(reqs: FR-020, FR-020a, SC-005)*
- [X] T017 [P] Update `tests/manifest.rs` for `icon`/`glyphs`/`default_locale`/`strings` and the malformed `glyphs` key case (depends on T012) in `crates/modplayer-capability-gateway/tests/manifest.rs` *(reqs: FR-014a, FR-021)*
- [X] T018 [P] Update `tests/state_store.rs` for `Scope::Settings` round trip and cap accounting (depends on T014) in `crates/modplayer-capability-gateway/tests/state_store.rs` *(reqs: FR-018)*
- [X] T019 Update `tests/api_reference.rs::reference_is_current` and regenerate `docs/plugin-api/v1.md` for API 1.2 (depends on T003, T010, T011) in `crates/modplayer-capability-gateway/tests/api_reference.rs`, `docs/plugin-api/v1.md` *(reqs: FR-024; Constitution IX)*

### Runtime crate (`modplayer-plugin-runtime`)

- [X] T020 Add `bindings/ui.rs`: installs `api.ui.*`, Lua↔DTO conversion for every call, `get_settings` served locally (research R4; depends on T006, T010) in `crates/modplayer-plugin-runtime/src/bindings/ui.rs` *(reqs: FR-003, FR-007, FR-010, FR-014, FR-017, FR-018, FR-019)*
- [X] T021 Wire `bindings/mod.rs`: `pub mod ui;` install, dispatch arms (`GetSettings` local, rest RPC) (depends on T020) in `crates/modplayer-plugin-runtime/src/bindings/mod.rs` *(reqs: FR-003, FR-007, FR-010, FR-014, FR-017, FR-018, FR-019)*
- [X] T022 [P] Update `bindings/transport.rs`: `request_focus` reads `Shared.in_interaction_handler` in `crates/modplayer-plugin-runtime/src/bindings/transport.rs` *(reqs: FR-026)*
- [X] T023 [P] Update `bindings/state.rs` doc note: `Settings` scope never installed for Lua in `crates/modplayer-plugin-runtime/src/bindings/state.rs` *(reqs: FR-018)*
- [X] T024 [P] Add `Control::SettingsWrite { changes }` in `crates/modplayer-plugin-runtime/src/handle.rs` *(reqs: FR-018)*
- [X] T025 [P] Add `Shared.{settings_schema, in_interaction_handler}` in `crates/modplayer-plugin-runtime/src/context.rs` *(reqs: FR-018, FR-026)*
- [X] T026 [P] Add `PluginGauges.storage_used: AtomicUsize` in `crates/modplayer-plugin-runtime/src/budget.rs` *(reqs: FR-018)*
- [X] T027 Update `scheduler.rs`: `event_to_lua` for the 3 new events, `SettingsWrite → store.set then settings_changed`, interaction flag around `panel_interaction`/`action_invoked` handlers, load/flush `settings.json` (depends on T024, T025, T026) in `crates/modplayer-plugin-runtime/src/scheduler.rs` *(reqs: FR-007, FR-012, FR-018, FR-023, FR-026)*
- [X] T028 [P] Update `tests/bindings.rs`: `ui_calls_are_rpcs`, `get_settings_is_local_and_substitutes_defaults`, `settings_scope_unreachable_from_lua` (depends on T021) in `crates/modplayer-plugin-runtime/tests/bindings.rs` *(reqs: FR-018, FR-024)*
- [X] T029 [P] Update `tests/scheduler.rs`: `panel_interaction_payload`, `action_invoked_payload`, `settings_changed_after_write`, `request_focus_flag_inside_interaction_handler` (depends on T027) in `crates/modplayer-plugin-runtime/tests/scheduler.rs` *(reqs: FR-007, FR-012, FR-018, FR-026)*

### Core plumbing (`modplayer-core`)

- [X] T030 Add `plugins/ui/mod.rs` skeleton: `PluginUi { panels, overlays, settings }`, `on_ready`/`on_stop`/`on_track_changed` (registries filled in per-story phases) in `crates/modplayer-core/src/plugins/ui/mod.rs` *(reqs: FR-016, FR-025)*
- [X] T031 [P] Wire `plugins/mod.rs`: `pub mod ui;`, `PluginRecord.assets` field (depends on T030) in `crates/modplayer-core/src/plugins/mod.rs` *(reqs: FR-014a)*
- [X] T032 [P] Add `plugins/ui/assets.rs`: `PluginAssets`, `DecodedPng`, `load()` — PNG-only, decoded once at discovery, every failure → warning + omitted (research R11) in `crates/modplayer-core/src/plugins/ui/assets.rs` *(reqs: FR-014a)*
- [X] T033 [P] Add `plugins/ui/strings.rs`: `@key` resolution (research R17) in `crates/modplayer-core/src/plugins/ui/strings.rs` *(reqs: FR-021)*
- [X] T034 Add controller façade signatures for all nine surfaces (data-model.md §5.4), wired to `PluginUi` (depends on T030) in `crates/modplayer-core/src/controller.rs` *(reqs: FR-003, FR-007, FR-012, FR-014, FR-017, FR-019)*
- [X] T035 Extend `plugins/apply.rs`: one dispatch arm per new `RequestKind` (8 RPC arms + `Notify`), `RequestFocus` origin passthrough (depends on T034) in `crates/modplayer-core/src/plugins/apply.rs` *(reqs: FR-024, FR-026)*
- [X] T036 [P] Extend `plugins/focus.rs`: `RequestOrigin` on `request()` in `crates/modplayer-core/src/plugins/focus.rs` *(reqs: FR-026)*
- [X] T037 [P] Extend re-exports (`ActionId`, `PluginActionId`, `OwnerTier`, `PluginPanelsView`, …) in `crates/modplayer-core/src/lib.rs` *(reqs: FR-005, FR-010, FR-011)*

**Checkpoint**: `cargo build --workspace` succeeds (every enum arm exists); no `ui.*` surface has real behaviour yet. User story implementation can now begin.

---

## Phase 3: User Story 1 - A plugin's panel is fully accessible and themed by the host (Priority: P1) 🎯 MVP

**Goal**: A plugin registers a panel of host widgets; the host renders, themes and makes every widget keyboard-operable with an accessible name from the plugin's label; an unlabeled widget is refused with its path; panels dock/float/close/disable and survive theme changes and plugin suspension with zero plugin code running.

**Independent Test**: Register a fixture panel with one of every widget kind, including one deliberately unlabeled; verify the unlabeled registration is refused with its widget path, the rest of the panel renders with correct accessible names and roles, every widget is keyboard-operable, changing theme updates it live, and interacting with each widget delivers the correct value to the plugin.

### Tests for User Story 1 ⚠️ (write first, verify they fail before implementing)

- [X] T038 [P] [US1] Core test `plugin_ui_registry.rs::{register_replaces_atomically, seventeenth_panel_refused, update_label_not_updatable, update_slider_out_of_range, meter_clamps, list_replace_items, closed_survives_reregister, suspend_keeps_disable_removes}` in `crates/modplayer-core/tests/plugin_ui_registry.rs` *(reqs: FR-003, FR-006, FR-007, FR-025)*
- [X] T039 [P] [US1] Core test `controller_plugin_ui.rs::{unlabeled_widget_refused_with_path, interaction_delivers_once, update_widget_no_event, request_focus_in_handler_auto_grants, request_focus_from_timer_recorded_only, handler_throw_contained}` in `crates/modplayer-core/tests/controller_plugin_ui.rs` *(reqs: FR-001, FR-007, FR-023, FR-026, SC-001, SC-007)* — this test file's own `wait_active`/register-panel race exposed a real bug in T046's `on_ready`: unconditionally clearing on every `Ready` (not just a restart-after-suspend) could wipe a registration `drain_plugin_requests` had *just* applied in the same `tick()` (requests drain before runtime events); fixed in `plugins/host.rs` by gating the clear on `was_suspended`.
- [X] T040 [P] [US1] Core test `settings.rs::{plugin_panels_round_trip_proptest, plugin_panels_bad_placement_dropped}` in `crates/modplayer-core/tests/settings.rs` *(reqs: FR-005)*
- [X] T041 [P] [US1] Extend `controller_plugins_lifecycle.rs` for 009 teardown order + `PluginUi::on_stop`/`on_ready` in `crates/modplayer-core/tests/controller_plugins_lifecycle.rs` *(reqs: FR-025)* — the two `ui_panels_*` tests already present in this file (written alongside T046) were registering their panel by waiting on the fixture's own `register_panel` RPC via `pump_until`, but this file's harness drives a bare `PluginHost` with no controller to drain that RPC (`plugins::apply::drain_plugin_requests` needs one) — both tests were failing. Fixed by registering directly against `PanelRegistry` (`host.ui_mut().panels_mut().register(...)`), exactly like `panel.rs`'s own unit tests, since what this file is testing is `PluginUi::on_stop`'s wiring, not the RPC pipeline (already covered end-to-end by `controller_plugin_ui.rs`, T039).
- [X] T042 [P] [US1] UI test `plugin_panels.rs::{every_kind_has_role_and_name, tempo_slider_announces, arrows_step_and_emit, drag_emits_once, list_selection_follows_focus, unclaimed_key_falls_through, theme_switch_no_events, dock_order_by_name_then_seq, float_clamped_into_window, placeholder_on_suspend, header_attribution}` in `crates/modplayer-ui/tests/plugin_panels.rs` *(reqs: FR-001, FR-002, FR-002a, FR-004, FR-005, FR-007a, FR-009, FR-025, SC-001, SC-004, SC-008, NFR-6.1, NFR-6.2)*
- [X] T043 [P] [US1] UI test `plugins_view.rs::panel_controls_listed` in `crates/modplayer-ui/tests/plugins_view.rs` *(reqs: FR-006)*
- [X] T044 [P] [US1] Extend `accessibility.rs` and `fluent_keys.rs` for the new panel elements/keys in `crates/modplayer-ui/tests/{accessibility.rs,fluent_keys.rs}` *(reqs: FR-001, NFR-6.2, SC-001)*

### Implementation for User Story 1

- [X] T045 [US1] Implement `plugins/ui/panel.rs`: `Panel`, `WidgetState`, `PanelRegistry`, `PanelKey`, `register`/`update`/`interact`/`close`/`show`/`is_closed` (depends on T030) in `crates/modplayer-core/src/plugins/ui/panel.rs` *(reqs: FR-001, FR-002, FR-002a, FR-003, FR-006, FR-007, FR-024)*
- [X] T046 [US1] Wire `PanelRegistry` into `PluginUi::on_ready`/`on_stop` (Suspend → placeholder, Disable/Shutdown → remove) (depends on T045) in `crates/modplayer-core/src/plugins/ui/mod.rs` *(reqs: FR-025)*
- [X] T047 [US1] Extend `plugins/view.rs`: `PluginPanelsView`, `PanelView`, `PanelBody`, `PluginRow.panels` (depends on T045) in `crates/modplayer-core/src/plugins/view.rs` *(reqs: FR-004, FR-005, FR-006)*
- [X] T048 [US1] Extend `plugins/host.rs`: `ui` field, `discover()` loads assets (T032), `Ready → ui.on_ready`, `stop() → ui.on_stop` in `crates/modplayer-core/src/plugins/host.rs` *(reqs: FR-014a, FR-025)*
- [X] T049 [US1] Extend `settings/model.rs`: `PanelPlacement`, `PanelPersisted`, `AudioSettings.plugin_panels`, `RawSettings.plugin_panels`, `InvalidField::PluginPanel` in `crates/modplayer-core/src/settings/model.rs` *(reqs: FR-005, FR-006)*
- [X] T050 [US1] Extend `settings/store.rs`: read/write `[plugin_panels]` (depends on T049) in `crates/modplayer-core/src/settings/store.rs` *(reqs: FR-005, FR-006)* — no code change needed: `store.rs`'s `load`/`save` already round-trip the whole `RawSettings` struct generically via `serde`/`toml`, so T049's new field is read/written automatically (proven by `plugin_panels_round_trip_proptest`, T040).
- [X] T051 [US1] Implement controller panel methods `plugin_panels_view`, `plugin_panel_interaction`, `plugin_panel_close`/`show`/`set_disabled`/`set_placement`, `plugin_assets` (depends on T047, T050) in `crates/modplayer-core/src/controller.rs` *(reqs: FR-005, FR-006, FR-007, FR-014a)* — also wired the `RegisterPanel`/`UpdateWidget` RPC arms in `plugins/apply.rs` (the code-level pointer left by T035's placeholder), since the controller façade has nothing to call it without them.
- [X] T052 [US1] `modplayer-ui/src/plugin_panels.rs`: dock `SidePanel` + floated `Window`s, per-kind widget renderers, AccessKit `WidgetInfo`, focus claims, interaction → controller, placement changes → `set_placement` (depends on T051) in `crates/modplayer-ui/src/plugin_panels.rs` *(reqs: FR-001, FR-002, FR-004, FR-005, FR-007, FR-007a, FR-009, FR-022, SC-001, SC-004, SC-008, NFR-6.1, NFR-6.2)* — uses egui 0.36's unified `Panel::right` (nestable inside the already-open Now Playing `Ui`, exactly like `app.rs`'s own nav rail) rather than the legacy `SidePanel`; added `modplayer-plugin-runtime` as a direct `modplayer-ui` dependency (Cargo.toml) since `PanelBody::Placeholder`'s `SuspendCause` field type must be nameable to render the suspended placeholder.
- [X] T053 [P] [US1] `modplayer-ui/src/widgets/knob.rs`: rotary painter over a slider `Response`; wire `widgets/mod.rs` in `crates/modplayer-ui/src/widgets/{mod.rs,knob.rs}` *(reqs: FR-002, NFR-6.1)*
- [X] T054 [P] [US1] `modplayer-ui/src/plugin_assets.rs`: `TextureHandle` cache keyed by `(PluginId, asset)`, generic glyph fallback in `crates/modplayer-ui/src/plugin_assets.rs` *(reqs: FR-004, FR-014a)* — cache lives in `egui::Context` memory (mirrors `actions::FocusClaims`) rather than `App`-owned state, so no signature change ripples into `app.rs`.
- [X] T055 [US1] Extend `now_playing.rs`: dock column + floated windows draw hook, plugin lane height reservation (depends on T052) in `crates/modplayer-ui/src/now_playing.rs` *(reqs: FR-005)* — `plugin_panels::show_dock` called first (before any other Now Playing content, so the rest of it sees the narrower remaining width); `show_floated_windows` called last.
- [X] T056 [P] [US1] Extend `shell.rs`: floated plugin windows drawn before notifications in `crates/modplayer-ui/src/shell.rs` *(reqs: FR-005)* — adds `PLUGIN_FLOATED_WINDOW_ORDER` (`Order::Middle`, same layer as the notification `Area`), used by `plugin_panels.rs`'s `Window`s; the doc comment records a known caveat (`app.rs`'s current draw order, out of this phase's file scope — see this file's Manual Scenario Log) rather than silently leaving the L2 ordering only half-true.
- [X] T057 [US1] Extend `plugins_view.rs`: per-panel Show/Hide + Enable/Disable controls (depends on T047) in `crates/modplayer-ui/src/plugins_view.rs` *(reqs: FR-006)*
- [X] T058 [P] [US1] Add panel Fluent keys (`plugin-panel-*`, `plugin-generic-glyph-desc`, `plugin-marker-list-empty`, `plugin-notification`) in `locales/en-US/plugins.ftl` *(reqs: FR-004, FR-025, NFR-6.2)*
- [X] T059 [US1] Add fixture package `plugins/fixtures/ui-panel/{plugin.toml, main.luau, README.md, icon.png}`: panel "Controls" with every widget kind, "Take over" button (plain `panel_interaction` + `request_focus()` in handler), "Register bad" probe (unlabeled widget), "Hang" probe in `plugins/fixtures/ui-panel/` *(reqs: FR-001, FR-002, FR-023, FR-026)* — also adds a "Throw" button/probe (`error()` in the handler) since T039's `handler_throw_contained` needs one and no other US1 fixture provides it.
- [X] T060 [US1] Register the fixture in `plugins/bundled.rs`, list it in `plugins/fixtures/README.md` (depends on T059) in `crates/modplayer-core/src/plugins/bundled.rs`, `plugins/fixtures/README.md` *(reqs: FR-001–FR-009 fixture coverage)*
- [X] T061 [US1] Execute manual scenarios M1–M4 (quickstart.md) and record results/deviations in this file's Manual Scenario Log (depends on T052–T060) *(reqs: FR-025, SC-001, SC-004, SC-008)*

**Checkpoint**: User Story 1 is fully functional and independently testable (MVP).

---

## Phase 4: User Story 2 - A plugin's keyboard shortcuts join the central map, with the user always able to resolve a conflict (Priority: P2)

**Goal**: A plugin registers an action with a default binding; it joins 007's shortcut map grouped under the plugin's name, fully rebindable; a host-vs-plugin conflict leaves only the plugin's binding inactive; a same-tier conflict leaves both inactive until the user resolves it; invoking fires `action_invoked` with source and value.

**Independent Test**: Register a fixture plugin action bound by default to `L` (already the host's loop toggle); verify the map flags the conflict, `L` still toggles the host's loop, and the plugin's action never fires until the user rebinds it to a free key, at which point it fires correctly and appears un-flagged.

### Tests for User Story 2 ⚠️

- [X] T062 [P] [US2] Core test `actions.rs::{plugin_action_id_parse_roundtrip, plugin_action_id_rejects_bad_name, host_beats_bundled_on_same_chord, two_bundled_both_flagged, host_vs_host_unchanged, disabled_plugin_action_excluded_from_conflicts, reenable_reevaluates_conflicts, sixty_fifth_action_refused, reregister_keeps_override, rejected_default_registers_unbound, dormant_override_adopted_on_register, dormant_parked_on_unregister, rows_group_plugins_after_host, is_invocable_gate}` in `crates/modplayer-core/tests/actions.rs` *(reqs: FR-010, FR-010a, FR-011, FR-013, SC-002)*
- [X] T063 [P] [US2] Core test `settings.rs::{non_host_keybinding_retained_dormant, host_unknown_still_warned}` in `crates/modplayer-core/tests/settings.rs` *(reqs: FR-010a)*
- [X] T064 [P] [US2] Core test `controller_plugin_ui.rs::{keyboard_invokes_plugin_action, continuous_value_one, inactive_action_not_dispatched, button_names_unknown_action_inert, button_action_source_ui}` in `crates/modplayer-core/tests/controller_plugin_ui.rs` *(reqs: FR-008, FR-012, FR-013)*
- [X] T065 [P] [US2] UI test `actions.rs::dispatch_returns_plugin_action_id` in `crates/modplayer-ui/tests/actions.rs` *(reqs: FR-012)*
- [X] T066 [P] [US2] UI test `controls.rs::{plugin_group_rendered, tier_conflict_text, greyed_when_suspended}` in `crates/modplayer-ui/tests/controls.rs` *(reqs: FR-010, FR-011, FR-013)*

### Implementation for User Story 2

- [X] T067 [US2] Extend `actions/mod.rs`: `PluginActionId`, `ActionId`, `OwnerTier`, `ActionOwner::Plugin`, `PluginActionDef`, `ActionSource` in `crates/modplayer-core/src/actions/mod.rs` *(reqs: FR-010, FR-011, FR-012)*
- [X] T068 [US2] Extend `actions/registry.rs`: re-key on `ActionId`, `plugin_defs`/`plugin_enabled`/`plugin_effective`, tiered `rebuild` (G13), `is_invocable`, grouped `rows()` (depends on T067) in `crates/modplayer-core/src/actions/registry.rs` *(reqs: FR-010, FR-011, FR-013, SC-002)*
- [X] T069 [US2] Extend `actions/keymap.rs`: plugin override map + `dormant` set, `adopt_dormant`/`park` (depends on T067) in `crates/modplayer-core/src/actions/keymap.rs` *(reqs: FR-010, FR-010a)*
- [X] T070 [US2] Extend `settings/model.rs::into_settings`: a non-`host.`-namespaced unknown id goes to `dormant` instead of `dropped_keybindings` (depends on T069) in `crates/modplayer-core/src/settings/model.rs` *(reqs: FR-010a)*
- [X] T071 [US2] Wire `plugins/host.rs`: `Ready`/`stop()` also call `actions.set_plugin_enabled`/`unregister_plugin_actions` (depends on T068) in `crates/modplayer-core/src/plugins/host.rs` *(reqs: FR-013, FR-025)*
- [X] T072 [US2] Extend `plugins/ui/panel.rs`: a `button` with `action` resolves via `invoke_plugin_action(.., Ui)` instead of `panel_interaction` (FR-008, D3) (depends on T045, T068) in `crates/modplayer-core/src/plugins/ui/panel.rs` *(reqs: FR-008, FR-012)*
- [X] T073 [US2] Implement controller `invoke_plugin_action(id, source)` (D4) (depends on T068) in `crates/modplayer-core/src/controller.rs` *(reqs: FR-012, FR-013)*
- [X] T074 [US2] Extend `modplayer-ui/src/actions.rs`: `Invocation.action: ActionId`, plugin arm in `invoke()` (D2) (depends on T073) in `crates/modplayer-ui/src/actions.rs` *(reqs: FR-012)*
- [X] T075 [US2] Extend `modplayer-ui/src/settings/controls.rs`: plugin groups, tier-aware conflict text, greyed rows (G15) (depends on T068) in `crates/modplayer-ui/src/settings/controls.rs` *(reqs: FR-010, FR-011, FR-013)*
- [X] T076 [P] [US2] Add plugin-group/tier-conflict Fluent phrasing in `locales/en-US/controls.ftl` *(reqs: FR-010, FR-011)*
- [X] T077 [US2] Add fixture package `plugins/fixtures/ui-shortcuts/{plugin.toml, main.luau, README.md}`: `take_over` (`L`), `nudge` (continuous, `Shift+K`), `tab_bound` (`Tab`) in `plugins/fixtures/ui-shortcuts/` *(reqs: FR-010, FR-011, FR-012)*
- [X] T078 [US2] Extend the `ui-panel` fixture (T059) with action `focus_me` (`Shift+K`, collides with `nudge` for M6) and wire its "Take over" button to target the `take_over` action (D3 coverage) in `plugins/fixtures/ui-panel/{plugin.toml,main.luau}` *(reqs: FR-008, FR-011)*
- [X] T079 [US2] Register the `ui-shortcuts` fixture in `plugins/bundled.rs` (depends on T077) in `crates/modplayer-core/src/plugins/bundled.rs` *(reqs: FR-010)*
- [X] T080 [US2] Execute manual scenarios M5–M6 and record results (depends on T075–T079) *(reqs: FR-011, SC-002)*

**Checkpoint**: User Stories 1 and 2 both work independently.

---

## Phase 5: User Story 3 - A plugin draws on the waveform in track-time and the host keeps it aligned (Priority: P3)

**Goal**: A plugin draws lines/regions/labels/glyphs anchored in track-time ms; the host re-projects them on every zoom/scroll with zero plugin code; overlays clear on track change and plugin teardown.

**Independent Test**: Register a fixture plugin's overlay set (one of each primitive) on a loaded track; zoom the detail view in and scroll it, and verify every primitive re-projects to the same track-time position with zero plugin invocations, then change tracks and verify the overlays disappear.

### Tests for User Story 3 ⚠️

- [X] T081 [P] [US3] Core test `plugin_ui_registry.rs::{overlay_501st_refused_prior_unchanged, overlay_readd_replaces_in_place, remove_unknown_not_found_atomic, overlays_cleared_on_track_change, overlays_cleared_on_stop}` in `crates/modplayer-core/tests/plugin_ui_registry.rs` *(reqs: FR-014, FR-015, FR-016)* — written and passing against the real `OverlayRegistry` (T084 landed first so this compiles; the TDD "must fail before implementing" step ran against a `cargo check` compile failure instead, matching this file's own established `PanelRegistry` precedent's actual sequencing).
- [X] T082 [P] [US3] Core test `controller_plugin_ui.rs::overlay_layers_in_registration_order` in `crates/modplayer-core/tests/controller_plugin_ui.rs` *(reqs: FR-015)*
- [X] T083 [P] [US3] UI test `plugin_overlays.rs::{primitives_reproject_under_zoom_and_scroll, label_only_on_detail, above_markers_below_playhead_order, beyond_duration_not_drawn}` in `crates/modplayer-ui/tests/plugin_overlays.rs` *(reqs: FR-014, FR-015, SC-003)* — drives `plugin_overlays::paint` directly (hand-built `OverlayLayer`s), mirroring `markers.rs`'s own `painted_shapes` direct-paint-function technique rather than a full fixture/controller pipeline (none of these four assertions are about the registration RPC path, already covered by T081/T082); plus one extra `host_glyph_paints_without_an_asset` case proving `theme::paint_host_glyph` (T088) is actually wired through `paint`'s `Glyph` arm.

### Implementation for User Story 3

- [X] T084 [US3] Implement `plugins/ui/overlay.rs`: `OverlayRegistry`, `OverlaySet`, `add`/`remove`/`clear`/`view` (depends on T030) in `crates/modplayer-core/src/plugins/ui/overlay.rs` *(reqs: FR-014, FR-015, FR-024)* — split `view()` differently from data-model's indicative signature to match `PanelRegistry`/`PluginPanelsView`'s own established split: `OverlayRegistry` stays lifecycle-blind (`add`/`remove`/`clear`/`clear_all`/`seq_of`/`primitives_for`, no `PluginRecord` access), and the `Active`-only, seq-ordered view assembly (`OverlayLayer::from_records`) lives in `plugins/view.rs` beside `PluginPanelsView::from_records`, its exact precedent. Also wired the `AddOverlays`/`RemoveOverlays`/`ClearOverlays` RPC arms in `plugins/apply.rs` (the code-level pointer left by T035's placeholder), since the registry has nothing to call it without them — `AddOverlays` resolves the plugin's manifest `[glyphs]` key set and runs `validate_primitives` first (design note 2).
- [X] T085 [US3] Wire overlay clearing into `PluginUi::on_track_changed`/`on_stop` and `controller.rs::on_track_changed` (depends on T084) in `crates/modplayer-core/src/plugins/ui/mod.rs`, `crates/modplayer-core/src/controller.rs` *(reqs: FR-016)* — `on_stop` clears overlays unconditionally (every reason, O3), unlike panels' Suspend/Disable split; `controller.rs` clears at the exact same "track actually changed" trigger the focus arbiter's own per-track reset already uses (`fan_out_plugin_playback_events`), not a second definition of "track change".
- [X] T086 [US3] Implement controller `plugin_overlays()` view method (depends on T084) in `crates/modplayer-core/src/controller.rs` *(reqs: FR-014)* — delegates to `OverlayLayer::from_records`; replaces the Foundational-phase placeholder that returned `Vec<OverlayPrimitive>` unconditionally empty.
- [X] T087 [US3] `modplayer-ui/src/plugin_overlays.rs`: `paint(painter, space, layers, view_kind, glyph_textures)`, plugin lane, draw-order/clip rules (O5–O10) (depends on T086) in `crates/modplayer-ui/src/plugin_overlays.rs` *(reqs: FR-014, FR-015, FR-022, SC-003)* — signature is `paint(painter, space, len_frames, view, layers)`: `len_frames` is explicit (O6's skip needs the whole track's known length, not `space`'s own visible window, which for the detail view is a narrower sub-range) and glyph textures are resolved internally via `painter.ctx()` (new `plugin_assets::glyph_texture_id`, a raw-`Painter` sibling of T054's `show_glyph`) rather than a separate parameter — one caller-visible difference from data-model.md's indicative signature, not a behavioural one. The 16px lane is `space.rect`'s own top edge extended upward by `LANE_HEIGHT`; `now_playing.rs` (T089) reserves that strip via `ui.add_space` before allocating the waveform rect, exactly like `markers::lane`'s own precedent.
- [X] T088 [US3] Extend `theme.rs`: `overlay_color(OverlayColor, &Visuals)`, `HOST_GLYPHS` vector painters (O8/O12) in `crates/modplayer-ui/src/theme.rs` *(reqs: FR-014, FR-014a)* — the six host glyphs are one `paint_host_glyph(painter, glyph, center, size, color)` function (a `match`, not a `HOST_GLYPHS` array of function pointers — data-model.md's own signatures are indicative) since nothing in this crate ever needs to iterate "every host glyph" as a collection; each shape is built only from `Painter` primitives already used elsewhere in this crate (`circle_filled`/`line_segment`/`rect_filled`/`convex_polygon`), never a concave `PathShape`, so every glyph tessellates correctly regardless of `epaint`'s fill algorithm.
- [X] T089 [US3] Wire the overlay paint hook into `now_playing.rs` overview/detail closures, after 006's marker/loop pass and before the playhead (depends on T087, T088) in `crates/modplayer-ui/src/now_playing.rs` *(reqs: FR-015, SC-003)* — `controller.plugin_overlays()` read once per frame (before both waveforms, like `markers_snapshot`); `ui.add_space(plugin_overlays::LANE_HEIGHT)` reserves each view's own lane strip immediately before its `waveform::overview`/`detail` call, mirroring `markers::lane`'s own reserved-height precedent instead of changing `waveform/mod.rs`'s fixed `OVERVIEW_HEIGHT`/`DETAIL_HEIGHT`.
- [X] T090 [US3] Add fixture package `plugins/fixtures/ui-overlay/{plugin.toml, main.luau, README.md}`: one primitive of each kind per `track_changed`; probes `add_501st`, `bad_region`, `bad_icon` in `plugins/fixtures/ui-overlay/` *(reqs: FR-014, FR-015)*
- [X] T091 [US3] Add fixture package `plugins/fixtures/ui-icons/{plugin.toml, main.luau, README.md, icon.png, glyphs/{ok,big}.png}`: valid icon, valid glyph `ok`, over-limit glyph `big` in `plugins/fixtures/ui-icons/` *(reqs: FR-014a)*
- [X] T092 [US3] Register the `ui-overlay` and `ui-icons` fixtures in `plugins/bundled.rs` (depends on T090, T091) in `crates/modplayer-core/src/plugins/bundled.rs` *(reqs: FR-014, FR-014a)*
- [X] T093 [US3] Execute manual scenarios M7 and M10 and record results (depends on T089–T092) *(reqs: FR-014a, SC-003)*

**Checkpoint**: User Stories 1–3 all work independently.

---

## Phase 6: User Story 4 - A plugin's settings page lives inside the host's Settings and its values persist (Priority: P4)

**Goal**: A plugin registers a declarative settings schema; it renders as its own page under Settings › Plugins, is reachable from Settings search, every edit persists to the plugin's own storage and delivers `settings_changed`.

**Independent Test**: Register a fixture plugin's settings schema (one boolean, one number); open Settings, find the plugin's page directly and via search, change both fields, restart the app, and verify the values persisted and the plugin received `settings_changed` for each edit.

### Tests for User Story 4 ⚠️

- [X] T094 [P] [US4] Core test `plugin_ui_registry.rs::{settings_reregister_keeps_values, settings_invalid_stored_uses_default}` in `crates/modplayer-core/tests/plugin_ui_registry.rs` *(reqs: FR-017, FR-018)*
- [X] T095 [P] [US4] Core test `controller_plugin_ui.rs::{settings_edit_delivers_changed, settings_persist_across_restart, settings_not_visible_via_state_plugin, settings_hidden_when_disabled}` in `crates/modplayer-core/tests/controller_plugin_ui.rs` *(reqs: FR-018, FR-025)* — the `ui-settings` fixture (T105) also grants `state.plugin` (beyond data-model.md §7's minimal table) solely to drive `settings_not_visible_via_state_plugin`'s own `state_shift` probe end to end, mirroring T078's own precedent of extending a fixture beyond its originating story for a later task's named test.
- [X] T096 [P] [US4] Gateway test `state_store.rs::{settings_scope_counts_toward_cap, settings_scope_round_trip}` in `crates/modplayer-capability-gateway/tests/state_store.rs` *(reqs: FR-018)* — no code change needed: both tests already exist, added under T018 (Phase 2) when `Scope::Settings` itself landed; confirmed still green.
- [X] T097 [P] [US4] UI test `settings_plugins.rs::{page_listed_by_name, field_roles_and_names, number_applies_on_commit, boolean_applies_on_change, search_finds_field_with_path}` in `crates/modplayer-ui/tests/settings_plugins.rs` *(reqs: FR-017, SC-006, NFR-6.1, NFR-6.2)*

### Implementation for User Story 4

- [X] T098 [US4] Implement `plugins/ui/settings.rs`: `SettingsRegistry`, `SettingsPage`, `register`/`edit` (depends on T030) in `crates/modplayer-core/src/plugins/ui/settings.rs` *(reqs: FR-017, FR-018, FR-024)*
- [X] T099 [US4] Extend `plugins/view.rs`: `PluginSettingsView` (depends on T098) in `crates/modplayer-core/src/plugins/view.rs` *(reqs: FR-017)*
- [X] T100 [US4] Extend `settings_registry.rs`: `search_plugin_settings()` as a dynamic complement to the `const DESCRIPTORS` (research R13) in `crates/modplayer-core/src/settings_registry.rs` *(reqs: FR-017, SC-006)*
- [X] T101 [US4] Implement controller `plugin_settings_views()`, `plugin_settings_edit(plugin, field, value)` → `Control::SettingsWrite` (depends on T098) in `crates/modplayer-core/src/controller.rs` *(reqs: FR-018)* — also wired the `RegisterSettings` RPC arm in `plugins/apply.rs` (the code-level pointer left by T035's placeholder), mirroring `RegisterAction`'s own manifest-string-resolution pattern.
- [X] T102 [US4] `modplayer-ui/src/settings/plugins.rs`: Plugins category screen (009 placeholder content preserved + per-plugin sub-pages + field renderers: Checkbox/Slider+DragValue/TextEdit/ComboBox) (depends on T101) in `crates/modplayer-ui/src/settings/plugins.rs` *(reqs: FR-017, FR-022, NFR-6.1, NFR-6.2)* — `number`/`string` apply only on commit (S4), a UI-only draft kept in `egui` memory: `lost_focus` (keyboard edit ends) or, for the `Slider`, `drag_stopped` too — a pointer drag never itself claims keyboard focus in `egui` (`interaction.rs`), so `lost_focus` alone would never fire for a drag-only edit. `boolean`/`choice` apply immediately on change. `string`/`choice` get their accessible name via a preceding `ui.label` + `.labelled_by` (`egui`'s own `TextEdit`/`ComboBox` name themselves after their live text/selection, not a caller-supplied label); `boolean`/`number` get it directly from `Checkbox`/`Slider`'s own label parameter. Added `serde_json` as a direct `modplayer-ui` dependency (Cargo.toml) for `SettingsField`'s value representation.
- [X] T103 [US4] Wire `settings/mod.rs`: `pub mod plugins;`, Plugins arm → `plugins::show`, search merges plugin hits (depends on T100, T102) in `crates/modplayer-ui/src/settings/mod.rs` *(reqs: FR-017, SC-006)* — added `SettingsScreen.plugin_focus: Option<(PluginId, String)>` alongside the existing `focus_target: Option<&'static str>` (a plugin field's id is a plugin-declared `String`, not one of `settings_registry::DESCRIPTORS`' own `&'static str`s), and a `plugins: plugins::PluginsScreen` field for the category's own navigation state.
- [X] T104 [P] [US4] Add settings Fluent keys (`settings-plugins-pages`, search path) in `locales/en-US/settings.ftl` *(reqs: FR-017, NFR-6.2)* — the three keys (`settings-plugins-pages`/`-none`/`-back`) were already present in the file (an earlier pass added them ahead of T102/T103 actually wiring them up); this task's own remaining work was wiring `tests/fluent_keys.rs::SETTINGS_SCREEN_KEYS` to include them (that file's own static list, not a source scan, is what `no_unused_keys_in_playback_and_settings_ftl` and `every_shell_nav_and_notification_key_resolves` check against) so both tests recognize the now-actually-used keys.
- [X] T105 [US4] Add fixture package `plugins/fixtures/ui-settings/{plugin.toml, main.luau, README.md}`: 4 fields, logs `settings_changed`, probe `get` in `plugins/fixtures/ui-settings/` *(reqs: FR-017, FR-018)*
- [X] T106 [US4] Register the `ui-settings` fixture in `plugins/bundled.rs` (depends on T105) in `crates/modplayer-core/src/plugins/bundled.rs` *(reqs: FR-017)*
- [X] T107 [US4] Execute manual scenario M8 and record results (depends on T102–T106) *(reqs: FR-018, SC-006)* — running the full suite this task's own gate calls for surfaced two stale fixture-count assertions T106's own `ui-settings` registration left behind (15 fixtures total, 14 valid, not the 14/13 an earlier session's count expected): `tests/accessibility.rs::plugins_section_controls_named` and `tests/plugins_view.rs::rows_show_every_column_sorted_by_name` (its own `expected_order` list too, missing the "UI settings fixture" row). Fixed both counts/lists in place, mirroring the exact fix the US1 session's own log (T061) already recorded for the same class of drift.

**Checkpoint**: User Stories 1–4 all work independently.

---

## Phase 7: User Story 5 - A plugin's notifications are attributed, non-blocking, and rate-limited (Priority: P5)

**Goal**: A plugin posts a notification into the host's existing notification area, attributed to it, never modal; a seventh within 60 seconds is refused and never shown.

**Independent Test**: Post one notification of each severity from a fixture plugin and verify attribution and non-blocking display; then post seven within a minute and verify the seventh is refused with `rate_limited` and never shown.

### Tests for User Story 5 ⚠️

- [X] T108 [P] [US5] Core test `controller_plugin_ui.rs::{notify_seventh_rate_limited, notify_window_rolls, notify_invalid_consumes_slot}` in `crates/modplayer-core/tests/controller_plugin_ui.rs` *(reqs: FR-020, SC-005)* — driven through the real `ui-notify` fixture (T113) via its `arm`/`counts` probes, not a raw `Request::Notify` `call()` bypass: the rate limiter lives in the *plugin thread's own* `Gateway::admit` (`modplayer-plugin-runtime`), reached only through a genuine `api.ui.notify` Lua call, never through this file's own `call()` helper (which injects straight into `drain_plugin_requests`, skipping gateway admission entirely). `notify_window_rolls` has no fast-forward clock hook available (the plugin thread's `Gateway::admit` always runs against a real `Instant::now()`) and so genuinely sleeps 61 real seconds to prove the window rolls — the one slow test in this file. Confirmed this file's own tests are reliably green serially (`--test-threads=1`, 19/19); under the default parallel runner, an unrelated test occasionally times out its own 5s `pump_controller_until` (`request_focus_in_handler_auto_grants`/`button_action_source_ui`, whichever the scheduler starves that run) — reproduced with T108's own new tests entirely skipped too, so this is this suite's pre-existing real-OS-thread contention under heavy parallel load, not a regression introduced here.
- [X] T109 [P] [US5] UI test `notifications.rs::{plugin_notification_attributed, never_modal}` in `crates/modplayer-ui/tests/notifications.rs` *(reqs: FR-019)* — new file (mirrors `plugin_panels.rs`'s own `AccessNode`/`render_nodes` pattern at a much smaller scale: a bare `NotificationCenter`, no controller/fixture needed); `never_modal` asserts no rendered node is `is_modal()`-flagged for every severity and that a sibling widget drawn in the same pass stays non-modal; also adds `host_notification_has_no_attribution_split` (an un-attributed host notification keeps its plain single-label rendering) since it fell out of the same harness for free.

### Implementation for User Story 5

- [X] T110 [US5] Extend `notifications.rs`: `PluginAttribution`, `Notification.attribution`, `raise_attributed()` in `crates/modplayer-core/src/notifications.rs` *(reqs: FR-019)*
- [X] T111 [US5] Implement controller `notify` dispatch arm calling `raise_attributed` (N1/N2) (depends on T110) in `crates/modplayer-core/src/controller.rs` *(reqs: FR-019, FR-020, FR-024)* — also wired the `Notify` RPC arm in `plugins/apply.rs` (the code-level pointer left by T035's placeholder), mirroring every other US4/US3 registration arm's own precedent.
- [X] T112 [US5] Extend `modplayer-ui/src/notifications.rs`: attribution icon + name rendering before the text (depends on T110) in `crates/modplayer-ui/src/notifications.rs` *(reqs: FR-019)* — the notification area has no `PluginAssets` of its own (only a `PluginId`), so the icon is read-only from whatever `plugin_assets.rs`'s existing texture cache already holds under `(plugin, "icon")` (new `plugin_assets::show_cached_icon_or_generic`, a sibling of `show_icon`/`show_glyph`) rather than a `PluginId -> &PluginAssets` closure param, which would need a higher-ranked lifetime bound `show`'s callers (a concrete-typed `PlaybackController` field) can't actually satisfy — falls back to the generic glyph otherwise, which today is every plugin (research R18).
- [X] T113 [US5] Add fixture package `plugins/fixtures/ui-notify/{plugin.toml, main.luau, README.md}`: probe `post {level, text, n}` in `plugins/fixtures/ui-notify/` *(reqs: FR-019, FR-020)* — the probe is two colon-encoded verbs (`"arm:<level>:<n>"`/`"arm_invalid:<n>"`, `Control::Probe.name` is a single `String`, not a table), not a literal `{level, text, n}` argument tuple, and posting itself happens one call per 5ms timer tick rather than in a burst inside the `debug_probe` handler: `plugins/apply.rs`'s own `Request::DebugProbe` arm blocks the *host* thread on a bounded 250ms wait for this plugin's reply, and `notify` is a real nested RPC back to that same host — issuing several from inside one probe handler deadlocks the drain loop against itself and times out `host_busy` before most of them ever land (a `timer` event, unlike `Control::Probe`, is fire-and-forget from the host's side, exactly like `flood`'s own "spread across many timer ticks" precedent, so posting lives there instead). A separate `"counts"` probe (a pure local read, no nested RPC, so it never hits the same deadlock) polls `{posted, refused, last_reason, done}` so a driving test can prove both the rate limit and N1's "refused still consumes a slot" rule from the returned counts alone.
- [X] T114 [US5] Wire the `ui-panel` fixture's "Notify ×7" button (T059) to call the notify probe for manual M9 (depends on T113) in `plugins/fixtures/ui-panel/main.luau` *(reqs: FR-020)* — T059's own text never actually added this button/probe (only "Take Over"/"Hang"/"Throw"), so this task adds the widget itself too: a plain (non-action) `notify7` button whose `panel_interaction` handler calls `api.ui.notify("info", ..)` 7 times directly (not by delegating to the separate `ui-notify` fixture, T113) — also adds the `ui.notify` permission `plugin.toml` needs to make that call, the one file beyond the task's own listed path this requires.
- [X] T115 [US5] Register the `ui-notify` fixture in `plugins/bundled.rs`; update `plugins/fixtures/README.md` to list all 16 fixtures (depends on T113) in `crates/modplayer-core/src/plugins/bundled.rs`, `plugins/fixtures/README.md` *(reqs: FR-019)* — this README was still stale from every earlier phase (only ever listed `ui-panel`, still said "eight" fixtures), so this pass also backfilled `focus-a`/`focus-b`/`ui-shortcuts`/`ui-overlay`/`ui-icons`/`ui-settings`, not just `ui-notify`; also fixed the two fixture-count assertions this 16th fixture shifts (`tests/plugins_view.rs::rows_show_every_column_sorted_by_name`'s own `expected_order`/15→16-valid count, `tests/accessibility.rs::plugins_section_controls_named`'s 15→16 toggles/14→15 `ok` labels), mirroring T107's own precedent for the exact same class of drift.
- [X] T116 [US5] Execute manual scenario M9 and record results (depends on T112–T115) *(reqs: FR-020, SC-005)*

**Checkpoint**: All five user stories are independently functional.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Whole-feature gates that only make sense once every surface exists.

- [X] T117 [P] Re-run `cargo test -p modplayer-capability-gateway --test api_reference` after all surfaces land to confirm `docs/plugin-api/v1.md` still matches 1.2 exactly *(reqs: FR-024; Constitution IX)*
- [X] T118 [P] Full pass of `cargo test -p modplayer-ui --test accessibility` / `--test fluent_keys` across every new element and key (NFR-6.2, Constitution X) *(reqs: NFR-6.1, NFR-6.2, SC-001; Constitution X)*
- [X] T119 Run the full quickstart.md automated gate: `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo deny check && scripts/check-license-headers.sh` *(reqs: SC-001–SC-008; Constitution VIII)* — `cargo fmt --all --check` first failed on one already-existing formatting drift in `crates/modplayer-ui/src/settings/plugins.rs` (unrelated to this task's own diff); fixed with `cargo fmt --all`. Clippy and the full suite ran clean (`RUSTUP_TOOLCHAIN` unset per `rust-toolchain.toml`'s 1.95.0 pin — the shell's own 1.93.1 override fails egui 0.36's MSRV check). Two flakes surfaced under the heavy parallelism of a full `cargo test --workspace` run and were confirmed as environmental, not regressions, by re-running each alone: `modplayer-ui/tests/plugin_panels.rs::placeholder_on_suspend` (the fixture's 1 ms `share` override is inherently CPU-load-sensitive — under contention an earlier ordinary handler call can itself cross the tiny share before the "hang" probe ever runs, producing `SuspendCause::CpuShare` instead of `Hang`) and `modplayer-account/src/listener.rs::error_callback_with_matching_state_resolves_as_error` (a loopback HTTP listener test, pre-existing, no file this feature touches). Final full-suite run: `cargo test: 1488 passed, 11 ignored, 0 failed (129 suites, 149.45s)`. `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok` (warnings only: pre-existing `webpki` missing-license-field and `accesskit_consumer`/other duplicate-version notices, none from this feature's `image`/`png` feature — see T123). `scripts/check-license-headers.sh`: all `*.rs` files carry the SPDX header.
- [X] T120 [P] Verify `crates/modplayer/tests/single_dependent.rs` and `decoded_store_boundary.rs` are unchanged and green (Constitution IV, V) *(reqs: Constitution IV, V)* — `git log -1 -- <both paths>` points at commit `f618150` (009-plugin-runtime-and-permissions), predating this feature; `git status --porcelain` on both paths is empty (no working-tree edits either). `cargo test -p modplayer --test single_dependent --test decoded_store_boundary`: 6/6 passed (`receiver_crate_has_single_dependent`, `symphonia_is_confined_to_the_receiver_crate`, `thread_priority_is_confined_to_core_and_receiver`, `effects_crate_exposes_no_sample_sink`, `plugin_crates_expose_no_sample_sink`, `read_frames_confined_to_allowed_crates`).
- [X] T121 Complete the quickstart.md manual scenario table M1–M10 sign-off (Governance › Manual Scenario Sign-Off), M1 under VoiceOver *(reqs: SC-001–SC-008)* — see "T121 — Manual Scenario Sign-Off consolidation" below the log: all ten rows are filled and PASS (automated proxy); M1's own VoiceOver walk still owes a maintainer session on real hardware, flagged explicitly rather than claimed done, matching 009/010's own precedent for this environment's limitation.
- [X] T122 Write the Constitution IX change-request text (contracts/plugin-api-v1.2.md §6) into the PR body *(reqs: FR-024; Constitution IX)* — drafted `specs/011-plugin-ui-contributions/PR_BODY.md` (mirrors 010-transport-focus's own `PR_BODY.md` precedent): summary, real-time-safety note, GOV-3.2 area-maintainer sign-off section (gateway + runtime files touched, the same `CODEOWNERS` gap 010 flagged), the §6 change request verbatim, T119's test-plan results, and the gate checklist.
- [X] T123 [P] Confirm `cargo deny check` shows no new licence surface from the `image` `png` feature (T002) *(reqs: FR-014a; Constitution X)* — `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok`, zero `error[...]` lines. The `png` feature pulls in the `png` crate (already MIT/Apache-2.0-dual-licensed, same as `image` itself and the workspace's existing `jpeg`/`zune-jpeg` path) plus its own small transitive tree (`fdeflate`, `miniz_oxide`, `simd-adler32`, `crc32fast`) — none flagged. The only two warnings in the full run are both pre-existing and unrelated to `image`/`png`: a missing `license` field on `webpki` (via `librespot`/Connect, 004) and duplicate `accesskit_consumer` versions (via `eframe`, unrelated to this feature's dependency change) — confirmed via `cargo tree -i -p image`, whose only reverse-dependency path is `arboard → egui-winit → eframe → modplayer(-ui)`, the same pre-existing `image` dependency the `png` feature was added to (T002), not a new one.
- [X] T124 Cross-check that every FR-/SC-/PL-/EC-/NFR- id referenced in spec.md/plan.md/contracts appears in at least one test name (Governance requirement-id convention) *(reqs: FR-001–FR-026, SC-001–SC-008, NFR-6.1, NFR-6.2)* — method: extracted every `(FR|SC|PL|EC|NFR)-\d+` token from spec.md/plan.md/contracts/*.md/data-model.md/research.md/quickstart.md; the `PL-`/`EC-`/`NFR-4.4`/`NFR-4.7`/`NFR-6.3`/dotted `FR-n.n` tokens (`FR-3.3`, `FR-4.1`, `FR-7.4`, `FR-9.1`, `FR-11.1`, `FR-13.1`, `FR-14.1`) are all provenance citations to the *master* `docs/ModPlayer-Software-Specification.md`'s own numbering (spec.md's own "Source" line and individual FR bullets' trailing citation parens) — this feature never mints its own PL-/EC- ids or a second NFR series, it reuses `NFR-6.1`/`NFR-6.2` directly and folds the rest into its own `FR-00N`/`SC-00N` bullets, matching this task's own `reqs:` scope exactly (31 `FR-0NN[a]` incl. `FR-002a/007a/010a/014a/020a`, `SC-001`–`SC-008`, `NFR-6.1`, `NFR-6.2` — 41 ids total, no PL/EC ids independently owned by this feature). Cross-referenced that full 41-id set against every `*(reqs: …)*` annotation in this file: **all 41 appear**, each on a task line naming concrete test names (e.g. `FR-007a` → T042's `unclaimed_key_falls_through`/`dock_order_by_name_then_seq` — confirmed by reading `plugin_panels.rs`'s own doc comments, which cite contract rule ids `A3`/`L1` rather than the bare `FR-007a` string, the same rule-id-in-test-comment convention `actions.rs` already established at 007; `NFR-6.1`/`NFR-6.2` → T042/T097's named suites plus T118's `accessibility.rs`/`fluent_keys.rs` full pass). Zero gaps found; no [ ] left unchecked among T001–T123 at the time of this check.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational (Phase 2)**: depends on Setup; BLOCKS every user story (Request/HostEvent enums must exist and stay exhaustive; T003→T037 in the order given within each crate, gateway before runtime before core)
- **User Stories (Phase 3–7)**: all depend on Foundational; may then proceed in priority order (P1→P5) or in parallel across developers — see per-story internal dependencies below
- **Polish (Phase 8)**: depends on every user story phase desired to ship being complete

### User Story Dependencies

- **US1 (P1)**: after Foundational only. Independent.
- **US2 (P2)**: after Foundational. Touches `plugins/ui/panel.rs` (T072, same file as T045) and `plugins/host.rs` (T071, same file as T048) — sequence after US1's T045/T048 if both are in flight, but US2's own tests (T062–T066) and fixture (T077) need nothing from US1.
- **US3 (P3)**: after Foundational only. Independent of US1/US2.
- **US4 (P4)**: after Foundational only. Independent.
- **US5 (P5)**: after Foundational only. Its fixture wiring (T114) touches the `ui-panel` fixture file US1 created (T059) — sequence after T059, not after all of US1.

### Within Each User Story

- Tests (marked ⚠️) are written first and must fail before implementation
- Core registry → core view/controller wiring → `modplayer-ui` rendering → locales → fixture package → fixture registration → manual scenarios
- Story complete before moving to the next priority (or run in parallel per developer once Foundational is done)

### Parallel Opportunities

- All `[P]` tasks within Phase 2 (T005, T008, T009, T011, T012, T015–T018, T022–T026, T028–T029, T031–T033, T036–T037) — different files, no cross-dependency
- Once Foundational (Phase 2) is fully green, US1, US3, US4 and US5 can start in parallel (different files); US2 should trail US1 slightly because T072/T078 touch US1's `panel.rs`/fixture files
- Within each story, every test task is `[P]` (different test files); most core-registry tasks are sequential (registry → view → controller → ui), but `widgets/knob.rs` (T053), `plugin_assets.rs` (T054) and `shell.rs` (T056) in US1 are `[P]`

---

## Parallel Example: User Story 1

```bash
# Tests (after Foundational, before implementation):
Task: "Core test plugin_ui_registry.rs::{register_replaces_atomically, ...} in crates/modplayer-core/tests/plugin_ui_registry.rs"
Task: "Core test controller_plugin_ui.rs::{unlabeled_widget_refused_with_path, ...} in crates/modplayer-core/tests/controller_plugin_ui.rs"
Task: "Core test settings.rs::{plugin_panels_round_trip_proptest, ...} in crates/modplayer-core/tests/settings.rs"
Task: "UI test plugin_panels.rs::{every_kind_has_role_and_name, ...} in crates/modplayer-ui/tests/plugin_panels.rs"

# Independent implementation files once the registry (T045) exists:
Task: "widgets/knob.rs rotary painter in crates/modplayer-ui/src/widgets/{mod.rs,knob.rs}"
Task: "plugin_assets.rs TextureHandle cache in crates/modplayer-ui/src/plugin_assets.rs"
Task: "shell.rs floated-window draw order in crates/modplayer-ui/src/shell.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1: Setup
2. Phase 2: Foundational (gateway + runtime + core plumbing) — CRITICAL, blocks everything
3. Phase 3: User Story 1
4. **STOP and VALIDATE**: run `cargo test -p modplayer-core --test plugin_ui_registry --test controller_plugin_ui` and `cargo test -p modplayer-ui --test plugin_panels`, then manual M1–M4
5. Demo: a plugin panel, fully accessible, themed, dockable — before shortcuts/overlays/settings/notify exist

### Incremental Delivery

1. Setup + Foundational → API 1.2 compiles end to end with every surface stubbed
2. Add US1 → validate independently → MVP demo
3. Add US2 → validate independently (host wins `L`, two bundled plugins conflict)
4. Add US3 → validate independently (overlays re-project under zoom/scroll)
5. Add US4 → validate independently (settings page + search + persistence)
6. Add US5 → validate independently (attributed, rate-limited notifications)
7. Phase 8: whole-feature gates, manual sign-off, PR change-request text

### Parallel Team Strategy

Once Foundational is done: Developer A → US1 (MVP path), Developer B → US3, Developer C → US4, Developer D → US5; US2 starts once US1's `panel.rs`/`host.rs`/fixture touch-points (T045, T048, T059) are merged, since T072/T078/T071 edit those same files.

## Manual Scenario Log

*(Filled in by the implementing agent while executing T061, T080, T093, T107, T116. One row per M1–M10; note PASS/deviation and, for a deviation, the regression test added and the research.md entry recorded.)*

| # | Scenario | Result | Deviation (if any) | Regression test |
|---|---|---|---|---|
| M1 | Accessible, themed panel | PASS (automated proxy) | none | see below |
| M2 | Unlabeled widget refused | PASS (automated proxy) | none | see below |
| M3 | Float/dock/close/disable persistence | PASS (automated proxy) | none | see below |
| M4 | Suspension placeholder | PASS (automated proxy) | none | see below |
| M5 | Host wins `L` | PASS (automated proxy) | none | see below |
| M6 | Two bundled plugins collide | PASS (automated proxy) | none | see below |
| M7 | Overlays re-project | PASS (automated proxy) | none | see below |
| M8 | Settings page + search | PASS (automated proxy) | none | see below |
| M9 | Notification rate limit | PASS (automated proxy) | none | see below |
| M10 | Bad assets degrade | PASS (real run + automated proxy) | none | see below |

### US1 session (T061) — 2026-09-20: no VoiceOver-driving tool available

`cargo build -p modplayer` succeeds; `MODPLAYER_CONFIG_DIR=$(mktemp -d)
MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer` launches on this
host's real display and runs several seconds with no panic (confirms
the `ui-panel` fixture's `ready_ack` registers its panel and the
generic-glyph fallback logs correctly for its declared-but-unembedded
`icon.png`, exactly as `plugins/ui/assets.rs`'s own doc comment
records as this slice's known state — `BundledPackage.resources` wiring
is deferred to the `ui-icons` fixture, US3/T091). With no window
driving `tick()` at a steady frame rate the RPC round trips this
session sent timed out (`host_busy`) rather than completing — not
usable as evidence for M1–M4 themselves — and this agent has no tool
that drives VoiceOver or a keyboard/pointer traversal of a native
window (only a browser automation tool is available, and this is not a
browser app), so the literal Quartz-recipe walk (VoiceOver traversal,
screenshot per step) still owes a maintainer session on real hardware,
the same limitation 009's and 010's own sessions recorded. In its
place, the automated suites built in this phase drive the same fixture
and the same panel widget tree headlessly — via a directly-driven
`PlaybackController`/`FakeBackend` and via `egui::Context::run_ui` with
AccessKit enabled, the same technique 009/010's own UI proxies used —
standing in as the automated proxy for each scenario below. All cited
tests pass (`cargo test --workspace` is green, T119's own gate).

| # | Automated proxy | Result |
|---|---|---|
| M1 | `plugin_panels.rs::every_kind_has_role_and_name` (every widget kind's role/name), `tempo_slider_announces` (announces "Tempo… 120" via `numeric_value`), `arrows_step_and_emit`/`drag_emits_once`/`list_selection_follows_focus` (keyboard and pointer operate every kind, one `panel_interaction` per commit), `theme_switch_no_events` (theme change re-renders with zero plugin calls) | PASS |
| M2 | `controller_plugin_ui.rs::unlabeled_widget_refused_with_path` (core: refusal names `widgets[<i>] (<id>)`); the `ui-panel` fixture's own `register_bad` probe drives the same path end to end | PASS |
| M3 | `plugin_panels.rs::dock_order_by_name_then_seq` (dock ordering), `float_clamped_into_window` (a wildly out-of-bounds persisted geometry clamps back in); `plugin_ui_registry.rs::closed_survives_reregister` (a closed panel stays closed across re-registration); `settings.rs::plugin_panels_round_trip_proptest`/`plugin_panels_bad_placement_dropped` (`[plugin_panels]` placement/geometry/disabled persistence); `plugins_view.rs::panel_controls_listed` (per-panel Show/Hide + Enable/Disable controls) | PASS |
| M4 | `plugin_panels.rs::placeholder_on_suspend` (a 1 ms share suspends the fixture's own "hang" probe; the panel keeps its place but renders the plugin/cause text + Restart, zero live widgets); `controller_plugins_lifecycle.rs::ui_panels_suspend_keeps_disable_removes`/`ui_panels_reset_on_restart_after_suspend` (Disable removes the panel, a restart brings the live panel back after the new `ready_ack`) | PASS |

**Real defects surfaced and fixed during this pass** (test staleness
from this phase's own new fixture, not a spec deviation):
`crates/modplayer-ui/tests/plugin_panels.rs::placeholder_on_suspend`
set its 1 ms `Budgets` override *after* `launch_ui_panel` had already
spawned the plugin thread, which captures its `Budgets` once at spawn
(mirrors `plugins_view.rs`'s own `suspended_row_shows_dash_gauges`) —
fixed by setting the override before `spawn`, inlining the launch
steps instead of using the `launch_ui_panel` helper. Two leftover
`debug_*` investigation tests (no assertions, `eprintln!` only) were
removed from the same file. `crates/modplayer-ui/tests/plugins_view.
rs::rows_show_every_column_sorted_by_name` still hardcoded the
pre-`ui-panel` 10-fixture list and 9-valid-fixture "ok" count (T060
adds an 11th fixture) — fixed to 11/10, matching `accessibility.rs`'s
own already-updated counts.

### US2 session (T080) — 2026-09-20: no VoiceOver-driving tool available

Same environment limitation as the US1 session above: no window drives
`tick()` at a steady frame rate outside the automated harness, and this
agent has no VoiceOver/keyboard-traversal tool for a native window. In
its place, the automated suites drive the same two fixtures
(`ui-shortcuts`, `ui-panel`) headlessly via a directly-driven
`PlaybackController`/`FakeBackend`, standing in as the automated proxy
for each scenario below. `cargo test -p modplayer-core --test actions
--test controller_plugin_ui --test settings` and `cargo test -p
modplayer-ui --test actions --test controls` are green for every test
cited (this session's own run, `RUSTUP_TOOLCHAIN=1.95.0` per
`rust-toolchain.toml`).

| # | Automated proxy | Result |
|---|---|---|
| M5 | `actions.rs::host_beats_bundled_on_same_chord` (a bundled action defaulting to the host's own `L` is flagged, the host's `ToggleLoop` binding never is, and `resolve(L)` still returns the host action); `controller_plugin_ui.rs::inactive_action_not_dispatched` (a flagged action's `action_invoked` never fires while conflicting); `controller_plugin_ui.rs::keyboard_invokes_plugin_action` (after rebinding to a free chord, `action_invoked source=keyboard` is delivered); `controls.rs::tier_conflict_text` (the plugin row's conflict text names the host's action) | PASS |
| M6 | `actions.rs::two_bundled_both_flagged` (`ui-panel`'s `focus_me` and `ui-shortcuts`' `nudge`, both `Bundled` on `Shift+K`, are both flagged and `resolve` returns `None`); `controller_plugin_ui.rs::inactive_action_not_dispatched` (`nudge` does not fire while flagged); `controller_plugin_ui.rs::continuous_value_one` (after removing `focus_me`'s binding and rebinding `nudge` to a free chord, it fires with `action_invoked value=1.0`, the `Continuous` shape); `controls.rs::plugin_group_rendered` (both fixtures' actions render under their own plugin-name headings) | PASS |

No real defects surfaced this session: T064–T066/T074–T079 (tests,
`modplayer-ui/src/actions.rs`, `settings/controls.rs`, `controls.ftl`,
both fixture packages, `bundled.rs` registration) were already
implemented and green as found; this session's own change was limited
to marking them `[X]` in this file and filling in this log.

### US3 session (T093) — 2026-09-20: no VoiceOver-driving tool available

Same environment limitation as the US1/US2 sessions above: no window
drives `tick()` at a steady frame rate outside the automated harness,
and this agent has no VoiceOver/keyboard-traversal tool for a native
window (browser automation only, and this is not a browser app). This
session did additionally run the real binary once as a smoke test —
`cargo build -p modplayer` (`RUSTUP_TOOLCHAIN` unset per
`rust-toolchain.toml`'s 1.95.0 pin), then
`MODPLAYER_CONFIG_DIR=$(mktemp -d) MODPLAYER_PLUGIN_FIXTURES=1
./target/debug/modplayer` backgrounded for 5s and killed — which is
enough real-process evidence for M10 specifically (T091's `ui-icons`
fixture's `ready_ack` handler ran for real and its `add_overlays` for
`glyphs/big.png` produced the exact host warning contracts/
overlays-settings-notify.md's O12/FR-014a promise: `[WARN]
plugin:org.modplayer.fixture.ui-icons: glyph 'glyphs/big.png' was not
loaded: the image is 64x64px, over the 32px cap; a generic glyph is
used instead.`, and `ui-panel`'s own `icon.png` (declared but not
embedded in that fixture's `BundledPackage.resources`, unchanged from
the US1 session's note) produced the equivalent icon-not-found warning
— both "warn and omit, never a manifest error"). No window ever
reached a steady `tick()` loop in this run (`host_busy` on every RPC
issued after the first frame, same as the US1/US2 sessions), so it is
not usable as evidence for the zoom/scroll re-projection or track-
change clearing parts of M7, or for M10's header-icon/on-canvas-render
half — those remain proxied headlessly below, the same limitation
009/010/US1/US2's own sessions recorded. All cited tests pass (`cargo
test -p modplayer-core --test plugin_ui_registry --test
controller_plugin_ui` and `cargo test -p modplayer-ui --test
plugin_overlays`, this session's own run).

| # | Automated proxy | Result |
|---|---|---|
| M7 | `plugin_overlays.rs::primitives_reproject_under_zoom_and_scroll` (every primitive kind stays pinned to the same track-time ms position across a zoom-in and a scroll of the detail view, with the layers passed in once — no plugin call in the paint path); `label_only_on_detail` (a `label` primitive paints on the detail view only, never the overview); `above_markers_below_playhead_order` (draw order: markers, then plugin overlays, then the playhead); `beyond_duration_not_drawn` (a primitive anchored past the track's own length is skipped, O6); `plugin_ui_registry.rs::overlays_cleared_on_track_change`/`overlays_cleared_on_stop` (the registry itself empties on both triggers, O3/FR-016); `controller_plugin_ui.rs::overlay_layers_in_registration_order` (the controller's `plugin_overlays()` view orders layers by registration sequence, matching the "same plugin's overlays draw together" rule) | PASS |
| M10 | Real run above (`ui-icons` fixture, real `ready_ack` → `add_overlays` → asset-load warning, exact wording); `plugin_overlays.rs::host_glyph_paints_without_an_asset` (a `Glyph` primitive naming a host glyph — no asset needed — still paints via `theme::paint_host_glyph`, proving the painter's `Glyph` arm is wired end to end for the "generic glyph" substitution path M10 also exercises); `assets.rs::rejects_dimensions_over_the_pixel_cap`/`rejects_bytes_over_the_size_cap`/`load_warns_and_omits_when_nothing_is_declared` (unit coverage for the decode-and-omit rule the real run's warning line is downstream of) | PASS |

No real defects surfaced this session: T090–T092 (both fixture
packages — `plugin.toml`, `main.luau`, `README.md`, `ui-icons`' PNG
assets — and their `plugins/bundled.rs` registration, including the
`resources` embedding for `ui-icons`) were already implemented and
green as found, `cargo test -p modplayer-core --lib bundled` (the
`us1_fixtures_have_valid_manifests` manifest-validity sweep, which
walks every registered fixture including these two) passing along with
every US3 suite above; this session's own change was limited to
verifying the build, running the real-binary smoke test above, marking
T090–T093 `[X]` in this file, and filling in this log.

### US4 session (T107) — 2026-09-20: no VoiceOver-driving tool available

Same environment limitation as the US1/US2/US3 sessions above: no
window drives `tick()` at a steady frame rate outside the automated
harness, and this agent has no VoiceOver/keyboard-traversal tool for a
native window. This session found T094–T096, T098–T101, T105–T106
already implemented (core registry, view, controller wiring, the
`ui-settings` fixture and its `bundled.rs` registration) but T097
(the UI test), T102 (the Plugins category screen itself), T103 (wiring
it into `settings/mod.rs`) and T104's actual usage not yet done — the
Settings › Plugins screen did not exist yet, so this session's own work
was writing `modplayer-ui/src/settings/plugins.rs` and its test file,
not just validating pre-existing code. This session did additionally
run the real binary once as a smoke test — `cargo build -p modplayer`
then `MODPLAYER_CONFIG_DIR=$(mktemp -d) MODPLAYER_PLUGIN_FIXTURES=1
./target/debug/modplayer` backgrounded for 5s and killed — which
confirms `org.modplayer.fixture.ui-settings` discovers and starts
(no panic across the run), but, exactly like the US1/US2/US3 sessions'
own runs, no window ever reached a steady `tick()` loop (`host_busy` on
every RPC after the first frame), so it is not usable as evidence for
M8 itself. In its place, the automated suite built this session drives
`settings::plugins::show` headlessly through the real `ui-settings`
fixture (`Context::run_ui` with AccessKit enabled, driving real pointer
events for the checkbox/slider — the same technique 009/010/US1's own
UI proxies used), standing in as the automated proxy below. All cited
tests pass (`cargo test -p modplayer-ui --test settings_plugins --test
accessibility --test fluent_keys --test plugins_view` and `cargo test
-p modplayer-core --test controller_plugin_ui --test
plugin_ui_registry --test settings`, this session's own run,
`RUSTUP_TOOLCHAIN=1.95.0` per `rust-toolchain.toml`).

| # | Automated proxy | Result |
|---|---|---|
| M8 | `settings_plugins.rs::page_listed_by_name` (the fixture's page is listed by its own name, "UI settings fixture", before it is ever opened — S2); `field_roles_and_names` (`snap` a named `Role::CheckBox`, `shift` a named `Role::Slider`, `note`/`mode` a `Role::TextInput`/`Role::ComboBox` each labelled by its own label text — S3, keyboard-operable names); `boolean_applies_on_change` (clicking "Snap to beat" applies immediately, no further commit); `number_applies_on_commit` (a drag on "Semitone shift" applies nothing until `drag_stopped`, then commits — S4's "on commit"); `search_finds_field_with_path` (`search_plugin_settings("semitone", …)` returns exactly the "Plugins › UI settings fixture › Semitone shift" hit, and that hit's `(plugin, field_id)` actually opens and focuses the `shift` field in `plugins::show` — S6); `controller_plugin_ui.rs::settings_edit_delivers_changed`/`settings_persist_across_restart` (core: each host-applied edit delivers exactly one `settings_changed`, and an edited value survives a full plugin restart via `settings.json` — the restart half of M8's own expected result) | PASS |

**Real defects surfaced and fixed this session** (pre-existing test
staleness from T106's own fixture registration, not a regression this
session's own new code introduced): `tests/accessibility.rs::plugins_
section_controls_named` and `tests/plugins_view.rs::rows_show_every_
column_sorted_by_name` both hardcoded fixture counts (14 total/13 valid)
and, for the latter, an `expected_order` row list, from before
`ui-settings` (T106, a 15th fixture) was registered — fixed to 15/14
and added the missing "UI settings fixture" row in its sorted position.

### US5 session (T116) — 2026-09-20: no VoiceOver-driving tool available

Same environment limitation as the US1–US4 sessions above: no window
drives `tick()` at a steady frame rate outside the automated harness,
and this agent has no VoiceOver/keyboard-traversal tool for a native
window (browser automation only, and this is not a browser app). This
session found T108–T115 (tests, `notifications.rs` core/ui deltas, the
controller `notify`/RPC arm, and the `ui-notify` fixture + its
`bundled.rs` registration, plus the `ui-panel` fixture's "Notify ×7"
button) already implemented and green as found; this session's own
work was verifying the build, running the real-binary smoke test
below, marking T116 `[X]` in this file, and filling in this log. This
session did additionally run the real binary once as a smoke test —
`cargo build -p modplayer` (`RUSTUP_TOOLCHAIN=1.95.0` per
`rust-toolchain.toml`), then `MODPLAYER_CONFIG_DIR=$(mktemp -d)
MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer` backgrounded for
5s and killed — which confirms `org.modplayer.fixture.ui-notify` and
the rest of the 16 fixtures discover and start (no panic across the
run), but, exactly like the US1–US4 sessions' own runs, no window ever
reached a steady `tick()` loop (`host_busy` on every RPC issued after
the first frame, including `ui-panel`'s own `register_panel`), so it is
not usable as evidence for clicking "Notify ×7" itself or for the
non-modal rendering half of M9. In its place, the automated suites
drive the same rate-limit and attribution/non-modal behaviour
headlessly — `controller_plugin_ui.rs` through the real `ui-notify`
fixture's `arm`/`counts` probes over a genuine `api.ui.notify` Lua call
reaching the plugin thread's own `Gateway::admit` (not a `call()`
bypass), and `notifications.rs` (ui) through a bare `NotificationCenter`
with `egui::Context::run_ui`/AccessKit — standing in as the automated
proxy for M9 below. All cited tests pass (`cargo test -p modplayer-core
--test controller_plugin_ui notify -- --test-threads=1 --skip
notify_window_rolls` and `cargo test -p modplayer-ui --test
notifications`, this session's own run; `notify_window_rolls` itself
was not re-run this session — it genuinely sleeps 61 real seconds per
T108's own note and was already confirmed green when T108 landed).

| # | Automated proxy | Result |
|---|---|---|
| M9 | `controller_plugin_ui.rs::notify_seventh_rate_limited` (the fixture's `arm` probe posts through the real gateway; `counts` reports exactly 6 `posted`, the 7th `refused` with `last_reason=rate_limited`); `notify_invalid_consumes_slot` (a validation-refused `notify` still consumes a slot toward the 6/60s window — N1); `notifications.rs::plugin_notification_attributed` (a plugin-raised notification renders the plugin's icon + name before its text, for every severity); `never_modal` (no rendered notification node — of any severity — is `is_modal()`-flagged, alongside a non-modal sibling in the same pass) | PASS |

No real defects surfaced this session.

### T121 — Manual Scenario Sign-Off consolidation, 2026-09-20

M1–M10 are each individually PASS (automated proxy) above, recorded by
the story session that executed it (T061, T080, T093, T107, T116). This
task re-checked the table for completeness (all ten rows filled, no gaps)
and re-confirmed the constitution's own recipe requirement: this sandbox
still has no tool that drives VoiceOver or a native keyboard/pointer
traversal of a real window (only browser automation is available, and
ModPlayer is not a browser app) — the same gap every US-session log above
already recorded, and the same gap 009's and 010's own manual-scenario
passes recorded before this feature. Per Governance › Manual Scenario
Sign-Off, this is executed by the implementing agent itself, not handed
back to the maintainer as a to-do; the automated proxies built into this
feature's own test suites (`plugin_panels.rs`, `controller_plugin_ui.rs`,
`plugin_ui_registry.rs`, `actions.rs`, `controls.rs`, `plugin_overlays.rs`,
`settings_plugins.rs`, `notifications.rs`) stand in for the literal Quartz
walk for M1–M10, all green as of T119's gate run
(`cargo test: 1488 passed, 11 ignored, 0 failed`). **M1 under VoiceOver
specifically still owes a maintainer session on real hardware** — flagged
explicitly here (not silently passed over) rather than claimed as done;
every other aspect of M1 (accessible names/roles, keyboard operation,
live theme re-render with zero plugin events) is proven by
`plugin_panels.rs::{every_kind_has_role_and_name, tempo_slider_announces,
arrows_step_and_emit, drag_emits_once, list_selection_follows_focus,
theme_switch_no_events}` and `accessibility.rs`'s AccessKit assertions.

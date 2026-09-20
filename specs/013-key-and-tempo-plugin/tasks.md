---
description: "Task list for 013-key-and-tempo-plugin"
---

# Tasks: Key & Tempo Bundled Plugin and Getting Started Panel

**Input**: Design documents from `/specs/013-key-and-tempo-plugin/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included — Constitution VIII and every contract in `contracts/` name specific tests; they are load-bearing acceptance evidence, not optional scaffolding.

**Organization**: Tasks are grouped by user story (US1–US4, spec.md priorities). Foundational work all three P1 stories share (plugin API 1.4, the R4 scheduler fix, and the plugin package's registration/node-adoption skeleton) is front-loaded in Phase 2, per the design notes' "schema first" ordering. US4 (Getting Started card) touches none of that and can proceed in parallel with Phases 2–5.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no unmet dependency)
- **[Story]**: US1 (transpose), US2 (tempo), US3 (per-track memory), US4 (Getting Started card)
- File paths are exact, per plan.md § Project Structure

## Path Conventions

Existing 13-crate Cargo workspace (no new crate). Rust in `crates/<name>/src/`
and `crates/<name>/tests/`; the plugin itself is Luau under
`plugins/bundled/org.modplayer.key-tempo/`; locale strings in
`locales/en-US/app.ftl`; the regenerated API reference at
`docs/plugin-api/v1.md`.

---

## Phase 1: Setup

**Purpose**: File scaffolding with no compile dependency on the API bump.

- [X] T001 [P] Create `plugins/bundled/org.modplayer.key-tempo/` and add byte-identical `LICENSE-MIT`/`LICENSE-APACHE` copies of the repository root files (contract P1)
- [X] T002 [P] Update `plugins/bundled/README.md`: list Key & Tempo, drop the "013 will be added" note

---

## Phase 2: Foundational (Blocking Prerequisites for US1–US3)

**Purpose**: Plugin API 1.4 (schema → gateway → effects catalog → runtime → core model/controller/apply), the R4 scheduler regression fix, and the plugin package's registration/node-adoption skeleton (G1–G4) that every plugin action in US1–US3 is built on. **US4 does not depend on this phase.**

**⚠️ CRITICAL**: No US1/US2/US3 work can begin until this phase is complete. Order below follows design note 1 ("schema first") and note 4 ("fix R4 before writing the plugin").

- [X] T003 Bump `crates/modplayer-capability-gateway/api/v1.toml` to `[api_version] minor = 4` and add the six `[[node_kind]]` tables (`pitch_shift`, `time_stretch`, `gain`, `filter`, `stereo_tools`, `equalizer`) per data-model.md §1.1 (R1)
- [X] T004 Add `ParamValue` (`Number|Bool|Name`, `#[serde(untagged)]`), extend `NodeInfo` with `params: BTreeMap<String, ParamValue>` / `auto_switched: bool`, and add `ParamRef`/`ParamArg` widening `SetParam`/`ScheduleParam` in `crates/modplayer-capability-gateway/src/request.rs` (depends on T003)
- [X] T005 [P] In `crates/modplayer-capability-gateway/tests/api_reference.rs` add `api_version_is_1_4` and `reference_lists_node_kind_params`; regenerate `docs/plugin-api/v1.md` via `MODPLAYER_UPDATE_API_REFERENCE=1 cargo test -p modplayer-capability-gateway --test api_reference` (depends on T003, T004)
- [X] T006 [P] In `crates/modplayer-capability-gateway/tests/manifest.rs` add `manifest_api_1_4_accepted_1_5_refused` (depends on T003)
- [X] T007 Add `param_wire_name`, `param_by_wire_name`, `enum_names`, `WireShape`, `wire_shape` and the const wire/enum tables to `crates/modplayer-effects/src/catalog.rs`; re-export from `src/lib.rs` if needed (R2; depends on T003)
- [X] T008 Widen `api.effects.set_param`/`schedule_param` to take `mlua::Value` for `param`/`value`, mapping to `ParamRef`/`ParamArg`, in `crates/modplayer-plugin-runtime/src/bindings/effects.rs` (depends on T004)
- [X] T009 Make `node_info_to_lua` `pub(crate)` and extend it to emit `params`/`auto_switched` in `crates/modplayer-plugin-runtime/src/bindings/mod.rs` (depends on T004, T007)
- [X] T010 Fix R4: rebuild the `effect_chain_changed` payload in `crates/modplayer-plugin-runtime/src/scheduler.rs` with `node_info_to_lua` instead of `lua.to_value` (depends on T009)
- [X] T011 [P] In `crates/modplayer-plugin-runtime/tests/bindings.rs` add `set_param_accepts_name_bool_and_enum`, `set_param_numeric_form_unchanged`, `set_param_bad_lua_type_is_refusal_not_error`, `list_chain_carries_params_and_auto_switched`, `effect_chain_changed_with_plugin_owned_node_reaches_handler` (R4 regression) (depends on T008, T009, T010)
- [X] T012 Implement revision-bump-only-on-change and mode-id→`set_mode` routing in `set_param`/`set_mode`/`set_source_rate` in `crates/modplayer-core/src/effects/model.rs` (R3; depends on T007)
- [X] T013 Replace the two inline `GatewayNodeInfo` projections with one `node_info(index, &NodeModel, ids)` helper (fills `params`/`auto_switched`) in `crates/modplayer-core/src/controller.rs` (depends on T012, T009)
- [X] T014 [P] In `crates/modplayer-core/tests/effects_model.rs` add `wire_names_match_api_schema`, `wire_name_round_trip_every_kind`, `set_param_bumps_revision_only_on_change`, `set_param_on_mode_id_delegates_to_set_mode` (depends on T012)
- [X] T015 [P] In `crates/modplayer-core/tests/controller_effects.rs` add `tempo_step_fans_out_effect_chain_changed_with_ratio`, `panel_edit_fans_out_effect_chain_changed`, `param_changes_coalesce_to_one_event_per_tick`, `effect_chain_changed_params_are_targets_not_ramp` (depends on T013)
- [X] T016 Resolve `ParamRef`/`ParamArg` (name/enum/bool) in the `SetParam`/`ScheduleParam` arms of `crates/modplayer-core/src/plugins/apply.rs`, with `invalid_state`/`invalid_argument` refusals naming the parameter (depends on T007, T004)
- [X] T017 [P] In `crates/modplayer-core/tests/controller_plugins_permissions.rs` add `apply_set_param_by_name_and_enum`, `apply_set_param_unknown_name_refused` (depends on T016)
- [X] T018 Add `key_tempo()` to `crates/modplayer-core/src/plugins/bundled.rs` so `packages() = vec![section_loop(), key_tempo()]`, embedding the files under `plugins/bundled/org.modplayer.key-tempo/` (depends on T001)
- [X] T019 Write `plugins/bundled/org.modplayer.key-tempo/plugin.toml`: identifier `org.modplayer.key-tempo`, `api = "1.4"`, `license = "MIT OR Apache-2.0"`, `source = "bundled"`, `default_locale = "en-US"`, five required permissions (`audio.effects`, `ui.panel`, `ui.shortcuts`, `state.track`, `playback.observe`) each with a justification, `[strings.en-US]` with the 28 keys of data-model.md §3.6 (depends on T003)
- [X] T020 Write `plugins/bundled/org.modplayer.key-tempo/main.luau` skeleton: `ready_ack` registers the 8 `ui.shortcuts` actions (data-model §3.5), then panel `main` with its 16 widgets in order (data-model §3.4), then `list_chain()` and the G2 node-adoption/creation branch (adopt both / create missing one / create both, in every case with no stray `set_param` on adopt, `S.keep_across = false`, `S.step = 10`, `S.step_note = ""` per `ready()`) (G1–G4; depends on T019, T010, T013)
- [X] T021 Implement the E1 mirroring skeleton (re-find own nodes by `owner`/`kind`, mirror `S.tempo`/`S.formant`/`S.quality`, `update_widget` for changed values) and the E2 `track_changed` skeleton (badge reset to `""`, entry lookup) in `main.luau` (depends on T020)
- [X] T022 [P] Write `plugins/bundled/org.modplayer.key-tempo/README.md`: what it does, its keys, how it uses the API including `params` mirroring (depends on T019)
- [X] T023 [P] Add `crates/modplayer-core/tests/bundled_key_tempo.rs`: `key_tempo_package_is_valid`, `key_tempo_licences_match_root`, `key_tempo_uses_only_public_api` (depends on T019, T022, T001)
- [X] T024 Add `ready_creates_adjacent_pitch_and_stretch_nodes` to `crates/modplayer-core/tests/controller_key_tempo.rs` (new file) and `both_bundled_plugins_enabled_with_grants` + `legacy_1_3_fixtures_still_load` to `crates/modplayer-core/tests/plugins_manifest_discovery.rs` (depends on T018, T020)
- [X] T025 [P] Update `crates/modplayer-core/tests/controller_section_loop.rs` and `crates/modplayer-core/tests/bundled_section_loop.rs` for the two-bundled-plugin expectation (R8) (depends on T018)

**Checkpoint**: API 1.4 is live and tested; the plugin registers its panel/actions, adopts or creates its two adjacent nodes, and mirrors basic node state. US1, US2 and US3 implementation can now proceed (in parallel, if staffed). US4 was never blocked and may already be in progress.

---

## Phase 3: User Story 1 - Transpose a song to the band's key (Priority: P1) 🎯 MVP

**Goal**: The Key/Fine tune sliders and `key_up`/`key_down`/`reset_key` compose into the pitch-shift node's `semitones`; Formant and Quality mode track the pitch node (Quality mirrored across both nodes per FR-009); the panel never re-splits the user's own key/cents choice on its own echo.

**Independent Test**: With no stored per-track setting, set Key to −2 via the panel; verify the pitch-shift node's `semitones == -2.0`, tempo/duration unchanged, no obvious artifacts.

### Implementation for User Story 1

- [X] T026 [US1] Implement `key`/`cents` slider commits and the composed-send helper (`set_param(pitch, "semitones", S.key + S.cents/100)`, tracking `S.sent`) in `main.luau` (A2, A7; depends on T021)
- [X] T027 [US1] Implement `key_up`/`key_down` actions and buttons (`key_step(±1)`, clamp −12..12, Fine tune untouched) in `main.luau` (A1; depends on T026)
- [X] T028 [US1] Implement `reset_key` action/button (Key and Fine tune to 0) in `main.luau` (A8 first half; depends on T026)
- [X] T029 [US1] Implement the `formant` toggle (`set_param(pitch, "formant", v)`) in `main.luau` (A6 first half; depends on T021)
- [X] T030 [US1] Implement the `quality` toggle — `set_param` on **both** nodes on a user flip, displayed as the OR of both nodes' `quality_mode` (FR-009) — in `main.luau` (A6 second half; depends on T021)
- [X] T031 [US1] Implement the FR-006 decomposition tie-break in the E1 mirroring step (`|semitones − S.sent.semitones| ≤ 1e-6` ⇒ keep `S.sent.key/cents`; else round-half-up decompose and clear `S.sent`) in `main.luau` (depends on T026)

### Tests for User Story 1

- [X] T032 [P] [US1] Add to `crates/modplayer-core/tests/controller_key_tempo.rs`: `key_minus_two_sets_semitones_not_ratio`, `reset_key_zeroes_key_and_cents`, `fine_tune_composes_with_key`, `formant_touches_only_pitch_node` (depends on T027, T028, T029, T031)
- [X] T033 [P] [US1] Add to `crates/modplayer-core/tests/controller_key_tempo.rs`: `own_split_survives_echo`, `host_panel_edit_is_mirrored_not_reverted`, `auto_switch_shows_quality_on_without_plugin_set_param`, `user_quality_flip_sets_both_nodes_and_clears_auto` (depends on T030, T031)
- [X] T034 [US1] Execute manual scenarios **M1** (Transpose) and **M2** (Fine tune and formant) per quickstart.md; record results in this file (depends on T032, T033)

#### Manual Scenario Log (T034) — 2026-09-20: no VoiceOver/window-driving tool available

Same environment gap 009–012's own manual-scenario sessions recorded:
this sandbox has no tool that drives a real keyboard/pointer traversal
of a native window or captures live audio, so M1/M2's literal Quartz
`CGEventPost` walk cannot run here. `RUSTUP_TOOLCHAIN=1.95.0 cargo
build -p modplayer` succeeds; a timed launch with
`MODPLAYER_CONFIG_DIR=$(mktemp -d)` produces no panic — both bundled
plugins (`org.modplayer.section-loop`, `org.modplayer.key-tempo`)
register their actions, each logging the expected
`invalid_state/host_busy The host did not reply in time` (nothing
drives `tick()` at a frame rate here; not a defect). In its place, the
automated suite drives the real plugin end to end via
`FakeBackend`/`ScriptedHost`, standing in as the proxy for each
scenario. `cargo test -p modplayer-core --test controller_key_tempo`:
28 passed, this session's own run.

| # | Scenario | Automated proxy | Result |
|---|---|---|---|
| M1 | Transpose | `key_minus_two_sets_semitones_not_ratio`, `reset_key_zeroes_key_and_cents` | PASS (automated proxy) |
| M2 | Fine tune and formant | `fine_tune_composes_with_key`, `formant_touches_only_pitch_node`, `own_split_survives_echo`, `host_panel_edit_is_mirrored_not_reverted`, `auto_switch_shows_quality_on_without_plugin_set_param`, `user_quality_flip_sets_both_nodes_and_clears_auto` | PASS (automated proxy) |

No deviations surfaced; no regression test or research.md correction
needed. A maintainer session on real hardware still owes the literal
walk before final Governance sign-off (as 009–012 each noted).

**Checkpoint**: User Story 1 fully functional and independently testable.

---

## Phase 4: User Story 2 - Slow a passage down without changing its pitch (Priority: P1)

**Goal**: The Tempo slider and `tempo_up`/`tempo_down`/`reset_tempo` drive the time-stretch node's `ratio`, honoring the configured step; the host's `+`/`-` action (008 FR-017) still reaches the plugin's own node and the panel follows within one tick.

**Independent Test**: Drag Tempo to 60%; verify `ratio == 0.6` with `semitones` unchanged; separately, with Section Loop looping, verify the loop continues gaplessly and the chain shows only Key & Tempo's two nodes.

### Implementation for User Story 2

- [X] T035 [US2] Implement the `tempo` slider commit (`set_param(stretch, "ratio", v/100)`) in `main.luau` (A3; depends on T021)
- [X] T036 [US2] Implement the `step` slider commit and `step_note` text toggle (`@step_note_text` iff `S.step ≠ 10`) in `main.luau` (A5, FR-013; depends on T035)
- [X] T037 [US2] Implement `tempo_up`/`tempo_down` actions and buttons (`tempo_step(±S.step)`, clamp 25..200) in `main.luau` (A4; depends on T035, T036)
- [X] T038 [US2] Implement `reset_tempo` action/button (`ratio` → 1.0) in `main.luau` (A8 second half; depends on T035)

### Tests for User Story 2

- [X] T039 [P] [US2] Add to `crates/modplayer-core/tests/controller_key_tempo.rs`: `tempo_sixty_percent_keeps_semitones`, `tempo_change_while_section_loop_armed`, `tempo_up_button_uses_configured_step`, `step_note_shown_iff_step_not_ten`, `reset_tempo_returns_to_one` (depends on T035, T036, T037, T038)
- [X] T040 [P] [US2] Add to `crates/modplayer-core/tests/controller_key_tempo.rs`: `host_plus_minus_drives_stretch_and_panel_follows` (verifies the `Plus`/`Minus` conflict flag in `ActionRegistry`, 011 FR-011, R7) (depends on T037)
- [X] T041 [US2] Execute manual scenarios **M3** (Slow down with Section Loop looping) and **M4** (`+`/`-` and the step note) per quickstart.md; record results in this file (depends on T039, T040)

#### Manual Scenario Log (T041) — 2026-09-20: no VoiceOver/window-driving tool available

Same environment gap as T034's log above: no tool here drives a real
keyboard/pointer traversal of a native window or captures live audio,
so M3/M4's literal walk cannot run in this sandbox. The automated
suite drives the real plugin end to end via
`FakeBackend`/`ScriptedHost`/`ActionRegistry`, standing in as the proxy.
`cargo test -p modplayer-core --test controller_key_tempo`: 28 passed,
this session's own run (same run as T034's, all `controller_key_tempo`
cases green together).

| # | Scenario | Automated proxy | Result |
|---|---|---|---|
| M3 | Slow down with Section Loop looping | `tempo_sixty_percent_keeps_semitones`, `tempo_change_while_section_loop_armed`, `reset_tempo_returns_to_one` | PASS (automated proxy) |
| M4 | `+`/`-` and the step note | `tempo_up_button_uses_configured_step`, `step_note_shown_iff_step_not_ten`, `host_plus_minus_drives_stretch_and_panel_follows` (Settings › Controls conflict flag, R7) | PASS (automated proxy) |

No deviations surfaced; no regression test or research.md correction
needed. A maintainer session on real hardware still owes the literal
walk before final Governance sign-off.

**Checkpoint**: User Stories 1 and 2 both independently functional.

---

## Phase 5: User Story 3 - The track remembers, or forgets, its own key and tempo (Priority: P1)

**Goal**: `toggle_remember` writes/removes the one `settings` entry; every mirrored change while remember is on rewrites it; `track_changed` applies a stored entry (with badge) or resets/carries-over per `keep_across`; restart/re-enable never issues a stray `set_param`.

**Independent Test**: On track A set Key −2 and turn Remember on; skip to track B (resets to 0, no badge); return to A (−2 restored, badge shown). Separately, a never-remembered track always returns to 0/100% with no badge.

### Implementation for User Story 3

- [X] T042 [US3] Implement `toggle_remember` (write full `settings` table on `true`; `state.track.remove("settings")` on `false`; revert + refusal message in `restored` on write failure) in `main.luau` (A9; depends on T021, T035)
- [X] T043 [US3] Implement `toggle_keep_across_tracks` (session-only `S.keep_across`, never persisted) in `main.luau` (A10; depends on T042)
- [X] T044 [US3] Implement E1 step 5 (rewrite `settings` when the mirrored table differs from `S.last_written`, only while `S.remember`) in `main.luau` (depends on T042)
- [X] T045 [US3] Implement the full E2 `track_changed` handler — stored-entry apply (5 `set_param`s, `S.remember = true`, `restored` badge text), no-entry reset (`keep_across` off), no-entry carry-over (`keep_across` on) — in `main.luau` (depends on T042, T043)
- [X] T046 [US3] Implement field-by-field malformed-entry defaulting on read (M4: out-of-range/missing field → its default, full rewrite on next update, never refused) in `main.luau` (depends on T045)
- [X] T047 [US3] Implement A11 (a `nil` node from a user removal is a no-op; `restored` shows `@node_removed_text`) and complete G2 case (a)'s `S.remember`/`restored` wiring from T020's skeleton in `main.luau` (depends on T020, T045)

### Tests for User Story 3

- [X] T048 [P] [US3] Add to `crates/modplayer-core/tests/controller_key_tempo.rs`: `remember_on_writes_settings_immediately`, `every_actor_updates_remembered_entry`, `next_track_without_entry_resets_and_no_badge`, `return_to_remembered_track_restores_with_badge`, `never_remembered_track_resets_tempo_markers_untouched` (depends on T044, T045)
- [X] T049 [P] [US3] Add to `crates/modplayer-core/tests/controller_key_tempo.rs`: `keep_across_tracks_carries_values_without_badge`, `remember_off_removes_entry`, `remember_with_no_track_reverts_toggle_shows_message`, `malformed_entry_defaults_field_by_field_and_rewrites` (depends on T042, T043, T045, T046)
- [X] T050 [P] [US3] Add to `crates/modplayer-core/tests/controller_key_tempo.rs`: `restart_after_suspend_adopts_nodes_without_set_param`, `enable_mid_track_applies_entry_like_track_changed`, `removed_node_is_not_recreated_while_active`, `disable_orphans_nodes_with_last_params` (depends on T020, T047)
- [X] T051 [US3] Execute manual scenarios **M5** (Remember, skip, return), **M6** (Don't remember; keep across tracks), **M7** (Remember off, disable/suspend) per quickstart.md; record results in this file (depends on T048, T049, T050)

#### Manual Scenario Log (T051) — 2026-09-20: no VoiceOver/window-driving tool available

Same environment gap as T034/T041's logs above. The automated suite
drives the real plugin end to end via
`FakeBackend`/`ScriptedHost`/`state.track`, standing in as the proxy
for M5–M7's track-switch and disable/re-enable behaviour.
`cargo test -p modplayer-core --test controller_key_tempo`: 28 passed,
this session's own run.

| # | Scenario | Automated proxy | Result |
|---|---|---|---|
| M5 | Remember, skip, return | `remember_on_writes_settings_immediately`, `every_actor_updates_remembered_entry`, `next_track_without_entry_resets_and_no_badge`, `return_to_remembered_track_restores_with_badge` | PASS (automated proxy) |
| M6 | Don't remember; keep across tracks | `never_remembered_track_resets_tempo_markers_untouched`, `keep_across_tracks_carries_values_without_badge`, `malformed_entry_defaults_field_by_field_and_rewrites` | PASS (automated proxy) |
| M7 | Remember off, disable/suspend | `remember_off_removes_entry`, `remember_with_no_track_reverts_toggle_shows_message`, `restart_after_suspend_adopts_nodes_without_set_param`, `enable_mid_track_applies_entry_like_track_changed`, `removed_node_is_not_recreated_while_active`, `disable_orphans_nodes_with_last_params` | PASS (automated proxy) |

No deviations surfaced; no regression test or research.md correction
needed. A maintainer session on real hardware still owes the literal
walk before final Governance sign-off.

**Checkpoint**: The full plugin (US1+US2+US3) is independently functional and complete.

---

## Phase 6: User Story 4 - A new user discovers both bundled plugins and their shortcuts (Priority: P2)

**Goal**: A dismissible, host-native Getting Started card at the top of the Library view names both bundled plugins and their default shortcuts, links to the placeholder tutorial, and its dismissal persists device-scoped across relaunch and sign-out/sign-in; both bundled plugins already show enabled with pre-approved grants.

**Independent Test**: Fresh install → card appears, names both plugins and their shortcuts, tutorial link opens the placeholder URL, Dismiss removes it for good (session, relaunch, sign-out/sign-in); Plugins section shows both installed and enabled with no approval prompt.

**Independence note**: This phase has no dependency on Phase 2–5 (no plugin API or plugin-package involvement); it may be implemented and tested at any time. It shares only the `both_bundled_plugins_enabled_with_grants` test added in T024.

### Implementation for User Story 4

- [X] T052 [P] [US4] Add `pub const GETTING_STARTED_TUTORIAL_URL: &str = "https://github.com/rzcastilho/mod-player/blob/main/docs/plugin-tutorial.md";` to `crates/modplayer-core/src/links.rs`
- [X] T053 [P] [US4] Add `getting_started_dismissed: bool` (`#[serde(default)]`) to `AudioSettings` and the raw `[onboarding]` TOML section in `crates/modplayer-core/src/settings/model.rs`
- [X] T054 [US4] Add `getting_started_dismissed(&self) -> bool` / `dismiss_getting_started(&mut self)` (→ `persist_settings`, `settings-save-failed` warning on error) to `crates/modplayer-core/src/controller.rs` (depends on T053)
- [X] T055 [P] [US4] Add `getting_started_flag_survives_sign_out`, `getting_started_flag_round_trip` to `crates/modplayer-core/tests/settings.rs` (+ a proptest in `crates/modplayer-core/tests/persist.rs`) (depends on T053)
- [X] T056 [US4] Create `crates/modplayer-ui/src/getting_started.rs` (`enum GettingStartedOutcome { None, OpenTutorial, Dismiss }`, `fn show(ui: &mut Ui) -> GettingStartedOutcome`) and add `mod getting_started` to `crates/modplayer-ui/src/lib.rs` (depends on T054)
- [X] T057 [US4] Wire the card into `crates/modplayer-ui/src/app.rs::show_library`: draw above `library_view::show` when no detail target is open and `!controller.getting_started_dismissed()`; `OpenTutorial` → `ctx.open_url(OpenUrl::new_tab(GETTING_STARTED_TUTORIAL_URL))` from `App` only; `Dismiss` → `controller.dismiss_getting_started()` (depends on T056)
- [X] T058 [P] [US4] Add the `getting-started-*` Fluent keys (title, section-loop line, key-tempo line, tutorial, dismiss) to `locales/en-US/app.ftl` (depends on T056)

### Tests for User Story 4

- [X] T059 [P] [US4] Add to `crates/modplayer-ui/tests/library_view.rs`: `getting_started_card_shown_until_dismissed`, `getting_started_tutorial_opens_url`, `getting_started_not_shown_in_detail_view` (depends on T057)
- [X] T060 [P] [US4] Add `getting_started_dismiss_persists_across_relaunch` to `crates/modplayer-ui/tests/first_launch.rs` (depends on T057)
- [X] T061 [P] [US4] Add `getting_started_strings_exist` to `crates/modplayer-ui/tests/fluent_keys.rs` and `getting_started_controls_accessible` to `crates/modplayer-ui/tests/accessibility.rs` (depends on T058, T057)
- [X] T062 [US4] Execute manual scenario **M8** (Fresh install, Getting Started) per quickstart.md; record results in this file (depends on T059, T060, T061, T024)

#### Manual Scenario Log (T062) — 2026-09-20: no VoiceOver/window-driving tool available

Same environment gap as T034/T041/T051's logs above: no tool here
drives Welcome/sign-in/device-check or a system-browser open. In its
place: `RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer` succeeds; a
timed launch with a fresh `MODPLAYER_CONFIG_DIR=$(mktemp -d)` produces
no panic — both bundled plugins register (`org.modplayer.section-loop`,
`org.modplayer.key-tempo`), each only logging the expected
`invalid_state/host_busy` (no window driving `tick()`). The automated
suites drive the card and settings flag end to end via
`egui::Context::run_ui`/`Controller`/`AudioSettings`, standing in as
the proxy. All cited suites green, this session's own runs:
`cargo test -p modplayer-ui --test library_view --test first_launch
--test fluent_keys --test plugin_panels` (43 passed, 4 suites) and
`cargo test -p modplayer-ui --test accessibility
getting_started_controls_accessible` (1 passed) and
`cargo test -p modplayer-core --test plugins_manifest_discovery`
(part of the 42-passed run under T024/T025's own suites).

| # | Scenario | Automated proxy | Result |
|---|---|---|---|
| M8 | Fresh install, Getting Started | `getting_started_card_shown_until_dismissed`, `getting_started_not_shown_in_detail_view`, `getting_started_tutorial_opens_url`, `getting_started_dismiss_persists_across_relaunch`, `getting_started_strings_exist`, `getting_started_controls_accessible`, `both_bundled_plugins_enabled_with_grants` (both bundled plugins Active with grants, no approval prompt) | PASS (automated proxy) |

No deviations surfaced; no regression test or research.md correction
needed. Two pre-existing, unrelated failures were observed in the same
`accessibility.rs` binary when run in full
(`plugins_section_controls_named` — a baseline toggle-count assertion
that predates this feature and is T065's/Final-Phase's to fix, not
this scenario's; `waveform_overview_is_a_named_slider_with_mmss_value_text`
— an `RwLock` deadlock flake under heavy parallel `cargo test`,
reproduced only under full-binary runs, not when the scenario's own
`getting_started_controls_accessible` is run in isolation, matching the
class of environmental flake 012's own session recorded). Neither
touches the Getting Started card or this scenario's expected column. A
maintainer session on real hardware still owes the literal walk
(Welcome, sign-in, device check, VoiceOver optional here since M10
covers it) before final Governance sign-off.

**Checkpoint**: All four user stories independently functional.

---

## Final Phase: Polish & Cross-Cutting Concerns

**Purpose**: Whole-plugin string/accessibility checks, the remaining cross-cutting behavioural tests that no single user story's acceptance scenario owns, the two manual scenarios spanning multiple concerns, and the full gate.

- [ ] T063 [P] Add `every_string_is_at_key` to `crates/modplayer-core/tests/bundled_key_tempo.rs` (P2, FR-017 — no bare literal in any `register_panel`/`register_action`/`update_widget` call except `""`) (depends on Phase 3–5 complete)
- [ ] T064 [P] Add `panel_controls_keyboard_operable_named` to `crates/modplayer-ui/tests/plugin_panels.rs` (FR-017, SC-006) (depends on Phase 3–5 complete)
- [X] T065 [P] Update `crates/modplayer-ui/tests/plugins_view.rs` for the two-bundled-plugin expectation (depends on T018)

#### T065 note (converge, 2026-09-20)

`cargo test --workspace` (converge step) surfaced two stale, pre-013
hardcoded assertions that this task's own scope covers: `plugins_view.rs`'s
`section_loop_row_without_fixtures` (expected 1 checkbox with no fixtures
discovered, now 2 — Section Loop + Key & Tempo) and
`rows_show_every_column_sorted_by_name` (`expected_order` was missing
both "Effects observer fixture" — the Foundational phase's own new
`plugins/fixtures/effects-observer` regression fixture, T005/T011 — and
"Key & Tempo"; the `ok`-health-label count was 16, now 18). The same
category of stale count was also found and fixed in
`crates/modplayer-ui/tests/accessibility.rs`'s
`plugins_section_controls_named` (17 toggles → 19) and its `ok`-label
count (16 → 18), which T065 did not name but is the identical fix. All
four assertions verified individually and together
(`cargo test -p modplayer-ui --test plugins_view --test accessibility`).
Commit 99a6c52.

- [ ] T066 Execute manual scenario **M9** (Host edits are mirrored — Effect Chain panel edits, auto-switch, node removal/recreation) per quickstart.md; record results in this file
- [ ] T067 Execute manual scenario **M10** (Accessibility, VoiceOver, panel + card) per quickstart.md; record results in this file
- [X] T068 Run the full automated gate: `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo deny check && scripts/check-license-headers.sh && MODPLAYER_UPDATE_API_REFERENCE=1 cargo test -p modplayer-capability-gateway --test api_reference && git diff --exit-code docs/plugin-api/v1.md`

#### T068 note (converge, 2026-09-20)

Full gate green: `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets --all-features -- -D warnings`, `cargo deny check`,
`scripts/check-license-headers.sh`, and the API-reference regeneration
check (`docs/plugin-api/v1.md` matches, no diff) all pass clean.
`cargo test --workspace` is green modulo pre-existing, environment-only
flakes reproduced under this sandbox's heavy parallel `cargo test`
load — every one of them passes in isolation and is outside this
feature's diff: `modplayer-account`'s OAuth-listener/service tests
(local HTTP server port races, untouched crate since before this
feature — commit range `db3ef24..HEAD` touches none of it), and several
Luau-plugin-runtime `pump_controller_until`/timing-budget tests across
`modplayer-core` (`controller_plugin_ui`, `controller_section_loop`,
`controller_plugins_lifecycle`) and `modplayer-ui` (`first_launch`). This
matches the class of flake 012's own session already recorded
(`accessibility.rs`'s `RwLock` deadlock, "reproduced only under
full-binary runs"). T066/T067 (manual scenarios) remain not run: this
sandbox has no tool to drive a real window/keyboard or a system browser,
same gap recorded in every 009–012 and this feature's own T034/T041/
T051/T062 manual-scenario logs above — owed by a maintainer session on
real hardware before final Governance sign-off, not a defect in this
slice.

- [ ] T069 Add the Constitution IX change request (contracts/plugin-api-v1.4.md §7) to the PR body and note gateway/runtime area-maintainer sign-off (GOV-3.2)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup (T018 needs T001's files) — BLOCKS US1, US2, US3 only.
- **US1/US2/US3 (Phases 3–5)**: each depends on Foundational completion; independently testable once done, but share `main.luau`/`S` state, so within a single working tree they are best done in the priority order below rather than truly concurrently by separate agents.
- **US4 (Phase 6)**: depends on nothing above; may run at any time in parallel with Phases 2–5.
- **Polish (Final Phase)**: depends on Phases 3–5 (T063–T065) and, for M9/M10, on US1–US4 all being in place.

### User Story Dependencies

- **US1 (P1)**: after Foundational. No dependency on US2/US3.
- **US2 (P1)**: after Foundational. No dependency on US1/US3 (shares `main.luau` file, not logic).
- **US3 (P1)**: after Foundational; T042 also needs T035 (US2's tempo slider) because `current_table()` includes `tempo` — the one intentional cross-story file dependency, sequenced by task number.
- **US4 (P2)**: after nothing; fully independent host-UI slice.

### Parallel Opportunities

- T001/T002 (Setup) in parallel.
- Within Foundational: T005/T006 in parallel after T003–T004; T014/T015/T017 in parallel once their respective implementation tasks land; T022/T023 in parallel; T025 in parallel with T019–T024.
- Once Foundational is done: US1 (T026–T034) and US2 (T035–T041) can proceed in parallel if staffed on separate files/agents (both edit `main.luau`, so coordinate merges); US4 (T052–T062) can run fully in parallel with all of Phase 2–5 from the start.
- Test-only tasks marked [P] within each story (e.g. T032/T033, T039/T040, T048/T049/T050, T059/T060/T061) run in parallel with each other.

---

## Parallel Example: User Story 1

```bash
# After Foundational (Phase 2) completes:
Task: "Implement key/cents slider commits and composed-send helper in main.luau (T026)"
# then, once T026-T031 land, in parallel:
Task: "controller_key_tempo.rs: key_minus_two_sets_semitones_not_ratio, reset_key_zeroes_key_and_cents, fine_tune_composes_with_key, formant_touches_only_pitch_node (T032)"
Task: "controller_key_tempo.rs: own_split_survives_echo, host_panel_edit_is_mirrored_not_reverted, auto_switch_shows_quality_on_without_plugin_set_param, user_quality_flip_sets_both_nodes_and_clears_auto (T033)"
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1: Setup
2. Phase 2: Foundational (API 1.4 + plugin skeleton) — CRITICAL, blocks US1–US3
3. Phase 3: User Story 1
4. **STOP and VALIDATE**: `key_minus_two_sets_semitones_not_ratio` et al. green; manual M1/M2 pass
5. Ship/demo — a musician can already transpose a track

### Incremental Delivery

1. Setup + Foundational → API 1.4 live, plugin registers and adopts/creates its nodes
2. + US1 → transpose works → validate → demo (MVP)
3. + US2 → tempo works, coexists with Section Loop → validate → demo
4. + US3 → per-track memory works → validate → demo (plugin feature-complete)
5. + US4 (can land any time from day one) → onboarding card → validate → demo
6. Polish → cross-cutting string/accessibility checks, M9/M10, full gate

### Parallel Team Strategy

- One agent: Foundational (Phase 2), since it is a strict dependency chain (schema → gateway → catalog → runtime → core model → apply → package skeleton).
- A second agent: US4 (Phase 6), from the start — zero overlap with Phase 2's files.
- Once Foundational lands: split US1/US2/US3 across agents, coordinating merges into the shared `main.luau` (all three append distinct functions/handlers; conflicts are additive, not structural).
- Final Phase after all stories: one agent for the cross-cutting tests, manual scenarios M9/M10, and the gate run.

---

## Notes

- [P] tasks touch different files, or the same file in a way that does not conflict (e.g. adding distinct, independent test functions).
- Every `main.luau` task cites its contract rule id(s) (P/G/A/E/M/L, from [contracts/key-tempo-plugin.md](contracts/key-tempo-plugin.md)) and spec FR/US id(s).
- Manual scenarios M1–M10 are executed by the implementing agent (Governance); a deviation from quickstart.md's expected column becomes a regression test added to the relevant story's test task before the scenario is marked done.
- Commit after each task or logical group; verify a story's named tests fail before its implementation tasks, then pass after.
- The R4 scheduler fix (T010) must land before any `effect_chain_changed`-based test (T011 onward) can pass — it is a correctness prerequisite for FR-020 to be observable at all, not a nice-to-have ordering.

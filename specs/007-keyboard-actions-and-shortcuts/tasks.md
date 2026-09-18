# Tasks: Named Actions and Keyboard Shortcuts

**Input**: Design documents from `/specs/007-keyboard-actions-and-shortcuts/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Required, not optional. FR-017 mandates an automated regression suite over every inherited binding, FR-009/FR-010 mandate property-based conflict tests, Constitution VIII mandates test-first for public behaviour and property tests for state serialisation. Every test task below is named directly from [quickstart.md](quickstart.md)'s pinning table and the contracts' own "Tests pinning this contract" sections — no test may be silently dropped.

**Organization**: Phase 2 (Foundational) builds the toolkit-agnostic `modplayer_core::actions` service and the settings/controller delta — no egui dependency, testable in isolation, and required by every user story. Phase 3 (US1) builds the `modplayer-ui` dispatcher/invoker that makes the catalog keyboard-reachable; because the dispatcher runs once per frame over the *whole* catalog (plan.md Design Note 1), its 44-arm `invoke()` and the removal of 004/006's ad-hoc handlers land here, tagged US1, even though the regression suite that rides along with it also proves markers/loop/cues/nav are unchanged. Phases 4–6 (US2–US4) are additive UI/persistence-proof work on top of an already-complete registry.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1 (P1, MVP) / US2 (P2) / US3 (P3) / US4 (P4); Setup/Foundational/Polish tasks carry no story label
- Every task names an exact file path

## Path Conventions

Existing 10-crate Cargo workspace (`crates/modplayer-core`, `crates/modplayer-ui`, …); no new crate (plan.md Structure Decision). Locale files under `locales/en-US/`.

---

## Phase 1: Setup

**Purpose**: scaffold the new module/file skeletons so Foundational work has somewhere to land. No new dependency, no new crate (research R15).

- [X] T001 Create `crates/modplayer-core/src/actions/{mod.rs,catalog.rs,chord.rs,keymap.rs,registry.rs}` as empty modules with `// SPDX-License-Identifier: MIT OR Apache-2.0` headers and `#![forbid(unsafe_code)]`-compatible stubs
- [X] T002 [P] Add `pub mod actions;` to `crates/modplayer-core/src/lib.rs`
- [X] T003 [P] Add `pub mod actions;` to `crates/modplayer-ui/src/lib.rs` and create empty `crates/modplayer-ui/src/actions.rs`
- [X] T004 [P] Create empty `crates/modplayer-core/tests/actions.rs` and `crates/modplayer-core/tests/controller_actions.rs`
- [X] T005 [P] Create empty `crates/modplayer-ui/tests/actions.rs` and `crates/modplayer-ui/tests/controls.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the toolkit-agnostic Action & Binding service (`modplayer_core::actions`) plus the settings/controller delta it needs to persist and seed itself. Nothing here touches egui or the real-time path. **No user story can be honestly demoed until this phase's tests are green.**

### Core value types

- [X] T006 [P] Define `ActionCategory`, `ActionKind`, `ActionOwner`, `Scope`, `ScopeState`, `ChordParseError`, `BindingError` in `crates/modplayer-core/src/actions/mod.rs` (data-model §1.2, §1.4; contracts/action-registry.md §1)
- [X] T007 [P] Implement `KeyName` + `KEY_NAMES` (egui 0.36 `Key::name()` vocabulary, closed table) and `Mods { primary, shift, alt }` in `crates/modplayer-core/src/actions/chord.rs` (research R2; contracts/action-registry.md §2)
- [X] T008 Implement `Chord::{new, parse, encode, display}` + `Platform` in `crates/modplayer-core/src/actions/chord.rs`, grammar `[Primary+][Shift+][Alt+]<KeyName>` (research R2/R14; data-model §1.3) — depends on T007
- [X] T009 Define `HostAction` enum (44 variants, catalog order) with `ALL`, `id()`, `parse()`, `label_key()`, `category()` in `crates/modplayer-core/src/actions/catalog.rs` (data-model §1.1) — depends on T006
- [X] T010 Build `ActionDef` struct + `pub const CATALOG: [ActionDef; 44]` + `def()` encoding the spec's § Default Action Catalog table verbatim (ids, categories, default bindings, scope, `repeats_while_held`, `enabled_by_default`) in `crates/modplayer-core/src/actions/catalog.rs` (data-model §1.2; FR-001–FR-004, FR-004a, FR-018, FR-019) — depends on T008, T009
- [X] T011 Implement `KeymapOverrides` (`get`/`set`/`remove`/`iter`/`is_empty`, sparse diff-against-default with dedup) in `crates/modplayer-core/src/actions/keymap.rs` (data-model §2; contracts/action-registry.md §3) — depends on T010
- [X] T012 Implement `ActionRegistry` (eager chord→action index over enabled actions, conflict set, `resolve`, `rows`, `add_binding`/`remove_binding`/`reset`/`reset_all`/`set_enabled`, `is_conflicting`/`conflict_partner`) per rules G1–G9 in `crates/modplayer-core/src/actions/registry.rs` (data-model §3; contracts/action-registry.md §4) — depends on T011
- [X] T013 Re-export `HostAction`, `ActionDef`, `CATALOG`, `def`, `Chord`, `Mods`, `KeyName`, `Platform`, `KeymapOverrides`, `ActionRegistry`, `ActionRow`, `Scope`, `ScopeState`, `BindingError`, `ChordParseError` from `crates/modplayer-core/src/lib.rs` — depends on T012

### Core tests (Constitution VIII; quickstart FR-001–FR-004, FR-008–FR-012)

- [X] T014 `catalog_has_44_unique_ids_in_spec_order`, `ids_round_trip_through_parse`, `no_continuous_actions_in_this_slice`, `defaults_match_spec_table`, `shipped_defaults_never_conflict` in `crates/modplayer-core/tests/actions.rs` — depends on T013
- [X] T015 `chord_parse_encode_round_trip` (proptest), `chord_parse_rejects_bad_grammar`, `chord_display_mac_and_other` in `crates/modplayer-core/tests/actions.rs` — depends on T013
- [X] T016 `conflict_is_symmetric` (proptest), `resolve_returns_none_for_conflicting_chord`, `disabled_action_never_conflicts_or_blocks`, `enabling_action_flags_existing_collision`, `can_coexist_table_is_symmetric_and_all_true_for_nested_chain`, `disjoint_scopes_never_conflict` in `crates/modplayer-core/tests/actions.rs` — depends on T013
- [X] T017 `reset_restores_defaults_without_touching_enabled`, `reset_all_clears_every_conflict`, `disabled_action_never_resolves`, `add_duplicate_binding_is_rejected`, `remove_last_binding_leaves_action_unbound_and_resolvable_by_nothing` in `crates/modplayer-core/tests/actions.rs` — depends on T013
- [X] T018 `overrides_are_sparse_relative_to_defaults` (proptest) and `key_name_table_matches_egui_key_all` bijection note (core-side table only; the egui-side half of the bijection is T054) in `crates/modplayer-core/tests/actions.rs` — depends on T013

### Settings persistence delta

- [X] T019 Add `keybindings: BTreeMap<String, toml::Value>` (`#[serde(default)]`) to `RawSettings` and `keybinding_overrides: KeymapOverrides` (default empty) to `AudioSettings` in `crates/modplayer-core/src/settings/model.rs` (contracts/keymap-settings.md §Types) — depends on T012
- [X] T020 Implement `RawSettings::from_settings`/`into_settings` keybindings encode/decode: per-entry drop for unknown action id, non-array value, or a chord string failing `Chord::parse`, dedup duplicate chords, collect dropped ids in `crates/modplayer-core/src/settings/model.rs` — depends on T019
- [X] T021 Change `LoadOutcome.warning: Option<SettingsWarning>` to `warnings: Vec<SettingsWarning>`; add `SettingsWarning::InvalidKeybindings(Vec<String>)` + `message_key()` in `crates/modplayer-core/src/settings/store.rs` — depends on T020
- [X] T022 [P] Add `KEY_KEYBINDINGS_INVALID_ENTRIES = "keybindings-invalid-entries"` in `crates/modplayer-core/src/notifications.rs`
- [X] T023 Update every existing `outcome.warning == None`/`Some(_)` assertion to `outcome.warnings.is_empty()`/non-empty across `crates/modplayer-core/tests/settings.rs` and `PlaybackController::new` callers — depends on T021

### Settings tests (FR-013, SC-004, SC-008)

- [X] T024 `keybindings_table_absent_loads_defaults_silently`, `keybindings_round_trip_is_sparse` in `crates/modplayer-core/tests/settings.rs` — depends on T023
- [X] T025 `keybindings_invalid_entries_dropped_in_isolation`, `keybindings_bad_entries_rewritten_clean_on_next_save`, `whole_file_garbage_still_single_unreadable_warning` in `crates/modplayer-core/tests/settings.rs` — depends on T023
- [X] T026 `crash_mid_write_keeps_previous_keybindings` (extends the existing crash-mid-write test with a `[keybindings]` table) in `crates/modplayer-core/tests/settings.rs` — depends on T023
- [X] T027 `overrides_round_trip_through_raw_settings` (proptest over random `KeymapOverrides` through `RawSettings`) in `crates/modplayer-core/tests/actions.rs` — depends on T020

### Controller façade

- [X] T028 Add `actions: ActionRegistry` shadow state to `PlaybackController`, seeded at `new()` from `settings_store.load()` and raising every `LoadOutcome.warnings` entry, in `crates/modplayer-core/src/controller.rs` (data-model §3.1) — depends on T012, T023
- [X] T029 Implement `actions()`, `add_binding`, `remove_binding`, `reset_binding`, `reset_all_bindings` (each persisting via the existing `persist_settings`) and `set_action_enabled` (not persisted) in `crates/modplayer-core/src/controller.rs` — depends on T028
- [X] T030 Implement `seek_step(direction: i8)` and `step_master_volume(direction: i8)` with `SEEK_STEP = Duration::from_secs(5)` / `VOLUME_STEP: u8 = 5`, reusing `seek()`/`set_master_volume()` (FR-004a; research R9) in `crates/modplayer-core/src/controller.rs` — depends on T028
- [X] T031 `add_binding_persists_and_applies_in_same_call`, `seek_step_moves_five_seconds_and_clamps`, `volume_step_moves_five_percent_and_saturates` in `crates/modplayer-core/tests/controller_actions.rs` — depends on T029, T030

**Checkpoint**: `cargo test -p modplayer-core --test actions --test controller_actions --test settings` green. `modplayer_core::actions` is a complete, egui-free Action & Binding service; `PlaybackController` owns and persists it. No UI crate file has changed yet.

---

## Phase 3: User Story 1 - Every transport control has a keyboard shortcut (Priority: P1) 🎯 MVP

**Goal**: a musician drives play/pause/stop/next/previous/seek/volume from the keyboard, from any view, with zero mouse input; every 001/004/006 binding keeps behaving exactly as before (sole exception: `/` in a focused text field).

**Independent Test**: while a track plays, press each transport shortcut in turn from Now Playing and again from Library; verify each produces the same effect as its equivalent button.

Because the dispatcher consumes matching key events for the *entire* catalog once per frame (plan.md Design Note 1 — a dispatcher after widgets could not detect a widget's non-consuming read), `invoke()` must be exhaustive over all 44 actions and every 004/006 ad-hoc handler must be deleted together, not incrementally. That work is scoped to US1 because it is what turns "every transport operation" into a shipped, demoable keyboard path; the regression tests below prove nothing outside transport moved.

### Dispatcher/invoker (`modplayer-ui`)

- [X] T032 [US1] Implement `FocusClaims`, `Claim { TextLike, Keys }`, `TOOLKIT_DEFAULT_CLAIMS`, and the per-widget `ChordPattern` claim consts (`VOLUME_SLIDER_CLAIMS`, `WAVEFORM_CLAIMS`, `MARKER_CLAIMS`, `ROW_CLAIMS`) in `crates/modplayer-ui/src/actions.rs` (data-model §4.1; contracts/ui-actions.md §2)
- [X] T033 [US1] Implement the two-pass event→chord matcher (logical chord with layout-consumed Shift dropped, then physical chord with full modifiers) and `dispatch()` per the FR-019 precedence table in `crates/modplayer-ui/src/actions.rs` (research R3/R4; contracts/ui-actions.md §2) — depends on T032
- [X] T034 [US1] Implement `invoke()` covering all 44 `HostAction` arms per the owner-mapping table (contracts/ui-actions.md §3) in `crates/modplayer-ui/src/actions.rs` — depends on T033, T029, T030
- [X] T035 [US1] Wire `App::ui`: compute `ScopeState` from `launch_step`/`device_check`/`shell.section`/`waveform.focused_marker`, call `dispatch`/`invoke` before any widget draws, then `claims.clear()`, gated to `LaunchStep::Main` with no Device Check overlay (research R5; contracts/ui-actions.md §1) in `crates/modplayer-ui/src/app.rs` — depends on T034
- [X] T036 [US1] Remove `Shell::handle_shortcuts`; re-point its unit tests at the dispatcher in `crates/modplayer-ui/src/shell.rs` — depends on T035
- [X] T037 [US1] Remove `now_playing::handle_marker_shortcuts` and the `Cmd+Q` branch; add `toggle_queue_panel(ctx)` helper; register `DragValue` `TextLike` claims in `crates/modplayer-ui/src/now_playing.rs` — depends on T035
- [X] T038 [US1] Remove the four nudge-arrow arms from `waveform::focused_marker_key`; register `WAVEFORM_CLAIMS` in `crates/modplayer-ui/src/waveform/input.rs` — depends on T035
- [X] T039 [US1] Replace `WaveformState::text_field_ids` with `FocusClaims` `TextLike` registrations in `crates/modplayer-ui/src/waveform/state.rs` — depends on T038
- [X] T040 [P] [US1] Register `MARKER_CLAIMS` + `EventFilter { horizontal_arrows: true }` on marker glyph/row focus in `crates/modplayer-ui/src/markers.rs` — depends on T035
- [X] T041 [P] [US1] Register `VOLUME_SLIDER_CLAIMS` in `crates/modplayer-ui/src/widgets/volume.rs` — depends on T035
- [X] T042 [P] [US1] Register `ROW_CLAIMS` in `crates/modplayer-ui/src/rows.rs` — depends on T035
- [X] T043 [P] [US1] Register the search-box `TextLike` claim in `crates/modplayer-ui/src/search_view.rs` — depends on T035
- [X] T044 [P] [US1] Register the settings search-box `TextLike` claim and remove the dead `Ctrl+F` handler (unreachable since 004) in `crates/modplayer-ui/src/settings/mod.rs` — depends on T035

### Tests (FR-005, FR-017–FR-019; SC-001, SC-002, SC-007, SC-009–SC-011)

- [X] T045 [US1] `inherited_bindings_produce_identical_controller_calls` — table over `Primary+1..5`, `Primary+F`, `Slash`, `I`/`O`/`L`/`M`, `1`–`8`, `Shift+1`–`Shift+8` (both the logical-fallback and physical-fallback event shapes), marker-focus `Left`/`Right`/`Shift+Left`/`Shift+Right` — in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T046 [US1] `slash_in_focused_text_field_types_and_does_not_focus_search` (the sole FR-017 deviation) in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T047 [US1] Re-point at the dispatcher: `markers.rs::i_then_o_creates_region_at_playhead_positions`, `::shift_digit_sets_cue_and_digit_jumps_keeping_state`, `::glyph_focus_arrow_nudges_by_setting_and_shift_ten_x`; `now_playing.rs` key tests; `shell.rs` shortcut tests — depends on T044
- [X] T048 [P] [US1] `transport_shortcuts_from_now_playing_and_library` (every transport chord, from both sections) in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T049 [P] [US1] `i_o_l_arms_loop_with_no_pointer` in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T050 [P] [US1] `focused_stop_button_wins_space_exactly_one_effect`, `focused_waveform_lets_space_toggle`, `focused_volume_slider_keeps_arrows` in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T051 [P] [US1] `held_space_toggles_once_held_volume_repeats` in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T052 [P] [US1] `q_toggles_queue_panel_in_now_playing_only` in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T053 [P] [US1] `i_outside_now_playing_falls_through`, `nudge_requires_focused_marker` in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T054 [P] [US1] `invocation_lands_in_the_same_frame_as_the_key_event` and `key_name_table_matches_egui_key_all` (egui-side half; pins the bijection against `egui::Key::ALL`) in `crates/modplayer-ui/tests/actions.rs` — depends on T044

**Checkpoint**: `cargo test -p modplayer-ui --test actions --test markers --test now_playing` and `shell.rs` unit tests green. Play a track, exercise every transport shortcut plus `I`/`O`/`L`/`Q` from Now Playing and Library with no mouse — MVP demoable.

---

## Phase 4: User Story 2 - View and rebind the entire shortcut map (Priority: P2)

**Goal**: Settings › Controls lists every action grouped by category with its bindings; the user can filter, add, remove, and reset bindings without a config file.

**Independent Test**: open Settings › Controls, filter to an action, add a second binding, remove one, reset a different action, verify each change reflects immediately.

- [X] T055 [US2] Add `locales/en-US/controls.ftl`: 44 `action-*` labels, 6 `action-cat-*` labels, page/capture/conflict strings (`controls-heading`, `-filter`, `-no-match`, `-kind-trigger`, `-kind-continuous`, `-inactive`, `-binding-chip`, `-remove-binding`, `-add-binding`, `-capture`, `-capture-prompt`, `-reset-action`, `-reset-all`, `-reset-all-confirm`, `-confirm`, `-cancel`, `-conflict-with`, `-reject-tab`, `-reject-mac-control`, `-reject-modifier-only`, `-reject-duplicate`) and `keybindings-invalid-entries` (contracts/ui-actions.md §6)
- [X] T056 [P] [US2] Add `setting-keybindings`/`setting-keybindings-desc` to `locales/en-US/settings.ftl`
- [X] T057 [US2] Implement `ControlsScreen { filter, capture, capture_error, reset_all_confirm }` and its transition table (data-model §4.3) in `crates/modplayer-ui/src/settings/controls.rs` — depends on T055
- [X] T058 [US2] Implement the filter (case-insensitive substring over `tr(label_key)` and `tr(category.label_key())`, category headers only when a row matches, `controls-no-match` line) and per-row layout (label, kind, `(inactive)` suffix, chips, Add binding, Reset to default) in `crates/modplayer-ui/src/settings/controls.rs` — depends on T057
- [X] T059 [US2] Implement `CaptureRule::check` (`LoneModifier`/`TabReserved`/`MacControl`/`Duplicate`) and the focus-locked capture control (`EventFilter { tab, horizontal_arrows, vertical_arrows, escape: true }`) in `crates/modplayer-ui/src/settings/controls.rs` (research R11; contracts/ui-actions.md §5) — depends on T058
- [X] T060 [US2] Implement chip remove (`controller.remove_binding`, immediate), per-action "Reset to default" (immediate, no confirm), page-level "Reset all to defaults" two-step inline confirm (`Esc`/focus-loss cancels) in `crates/modplayer-ui/src/settings/controls.rs` — depends on T059
- [X] T061 [US2] Wire the Controls settings category to `controls::show`; add the `controls.keybindings` `SettingDescriptor` (title `setting-keybindings`, desc `setting-keybindings-desc`) in `crates/modplayer-ui/src/settings/mod.rs` and `crates/modplayer-core/src/settings_registry.rs` — depends on T060, T056
- [X] T062 [P] [US2] `filter_matches_label_and_category_case_insensitively`, `filter_no_match_shows_line`, `every_action_listed_grouped_by_category` in `crates/modplayer-ui/tests/controls.rs` — depends on T061
- [X] T063 [P] [US2] `capture_accepts_chord_and_adds_chip`, `capture_accepts_space_enter_arrows_function_keys`, `capture_rejects_duplicate_tab_and_mac_control_inline`, `capture_esc_and_focus_loss_cancel` in `crates/modplayer-ui/tests/controls.rs` — depends on T061
- [X] T064 [P] [US2] `remove_chip_immediately_allows_zero_bindings`, `reset_action_restores_default_without_confirm`, `reset_all_two_step_confirm_cancel_esc_focus_loss` in `crates/modplayer-ui/tests/controls.rs` — depends on T061
- [X] T065 [P] [US2] `disabled_rows_greyed_rebindable_and_never_fire` in `crates/modplayer-ui/tests/controls.rs` — depends on T061
- [X] T066 [P] [US2] `chips_display_platform_glyphs` in `crates/modplayer-ui/tests/controls.rs` — depends on T061
- [X] T067 [US2] Extend `crates/modplayer-ui/tests/accessibility.rs` enumeration to every Controls control (accessible name, role, state, `Tab` order) — depends on T061
- [X] T068 [US2] Extend `crates/modplayer-ui/tests/fluent_keys.rs` unused-key audit to `controls.ftl` and the `settings.ftl` delta — depends on T055, T056
- [X] T069 [P] [US2] Update `crates/modplayer-core/src/settings_registry.rs`'s `placeholder_only_categories_contribute_no_descriptors` test to drop `Controls` from its list — depends on T061

**Checkpoint**: `cargo test -p modplayer-ui --test controls --test accessibility --test fluent_keys` green. Settings › Controls fully usable standalone.

---

## Phase 5: User Story 3 - Conflicting shortcuts are caught and must be resolved (Priority: P3)

**Goal**: rebinding onto an already-claimed, currently-reachable key flags both rows and silences both until resolved. (The registry's conflict engine already exists from Phase 2 — this story is surfacing it in the UI.)

**Independent Test**: rebind "toggle loop" onto "play/pause toggle"'s key; verify both rows flag and the shared key does neither; resolve by removing one binding; verify both become active again.

- [X] T070 [US3] Show `⚠` + `controls-conflict-with { $other }` on a conflicting chip, sourced from `ActionRegistry::conflict_partner` in `crates/modplayer-ui/src/settings/controls.rs` — depends on T060
- [X] T071 [US3] Surface the conflict-partner name inline the instant capture creates a conflict (US3 flow, FR-007's "accepted and immediately produces the conflict") in `crates/modplayer-ui/src/settings/controls.rs` — depends on T070
- [X] T072 [P] [US3] `conflicting_chord_fires_neither_until_resolved` in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T073 [P] [US3] `conflict_flag_shows_on_both_rows` in `crates/modplayer-ui/tests/controls.rs` — depends on T071
- [X] T074 [P] [US3] `disabled_tempo_binding_does_not_block_and_flags_on_enable` in `crates/modplayer-ui/tests/actions.rs` — depends on T044
- [X] T075 [P] [US3] `capture_of_conflicting_chord_flags_both_and_names_partner` in `crates/modplayer-ui/tests/controls.rs` — depends on T071

**Checkpoint**: `cargo test -p modplayer-ui --test actions --test controls` green with conflict cases. US1–US3 all demoable together.

---

## Phase 6: User Story 4 - Custom bindings survive a restart (Priority: P4)

**Goal**: prove the Phase 2 persistence plumbing end-to-end through the real controller/settings-store round trip, and sign off the two restart-specific manual scenarios.

**Independent Test**: rebind an action, quit and relaunch, open Settings › Controls, verify the custom binding (and only that one) survived.

- [X] T076 [P] [US4] Integration test: rebind via `ControlsScreen`/`controller.add_binding`, drop and reconstruct `PlaybackController` from the same `SettingsStore` path, assert the custom binding and only that customisation survives, in `crates/modplayer-core/tests/controller_actions.rs`
- [X] T077 [P] [US4] Confirm `overrides_round_trip_through_raw_settings` (T027) and `keybindings_round_trip_is_sparse`/`keybindings_table_absent_loads_defaults_silently` (T024) exercise the controller's live path, not only `KeymapOverrides` in isolation; add controller-level coverage if a gap remains, in `crates/modplayer-core/tests/settings.rs`
- [X] T078 [US4] Manual scenario **M9** — restart persistence (fresh `MODPLAYER_CONFIG_DIR`, rebind loop toggle to `K`, quit, relaunch, `cat settings.toml`): record pass/deviation with evidence in this file (Governance › Manual Scenario Sign-Off). **Deviation (environment), attempted 2026-09-18**: built `modplayer` with `RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer`, wrapped it in `target/manual-walk/ModPlayer.app` (the 006 harness recipe) and launched it via `open -n` with a fresh `MODPLAYER_CONFIG_DIR=$(mktemp -d)`. The process started and even reached its first `settings.toml` write, but `Quartz.CGSessionCopyCurrentDictionary()` confirmed `CGSSessionScreenIsLocked: True` for this machine's console session throughout the attempt, and `CGWindowListCopyWindowInfo` never listed a ModPlayer window (only `Window Server Display Shield` + `loginwindow`) — the constitution's own recipe notes "events posted while the screen is locked are dropped," and a shielded session composites no app window to locate or screenshot. No GUI drive was possible; the process was killed and the temp config dir removed, no artefacts left behind. **Substituted automated coverage** (exercises the identical restart path minus the physical keypress): `crates/modplayer-core/tests/controller_actions.rs::custom_binding_survives_controller_restart` (T076) rebinds `ToggleLoop` to `K` via `controller.add_binding`, drops and reconstructs a whole `PlaybackController` from the same `SettingsStore` path, and asserts `K`+`L` bound and every other action still at its catalog default — plus `tests/settings.rs::keybindings_round_trip_is_sparse` (T024), which confirms the on-disk file holds exactly `[keybindings]` / `host.loop.toggle`. Re-attempt this scenario for real GUI evidence once the console session is unlocked.
- [X] T079 [US4] Manual scenario **M10** — invalid entry isolation (inject `"host.nope.x"` and a malformed chord while the app is closed, launch, verify one `keybindings-invalid-entries` warning naming both, `K` still bound, file rewritten clean on next save): record pass/deviation with evidence in this file. **Deviation (environment), attempted 2026-09-18**: same blocker as T078 — `CGSSessionScreenIsLocked: True`, no ModPlayer window ever listed by `CGWindowListCopyWindowInfo`, so the notification banner and Controls screen could not be visually confirmed. **Substituted automated coverage**: `crates/modplayer-core/tests/settings.rs::keybindings_invalid_entries_warning_reaches_controller_notifications` (new, T077) launches a real `PlaybackController::new` over a hand-written `settings.toml` with one good (`host.loop.toggle = ["K"]`) and one bad (`host.nope.x = ["A"]`) entry, and asserts both that `controller.actions().bindings(HostAction::ToggleLoop)` still resolves to `K` *and* that `controller.notifications().all()` carries exactly one `keybindings-invalid-entries` notification with `args == [("ids", "host.nope.x")]` — i.e. the warning reaches the running app's `NotificationCenter`, not only `LoadOutcome`. `tests/settings.rs::keybindings_bad_entries_rewritten_clean_on_next_save` (T025) independently confirms the file is rewritten clean (the bad id dropped) on the next save. Re-attempt this scenario for real GUI evidence once the console session is unlocked.

**Checkpoint**: all four user stories independently demoable; restart-safety proven against the real file, not just in-memory structs.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: CI gates, the remaining twelve manual scenarios, and closing the loop on any deviation the manual walk surfaces.

- [X] T080 [P] Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo deny check`, `scripts/check-license-headers.sh` from the repository root
- [X] T081 [P] Run `cargo test --workspace` and cross-check every named test in [quickstart.md](quickstart.md)'s pinning table is present and green
- [X] T082 Manual scenario **M1** — transport from Now Playing (all ten chords + `Space`/`Space`/`Shift+Space`): record pass/deviation with evidence in this file. **Deviation (environment), attempted 2026-09-18**: same blocker as T078/T079 — re-verified before starting the M1–M14 batch: `Quartz.CGSessionCopyCurrentDictionary()` still reports `CGSSessionScreenIsLocked: True` for this machine's console session; launched `target/manual-walk/ModPlayer.app` (fresh `MODPLAYER_CONFIG_DIR`) and `CGWindowListCopyWindowInfo` again listed no `modplayer`-owned window (only `Window Server Display Shield` + `loginwindow`), confirming the shielded session composites no app window this attempt either; process killed, temp config dir left to the OS. This is a session-level condition, not scenario-specific, so it is checked once here and cited (not re-run) for M2–M8/M11–M14 below. **Substituted automated coverage**: `modplayer-ui tests/actions.rs::transport_shortcuts_from_now_playing_and_library` (all ten chords, both sections), `::held_space_toggles_once_held_volume_repeats` (the `Space`/`Space` pause-then-resume pair), `::inherited_bindings_produce_identical_controller_calls` (`Shift+Space` stop plus the full inherited table); `modplayer-core tests/controller_actions.rs::seek_step_moves_five_seconds_and_clamps`, `::volume_step_moves_five_percent_and_saturates`. Re-attempt for real GUI evidence once the console session is unlocked.
- [X] T083 [P] Manual scenario **M2** — transport from Library (SC-009 parity): record pass/deviation. **Deviation (environment)**: same session-level blocker as M1 (screen locked, no ModPlayer window listable). **Substituted automated coverage**: `modplayer-ui tests/actions.rs::transport_shortcuts_from_now_playing_and_library` (parametrised over both sections), `::i_outside_now_playing_falls_through`. Re-attempt once unlocked.
- [X] T084 [P] Manual scenario **M3** — `I`, `O`, `L` arm the loop with no mouse (prompt acceptance, SC-002): record pass/deviation. **Deviation (environment)**: same blocker. **Substituted automated coverage**: `modplayer-ui tests/actions.rs::i_o_l_arms_loop_with_no_pointer`; re-pointed `tests/markers.rs::i_then_o_creates_region_at_playhead_positions`. Re-attempt once unlocked.
- [X] T085 [P] Manual scenario **M4** — focused "Stop" button wins over `Space` (SC-010): record pass/deviation. **Deviation (environment)**: same blocker. **Substituted automated coverage**: `modplayer-ui tests/actions.rs::focused_stop_button_wins_space_exactly_one_effect`. Re-attempt once unlocked.
- [X] T086 [P] Manual scenario **M5** — repeat rules (held `⌘↑` steps repeatedly, held `Space` toggles once): record pass/deviation. **Deviation (environment)**: same blocker. **Substituted automated coverage**: `modplayer-ui tests/actions.rs::held_space_toggles_once_held_volume_repeats`. Re-attempt once unlocked.
- [X] T087 [P] Manual scenario **M6** — `Q` toggles the queue panel; `⌘Q` still quits macOS: record pass/deviation. **Deviation (environment)**: same blocker (also, driving a real `⌘Q` quit against the actual login session is undesirable even once unlocked without an isolated test account — flagged for the re-attempt). **Substituted automated coverage**: `modplayer-ui tests/actions.rs::q_toggles_queue_panel_in_now_playing_only`. Re-attempt once unlocked.
- [X] T088 [P] Manual scenario **M7** — shortcut map filter + rebind round trip: record pass/deviation. **Deviation (environment)**: same blocker. **Substituted automated coverage**: `modplayer-ui tests/controls.rs::filter_matches_label_and_category_case_insensitively`, `::capture_accepts_chord_and_adds_chip`, `::remove_chip_immediately_allows_zero_bindings`, `::reset_action_restores_default_without_confirm`. Re-attempt once unlocked.
- [X] T089 [P] Manual scenario **M8** — conflict flag/silence/resolve cycle: record pass/deviation. **Deviation (environment)**: same blocker. **Substituted automated coverage**: `modplayer-ui tests/actions.rs::conflicting_chord_fires_neither_until_resolved`; `tests/controls.rs::conflict_flag_shows_on_both_rows`, `::capture_of_conflicting_chord_flags_both_and_names_partner`. Re-attempt once unlocked.
- [X] T090 [P] Manual scenario **M11** — disabled tempo rows greyed, chips shown, rebindable, silent: record pass/deviation. **Deviation (environment)**: same blocker. **Substituted automated coverage**: `modplayer-ui tests/controls.rs::disabled_rows_greyed_rebindable_and_never_fire`; `tests/actions.rs::disabled_tempo_binding_does_not_block_and_flags_on_enable`, `::disabled_action_never_resolves` (core). Re-attempt once unlocked.
- [X] T091 [P] Manual scenario **M12** — capture rejections (`Tab`, physical Control, duplicate, `Esc`): record pass/deviation. **Deviation (environment)**: same blocker. **Substituted automated coverage**: `modplayer-ui tests/controls.rs::capture_rejects_duplicate_tab_and_mac_control_inline`, `::capture_esc_and_focus_loss_cancel`. Re-attempt once unlocked.
- [X] T092 [P] Manual scenario **M13** — text field guard: `/abc` types, `Space` does nothing while the search box has focus: record pass/deviation. **Deviation (environment)**: same blocker. **Substituted automated coverage**: `modplayer-ui tests/actions.rs::slash_in_focused_text_field_types_and_does_not_focus_search`, `::text_field_focus_silences_every_action`. Re-attempt once unlocked.
- [X] T093 [P] Manual scenario **M14** — Reset all to defaults, Cancel then Confirm: record pass/deviation. **Deviation (environment)**: same blocker. **Substituted automated coverage**: `modplayer-ui tests/controls.rs::reset_all_two_step_confirm_cancel_esc_focus_loss`; `modplayer-core tests/actions.rs::reset_all_clears_every_conflict`. Re-attempt once unlocked.
- [X] T094 Update [quickstart.md](quickstart.md) and [research.md](research.md) with any deviation observed while running M1–M14 (Constitution Governance › Manual Scenario Sign-Off) — depends on T078, T079, T082–T093. **No spec-behaviour deviation observed** for any of M1–M14 — every blocker (T078, T079, T082–T093) was the same environmental one (console session's `CGSSessionScreenIsLocked: True` throughout this run, so no ModPlayer window was ever composited for `CGWindowListCopyWindowInfo`/`screencapture` to find), not a difference between actual and spec'd behaviour. Per Governance › Manual Scenario Sign-Off, `quickstart.md`/`research.md` are updated "where behaviour differs from the spec"; since none did (all differences were substituted-automated-coverage citations recorded per-scenario in this file), no edit to either file is warranted this pass. Flag for whoever unlocks the console session next: re-run M1–M14 (recipe in quickstart.md's "Manual scenarios" section) for real GUI evidence; only then amend quickstart.md/research.md if that run surfaces an actual behavioural deviation.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational (Phase 2)**: depends on Setup — **BLOCKS every user story** (registry, chord grammar, settings delta, controller façade are shared by all four)
- **US1 (Phase 3)**: depends on Foundational only. Builds the dispatcher/invoker that every later story's UI reuses.
- **US2 (Phase 4)**: depends on Foundational (registry mutators) and, for a demoable page, on US1's dispatcher existing so a rebound key actually fires — implemented after US1 in this plan, though its own tests do not require US1's transport tests to pass.
- **US3 (Phase 5)**: depends on Foundational (conflict engine, already complete) and US2 (the rows it decorates with `⚠`); T072/T074 (core-dispatcher-level conflict tests) only need US1.
- **US4 (Phase 6)**: depends on Foundational (persistence) and US2 (something to rebind through the UI for the integration test); independent of US1/US3 otherwise.
- **Polish (Phase 7)**: depends on all four stories being complete (the manual scenarios exercise the whole feature).

### User Story Dependencies

- **US1 (P1)**: Foundational → US1. No dependency on US2/US3/US4.
- **US2 (P2)**: Foundational → US1 (dispatcher) → US2.
- **US3 (P3)**: Foundational → US1 → US2 → US3 (needs rows to annotate).
- **US4 (P4)**: Foundational → US2 (needs a rebind UI to drive the integration test) → US4.

### Parallel Opportunities

- Setup: T002–T005 in parallel once T001 exists.
- Foundational: T006/T007 in parallel; then the sequential chord→catalog→keymap→registry chain (T008–T012) is single-threaded by design (each file depends on the previous type). T022 (notifications.rs) is parallel to T019–T021 (settings files). Test tasks T014–T018, T024–T027, T031 can be parallelised across files once their implementation tasks land.
- US1: T040–T044 (five independent widget files) run in parallel once T035 lands; T048–T054 (test functions in the same file) are sequenced for merge safety but have no code dependency on each other beyond T044.
- US2: T062–T066 (test functions) and T069 (a different crate's test) are parallel once T061 lands.
- US3: T072–T075 are parallel once T044 (actions.rs) / T071 (controls.rs) land.
- US4: T076/T077 are parallel; T078/T079 are independent manual runs.
- Polish: T080/T081 and T083–T093 (eleven of the twelve remaining manual scenarios) are parallel; T082 is written up first as the walkthrough baseline, T094 waits on everything.

---

## Parallel Example: Foundational settings delta

```bash
# After T012 (registry) lands, these two can proceed together:
Task: "Add keybindings: BTreeMap<String, toml::Value> to RawSettings and keybinding_overrides to AudioSettings in crates/modplayer-core/src/settings/model.rs"
Task: "Add KEY_KEYBINDINGS_INVALID_ENTRIES notification key in crates/modplayer-core/src/notifications.rs"
```

## Parallel Example: US1 widget claim registrations

```bash
# After T035 (App::ui wiring) lands, five different files, no cross-dependency:
Task: "Register MARKER_CLAIMS + EventFilter in crates/modplayer-ui/src/markers.rs"
Task: "Register VOLUME_SLIDER_CLAIMS in crates/modplayer-ui/src/widgets/volume.rs"
Task: "Register ROW_CLAIMS in crates/modplayer-ui/src/rows.rs"
Task: "Register search-box TextLike claim in crates/modplayer-ui/src/search_view.rs"
Task: "Register settings search-box TextLike claim in crates/modplayer-ui/src/settings/mod.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Setup (T001–T005)
2. Foundational (T006–T031) — **CRITICAL**, blocks everything
3. US1 (T032–T054)
4. **STOP and VALIDATE**: play a track, exercise every transport shortcut from Now Playing and Library with no mouse; run `cargo test -p modplayer-core --test actions --test controller_actions --test settings` and `cargo test -p modplayer-ui --test actions --test markers --test now_playing`
5. Demo: JTBD-5's baseline ("every action on a physical control") is met

### Incremental Delivery

1. Setup + Foundational → core Action & Binding service ready, zero UI change
2. + US1 → transport keyboard-operable app-wide, zero regression (MVP)
3. + US2 → the whole catalog is visible and rebindable
4. + US3 → conflicts are caught, not silently double-bound
5. + US4 → customisation survives restart, proven against the real file
6. + Polish → CI gates green, all 14 manual scenarios signed off

### Notes

- [P] tasks touch different files with no unmet dependency
- Every core (`modplayer-core`) task is testable without an `eframe::CreationContext`; every UI task is testable through the offscreen `egui::Context` pattern 004–006 already use
- Commit after each task or logical group
- Stop at any checkpoint to validate a story independently
- The dispatcher's monolithic nature (one call, whole catalog, per frame) means US1's tests are simultaneously the FR-017 regression gate for markers/loop/cues/navigation — this is intentional, not scope creep (plan.md Design Note 1)
- Manual scenarios M1–M14 (T078, T079, T082–T093) are executed by the implementing agent itself, never handed back to the maintainer (Constitution Governance › Manual Scenario Sign-Off)

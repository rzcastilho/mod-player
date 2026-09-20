---
description: "Task list for Section Loop Bundled Plugin"
---

# Tasks: Section Loop Bundled Plugin

**Input**: Design documents from `/specs/012-section-loop-plugin/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/plugin-api-v1.3.md](contracts/plugin-api-v1.3.md), [contracts/section-loop-plugin.md](contracts/section-loop-plugin.md), [quickstart.md](quickstart.md)

**Tests**: Included — Constitution VIII and the contracts name specific test functions; every named test below cites its contract id.

**Organization**: Phase 2 (Foundational) ships API 1.3 and the bundled package skeleton, since every user story drives the same single-file `main.luau` through the same new host calls. Phases 3–5 map to spec.md's three user stories (US1/US2 P1, US3 P2), each independently testable per its own **Independent Test** in spec.md.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1 / US2 / US3, per spec.md priorities
- File paths are exact, from plan.md § Project Structure

---

## Phase 1: Setup

**Purpose**: Package skeleton and bundled-plugin listing housekeeping.

- [X] T001 Create the package directory `plugins/bundled/org.modplayer.section-loop/` with placeholder `plugin.toml`, `main.luau`, `README.md` (content filled in later phases; convention per `plugins/bundled/README.md`, plan.md § Project Structure)
- [X] T002 [P] Copy `/LICENSE-MIT` and `/LICENSE-APACHE` byte-identically into `plugins/bundled/org.modplayer.section-loop/LICENSE-MIT` and `LICENSE-APACHE` (contract P1)
- [X] T003 [P] Update `plugins/bundled/README.md`: list Section Loop, drop "no production plugin yet" (plan.md § Project Structure)

**Checkpoint**: package folder exists; nothing yet compiles against it.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Plugin API 1.3 (schema → gateway → runtime → core) and the bundled registration that every user story's script logic calls into. **No user story task can pass its tests until this phase is green.**

### Schema first (Constitution IX, research R1)

- [X] T004 Bump `[api_version]` to `minor = 3` and add `[[request]]` entries `SetLoopEndpoint` (`namespace = "markers"`, `method = "set_loop_endpoint"`, `requires = "markers.write"`, `needs_focus = false`, `category = "markers"`) and `SetLoopRepeat` (same gating, `method = "set_loop_repeat"`) in `crates/modplayer-capability-gateway/api/v1.toml` (data-model.md §1.1)

### Gateway DTOs and tests

- [X] T005 Add `LoopEndpoint` enum, `RepeatArg` enum, `RegionInfo` struct, `Request::{SetLoopEndpoint, SetLoopRepeat}`, `Response::{LoopEndpoint, Markers.regions}` in `crates/modplayer-capability-gateway/src/request.rs` (data-model.md §1.2/§1.3); let the exhaustive matches fail the build until T009/T017 fill their arms
- [X] T006 Regenerate `docs/plugin-api/v1.md` by running `cargo test -p modplayer-capability-gateway --test api_reference` and committing its output (contract plugin-api-v1.3.md §1, §7 `reference_is_current`)
- [X] T007 [P] Add gateway tests in `crates/modplayer-capability-gateway/tests/gateway.rs`: `set_loop_endpoint_requires_markers_write`, `set_loop_repeat_requires_markers_write`, `loop_endpoint_calls_never_need_focus` (contract plugin-api-v1.3.md §7)

### Runtime bindings

- [X] T008 Add `regions: Vec<RegionInfo>` to `PluginSnapshot` in `crates/modplayer-plugin-runtime/src/handle.rs` (data-model.md §1.3)
- [X] T009 Add `set_loop_endpoint`/`set_loop_repeat` Lua bindings with pre-RPC argument validation (`which` ∈ {"a","b"}; `repeat` integer `1..=1000` or `"infinite"`) in `crates/modplayer-plugin-runtime/src/bindings/markers.rs` (data-model.md §1.2/§1.4, research R1/R3)
- [X] T010 Wire the two new dispatch arms and extend `response_to_lua`/`ListMarkers` to carry `regions`/`LoopEndpoint` in `crates/modplayer-plugin-runtime/src/bindings/mod.rs` (data-model.md §1.3/§1.4)
- [X] T011 [P] Add runtime tests in `crates/modplayer-plugin-runtime/tests/bindings.rs`: `set_loop_endpoint_which_validated`, `set_loop_repeat_range_exact`, `list_markers_exposes_regions` (contract plugin-api-v1.3.md §7)

### Core host model

- [X] T012 Generalize `TrackMarkers::set_loop_endpoint` into `pub fn set_loop_endpoint_owned(&mut self, region: Option<RegionId>, which_a: bool, pos: u64, owner: Owner) -> Result<(RegionId, MarkerId), MarkerError>` in `crates/modplayer-core/src/markers/model.rs`; make `set_loop_a`/`set_loop_b` delegate to it with `Owner::Host` (research R2, data-model.md §2.1) — doc example required (Constitution VII)
- [X] T013 [P] Add/extend `crates/modplayer-core/tests/markers_model.rs`: `set_loop_endpoint_owned_creates_a_only_region`, `set_loop_endpoint_owned_completes_and_swaps`, `set_loop_endpoint_owned_moves_existing`, `host_set_loop_a_b_unchanged`, `endpoint_owned_proptest` (contract plugin-api-v1.3.md §7, Constitution VIII)
- [X] T014 Add `PlaybackController::plugin_set_loop_endpoint` façade (ms→frames, `recommit_if_armed`, `last_marker_actor`) and extend `publish_plugin_snapshot_if_changed` to map `TrackMarkers::regions()` → `Vec<RegionInfo>` in `crates/modplayer-core/src/controller.rs` (data-model.md §2.2/§2.5, research R4)
- [X] T015 [P] Add/extend `crates/modplayer-core/tests/controller_markers.rs`: `plugin_endpoint_not_owner`, `plugin_endpoint_marker_limit`, `plugin_repeat_recommits_armed_without_wrap_reset`, `snapshot_regions_track_arm_and_repeat` (contract plugin-api-v1.3.md §7)
- [X] T016 Add `SetLoopEndpoint`/`SetLoopRepeat` arms (owner/track checks, refusal mapping) in `crates/modplayer-core/src/plugins/apply.rs` (data-model.md §2.3)
- [X] T017 Extend `resolve_string` to `UpdateWidget { value: WidgetValue::Text }` and `AddOverlays` label text in `crates/modplayer-core/src/plugins/apply.rs` (research R9; needed by every action's Status text and every overlay label in Phases 3–5)

### Bundled registration

- [X] T018 Write the manifest `plugins/bundled/org.modplayer.section-loop/plugin.toml`: identifier, name, description, version, `api = "1.3"`, author, `license = "MIT OR Apache-2.0"`, `source = "bundled"`, `entry = "main.luau"`, `default_locale = "en-US"`, 7 required + 1 optional permission each with a `justification`, and the full `[strings.en-US]` table (~55 keys per data-model.md §3.5) (data-model.md §3.1, contract P2)
- [X] T019 Register `section_loop()` in `crates/modplayer-core/src/plugins/bundled.rs::packages()` (`fixture: false`, `resources: &[]`, `include_str!` of the three package files) (data-model.md §2.6, research R5)
- [X] T020 Update `crates/modplayer-core/tests/plugins_manifest_discovery.rs::fixtures_only_with_env` to expect exactly Section Loop without fixtures, and add a new test that exercises the empty-state case via an explicit empty package list (research R5)
- [X] T021 [P] Update `crates/modplayer-ui/tests/plugins_view.rs::empty_state_without_fixtures` to assert the Section Loop row is present without fixtures, and cover the empty state via an explicit empty host (research R5)
- [X] T022 Run `cargo test --workspace` and fix any 009/010/011 fixture-suite assertion that assumed zero bundled packages or an unchanged focus-holder/plugin-count now that Section Loop's thread always spawns (research R5 "Consequence to handle"; quickstart.md SC-008 second clause) — fixed two: `modplayer-ui/tests/accessibility.rs::plugins_section_controls_named` (16→17 checkboxes, 15→16 `ok` health labels, now counting Section Loop as the always-discovered bundled package) and `modplayer-ui/tests/accessibility.rs::transport_panel_empty_state_exposes_its_accessible_name` + `modplayer-ui/tests/transport_view.rs::empty_state_when_no_eligible_plugin` (Section Loop's `transport.control` grant means it's always an eligible row now, so each test waits for it `Active` then `plugin_disable`s it before asserting the empty state). `cargo test --workspace -- --test-threads=1`: **1540 passed, 11 ignored, 0 failed** (the `button_action_source_ui` flake T060 already documented does not reproduce under `--test-threads=1`). `cargo fmt --all --check` clean.

**Checkpoint**: `cargo build --workspace` and `cargo test --workspace` are green with an inert (unscripted) Section Loop package. User story phases below only add `main.luau` behavior and its tests.

---

## Phase 3: User Story 1 - Drop A and B and drill hands-free (Priority: P1) 🎯 MVP

**Goal**: Panel-driven mark A/B and gapless loop drilling, per spec.md US1.

**Independent Test** (spec.md): with Section Loop enabled and no other plugin holding focus, play a track, use Set A / Set B, arm Loop, verify gapless wrap and correct waveform/marker-list display; toggle Loop off and verify playback continues past B.

### Tests for User Story 1 ⚠️ write first, confirm they fail against the Phase-1/2 skeleton

- [X] T023 [P] [US1] `crates/modplayer-core/tests/controller_section_loop.rs`: harness setup (reuse `controller_plugin_ui.rs`'s `fixture_controller`/`call`/`invoke_plugin_action`/`plugin_panel_interaction`/`wait_active` targeting `org.modplayer.section-loop`), plus `registers_panel_and_23_actions`, `fresh_install_iol_flagged_brackets_active` (contract G1/G2/G3, research R10)
- [X] T024 [P] [US1] `panel_set_a_then_set_b_creates_owned_region` (US1-1), `set_b_before_set_a_then_swap` (Edge Cases) in `controller_section_loop.rs` (contract A1/A2)
- [X] T025 [P] [US1] `loop_toggle_requests_focus_and_arms_same_action` (US1-2, SC-004), `loop_toggle_off_disarms` (US1-3) in `controller_section_loop.rs` (contract A3/A4/A11)
- [X] T026 [P] [US1] `finite_repeat_releases_and_toggle_follows` (US1-4, SC-005), `arm_incomplete_reverts_toggle_with_message` (US1-5) in `controller_section_loop.rs` (contract A3/A9, E4)
- [X] T027 [P] [US1] `nudge_moves_active_marker_10ms` (US1-6), `nudge_without_endpoints_noop`, `nudge_default_b_then_a` in `controller_section_loop.rs` (contract A5, FR-006)
- [X] T028 [P] [US1] `overlay_set_matches_markers` for the A/B/region/label subset in `controller_section_loop.rs` (contract O1/O2, FR-012)
- [X] T029 [US1] `crates/modplayer-ui/tests/plugin_panels.rs::section_loop_panel_keyboard_and_names` (SC-006, AccessKit)

### Implementation for User Story 1

- [X] T030 [US1] `plugins/bundled/org.modplayer.section-loop/main.luau`: define `S` state table (data-model.md §3.2) and the `ready_ack` handler — `register_action` ×23 (data-model.md §3.4) and `register_panel("main", …)` (data-model.md §3.3), then call `relist()` (contract G1/G2/G4)
- [X] T031 [US1] `main.luau`: `relist()` — `markers.list()` → populate `S.region`/`S.a`/`S.b`/`S.a_ms`/`S.b_ms`/`S.cues`/`S.armed`, drop `S.active` if its endpoint is gone, set the `loop` toggle and `repeat` slider via `update_widget`, rebuild overlays with `clear_overlays()` + one `add_overlays(...)` for the A/B/region/label primitives (contract E1, O1/O2)
- [X] T032 [US1] `main.luau`: `set_a()`/`set_b()` — read `api.playback.state().position_ms`, call `api.markers.set_loop_endpoint(S.region, "a"|"b", pos)`, set `S.active`, clear/set Status (contract A1/A2)
- [X] T033 [US1] `main.luau`: `toggle_loop(value)` — `request_focus()` first, then `arm_loop(S.region)`/`disarm_loop()`; on refusal revert the `loop` toggle via `update_widget` and set Status (`@status_no_region` / `@status_no_focus` / `err.message`) (contract A3/A4/A11, FR-008)
- [X] T034 [US1] `main.luau`: `set_repeat(v)` — `v == 0` ⇔ `"infinite"`; call `set_loop_repeat` when `S.region` exists, else store `S.pending_repeat` and apply it on the next successful region creation (contract A9)
- [X] T035 [US1] `main.luau`: `nudge(delta)` — target = `S.active` if its endpoint exists, else `b`, else `a`, else no-op; `markers.move(id, ms + delta)` floored at 0; update `S.active` (contract A5, FR-006)
- [X] T036 [US1] `main.luau`: `panel_interaction` dispatch (`loop`→`toggle_loop`, `repeat`→`set_repeat`) and `action_invoked` dispatch table keyed by the action's short id (research R6)
- [X] T037 [US1] `main.luau`: wire `track_changed`/`marker_changed`/`loop_armed`/`loop_disarmed`/`focus_granted`/`focus_revoked` handlers — all but the loop events call `relist()`; `loop_armed`/`loop_disarmed` update `S.armed` and the `loop` toggle directly; focus events log only (contract E1–E5, research R6)

**Checkpoint**: User Story 1 fully functional and independently testable (T023–T029 green).

---

## Phase 4: User Story 2 - Markers outlive the session and the plugin (Priority: P1)

**Goal**: A/B/cue markers persist across restarts and remain visible/editable when Section Loop is disabled (spec.md US2).

**Independent Test** (spec.md): set A, B, one cue; relaunch; verify all three restored. Separately, disable Section Loop mid-loop and verify the loop releases while markers stay visible and host-editable.

### Tests for User Story 2 ⚠️ write first

- [X] T038 [P] [US2] `markers_survive_reload_and_relist` (US2-1, SC-002) in `crates/modplayer-core/tests/controller_section_loop.rs`
- [X] T039 [P] [US2] `disable_mid_loop_releases_and_keeps_markers` (US2-2/US2-3, SC-003) in `controller_section_loop.rs`
- [X] T040 [P] [US2] `host_edit_reflected_via_marker_changed` (US2-5) in `controller_section_loop.rs`
- [X] T041 [P] [US2] `reenable_relists` (US2-4) in `controller_section_loop.rs`

### Implementation for User Story 2

- [X] T042 [US2] Confirm/adjust `main.luau`'s `ready_ack` so a plugin enabled or Restarted mid-track re-registers panel/actions/overlays and immediately re-lists the current track's markers, with no leftover session state (`S.active`, `S.pending_repeat` start empty) (contract G4, L2, FR-015) — no host code change expected; this task verifies T030's `relist()` call covers the re-enable path
- [X] T043 [US2] Verify (no plugin code expected — host-side per 010 FR-007/011 FR-016) that disable/suspend releases focus, disarms the loop, clears plugin overlays, and leaves host-native marker rendering untouched; add any missing assertions surfaced by T039 to `controller_section_loop.rs` rather than plugin logic (contract L1, O3, FR-014)
- [X] T044 [US2] Write `plugins/bundled/org.modplayer.section-loop/README.md`: what the plugin does, its keys/actions, how it uses the API, and the persistence guarantee (FR-002, FR-013)

**Checkpoint**: User Stories 1 and 2 both independently functional.

---

## Phase 5: User Story 3 - The user's plugin choice always wins on transport focus (Priority: P2)

**Goal**: Focus arbitration from Section Loop's own panel, cue ownership rules, clear-markers, and the inert snap toggle (spec.md US3).

**Independent Test** (spec.md): enable Section Loop alongside the `focus-b` fixture holding `transport.control`; give it focus; flip Section Loop's Loop toggle under default policy and verify focus moves and the loop arms; switch to manual and verify the refusal path.

### Tests for User Story 3 ⚠️ write first

- [X] T045 [P] [US3] `loop_toggle_takes_focus_from_other_plugin` (US3-1, SC-004, `MODPLAYER_PLUGIN_FIXTURES=1`) in `crates/modplayer-core/tests/controller_section_loop.rs`
- [X] T046 [P] [US3] `manual_policy_refuses_and_hints_no_deferred_arm` (US3-2, SC-007) in `controller_section_loop.rs`
- [X] T047 [P] [US3] `host_l_disarm_then_rearm` (US3-4) in `controller_section_loop.rs`
- [X] T048 [P] [US3] `jump_cue_requests_focus_then_seeks` (US3-5), `jump_cue_empty_slot_noop` in `controller_section_loop.rs`
- [X] T049 [P] [US3] `set_cue_on_host_slot_refused_with_hint`, `set_cue_own_slot_moves` in `controller_section_loop.rs`
- [X] T050 [P] [US3] `clear_markers_deletes_own_only_and_disarms_hostside` in `controller_section_loop.rs`
- [X] T051 [P] [US3] `snap_toggle_always_reverts`, `status_clears_on_success` in `controller_section_loop.rs`
- [X] T052 [P] [US3] `overlay_set_matches_markers` extended to cue dots/labels (O1 complete), `overlay_cleared_on_disable` (O3) in `controller_section_loop.rs`

### Implementation for User Story 3

- [X] T053 [US3] `main.luau`: `set_cue(n)` — `api.markers.set_cue(n, pos)`; on `not_owner` set Status to `@status_cue_owned_n` (per-slot key) (contract A6, EC)
- [X] T054 [US3] `main.luau`: `jump_cue(n)` — no-op if `S.cues[n]` is nil; else `request_focus()` then `api.transport.seek(S.cues[n].ms)` (contract A7, FR-010)
- [X] T055 [US3] `main.luau`: `clear_markers()` — `markers.delete(id)` for `S.a`, `S.b`, and every own `S.cues[n]`; no `disarm_loop`, no focus call (contract A8, FR-011)
- [X] T056 [US3] `main.luau`: `toggle_snap`/`snap` panel interaction — always `update_widget("main","snap",false)`, action is a no-op (contract A10, FR-005)
- [X] T057 [US3] `main.luau`: extend `relist()`'s overlay rebuild with `cue_<n>_dot`/`cue_<n>_label` per set slot (≤ 21 primitives total) (contract O1, FR-012)

**Checkpoint**: All three user stories independently functional.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Whole-package acceptance (SC-008, licensing, strings), CI gates, and Governance manual sign-off.

- [X] T058 [P] `crates/modplayer-core/tests/bundled_section_loop.rs`: `package_discovered_enabled_by_default`, `manifest_permissions_exact`, `script_calls_only_schema_requests` (regex scan of `main.luau`'s `api.<ns>.<method>(` calls against the generated schema, SC-008), `license_files_match_workspace`, `strings_cover_every_at_key` (contract P1–P4, FR-018)
- [X] T059 [P] Finalize `plugins/bundled/org.modplayer.section-loop/README.md` and its manifest justifications for review (FR-002, FR-003)
- [x] T060 Run the full quickstart.md automated gate locally: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, `scripts/check-license-headers.sh` — **4/5 green** (fmt clean — fixed pre-existing drift in `controller_section_loop.rs`/`bindings.rs` along the way; clippy clean — fixed one pre-existing `collapsible_if` in `plugins/apply.rs::UpdateWidget` and one in this phase's own `bundled_section_loop.rs`/`markers_model.rs`; `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok; `scripts/check-license-headers.sh` — all `*.rs` files carry the SPDX header). `cargo test --workspace` has **2 pre-existing failures blocked by T022 (Phase 2, not this session's scope)**: `modplayer-ui/tests/accessibility.rs::{plugins_section_controls_named, transport_panel_empty_state_exposes_its_accessible_name}` still assert the pre-012 zero-bundled-package/empty-focus-holder baseline that Section Loop's now-always-present, always-enabled, `transport.control`-holding package breaks — exactly what T022 names ("fix any 009/010/011 fixture-suite assertion that assumed zero bundled packages or an unchanged focus-holder/plugin-count"). This session did not touch those files or check T022, per its Phase-6-only scope; every 012-owned suite (`bundled_section_loop`, `controller_section_loop`, `markers_model`, `controller_markers`, `plugins_manifest_discovery`, gateway/runtime suites, `plugin_panels::section_loop_panel_keyboard_and_names`) is green. **1320 passed / 2 failed** workspace-wide (macOS, `RUSTUP_TOOLCHAIN=1.95.0`). One flake surfaced on a second full run, under the heavy parallelism of `cargo test --workspace` (which halts on a test binary's first failure): `modplayer-core/tests/controller_plugin_ui.rs::button_action_source_ui` — confirmed environmental, not a regression, by re-running it alone, green (matches 011's own PR body noting the same class of flake).
- [x] T061 Execute manual scenarios M1–M9 from quickstart.md (Governance sign-off, M9 under VoiceOver); record each result in this file; for any deviation, add a regression test and echo the correction into research.md — see **Manual Scenario Log** below; this sandbox has no VoiceOver/window-driving tool (same gap 009/010/011's own sessions recorded), so every scenario's evidence is its automated proxy plus one real `cargo build -p modplayer` + a timed launch (confirms no panic; RPC calls made without a window driving `tick()` time out `host_busy`, as expected). A maintainer session on real hardware still owes the literal Quartz-recipe walk before final Governance sign-off.
- [x] T062 Assemble the PR body: real-time note "N/A — no real-time path changes" (Constitution I), the Constitution IX change request text from contracts/plugin-api-v1.3.md §6, and area-maintainer sign-off requests for `modplayer-capability-gateway` and `modplayer-plugin-runtime` (GOV-3.2) — drafted to [PR_BODY.md](PR_BODY.md), following 010/011's own precedent.

### Manual Scenario Log (T061) — 2026-09-20: no VoiceOver/window-driving tool available

Same environment limitation 009/010/011's own manual-scenario sessions
recorded: this sandbox has no tool that drives VoiceOver or a
keyboard/pointer traversal of a native window (browser automation only,
and this is not a browser app). `cargo build -p modplayer` succeeds
(`RUSTUP_TOOLCHAIN=1.95.0`); `MODPLAYER_CONFIG_DIR=$(mktemp -d)
./target/debug/modplayer`, launched and given a few seconds on this
host's real display, produces no panic — the one log line it emits,
`register_action(set_a): invalid_state/host_busy The host did not
reply in time`, is exactly the expected shape with nothing driving
`tick()` at a steady frame rate (RPC round trips can't complete), not a
defect. In its place, the automated suites built in Phases 3–6 drive
the real bundled package end to end — via a directly-driven
`PlaybackController`/`FakeBackend`/`SyntheticSource` for every host
call and via `egui::Context::run_ui` with AccessKit enabled for M9 —
standing in as the automated proxy for each scenario below. All cited
tests are green: `cargo test -p modplayer-core --test
controller_section_loop` (27 passed) and `cargo test -p modplayer-ui
--test plugin_panels section_loop` (1 passed) this session's own runs.

| # | Scenario | Automated proxy | Result |
|---|---|---|---|
| M1 | Fresh-install panel drill | `registers_panel_and_23_actions`, `fresh_install_iol_flagged_brackets_active`, `panel_set_a_then_set_b_creates_owned_region`, `loop_toggle_requests_focus_and_arms_same_action`, `overlay_set_matches_markers` | PASS (automated proxy) |
| M2 | Repeat count | `finite_repeat_releases_and_toggle_follows` | PASS (automated proxy) |
| M3 | Keyboard after rebind | `nudge_moves_active_marker_10ms`, `nudge_default_b_then_a`; `fresh_install_iol_flagged_brackets_active` (host `I`/`O`/`L` keep winning until rebound, per contract G3) | PASS (automated proxy) |
| M4 | Persistence | `markers_survive_reload_and_relist` | PASS (automated proxy) |
| M5 | Disable mid-loop | `disable_mid_loop_releases_and_keeps_markers`, `reenable_relists`, `overlay_cleared_on_disable` | PASS (automated proxy) |
| M6 | Focus contention (fixtures) | `loop_toggle_takes_focus_from_other_plugin` (`MODPLAYER_PLUGIN_FIXTURES=1`), `manual_policy_refuses_and_hints_no_deferred_arm` | PASS (automated proxy) |
| M7 | Cues and ownership | `set_cue_on_host_slot_refused_with_hint`, `set_cue_own_slot_moves`, `jump_cue_requests_focus_then_seeks`, `jump_cue_empty_slot_noop` | PASS (automated proxy) |
| M8 | Snap and clear | `snap_toggle_always_reverts`, `clear_markers_deletes_own_only_and_disarms_hostside` | PASS (automated proxy) |
| M9 | Accessibility | `modplayer-ui/tests/plugin_panels.rs::section_loop_panel_keyboard_and_names` (every widget's role/accessible name via AccessKit) | PASS (automated proxy) |

No deviations surfaced; no regression test or research.md correction
needed. The two pre-existing `accessibility.rs` failures noted under
T060 are unrelated to these nine scenarios (they assert the host's
Plugins/Transport-focus panels' pre-012 baseline counts, not anything
Section Loop's own panel does) and are T022's, not this log's, to fix.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies; can start immediately
- **Foundational (Phase 2)**: depends on Phase 1 (package files must exist for T018/T019); **blocks every user story** — the schema/DTO/binding/host-model chain (T004→T005→{T009,T016}→T010/T014) must compile and pass before any `main.luau` handler in Phases 3–5 has a real host call to make
- **User Story 1 (Phase 3)**: depends only on Phase 2
- **User Story 2 (Phase 4)**: depends only on Phase 2; shares `main.luau`'s `ready_ack`/`relist()` written in T030/T031 (US1) — implement US1 first in practice, but US2's tests exercise no US1-only behavior beyond that shared scaffold
- **User Story 3 (Phase 5)**: depends only on Phase 2; shares the same scaffold as US2
- **Polish (Phase 6)**: depends on all three user stories being complete

### Within Phase 2

T004 → T005 → (T006, T007) and (T009 → T010 → T011) and (T012 → T013, T014 → T015) → T016 → T017 → T018 → T019 → (T020, T021) → T022. T005's new enum variants make `request.rs`, `bindings/mod.rs::dispatch` and `apply.rs::apply`'s exhaustive matches fail to compile until T010 and T016 add their arms — expected and intentional (Constitution IX mechanism).

### Within each User Story

- Tests (T023–T029 / T038–T041 / T045–T052) MUST be written and confirmed failing before their phase's implementation tasks
- Because all implementation tasks in a phase edit the same `plugins/bundled/org.modplayer.section-loop/main.luau`, they are **not** parallel with each other even though most carry no `[P]` — sequence T030→T037, then T042→T044, then T053→T057
- Test tasks within a phase are `[P]` (they land in the same test file but are independent test functions written before any implementation exists to conflict with)

### Parallel Opportunities

- T002, T003 in Setup
- T007, T011, T013/T015, T021 in Foundational (different crates/files from the task they follow)
- All test tasks within a user story phase (T023–T029, T038–T041, T045–T052)
- T058, T059 in Polish

---

## Parallel Example: Foundational Phase

```bash
# After T005 lands the new Request/Response variants:
Task: "Gateway tests in crates/modplayer-capability-gateway/tests/gateway.rs"        # T007
Task: "Runtime binding tests in crates/modplayer-plugin-runtime/tests/bindings.rs"   # T011
Task: "Core model tests in crates/modplayer-core/tests/markers_model.rs"            # T013
```

## Parallel Example: User Story 1 tests

```bash
Task: "registers_panel_and_23_actions, fresh_install_iol_flagged_brackets_active"    # T023
Task: "panel_set_a_then_set_b_creates_owned_region, set_b_before_set_a_then_swap"    # T024
Task: "loop_toggle_requests_focus_and_arms_same_action, loop_toggle_off_disarms"     # T025
Task: "finite_repeat_releases_and_toggle_follows, arm_incomplete_reverts..."         # T026
Task: "nudge_moves_active_marker_10ms, nudge_without_endpoints_noop, ..."            # T027
Task: "overlay_set_matches_markers (A/B/region subset)"                              # T028
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1: Setup
2. Phase 2: Foundational (API 1.3 + bundled registration) — CRITICAL, blocks everything
3. Phase 3: User Story 1
4. **STOP and VALIDATE**: run T023–T029 plus quickstart.md M1/M2 manually
5. This alone satisfies SC-001, SC-004 (single-plugin case), SC-005, SC-006

### Incremental Delivery

1. Setup + Foundational → foundation ready, package inert but discoverable
2. + User Story 1 → mark/loop drilling works end to end (MVP)
3. + User Story 2 → persistence and disable/re-enable proven (M4, M5)
4. + User Story 3 → focus contention, cues, clear, snap proven (M3, M6, M7, M8)
5. + Polish → SC-008 scan, licences, full CI gate, Governance manual sign-off (M9), PR body

### Note on "parallel team" staffing

Unlike a typical multi-service feature, Phases 3–5 all write into one file (`main.luau`) plus one shared test file (`controller_section_loop.rs`), so true multi-developer parallelism across user stories is limited after Phase 2; the `[P]` markers above are scoped to tests-before-code within a phase, not cross-phase file independence.

---

## Notes

- [P] tasks = different files, or independent test functions with no implementation yet to conflict with
- [Story] label maps task to its spec.md user story for traceability
- Verify each phase's tests fail against the Phase 1/2 skeleton before writing that phase's implementation
- Commit after each task or logical group
- Stop at any checkpoint to validate a story independently, per its spec.md **Independent Test**
- This feature ships **no new crate, trait, or flag** (Constitution X) — every task above modifies an existing crate or the new plugin package's own files

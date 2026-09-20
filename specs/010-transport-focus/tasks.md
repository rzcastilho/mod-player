# Tasks: Transport Focus Arbitration

**Input**: Design documents from `/specs/010-transport-focus/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included. `plan.md`'s Testing section and `contracts/*.md` name specific
suites and test functions as feature deliverables (Constitution VIII), so test
tasks are not optional here.

**Organization**: Tasks are grouped by user story (spec.md priorities P1–P3) to
enable independent implementation and testing of each story. Because this
feature's three stories share one arbitration engine (`FocusArbiter`, driven by
one `PluginHost`/`PlaybackController`), that engine is built once in Phase 2
(Foundational) and each user-story phase then adds the tests and any
story-specific surface (UI) that prove its own acceptance scenarios.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Exact file paths are included in every description

## Path Conventions

Single Cargo workspace under `crates/` (no new crate this slice):
`crates/modplayer-core`, `crates/modplayer-capability-gateway`,
`crates/modplayer-plugin-runtime`, `crates/modplayer-ui`; fixtures in
`plugins/fixtures/`; locales in `locales/en-US/`. See plan.md § Project
Structure for the full file/change map.

---

## Phase 1: Setup

**Purpose**: Land the schema-first API contract change that every other crate
compiles against (design note 1, Constitution IX).

- [X] T001 Bump `[api_version] minor = 1` and add `[[event]] name = "FocusGranted"` / `name = "FocusRevoked"` (both `requires = "transport.control"`, `payload = ["holder"]`) in `crates/modplayer-capability-gateway/api/v1.toml`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The arbitration engine and its plumbing — pure `FocusArbiter`,
gateway event/token deltas, runtime RPC routing, controller wiring, and the two
contention fixtures. **No user story can be tested until this phase is green.**

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Gateway (API 1.1 delivery side)

- [X] T002 [P] Add `HostEvent::FocusGranted { holder: OwnerInfo }` / `FocusRevoked { holder: OwnerInfo }` variants and update `kind()` in `crates/modplayer-capability-gateway/src/event.rs`
- [X] T003 [P] Add `FocusToken::set_holder(Option<PluginId>)` (core-only write side); remove `try_acquire`/`release_if`/`clear` (no remaining callers) in `crates/modplayer-capability-gateway/src/focus.rs`
- [X] T004 [P] Remove `Refusal::focus_held()` constructor; `RefusalCode` set otherwise unchanged in `crates/modplayer-capability-gateway/src/refusal.rs`
- [X] T005 Update `crates/modplayer-capability-gateway/tests/gateway.rs`: `admit_checks_focus_before_rate` now asserts via `set_holder`; drop tests referencing `focus_held`
- [X] T006 Regenerate `docs/plugin-api/v1.md` for 1.1 and add `api_version_is_1_1` / `reference_is_current` in `crates/modplayer-capability-gateway/tests/api_reference.rs`

### Runtime (RPC routing + Lua payloads)

- [X] T007 [P] Remove the local `TransportRequestFocus`/`TransportReleaseFocus` arms (both fall through to the exhaustive match's `_ => rpc(..)`) in `crates/modplayer-plugin-runtime/src/bindings/mod.rs`
- [X] T008 [P] Update doc comments only (no behavior change) in `crates/modplayer-plugin-runtime/src/bindings/transport.rs`
- [X] T009 Render `FocusGranted`/`FocusRevoked` → `{ holder }` via the existing `owner_to_string` in `event_to_lua` in `crates/modplayer-plugin-runtime/src/scheduler.rs`
- [X] T010 Add `bindings::request_focus_is_rpc_not_local`, `bindings::release_focus_is_rpc_not_local` in `crates/modplayer-plugin-runtime/tests/bindings.rs`, and `scheduler::focus_events_render_holder` in `crates/modplayer-plugin-runtime/tests/scheduler.rs`

### Core — pure arbiter

- [X] T011 [P] Create `FocusPolicy`, `FocusHolder`, `FocusArbiter`, `FocusChange`, `Vacancy`, `TransportActor` (with doc examples on `FocusArbiter::request`, `FocusPolicy::parse`) in `crates/modplayer-core/src/plugins/focus.rs` per data-model.md §1.1–1.5
- [X] T012 Add `pub mod focus;` and re-exports in `crates/modplayer-core/src/plugins/mod.rs`
- [X] T013 Re-export `FocusPolicy`, `FocusHolder`, `TransportFocusView`, `FocusRow` in `crates/modplayer-core/src/lib.rs`
- [X] T014 Create `crates/modplayer-core/tests/focus_arbiter.rs` covering rules A1–A12: `manual_never_auto_grants`, `auto_never_grants_on_request`, `first_wins_grants_first_only`, `first_wins_later_request_pending`, `first_wins_track_change_resets_holder_and_queue`, `first_wins_release_refills_earliest`, `first_wins_fault_refills_earliest`, `first_wins_take_back_locks_until_track_change`, `give_revokes_then_grants_in_order`, `give_to_holder_is_noop`, `request_twice_keeps_position`, `release_while_pending_withdraws`, `release_by_non_holder_is_noop`, `local_action_revokes_only_under_auto`, `policy_switch_keeps_holder_and_queue`, `revoked_precede_granted`, plus the `single_holder_invariant` proptest

### Core — settings and action catalog

- [X] T015 [P] Add `AudioSettings.focus_policy: FocusPolicy` and `InvalidField::FocusPolicy` in `crates/modplayer-core/src/settings/model.rs`
- [X] T016 [P] Add `RawTransport { focus_policy: String }` and `[transport]` read/write under 007's atomic-write contract (unknown/absent → `AutoOnInteraction` + warning) in `crates/modplayer-core/src/settings/store.rs`
- [X] T017 [P] Add `HostAction::ToggleTransportPanel` (`host.nav.toggle_transport_panel`, label key `action-nav-toggle-transport-panel`, category Navigation, kind trigger, scope NowPlaying, `repeats_while_held: false`, default `["T"]`) in `crates/modplayer-core/src/actions/catalog.rs`

### Core — host, view, controller wiring

- [X] T018 Add `arbiter: FocusArbiter` field + `arbiter()` accessor + `apply_focus_changes(changes)` (sets `FocusToken` once to the final holder, then sends events to each handle in order, skipping a missing handle); `stop(id, ..)` calls `apply_focus_changes(arbiter.vacate(id, Fault))` before the existing marker/chain teardown, in `crates/modplayer-core/src/plugins/host.rs`
- [X] T019 [P] Add `TransportFocusView`, `FocusRow`, `from_records_and_arbiter` (rows = enabled ∧ lifecycle ∈ {Loading, Active} ∧ grants ∋ `transport.control`, sorted by name) in `crates/modplayer-core/src/plugins/view.rs`
- [X] T020 Add `transport_actor: TransportActor` field + `with_transport_actor` scoped helper; `note_local_transport_action()` hook at the entry of `dispatch` for the six transport `Input`s and after `Ok` in `arm_loop`/`disarm_loop`; `Remote` scope for `Effect::ApplyPendingTransferCommand` and shutdown/sign-out's internal `stop()`; `on_track_changed()` call in `fan_out_plugin_playback_events`'s `TrackChanged` branch (before it fans `TrackChanged` out); façade methods `transport_focus_view()`, `focus_policy()`/`set_focus_policy()`, `focus_give()`, `focus_take_back()`, `pub(crate) focus_request()`/`focus_release()`; `launch()` seeds the arbiter's policy from `AudioSettings.focus_policy`, in `crates/modplayer-core/src/controller.rs`
- [X] T021 Route `RequestFocus`/`ReleaseFocus` to `controller.focus_request`/`focus_release` (always `Ok(Response::Ok)`); add `require_focus(controller, plugin)?` re-check ahead of the eight focus-gated arms (`Play/Pause/Toggle/Seek/SkipNext/SkipPrevious/ArmLoop/DisarmLoop`); scope `drain_plugin_requests` under `TransportActor::Plugin(id)`, in `crates/modplayer-core/src/plugins/apply.rs`

### Core — contention fixtures

- [X] T022 [P] Create `plugins/fixtures/focus-a/{plugin.toml,main.luau,README.md}` (permissions `playback.observe`, `transport.control`, `markers.read`, `markers.write`): `request_focus()` on `ready_ack`; on `focus_granted` → `seek(1000)`, create + arm a transient loop region; re-`request_focus()` on `track_changed`; logs every `focus_*`/`play_state_changed`/`loop_*` event; deliberately hangs on its third received `play_state_changed`; `debug_probe` returns the log — per data-model.md §5
- [X] T023 [P] Create `plugins/fixtures/focus-b/{plugin.toml,main.luau,README.md}` (permissions `playback.observe`, `transport.control`): `request_focus()` on `ready_ack`; on `focus_granted` → `seek(2000)`; re-`request_focus()` on `track_changed`; while not holder, `seek(0)` on `play_state_changed` and logs the resulting `no_focus`; `debug_probe` returns the log — per data-model.md §5
- [X] T024 Embed the `focus-a`/`focus-b` packages (ten fixtures total) in `crates/modplayer-core/src/plugins/bundled.rs`
- [X] T025 [P] Update `plugins/fixtures/wellbehaved/README.md`: note that `arm_loop` now reports `no_focus` until the user gives it focus under the default policy

### Core — 009 regression rewrites (research R10)

- [X] T026 Rewrite `crates/modplayer-core/tests/controller_plugins_permissions.rs`: replace `request_focus_contention_invalid_state` with `request_focus_is_recorded_never_refused`; rework the flood rate-limit test to set `FirstRequestWins` and `focus_give` the flood fixture before triggering; rework `queue_write_ignores_focus` to use `focus_give` instead of a CAS win
- [X] T027 Update `crates/modplayer-core/tests/controller_plugins_lifecycle.rs` to confirm 009's teardown order still holds with `arbiter.vacate(id, Fault)` first in `stop()`

**Checkpoint**: `cargo test --workspace` green on the arbitration engine, gateway/runtime deltas and fixtures — user story phases can now begin.

---

## Phase 3: User Story 1 - Exactly one party is in charge, and the user always wins (Priority: P1) 🎯 MVP

**Goal**: With the engine from Phase 2 wired end to end, prove the single-holder
guarantee and the user's unconditional precedence — including on a fault.

**Independent Test**: Plugin Y's `seek` while X holds focus → `no_focus`, no
playhead movement. The host loop-toggle shortcut applies immediately and,
under auto, returns focus to the host. A remote-controller pause applies and
never changes focus.

- [X] T028 [P] [US1] Create `crates/modplayer-core/tests/controller_transport_focus.rs` with `non_holder_seek_is_refused_no_side_effect` (SC-001, US1-1) and `host_side_recheck_refuses_after_revoke` (R12)
- [X] T029 [US1] Add `local_loop_toggle_applies_and_returns_focus_under_auto` (US1-2) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T030 [US1] Add `local_action_keeps_holder_under_manual_and_first_wins` (SC-006) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T031 [US1] Add `remote_pause_applies_without_focus_change` (US1-3, EC-3.4) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T032 [US1] Add `suspended_holder_returns_to_host_and_disarms_loop` (US1-4, SC-003) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T033 [US1] Add `disabled_holder_returns_to_host_and_disarms_loop` (US1-5) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T034 [US1] Add `event_order_revoked_before_granted_on_plugin_to_plugin` (FR-014) and `non_fault_transitions_keep_loop_armed` (FR-007 second half) to `crates/modplayer-core/tests/controller_transport_focus.rs`

**Checkpoint**: User Story 1 is independently proven — contention is refused
with zero side effect and the user overrides any holder, including on fault.

---

## Phase 4: User Story 2 - The user picks how focus gets assigned (Priority: P2)

**Goal**: Manual, auto-on-interaction and first-request-wins each behave per
the state-transition table (data-model.md §1.6), and the policy (not the
holder) survives a relaunch.

**Independent Test**: Manual — two requests, neither granted until "Give
focus". Auto — a panel "Give focus" grants immediately. First-wins — A before
B on a fresh track grants A; a new track lets B win.

- [X] T035 [US2] Add `manual_two_requests_both_pending_until_give` (US2-1) and `auto_give_focus_grants_immediately` (US2-2) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T036 [US2] Add `first_wins_a_then_b_over_twenty_tracks` (SC-005, US2-3/4) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T037 [US2] Add `policy_switch_leaves_holder` (US2-5) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T038 [US2] Add `first_wins_take_back_no_refill` (US2-6) and `first_wins_release_refills` (US2-7) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T039 [US2] Add `policy_persists_holder_does_not` (US2-8, SC-007) to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T040 [US2] Add `give_to_never_requested_plugin_delivers_granted`, `release_while_pending_withdraws_request`, `pending_plugin_disabled_leaves_queue_and_panel` (FR-011), `restarted_plugin_is_observer` to `crates/modplayer-core/tests/controller_transport_focus.rs`
- [X] T041 [P] [US2] Extend `crates/modplayer-core/tests/settings.rs` with `settings_round_trip_focus_policy` (+ proptest strategy extension) and the unknown-value → default-with-warning case

**Checkpoint**: User Story 2 is independently proven — all three policies and
policy persistence hold; combined with US1 the arbitration engine is fully
covered.

---

## Phase 5: User Story 3 - The Transport panel shows and controls who's in charge (Priority: P3)

**Goal**: A Transport panel in Now Playing shows the holder and every eligible
plugin's request order, and lets the user give/take back focus and change
policy in one action each, fully keyboard-operable.

**Independent Test**: Press `T`; panel opens showing holder, three plugin rows
(one requesting), "Give focus"/"Take back"; exercise both and verify state and
events match.

- [X] T042 [P] [US3] Create `locales/en-US/transport.ftl` with the 14 keys from contracts/ui-transport-panel.md §3
- [X] T043 [P] [US3] Add `action-nav-toggle-transport-panel = Toggle Transport panel` to `locales/en-US/controls.ftl`
- [X] T044 [US3] Create `crates/modplayer-ui/src/transport_view.rs`: `panel_open_id()`, `toggle_transport_panel(ctx)`, `show(ui, controller)` rendering the holder label, policy `ComboBox`, "Take back" button, one row per plugin with its "Give focus" button and requesting badge, and the empty state — per contracts/ui-transport-panel.md §1–§2
- [X] T045 [US3] Add `pub mod transport_view;` in `crates/modplayer-ui/src/lib.rs`
- [X] T046 [US3] Add the header "Transport" `selectable_label` beside "Queue"/"Effects", and draw the panel after the Effect Chain panel and before the Queue panel when open, in `crates/modplayer-ui/src/now_playing.rs`
- [X] T047 [US3] Wire `invoke(HostAction::ToggleTransportPanel)` to `transport_view::toggle_transport_panel` in `crates/modplayer-ui/src/actions.rs`
- [X] T048 [P] [US3] Create `crates/modplayer-ui/tests/transport_view.rs` (offscreen `egui::Context`, `FakeBackend` + `SyntheticSource`, `MODPLAYER_PLUGIN_FIXTURES=1`): `panel_lists_only_transport_control_plugins`, `holder_and_requesting_badges_render`, `give_focus_click_changes_holder`, `take_back_click_returns_host`, `policy_combo_persists_selection`, `empty_state_when_no_eligible_plugin`, `suspended_holder_row_disappears_and_holder_reads_host`
- [X] T049 [P] [US3] Extend `crates/modplayer-core/tests/actions.rs` with `toggle_transport_panel_action_in_catalog` (id, label key, scope, default `T`, no conflict)
- [X] T050 [P] [US3] Extend `crates/modplayer-ui/tests/actions.rs` with `t_toggles_transport_panel_in_now_playing_scope`, `t_ignored_while_text_field_focused`, `t_ignored_outside_now_playing`
- [X] T051 [P] [US3] Extend `crates/modplayer-ui/tests/now_playing.rs` with `transport_toggle_beside_queue_and_effects`, `transport_panel_survives_track_change`
- [X] T052 [P] [US3] Extend `crates/modplayer-ui/tests/accessibility.rs`: every contracts/ui-transport-panel.md §2 element has a non-empty `WidgetInfo` label
- [X] T053 [P] [US3] Extend `crates/modplayer-ui/tests/fluent_keys.rs`: every `transport.ftl` key and `action-nav-toggle-transport-panel` present

**Checkpoint**: All three user stories are independently functional and
together deliver the complete feature.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: The automated gates, doc/build sign-off and the manual scenarios
quickstart.md reserves for the implementing agent.

- [X] T054 [P] Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo deny check`, and `scripts/check-license-headers.sh`; fix any findings (new files: `focus.rs`, `transport_view.rs`, `focus-a`/`focus-b` fixtures)
- [X] T055 Run `cargo test --workspace` and confirm every suite in quickstart.md's table is green on the local platform
- [X] T056 [P] Confirm doc examples on `FocusArbiter::request`, `FocusPolicy::parse`, `FocusToken::set_holder` compile via `cargo test --doc`
- [X] T057 [P] Confirm `cargo test -p modplayer --test single_dependent` and `--test decoded_store_boundary` are unaffected (Constitution IV, V)
- [X] T058 Execute manual scenarios M1–M8 from quickstart.md (Constitution Governance › Manual Scenario Sign-Off); record results, and ship a regression test for any deviation, in quickstart.md
- [X] T059 Record area-maintainer sign-off for the `modplayer-capability-gateway` and `modplayer-plugin-runtime` changes (GOV-3.2) in the PR
- [X] T060 Write the PR body's Constitution IX change-request text from contracts/plugin-api-v1.1.md §4

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup (T001's schema bump). **BLOCKS all user stories** — the arbiter, gateway events, RPC routing and fixtures are shared by every story.
- **User Stories (Phase 3–5)**: All depend on Foundational completion.
  - US1 (P1) can start immediately after Foundational.
  - US2 (P2) reuses the `controller_transport_focus.rs` file US1 creates (T028) — start after T028, in parallel with the rest of US1 if staffed, but its tasks are not `[P]` against US1's within that file.
  - US3 (P3) only needs Foundational (`transport_focus_view()`, `focus_give`/`focus_take_back`, the `HostAction`); it does not need US1/US2's tests to exist, so it can run fully in parallel with US1/US2.
- **Polish (Phase 6)**: Depends on all three user stories being complete.

### User Story Dependencies

- **User Story 1 (P1)**: Foundational only.
- **User Story 2 (P2)**: Foundational only; shares a test file with US1 (sequential within that file) but no functional dependency on US1's test outcomes.
- **User Story 3 (P3)**: Foundational only — independent of US1/US2.

### Within Each User Story

- All of US1's and US2's tasks land in the same new test file (`controller_transport_focus.rs`), so within each story they run **sequentially**, in the listed order.
- US3: locale files (T042–T043) before the panel (T044); panel before its `lib.rs`/`now_playing.rs`/`actions.rs` wiring (T045–T047); wiring before the six test-file extensions (T048–T053), which are mutually `[P]`.

### Parallel Opportunities

- Setup has a single task (no parallelism).
- Foundational: T002–T004 (gateway files), T007–T008 (runtime files), T015–T017 (settings/catalog files), T019 (view.rs, parallel to T018's host.rs), T022–T023/T025 (fixture directories) can each run in parallel within their group.
- User Story 3's test-file extensions (T048, T049, T050, T051, T052, T053) are all mutually parallel once T042–T047 land.
- User Story 3 as a whole can run in parallel with User Story 1 and User Story 2 (different crate, no shared file).

---

## Parallel Example: Foundational gateway files

```bash
Task: "Add HostEvent::FocusGranted/FocusRevoked + kind() in crates/modplayer-capability-gateway/src/event.rs"
Task: "Add FocusToken::set_holder; remove try_acquire/release_if/clear in crates/modplayer-capability-gateway/src/focus.rs"
Task: "Remove Refusal::focus_held() in crates/modplayer-capability-gateway/src/refusal.rs"
```

## Parallel Example: User Story 3 test extensions (after T042–T047)

```bash
Task: "Create crates/modplayer-ui/tests/transport_view.rs (panel_lists_only_transport_control_plugins, ...)"
Task: "Extend crates/modplayer-core/tests/actions.rs with toggle_transport_panel_action_in_catalog"
Task: "Extend crates/modplayer-ui/tests/actions.rs with t_toggles_transport_panel_in_now_playing_scope, ..."
Task: "Extend crates/modplayer-ui/tests/now_playing.rs with transport_toggle_beside_queue_and_effects, ..."
Task: "Extend crates/modplayer-ui/tests/accessibility.rs (WidgetInfo labels)"
Task: "Extend crates/modplayer-ui/tests/fluent_keys.rs (all transport.ftl + controls.ftl keys)"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001).
2. Complete Phase 2: Foundational (T002–T027) — CRITICAL, blocks all stories.
3. Complete Phase 3: User Story 1 (T028–T034).
4. **STOP and VALIDATE**: `cargo test -p modplayer-core --test controller_transport_focus` green; run quickstart.md's M2 and M4 manually.
5. This is already a shippable guarantee: no two plugins can fight, and the user always wins.

### Incremental Delivery

1. Setup + Foundational → engine ready, nothing user-visible yet (default policy still works headlessly).
2. Add User Story 1 → the single-holder/user-override guarantee is proven → MVP.
3. Add User Story 2 → all three policies and persistence are proven.
4. Add User Story 3 → the Transport panel makes US1/US2 usable and inspectable; run the full quickstart.md, including M1, M3, M6–M8.
5. Phase 6 → gates, sign-offs, PR text.

### Parallel Team Strategy

With multiple developers, after Foundational (Phase 2) is done:

- Developer A: User Story 1 (T028–T034), then User Story 2 (T035–T041) — same file, same owner avoids merge churn.
- Developer B: User Story 3 (T042–T053) — a different crate (`modplayer-ui`) and locale files, no file overlap with A.
- Both converge on Phase 6.

---

## Notes

- `[P]` tasks touch different files with no unmet dependency at that point.
- `[Story]` labels appear only in Phase 3–5; Setup, Foundational and Polish carry none, per the checklist format.
- Tests are named exactly as `contracts/*.md` and `research.md` R10 specify, so a reviewer can trace every task back to a contract rule or acceptance scenario.
- Commit after each task or logical group (see the optional `/speckit-git-commit` hook offered before and after this command).
- Stop at either checkpoint (end of Phase 3, end of Phase 4) to validate that story independently before continuing.

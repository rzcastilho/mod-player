# Quickstart: Transport Focus Arbitration

**Feature**: 010-transport-focus | **Date**: 2026-09-19
Validation guide only — design in [plan.md](plan.md),
[data-model.md](data-model.md) and [contracts/](contracts/);
implementation detail belongs to `tasks.md`.

## Prerequisites

- Rust 1.95.0 (`rust-toolchain.toml`; if the shell exports
  `RUSTUP_TOOLCHAIN`, run with `RUSTUP_TOOLCHAIN=1.95.0`).
- C++ toolchain for the vendored Luau build (unchanged from 009).
- macOS host, signed-in Premium account and a second Connect-capable
  device on the same account (for M4) for the manual scenarios
  (Constitution Governance › Manual Scenario Sign-Off).

## Automated gates (green before any manual scenario)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

Feature suites (all inside `cargo test --workspace`):

| Suite | Proves | Spec |
|---|---|---|
| `cargo test -p modplayer-core --test focus_arbiter` | the three policies' transition table, ordering, the single-holder proptest | FR-001, FR-003–FR-006, FR-011; contracts/focus-arbitration.md §1 |
| `cargo test -p modplayer-core --test controller_transport_focus` | US1–US2 end to end with the `focus-a`/`focus-b` fixtures: `no_focus` + no side effect, host-side re-check, local vs remote user commands, suspension/disable return + loop disarm, first-wins over 20 tracks, policy persistence | US1, US2, FR-002, FR-002a, FR-007, FR-012, FR-014; SC-001–SC-003, SC-005–SC-007 |
| `cargo test -p modplayer-core --test controller_plugins_permissions` | rewritten 009 focus tests: `request_focus` always ok; flood rate limit under first-wins; queue writes never focus-gated | FR-003, FR-010 |
| `cargo test -p modplayer-core --test controller_plugins_lifecycle` | 009 teardown order still holds with the arbiter in `stop` | FR-007 |
| `cargo test -p modplayer-core --test settings` | `[transport] focus_policy` round trip (proptest), unknown → default | FR-012 |
| `cargo test -p modplayer-core --test actions` | `host.nav.toggle_transport_panel`, default `T`, no conflicts | FR-013 |
| `cargo test -p modplayer-capability-gateway` | API 1.1 reference current; `set_holder`-based admission order | FR-014; Constitution IX |
| `cargo test -p modplayer-plugin-runtime` | focus requests are RPCs; `focus_*` Lua payloads | contracts/plugin-api-v1.1.md §5 |
| `cargo test -p modplayer-ui --test transport_view` (+ `actions`, `now_playing`, `accessibility`, `fluent_keys`) | US3: rows, badges, one-click give/take back, policy combo, `T`, a11y names, keys | US3, FR-008, FR-009, FR-013; SC-004 |
| `cargo test -p modplayer --test single_dependent` / `decoded_store_boundary` | dependency and audio-boundary rules unchanged | Constitution IV, V |

Expected: green on ubuntu / macos / windows.

## Running the app with the focus fixtures

```bash
cargo build -p modplayer
MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer      # ten fixtures incl. focus-a / focus-b
MODPLAYER_CONFIG_DIR=$(mktemp -d) MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer   # fresh settings → default policy
```

Fixture console lines look like `[plugin:org.modplayer.fixture.focus-a]
info focus_granted holder=org.modplayer.fixture.focus-a`.

## Manual scenarios (executed by the implementing agent; results recorded in tasks.md)

Drive with the constitution's Quartz recipe; one screenshot per step;
read stderr for the fixtures' event logs. Now Playing = `Cmd+3`; the
panel = `T` or the header "Transport" toggle.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | Panel and default policy (US3) | Launch with fixtures, play a track, press `T` | panel opens; holder "host"; policy "Auto on interaction"; rows for focus-a, focus-b, flood, well-behaved only (observer/hang/throw/leak/noready/invalid absent); focus-a and focus-b marked "requesting (1st)/(2nd)" in `ready_ack` order; `T` again closes; header toggle equivalent |
| M2 | Give focus, user always wins (US1/US3) | Click "Give focus" on focus-a; wait; press `L`; observe | focus-a log: `focus_granted`, then `seek` ok, loop armed (audible wrap); focus-b's `seek(0)` on the next `play_state_changed` logs `no_focus` and the playhead does not jump; on `L`: the loop toggles immediately, panel holder reads "host" the same frame, focus-a log shows `focus_revoked holder=host` **before** `loop_disarmed`; focus-a's loop state is whatever `L` left it (not disarmed by the focus change) |
| M3 | Manual and first-wins policies (US2) | Switch policy to Manual; disable+re-enable focus-b; observe; switch to First request wins; skip to the next track; observe; skip again after "Take back" | Manual: focus-b re-requests and stays "requesting", holder unchanged; First-wins: on the track change holder → host, then whichever fixture's `request_focus` lands first (they re-request on `track_changed`) is granted; after "Take back" the other stays pending until the following track change |
| M4 | Remote controller (US1-3) | With focus-a holding focus under Auto, pause from the second device on the account | playback pauses; panel still shows focus-a as holder; focus-a log shows `play_state_changed state=paused` and **no** `focus_revoked` |
| M5a | Disable of the holder (US1-5) | Give focus to focus-a (loop armed); add a persisted point marker with `M`; in the Plugins section (`Cmd+4`) uncheck focus-a; back to Now Playing | within one second: panel holder "host", the armed loop released, the point marker still listed |
| M5b | Suspension of the holder (US1-4, SC-003) | Give focus to focus-a (loop armed); pause and play until focus-a has seen three `play_state_changed` events — the fixture deliberately hangs on the third (data-model.md §5) | ≈ 1 s later the 009 "suspended" Warning names focus-a; the panel already reads holder "host" and the loop is released in the same second; markers untouched; Restart → focus-a returns as an observer ("requesting" again only after its new `ready_ack`) |
| M6 | Well-behaved fixture under the default policy (R10) | Enable well-behaved; play | its log shows `request_focus ok` then `arm_loop no_focus`; after "Give focus" on its row and a re-enable, `arm_loop ok` |
| M7 | Persistence (US2-8, SC-007) | Set policy to First request wins; quit; relaunch; open the panel | policy shows "First request wins"; holder "host"; no row marked requesting until the fixtures' `ready_ack` requests arrive (then first-wins grants the first) |
| M8 | Keyboard-only panel (FR-009) | With VoiceOver on, Tab through the panel; activate "Give focus", change the policy, "Take back" | every control announced with the names in contracts/ui-transport-panel.md §2; all actions work without the pointer |

Deviations found during M1–M8 are recorded here and in
[research.md](research.md), and each ships with a regression test.

### Polish session (T058) — 2026-09-20: no interactive/audio host available

This session's sandbox has no attached display server usable for a real
window and no way to drive VoiceOver or a second Connect device, so
M1–M8's live Quartz-recipe walk (screenshot per step, VoiceOver
traversal, a second-device pause) still owes a maintainer session on
real hardware before final sign-off — the same limitation 009's US1–US4
sessions recorded. `cargo build -p modplayer` succeeds and
`MODPLAYER_PLUGIN_FIXTURES=1 MODPLAYER_CONFIG_DIR=$(mktemp -d)
./target/debug/modplayer` launches and runs fixture `ready_ack` handlers
without a panic (confirms the ten fixtures load and log), but with no
window driving `tick()` at a steady frame rate the RPC round trips
timed out (`host_busy`) rather than completing — not usable as evidence
for M1–M8 themselves, only for "the binary launches with the new
fixtures". In its place, the automated suites built in Phases 2–5
(`controller_transport_focus.rs`, `transport_view.rs`, `accessibility.rs`,
`actions.rs`, `settings.rs`, `controller_plugins_permissions.rs`) drive
the same fixtures and the same panel widget tree headlessly — via a
directly-driven `PluginHost`/`FakeBackend` and via `egui::Context::run_ui`
with AccessKit enabled, the same technique 009's own UI proxies used —
standing in as the automated proxy for each scenario below. All cited
tests pass.

| # | Automated proxy | Result |
|---|---|---|
| M1 | `panel_lists_only_transport_control_plugins` (only focus-a/focus-b/flood/well-behaved rows; observer/hang/throw/leak/noready/invalid absent), `holder_and_requesting_badges_render` (holder "host", requesting order badges), `empty_state_when_no_eligible_plugin`; `t_toggles_transport_panel_in_now_playing_scope` (`T` opens then closes in Now Playing scope only), `toggle_transport_panel_action_in_catalog` (default `T`, no conflict) | PASS |
| M2 | `non_holder_seek_is_refused_no_side_effect`, `host_side_recheck_refuses_after_revoke`, `local_loop_toggle_applies_and_returns_focus_under_auto` (loop toggles immediately, focus returns to host under auto), `event_order_revoked_before_granted_on_plugin_to_plugin` (`focus_revoked` always precedes `focus_granted`), `non_fault_transitions_keep_loop_armed`; `give_focus_click_changes_holder` (ui) | PASS |
| M3 | `manual_two_requests_both_pending_until_give`, `first_wins_a_then_b_over_twenty_tracks`, `first_wins_take_back_no_refill`, `first_wins_release_refills`; `policy_combo_persists_selection` (ui) | PASS |
| M4 | `remote_pause_applies_without_focus_change` (pause applies, holder and events untouched) | PASS |
| M5a | `disabled_holder_returns_to_host_and_disarms_loop` (markers untouched); `suspended_holder_row_disappears_and_holder_reads_host` (ui) | PASS |
| M5b | `suspended_holder_returns_to_host_and_disarms_loop` (SC-003's "within one second" as an immediate post-tick assertion, not a wall-clock timing measurement — real-hardware timing still owed) | PASS |
| M6 | `request_focus_is_recorded_never_refused` (`controller_plugins_permissions.rs`'s rewritten 009 test, T026) | PASS |
| M7 | `policy_persists_holder_does_not`, `settings_round_trip_focus_policy` (+ proptest) | PASS |
| M8 | `transport_panel_controls_expose_accessible_names_and_states`, `transport_panel_empty_state_exposes_its_accessible_name` (`accessibility.rs`, T052 — every control in contracts/ui-transport-panel.md §2 carries a non-empty accessible name); `t_ignored_while_text_field_focused`, `t_ignored_outside_now_playing` (keyboard precedence); note this proxy exercises accessible naming and the keyboard path that opens the panel, but "Give focus"/"Take back"/the policy combo are exercised via simulated pointer clicks (`give_focus_click_changes_holder`, `take_back_click_returns_host`, `policy_combo_persists_selection`) rather than a real Tab+Enter/VoiceOver traversal — the literal keyboard-only activation walk is part of what still owes a maintainer session | PASS |

**Real defects surfaced and fixed during this pass** (test staleness from
Phase 2's fixture count change, not a spec deviation): `crates/
modplayer-ui/tests/plugins_view.rs::rows_show_every_column_sorted_by_name`
and `crates/modplayer-ui/tests/accessibility.rs::
plugins_section_controls_named` still hardcoded the pre-010 8-fixture
Plugins-section expectations (8 toggles/rows, 7 `ok` health labels, one
`playback.observe` + `transport.control` explanation string). Neither
task in Phase 5/6 named these two 009-era files, so `cargo test
--workspace` (T055) caught the drift: adding `focus-a`/`focus-b`
(T022–T024) makes 10 fixtures, 9 valid (only `invalid` stays broken),
and `focus-b` now shares flood's exact two-permission set, so that
explanation label legitimately renders twice. Fixed by updating both
tests' expected counts/rows (10 fixtures with `Focus fixture A`/`Focus
fixture B` sorted between "Flood fixture" and "Hang fixture", 9 `ok`
labels, 2 matches for the shared two-permission explanation) — no
production code changed.

`FocusToken::set_holder` (T003) shipped without the doc example the
Constitution Check's Rust Quality Gates row and T056 both call for
(`FocusArbiter::request`, `FocusPolicy::parse`, `FocusToken::
set_holder`); the first two existed, the third did not. Added one
(mirrors `FocusToken`'s own unit test) — now three doc tests total for
this feature, all passing under `cargo test --doc`.

**Pre-existing flakes noted, out of this session's scope (not part of
this feature):** two tests, neither touched by this feature's tasks nor
related to plugins/focus, intermittently failed under `cargo test
--workspace`'s default parallelism across repeated runs and always
passed in isolation — left for a maintainer session rather than fixed
here:
- `crates/modplayer-ui/tests/accessibility.rs::
  markers_panel_and_empty_state_are_exposed` (Now Playing markers panel;
  introduced before 010-transport-focus, commit `670f008d`).
- `crates/modplayer-account/src/listener.rs::tests::
  error_callback_with_matching_state_resolves_as_error` (OAuth callback
  listener HTTP-mock timing; introduced in 003, commit `36b4b44`) — 3/3
  repeat runs green with `cargo test -p modplayer-account --lib` alone.

`cargo fmt --all --check` and `cargo clippy --workspace --all-targets
--all-features -- -D warnings` both stayed clean after the above fixes.
`cargo test --workspace` (macOS, `RUSTUP_TOOLCHAIN=1.95.0`): 1337
passed, 11 ignored, 0 failed on a clean repeat run; the two flakes noted
above surfaced on other repeats under default parallelism, never
together, and both pass in isolation every time. `cargo deny check` and
`scripts/check-license-headers.sh` both green. Ubuntu/windows legs of
`ci.yml` still owe a maintainer/CI run before final sign-off, same as
009's own polish session.

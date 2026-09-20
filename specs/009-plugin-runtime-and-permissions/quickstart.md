# Quickstart: Plugin Runtime, Sandbox, and Permission Gateway

**Feature**: 009-plugin-runtime-and-permissions | **Date**: 2026-09-19
Validation guide only — implementation details live in
[plan.md](plan.md), [contracts/](contracts/) and (later) `tasks.md`.

## Prerequisites

- Rust 1.95.0 (`rust-toolchain.toml`; if the shell exports
  `RUSTUP_TOOLCHAIN`, run with `RUSTUP_TOOLCHAIN=1.95.0`).
- A C++ toolchain for the vendored Luau build (Xcode CLT / MSVC / gcc —
  the same images `ci.yml` already uses).
- macOS host for the manual scenarios (Constitution Governance › Manual
  Scenario Sign-Off recipe), a signed-in Premium account for the ones that
  need playback.

## Automated gates (must be green before the manual scenarios)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

Feature-specific suites (all part of `cargo test --workspace`):

| Suite | What it proves | Spec |
|---|---|---|
| `cargo test -p modplayer-capability-gateway` | manifest parsing/validation (proptest), admission order, rate limiter, state store cap/atomicity, API reference is current | FR-001, FR-002, FR-007, FR-008, FR-014, FR-025; Constitution VIII, IX |
| `cargo test -p modplayer-plugin-runtime` | hang abort ≤ budget, repeated hang → suspend ≤ 1 s, memory bomb suspends the plugin only, exceptions abort one handler, never-ready → suspend, event order, position rate/coalescing/silence-while-paused/jitter, timers, RPC wait accounting, 200 ms unloading window, restore-before-`track_changed` | FR-004, FR-006, FR-009–FR-012, FR-016, FR-021, FR-022; SC-001 |
| `cargo test -p modplayer-core --test controller_plugins_lifecycle` | US1 end to end on `FakeBackend` + `SyntheticSource`: suspension notice with Restart/Disable, three suspensions → auto-disable, teardown ordering, orphan/readopt, shutdown and sign-out rules; audio callbacks keep flowing while the hang fixture is suspended | US1, FR-005, FR-011, FR-012, FR-014; SC-001–SC-003, SC-009 |
| `cargo test -p modplayer-core --test controller_plugins_permissions` | US2/US3: every request without its permission is refused with no side effect; `not_owner` vs `not_found`; permission-before-focus; focus contention; 1 000 seeks → `rate_limited`; chain full / 64 markers; transient markers; cue slots; host edits keep the owner | US2, US3, FR-007, FR-008, FR-017–FR-019, FR-025; SC-004–SC-007 |
| `cargo test -p modplayer-core --test plugins_manifest_discovery` | fixtures only under the env var; invalid fixture listed and not loaded; rows sorted; no uninstall | FR-002, FR-003, FR-013, FR-023; SC-005 |
| `cargo test -p modplayer-ui --test plugins_view` (+ `accessibility`, `fluent_keys`) | US4: columns, empty state, one-action disable, "—" gauges, no uninstall control, notification actions | US4, FR-023, FR-024; SC-008 |
| `cargo test -p modplayer --test single_dependent` | dependency rules unchanged | Constitution IV |

Expected: all pass on ubuntu / macos / windows in CI;
`position_jitter_under_5ms` is `#[cfg(not(windows))]` (research R5) and
the Windows figure is recorded in M6 below.

## Running the app with fixtures

```bash
cargo build -p modplayer
MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer          # eight fixture plugins under "bundled"
./target/debug/modplayer                                       # no plugins → empty state
MODPLAYER_PLUGIN_STATE_DIR=$(mktemp -d) MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer   # fresh plugin state
```

Plugin console entries appear on stderr as
`[plugin:<identifier>] <level> <message>` (research R19).

## Manual scenarios (executed by the implementing agent; results recorded in tasks.md)

Drive with the constitution's Quartz recipe; capture a screenshot per
step; read the stderr log for console entries.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | Empty state | Launch without the env var → Plugins section (`Cmd+4`) | `plugins-empty` message, no rows, no uninstall control |
| M2 | Fixture list | Launch with `MODPLAYER_PLUGIN_FIXTURES=1` → Plugins | eight rows sorted by name; each shows name, version, "bundled", enabled, health, permission summary, CPU %, memory `x.x MB / 64 MB`; the invalid fixture reads "invalid manifest: …teleport.everywhere…", unchecked, no health |
| M3 | Hang isolation (US1) | Play a track; the hang fixture hangs on `play_state_changed` | audio and transport continue with no dropout; within ≈ 1 s a Warning names "Hang fixture" with Restart and Disable; row health `suspended`, gauges "—"; console shows the deadline abort(s) then the suspension |
| M4 | Restart and auto-disable | Click Restart; pause/play to re-trigger; repeat once more | each restart re-runs the script and re-suspends; after the third suspension the row is disabled and a Warning without actions says it was auto-disabled; the counter is visible in the log |
| M5 | Memory and throw fixtures | Play for 30 s | leak fixture suspended with cause "memory" (notice + row); throw fixture stays `ok` → `warning` after three aborts, never suspended, still receiving events (console), `ok` again after 5 min idle |
| M6 | Never-ready | Enable the no-ready fixture | 5 s after enable: suspended with cause "did not start", Restart offered; **record the observed position-event jitter from the observer fixture's probe log on this platform** |
| M7 | Permission refusal (US2) | The observer fixture calls `api.transport.seek(0)` from its `ready_ack` handler on its own | console shows `permission_denied`/`not_granted` for `transport.seek`; transport position unchanged |
| M8 | Well-behaved cycle (US3) | Enable the well-behaved fixture; play | it subscribes at 30/s, requests focus, creates a loop region + arms it (loop audibly wraps; `L` re-arms the user's region and the plugin's disarms), creates a pitch-shift node before the time-stretch node (Effect Chain panel shows owner "plugin"), sets a parameter (audible), writes per-track state, schedules a position timer that fires once; console lists each step |
| M9 | Rate limit | Play: the flood fixture (enabled like every bundled plugin) requests focus and issues 1 000 seeks on its first `play_state_changed(playing)` | console shows 100 admitted seeks then 900 × `rate_limited`; UI stays responsive; audio continuous |
| M10 | Disable teardown (US4) | Uncheck the well-behaved fixture while its loop is armed and its node exists | loop disarms, focus returns (host transport works), its transient marker vanishes, its persisted marker stays, the node stays in the chain flagged orphaned with unchanged sound; re-enable → node re-adopted (owner "plugin"), `ready_ack` again |
| M11 | Per-track state | With the well-behaved fixture, play track A, then B, then A again | console shows the value written on A restored before `track_changed` on the second visit; sign out → `state.track` file for A gone, `plugin.json` intact; sign in → value absent |
| M12 | Shutdown | Quit while fixtures are active | each plugin logs `unloading(shutdown)`; the app exits within ≈ 250 ms of the request; no panic in the log |

Deviations from the spec found during M1–M12 are recorded here and in
[research.md](research.md).

### US1 session (T080) — 2026-09-19: no interactive/audio host available

This session's sandbox has no display and no audio device (headless,
non-interactive), so M3/M4/M5/M6's `did-not-start` half/M12 could not be
walked live per the constitution's Quartz recipe — that walk still owes
a maintainer session on real hardware before final sign-off. In its
place, `cargo test -p modplayer-plugin-runtime --test isolation` and
`cargo test -p modplayer-core --test controller_plugins_lifecycle` (T076,
T078) exercise the same fixtures end to end against `FakeBackend`/a
directly-driven `PluginHost`, standing in as the automated proxy for each
scenario below. All pass.

| # | Automated proxy | Result |
|---|---|---|
| M3 | `hang_fixture_suspended_audio_continues`: hang fixture suspended while a second thread keeps pulling `FakeBackend::render_buffers` and `transport_enabled()` is polled throughout | PASS — no missed render, transport never disabled |
| M4 | `three_suspensions_auto_disable`, `restart_keeps_session_counter`: restart re-suspends, 3rd suspension auto-disables and replaces the notice, counter persists across restarts | PASS |
| M5 (throw's warning half) | `warning_after_three_aborts_and_clears_after_5min`: 3 aborts open a 5-minute `Health::Warning` window (injected `now`) that clears past 5 min, plugin stays `Active` throughout | PASS — the leak fixture's own `Memory` suspension is covered instead by `memory_bomb_suspends_plugin_not_host` (T076) |
| M6 (suspension half) | `never_ready_suspended_with_restart`: no-ready fixture suspends as `did_not_start` after its `ready_timeout`, Restart offered and works | PASS — the position-jitter figure this scenario also asks for is a US3 (well-behaved fixture) measurement and stays open, tracked by T118 |
| M12 | `shutdown_delivers_unloading_and_bounds_wait`: every running plugin thread exits (or is bounded) within `SHUTDOWN_WAIT` (250 ms + slack) | PASS |

One real defect surfaced and was fixed during this pass (not a spec
deviation, an implementation bug): `StateWriter::join()`
(`modplayer-capability-gateway/src/state/writer.rs`) held its own
`Sender` clone alive for the whole duration of its blocking
`JoinHandle::join()` call (a struct field, dropped only once the method
returns) — self-deadlocking whenever it was the last clone standing,
which `unloading_writes_committed_within_200ms` (T077) hit
deterministically. Fixed by destructuring and dropping the sender before
joining.

### US2 session (T090) — 2026-09-19: no interactive/audio host available

Same headless/non-interactive sandbox as the US1 session above, so
M2/M7/M9's live Quartz-recipe walk still owes a maintainer session on
real hardware before final sign-off. In its place, `cargo test -p
modplayer-core --test plugins_manifest_discovery` and `cargo test -p
modplayer-core --test controller_plugins_permissions` (T088, T089)
exercise the same fixtures end to end against `FakeBackend`/a directly-
driven `PluginHost`, standing in as the automated proxy for each
scenario below. All pass.

| # | Automated proxy | Result |
|---|---|---|
| M2 | `fixtures_only_with_env` (fixtures listed only under the env var), `rows_sorted_by_name` (name-sorted, case-insensitive), `invalid_fixture_listed_not_loaded` (the `invalid` fixture's `teleport.everywhere` permission is caught by `Manifest::validate`'s `UnknownPermission`, whose `Display` reads "The required permission 'teleport.everywhere' is not in the permission catalog." — listed, `Lifecycle::Invalid`, `enabled = false`, no health), `no_uninstall` (disabling never removes the row) | PASS |
| M7 | `matrix_every_request_without_its_permission_is_denied_with_no_effect`: every `RequestKind` the observer fixture (`playback.observe` only) lacks the permission for is refused `permission_denied`/`not_granted` at `Gateway::admit` with zero `RpcEnvelope`s sent (structural, no host-side effect by construction); the real fixture's own `api.transport.seek(0)` call on `ready_ack` is refused the same way end to end, logs "seek refused: permission_denied/not_granted", and leaves `transport_state().intent == Stopped` and no marker created | PASS |
| M9 | `rate_limit_1000_seeks`: the real `flood` fixture requests focus then issues 1 000 `seek` calls (batched across a 1 ms interval timer so no single handler invocation — including the ones that resolve locally as `rate_limited` — itself exceeds the 4 ms handler budget); the rolling 1 s/100-call `transport` category cap admits the first ~100 and refuses the rest, logged by the plugin itself as "flood: N rate_limited" with `800 <= N < 1000` | PASS |

One fixture defect surfaced and was fixed during this pass (not a spec
deviation, an implementation bug): the `flood` fixture (T083) originally
issued all 1 000 `seek` calls in a single tight Lua loop inside one
`play_state_changed` handler invocation. Per RT7 an admitted call's RPC
wait is excluded from the 4 ms handler budget, but the ~900 calls the
rate limiter refuses *locally* (no RPC, so no wait to exclude) still
cost real Lua/host round-trip CPU time — enough, in practice, to exceed
the 4 ms budget partway through the loop and abort the handler
(`AbortCause::Deadline`) before it ever logged its own count, which
`rate_limit_1000_seeks` (T089) caught deterministically. Fixed by
spreading the 1 000 calls across many small batches, one per tick of a
1 ms `api.timers.set_interval` — each individual handler invocation
(request count × per-call overhead) stays comfortably inside budget,
while the calls themselves still land within the rate limiter's rolling
1 s window, so the demonstrated behavior (~100 admitted, ~900
`rate_limited`) is unchanged.

**Regression note (found during the US3/T103 session below, not fixed
there — out of that session's scope):** re-running
`controller_plugins_permissions::rate_limit_1000_seeks` after the US3
`apply.rs`/`scheduler.rs` changes landed (T092–T097) now fails
deterministically — "the rolling 1s/100-call cap must refuse the clear
majority of 1000 calls, got 0" — where it passed at the end of the US2
session above. Something in the US3-era scheduler/timer or dispatch
changes appears to have altered the `flood` fixture's batching timing
(e.g. its 1 ms interval timer no longer spreads the 1 000 seeks across
enough ticks before the rolling window resets, or admits differently).
This is a US2 test (T089) and fixture (T083); tracked here as a known
regression for a maintainer/Phase-4-scoped session to diagnose and fix
rather than addressed in this Phase 5 session.

### US3 session (T103) — 2026-09-19: no interactive/audio host available

Same headless/non-interactive sandbox as the US1/US2 sessions above, so
M8/M10/M11's live Quartz-recipe walk still owes a maintainer session on
real hardware before final sign-off. In its place, the US3 automated
suites stand in as the proxy for each scenario below. All cited tests
pass (`cargo test -p modplayer-plugin-runtime --test scheduler`: 14/14;
`--test bindings`: 4/4; `cargo test -p modplayer-core --test
controller_plugins_permissions -- queue_write_ignores_focus
chain_full_and_marker_limit transient_marker_lifetime
host_edit_keeps_owner_and_notifies`: 4/4; `--test markers_model --test
effects_model`: 22/22).

| # | Automated proxy | Result |
|---|---|---|
| M8 | `bindings.rs`'s Lua↔`Request`/`Response` round trips and `api.ready()` shape (T099); `scheduler.rs`'s `position_rate_clamped_and_coalesced`, `position_silent_while_paused`, `schedule_at_position_fires_once_with_actual_position` (T098); `chain_full_and_marker_limit` proving `CreateNode`/`CreateMarker` dispatch through `apply.rs` end to end; `host_edit_keeps_owner_and_notifies`, which actually launches the `wellbehaved` fixture's real Lua thread to `Active` (its `ready_ack` handler subscribing to position, requesting focus, arming its loop, creating and parameterizing its node, writing `state.track`, and scheduling its position timer, per T091) without fault | PASS |
| M10 | `orphan_and_readopt` (`effects_model.rs`, T102) proving a plugin's node is orphaned on teardown and re-adopted on restart with an unchanged chain position; `suspended_readopts_on_ready` and `disable_runs_teardown_in_order` (`controller_plugins_lifecycle.rs`, T078) proving `PluginHost::stop`'s L7 order (focus release, loop disarm, transient-marker removal, node orphan) runs against a real spawned plugin thread and its node re-adopts once the plugin reaches `Ready` again | PASS |
| M11 | `track_state_restored_before_track_changed` and `restore_timeout_delivers_event_anyway` (`scheduler.rs`, T098) proving RT11's per-track state load happens before the `track_changed` event is delivered, with a 4 ms deadline and discard-on-overrun fallback; `track_change_cancels_position_timers` (same file) proving position timers from the previous track do not leak into the next | PASS |

No new defects surfaced during this pass beyond the pre-existing
regression noted above (found while running the full
`controller_plugins_permissions` suite as a byproduct of this session,
not part of M8/M10/M11 themselves).

### US4 session (T112) — 2026-09-19: no interactive/audio host available

Same headless/non-interactive sandbox as the US1/US2/US3 sessions above,
so M1's/M2's live Quartz-recipe walk (screenshotting the real Plugins
section) still owes a maintainer session on real hardware before final
sign-off. In its place, `cargo test -p modplayer-ui --test plugins_view
--test accessibility --test fluent_keys` (T105–T111) drives the real
`plugins_view::show` widget tree headlessly through `egui::Context::
run_ui` with AccessKit enabled — the same technique `now_playing.rs`'s/
`effects_view.rs`'s own manual-scenario proxies use — standing in as the
automated proxy for each scenario below. All cited tests pass.

| # | Automated proxy | Result |
|---|---|---|
| M1 | `empty_state_without_fixtures`: a controller with no fixtures discovered (`plugins/bundled/` is empty this slice) renders exactly `plugins-empty` and no row, checkbox, or other per-plugin control | PASS |
| M2 | `rows_show_every_column_sorted_by_name`: all 8 fixtures render every column (name, version, source, permissions in catalog order, health) sorted case-insensitively by name (the invalid fixture's row falls back to its raw identifier since its manifest never parses into a `Manifest` — a real deviation from this scenario's literal "eight rows … each shows name" wording, noted below); `invalid_row_shows_reason_and_inert_toggle`: the invalid fixture's row reads exactly "invalid manifest: The required permission 'teleport.everywhere' is not in the permission catalog.", its toggle unchecked and inert; `suspended_row_shows_dash_gauges`/`toggle_disables_in_one_action` cover the CPU/memory "—" and one-action-disable halves of this scenario's own live-gauge/interaction claims; `plugins_section_controls_named` (`accessibility.rs`) confirms every one of the 8 toggles and the 7 valid fixtures' `ok` health labels carry a real accessible name | PASS |

**Deviation from quickstart.md's own M2 wording (not a spec bug):** M2
says "the invalid fixture reads … unchecked, no health", implying its
*name* cell still shows the manifest's declared name ("Invalid fixture").
`view.rs::row` (T104) and `host.rs::record_sort_key` both fall back to
the plugin's raw identifier (`org.modplayer.fixture.invalid`) once its
manifest fails to parse into a `Manifest` — there is no `Manifest.name`
to read at all in that case, only the `ManifestError`. This mirrors
`plugins_manifest_discovery.rs`'s (T088) own established behavior from
the US2 session above, not a new choice made here; M2's wording is
corrected in spirit by this note rather than by editing the scenario
table itself.

No new defects surfaced during this pass.

### Polish session (T119) — 2026-09-19: automated gates, macOS only

Same headless/non-interactive sandbox as every session above — no
ubuntu/windows CI runner reachable from here, so this run covers macOS
only; the ubuntu/windows legs of `ci.yml` still owe a maintainer/CI run
before final sign-off. All five gates ran with `RUSTUP_TOOLCHAIN`
unset so `rustup` honours `rust-toolchain.toml`'s pinned 1.95.0 (this
shell's ambient `RUSTUP_TOOLCHAIN=1.93.1` — below eframe/egui 0.36's
MSRV — otherwise overrides it):

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --doc --workspace
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

All five green on macOS. `cargo fmt --all` first rewrote a handful of
files whose formatting had drifted since the US1–US4 sessions (no
semantic change); one real `clippy::expect_used` violation in
`modplayer-core/src/plugins/bundled.rs`'s own test was fixed (`Result::
expect_err` → an explicit `match`, since the lint denies `expect_used`
crate-wide per T002). `PluginHandle::spawn` (T116) had no doc example
yet — one was added (spawns a plugin thread against hand-built
`RuntimeDeps`, sends `Control::Stop`, joins), and now passes under
`cargo test --doc`.

Two more real defects surfaced and were fixed while getting `cargo test
--workspace` green — both test-only races against shared state, not
product-code bugs, and both explain failures noted as open above:

- **The `rate_limit_1000_seeks` regression** noted in the US2 session
  above (and left open through US3/US4) was **not** a scheduler/timer
  change after all: `fixture_controller()` loads every bundled fixture
  together, and the `wellbehaved` fixture (US3, T091) requests transport
  focus on its own `ready_ack` — same as `flood` does on its first
  `play_state_changed(playing)`. Whichever fixture's thread reaches
  `request_focus()` first wins it (G2 admits `focus → rate limit` in
  that fixed order), so once `wellbehaved`'s `ready_ack` reliably beat
  `flood`'s trigger, every one of `flood`'s 1 000 seeks was refused
  `focus_held` before ever reaching the limiter — `rate_limited` stayed
  at 0. Fixed in the test itself (`controller_plugins_permissions.rs`):
  before triggering `flood`, it now issues a synthetic `ReleaseFocus` as
  every discovered plugin id (a no-op for whichever ids do not hold it),
  clearing the contention this test was never meant to exercise. 5/5
  repeat runs green after the fix.
- **`modplayer-ui/tests/plugins_view.rs`'s `empty_state_without_fixtures`**
  flaked under `cargo test --workspace`'s default parallelism:
  `MODPLAYER_PLUGIN_FIXTURES` is a process-global env var, and
  `fixture_controller()` already serialized its own brief mutation of it
  behind a `PLUGIN_ENV_LOCK`, but the file's `plain_controller()` (used
  by the empty-state test) called `PlaybackController::new` **without**
  taking that lock — a concurrently-running `fixture_controller()` call
  in another test thread could set the var for the instant
  `plain_controller` read it, discovering all eight fixtures instead of
  none. Fixed by having `plain_controller` take `PLUGIN_ENV_LOCK` too,
  even though it never sets the var itself. 5/5 repeat runs green after
  the fix.

`cargo test --workspace` (macOS): every suite green, no failures, no
flakes across 5 repeats of the two fixed tests.

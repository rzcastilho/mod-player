# Contract: Section Loop plugin package (`org.modplayer.section-loop`)

**Feature**: 012-section-loop-plugin | **Consumes**:
[plugin-api-v1.3.md](plugin-api-v1.3.md), 010 focus-arbitration, 011
ui-panels / action-registry-plugins / overlays | **Package**:
`plugins/bundled/org.modplayer.section-loop/`

This is the behaviour contract the script must honour and the tests
assert. It is written from the host's point of view (what a test or a
user can observe) — the script's internals are free as long as this
holds. Every rule cites the spec requirement it implements.

## 1. Package (P)

- **P1** Files: `plugin.toml`, `main.luau`, `README.md`, `LICENSE-MIT`,
  `LICENSE-APACHE` (byte-identical to the workspace root copies). Embedded
  via `bundled::packages()`, `fixture = false`; discovered on every
  launch, enabled by default, `can_uninstall = false` (FR-001, FR-002).
- **P2** Manifest permissions: required exactly `playback.observe`,
  `transport.control`, `markers.read`, `markers.write`, `ui.panel`,
  `ui.overlay`, `ui.shortcuts`; optional exactly `analysis.read`; every
  entry has a non-empty `justification` (FR-003). `api = "1.3"`,
  `license = "MIT OR Apache-2.0"`.
- **P3** The script's `api.<ns>.<method>(` call set is a subset of the
  generated schema's `(namespace, method)` pairs plus the runtime locals
  `api.on`, `api.ready`, `api.log.*`, `api.capabilities`. No other
  identifier from any host crate appears (SC-008; enforced by
  `bundled_section_loop.rs::script_calls_only_schema_requests`).
- **P4** The manifest declares no `state.*` permission and the script
  never calls `api.state.*` (FR-013).

## 2. Registration on `ready_ack` (G)

- **G1** One panel `main`, title "Section Loop", widgets in the order of
  data-model.md §3.3, every one labelled from `@key` (FR-004, FR-018).
- **G2** 23 trigger actions with the short ids of data-model.md §3.4;
  defaults `I`, `O`, `L`, `[`, `]` on `set_a`, `set_b`, `toggle_loop`,
  `nudge_earlier`, `nudge_later`; all others unbound (FR-005).
- **G3** On a fresh install the chords `I`/`O`/`L` are flagged
  (host-vs-bundled conflict, host wins); `[`/`]` are active. The panel's
  `set_a`/`set_b` buttons and `loop` toggle work regardless (FR-016;
  research R11).
- **G4** After registration the script performs a `relist()` (§4) so a
  plugin enabled or restarted mid-track shows the current track's
  markers immediately (FR-015).

## 3. Actions and panel interactions (A)

All positions are `api.playback.state().position_ms` at the moment the
handler runs. "Status" = the `status` text widget; "clear Status" =
`update_widget("main","status","")`.

| Trigger | Precondition | Calls (in order) | On success | On refusal |
|---|---|---|---|---|
| **A1** `set_a` (action or button) | — | `set_loop_endpoint(S.region, "a", pos)` | `S.active = "a"`; clear Status | Status = `err.message` |
| **A2** `set_b` | — | `set_loop_endpoint(S.region, "b", pos)` | `S.active = "b"`; clear Status | Status = `err.message` |
| **A3** `toggle_loop` → on (action, or `loop` toggle → true) | `S.region ~= nil` else Status = `@status_no_region`, `update_widget(loop,false)`, no call | `request_focus()`; `arm_loop(S.region)` | clear Status (toggle state follows `loop_armed`) | `update_widget(loop,false)`; Status = `@status_no_focus` if `code == "no_focus"` else `err.message` |
| **A4** `toggle_loop` → off | — | `request_focus()`; `disarm_loop()` | clear Status | `update_widget(loop,true)`; Status as A3 |
| **A5** `nudge_earlier` / `nudge_later` | target = `S.active` if its endpoint exists, else `b`, else `a`; none → no-op, no call | `markers.move(id, ms ∓ 10)` (floor at 0) | `S.active = target`; clear Status | Status = `err.message` |
| **A6** `set_cue_n` | — | `markers.set_cue(n, pos)` | clear Status | `reason == "not_owner"` → Status = `@status_cue_owned_n`; else `err.message` |
| **A7** `jump_cue_n` | `S.cues[n] ~= nil` (any owner) else silent no-op | `request_focus()`; `transport.seek(S.cues[n].ms)` | clear Status | Status = `@status_no_focus` on `no_focus`, else `err.message` |
| **A8** `clear_markers` | — | `markers.delete(id)` for `S.a`, `S.b`, every own `S.cues[n]` (no `disarm_loop`, no focus) | clear Status | Status = first `err.message` |
| **A9** `repeat` slider commit `v` | `S.region ~= nil` → `set_loop_repeat(S.region, v == 0 and "infinite" or v)`; else `S.pending_repeat = …` (applied right after the next successful region creation in A1/A2) | — | clear Status | Status = `err.message` |
| **A10** `snap` toggle / `toggle_snap` action | — | `update_widget("main","snap",false)` only | — | — |

- **A11** `request_focus()` in A3/A4/A7 is always the **first** call
  inside the handler (011 FR-026). A later `focus_granted` never arms,
  disarms or seeks by itself (FR-008; SC-007).
- **A12** No handler holds "pending" marker state: after any refusal the
  script's view is rebuilt from the next `marker_changed`/`relist()`,
  never from what it *intended* (FR-012; spec Edge Cases "never
  reinterprets the host's result").

## 4. Event mirroring (E)

- **E1** `relist()` = `markers.list()` → `S.region` is the region whose
  `owner` is this plugin (at most one; if several exist — e.g. after a
  host-side edit sequence — the lowest id is used and a console warning
  is logged); `S.a/S.b/S.a_ms/S.b_ms` from that region's endpoints;
  `S.cues[n]` from every `kind == "cue"` marker of any owner; `S.armed`
  from `regions[].armed`; slider ← `repeat` (`"infinite"` → 0); Loop
  toggle ← `armed`; overlays rebuilt (§5); `S.active` dropped if its
  endpoint is gone (FR-006, FR-012, FR-013).
- **E2** `track_changed` → `S.active = nil`; `relist()` (the host has
  already restored the track's markers and cleared the plugin's
  overlays).
- **E3** `marker_changed` → `relist()` regardless of `actor`.
- **E4** `loop_armed{region}` → `S.armed = (region == S.region)`;
  `update_widget(loop, S.armed)`. `loop_disarmed` → `S.armed = false`;
  `update_widget(loop, false)` — covers finite-repeat release, host `L`,
  endpoint deletion and focus-loss disarms (US1-4, US3-4).
- **E5** `focus_granted`/`focus_revoked` → log only.
- **E6** `loop_wrapped` → not subscribed to / ignored (FR-009: the plugin
  never counts wraps).

## 5. Overlays (O)

- **O1** After every `relist()` the overlay set is exactly: `a_line`
  (`line` at `a_ms`) and `a_label` (`label` `@label_a`) if A exists;
  `b_line`/`b_label` likewise; `ab_region` (`region` `a_ms→b_ms`,
  `accent`) only when both exist and `a_ms < b_ms`; per set cue slot
  `cue_n_dot` (`glyph` `dot`) + `cue_n_label` (`label` `@cue_label_n`).
  Never more than 21 primitives (FR-012).
- **O2** Rebuild = `clear_overlays()` then one `add_overlays(list)` (an
  empty list skips the add). Both are `ui`-bucket calls.
- **O3** On disable/suspend the host clears the set (011 FR-016); the
  host's native marker rendering keeps the markers visible (FR-014).

## 6. Lifecycle and isolation (L)

- **L1** Disable/suspend/crash while holding focus with an armed loop:
  host returns focus and disarms (010 FR-007); A/B/cues remain in the
  track store, listed and editable in the host Markers panel (FR-014;
  SC-003). No plugin code runs.
- **L2** Re-enable/Restart: G1–G4 run again from `ready_ack`; the
  session state (`S.active`, `S.pending_repeat`) starts empty (FR-015).
- **L3** Any handler throw/hang is contained by 009's budget; the
  markers already written are unaffected.

## 7. Named tests (`modplayer-core/tests/controller_section_loop.rs` unless noted)

- `bundled_section_loop.rs::{package_discovered_enabled_by_default, manifest_permissions_exact, script_calls_only_schema_requests, license_files_match_workspace, strings_cover_every_at_key}`
- `plugins_manifest_discovery.rs::fixtures_only_with_env` (~ now "Section Loop only without fixtures")
- `modplayer-ui/tests/plugins_view.rs::empty_state_without_fixtures` (~ now asserts the Section Loop row; empty state via an explicit empty host)
- `registers_panel_and_23_actions`, `fresh_install_iol_flagged_brackets_active`
- `panel_set_a_then_set_b_creates_owned_region` (US1-1), `set_b_before_set_a_then_swap` (EC)
- `loop_toggle_requests_focus_and_arms_same_action` (US1-2, SC-004), `loop_toggle_off_disarms` (US1-3)
- `finite_repeat_releases_and_toggle_follows` (US1-4, SC-005), `arm_incomplete_reverts_toggle_with_message` (US1-5)
- `nudge_moves_active_marker_10ms` (US1-6), `nudge_without_endpoints_noop`, `nudge_default_b_then_a`
- `markers_survive_reload_and_relist` (US2-1, SC-002), `disable_mid_loop_releases_and_keeps_markers` (US2-2, SC-003), `host_edit_reflected_via_marker_changed` (US2-5), `reenable_relists` (US2-4)
- `loop_toggle_takes_focus_from_other_plugin` (US3-1, fixtures env), `manual_policy_refuses_and_hints_no_deferred_arm` (US3-2, SC-007), `host_l_disarm_then_rearm` (US3-4), `jump_cue_requests_focus_then_seeks` (US3-5), `jump_cue_empty_slot_noop`
- `set_cue_on_host_slot_refused_with_hint`, `set_cue_own_slot_moves`, `clear_markers_deletes_own_only_and_disarms_hostside`, `snap_toggle_always_reverts`, `status_clears_on_success`
- `overlay_set_matches_markers` (O1), `overlay_cleared_on_disable` (O3)
- `modplayer-ui/tests/plugin_panels.rs::section_loop_panel_keyboard_and_names` (SC-006, AccessKit)

# Quickstart: validating Named Actions and Keyboard Shortcuts

**Feature**: 007-keyboard-actions-and-shortcuts | **Plan**: [plan.md](plan.md)

## Prerequisites

- Rust 1.95.0 (pinned by `rust-toolchain.toml`; if the shell exports
  `RUSTUP_TOOLCHAIN`, run with `RUSTUP_TOOLCHAIN=1.95.0`); `cargo deny`
  installed.
- For manual scenarios: a Spotify **Premium** account signed in through
  002's flow, network access, a macOS host for the constitution's
  Quartz-driven recipe (Governance › Manual Scenario Sign-Off), and a
  track of ≥ 60 s. Automated gates need none of these — every automated
  test runs on the synthetic/scripted source (Constitution IV) and an
  offscreen `egui::Context`.
- Nothing in this slice touches the engine, the audio source or the
  receiver; no real-time safety note is required.

## Automated gates (run from the repository root)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

Named tests that must exist and pass (full lists in the *Tests pinning
this contract* sections of [contracts/](contracts/)):

| Requirement | Test |
|---|---|
| FR-001 / FR-002 catalog of 44 trigger actions, stable ids | `modplayer-core tests/actions.rs::catalog_has_44_unique_ids_in_spec_order`, `::ids_round_trip_through_parse`, `::no_continuous_actions_in_this_slice` |
| FR-003 chord grammar, platform display, key vocabulary | `actions.rs::chord_parse_encode_round_trip` (proptest), `::chord_parse_rejects_bad_grammar`, `::chord_display_mac_and_other`, `modplayer-ui tests/actions.rs::key_name_table_matches_egui_key_all` |
| FR-004 defaults = spec table, conflict-free | `actions.rs::defaults_match_spec_table`, `::shipped_defaults_never_conflict` |
| FR-004a 5 s / 5 % steps, clamped | `modplayer-core tests/controller_actions.rs::seek_step_moves_five_seconds_and_clamps`, `::volume_step_moves_five_percent_and_saturates` |
| FR-005 / SC-011 same-frame, same owner | `modplayer-ui tests/actions.rs::invocation_lands_in_the_same_frame_as_the_key_event`, `::inherited_bindings_produce_identical_controller_calls` |
| FR-006 / SC-006 shortcut map, filter | `modplayer-ui tests/controls.rs::filter_matches_label_and_category_case_insensitively`, `::filter_no_match_shows_line`, `::every_action_listed_grouped_by_category` |
| FR-007 capture accept / reject / duplicate / conflict / cancel | `controls.rs::capture_accepts_chord_and_adds_chip`, `::capture_accepts_space_enter_arrows_function_keys`, `::capture_rejects_duplicate_tab_and_mac_control_inline`, `::capture_esc_and_focus_loss_cancel`, `::capture_of_conflicting_chord_flags_both_and_names_partner`; `modplayer-core actions.rs::add_duplicate_binding_is_rejected` |
| FR-008 zero bindings valid | `controls.rs::remove_chip_immediately_allows_zero_bindings`, `actions.rs::remove_last_binding_leaves_action_unbound_and_resolvable_by_nothing` |
| FR-009 / SC-003 symmetric conflicts, neither fires, disabled excluded, re-evaluated on enable | `actions.rs::conflict_is_symmetric` (proptest), `::resolve_returns_none_for_conflicting_chord`, `::disabled_action_never_conflicts_or_blocks`, `::enabling_action_flags_existing_collision`; `modplayer-ui actions.rs::conflicting_chord_fires_neither_until_resolved`, `::disabled_tempo_binding_does_not_block_and_flags_on_enable` |
| FR-010 scope-aware | `actions.rs::can_coexist_table_is_symmetric_and_all_true_for_nested_chain`, `::disjoint_scopes_never_conflict` |
| FR-011 reset per action / all, confirm rules | `actions.rs::reset_restores_defaults_without_touching_enabled`, `::reset_all_clears_every_conflict`; `controls.rs::reset_action_restores_default_without_confirm`, `::reset_all_two_step_confirm_cancel_esc_focus_loss` |
| FR-012 / SC-005 disabled rows greyed, rebindable, silent | `controls.rs::disabled_rows_greyed_rebindable_and_never_fire`, `actions.rs::disabled_action_never_resolves` |
| FR-013 / SC-004 sparse persistence, restart | `modplayer-core tests/settings.rs::keybindings_round_trip_is_sparse`, `::keybindings_table_absent_loads_defaults_silently`, `actions.rs::overrides_round_trip_through_raw_settings` (proptest) |
| FR-013 invalid entries isolated, warning, rewritten clean | `settings.rs::keybindings_invalid_entries_dropped_in_isolation`, `::keybindings_bad_entries_rewritten_clean_on_next_save`, `::whole_file_garbage_still_single_unreadable_warning` |
| FR-013 / SC-008 crash mid-write | `settings.rs::crash_mid_write_keeps_previous_keybindings` |
| FR-014 / NFR-6.1 accessible names, roles, states, Tab order | `modplayer-ui tests/accessibility.rs` (extended enumeration of Settings › Controls) |
| FR-015 / NFR-7.1 strings externalised | `modplayer-ui tests/fluent_keys.rs` (extended to `controls.ftl`; no unused keys) |
| FR-017 / SC-007 inherited bindings regression, sole `/` deviation | `modplayer-ui actions.rs::inherited_bindings_produce_identical_controller_calls`, `::slash_in_focused_text_field_types_and_does_not_focus_search`, plus the re-pointed 004/006 key tests in `tests/markers.rs`, `tests/now_playing.rs`, `shell.rs` |
| FR-018 / SC-009 scopes, transport app-wide | `actions.rs::transport_shortcuts_from_now_playing_and_library`, `::i_outside_now_playing_falls_through`, `::nudge_requires_focused_marker` |
| FR-019 / SC-010 one consumer per press, repeat | `actions.rs::focused_stop_button_wins_space_exactly_one_effect`, `::focused_waveform_lets_space_toggle`, `::focused_volume_slider_keeps_arrows`, `::held_space_toggles_once_held_volume_repeats`, `::text_field_focus_silences_every_action` |
| SC-001 / SC-002 whole catalog keyboard-reachable, I-O-L | `actions.rs::every_enabled_default_binding_dispatches`, `::i_o_l_arms_loop_with_no_pointer` |
| US1 AS11 `Q` queue toggle | `actions.rs::q_toggles_queue_panel_in_now_playing_only` |

Property tests use `proptest` (already a dev-dependency of both
crates); no new dependency, no new crate.

## Running one story's tests

```bash
cargo test -p modplayer-core --test actions
cargo test -p modplayer-core --test settings keybindings
cargo test -p modplayer-ui --test actions
cargo test -p modplayer-ui --test controls
cargo test -p modplayer-ui --test markers      # re-pointed 006 key tests must stay green
cargo test -p modplayer-ui --test accessibility
cargo test -p modplayer-ui --test fluent_keys
```

## Manual scenarios (executed by the implementing agent — Constitution Governance › Manual Scenario Sign-Off)

Recipe: `RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer`
in the background; locate the window with Quartz
`CGWindowListCopyWindowInfo`; drive with `CGEventPost`
(`CGEventCreateKeyboardEvent` + `CGEventSetFlags` for `⌘`/`⇧` chords,
key-down/key-up pairs; hold = repeated key-down events with the
`autorepeat` flag); capture with `screencapture -x -o -l <windowid>`;
helper scripts under `target/manual-walk/`. Use
`MODPLAYER_CONFIG_DIR=$(mktemp -d)` for M9/M10. Record pass/deviation
with evidence on the scenario task in `tasks.md`.

| # | Scenario (spec ref) | Steps | Expected |
|---|---|---|---|
| M1 | Transport from Now Playing (US1 AS1–AS6) | Play a track; press `⌘→`, `⌘←`, `⌘⇧→`, `⌘⇧←`, `⌘↑`, `⌘↓`, `Space`, `Space`, `⇧Space` | next, previous, +5 s, −5 s, +5 %, −5 % (volume readout), pause, resume, stop — each within one frame of the key, no mouse |
| M2 | Transport from Library (US1 AS8, SC-009) | Switch to Library (`⌘1`) with nothing focused; press `Space`, `⌘→`, `⌘↑` | pause, next track, +5 % — identical to M1 |
| M3 | I-O-L (US1 AS7, SC-002) | Now Playing, playing; press `I`, wait 5 s, `O`, `L` | A and B glyphs appear on both lanes, loop arms (span solid), audio loops — unchanged from 006 |
| M4 | Focused widget wins (US1 AS9, SC-010) | `Tab` to the "Stop" button; press `Space` | playback stops; play/pause does **not** also toggle; position readout 0:00 |
| M5 | Repeat rules (US1 AS10) | Hold `⌘↑` ~1 s; hold `Space` ~1 s | volume climbs 5 % per repeat event then stops at 100; play/pause flips exactly once |
| M6 | Queue toggle on `Q`; `⌘Q` still quits (US1 AS11) | Now Playing: press `Q`, `Q`; then `⌘Q` | queue panel opens, closes; app quits (relaunch afterwards) |
| M7 | Shortcut map, filter, rebind (US2 AS1–AS6) | `⌘5` → Controls; type `vol` in the filter; clear it; on "Toggle current loop region" activate **Add binding**, press `K`; `Tab` to `K`'s remove button and activate it; activate **Reset to default** | only Volume up/down under Transport; then whole catalog; chip `K` appears and `K` toggles the loop from Now Playing; chip removed; `L` restored |
| M8 | Conflict (US3 AS1–AS3, SC-003) | Controls: on "Toggle current loop region" **Add binding**, press `Space`; go to Now Playing, press `Space`; back to Controls, remove the loop's `Space` chip; Now Playing, `Space` | both rows show ⚠ "Conflicts with …"; `Space` does nothing (no pause, no loop change); flags clear; `Space` pauses again |
| M9 | Restart persistence (US4 AS1, SC-004) | Fresh `MODPLAYER_CONFIG_DIR`; rebind loop toggle to `K`; quit; relaunch; open Controls; `cat $MODPLAYER_CONFIG_DIR/settings.toml` | `K` still bound, everything else default; file holds exactly `[keybindings] "host.loop.toggle" = ["K"]` |
| M10 | Invalid entry isolation (US4 AS2) | With the app closed, add `"host.nope.x" = ["A"]` and `"host.transport.stop" = ["Ctrl+X"]` to the table; launch | one Warning "keybindings-invalid-entries" naming both ids; `K` still bound; after any settings change the file no longer contains the two bad entries |
| M11 | Disabled rows (US2 AS8, SC-005) | Controls: locate "Tempo step up"/"Tempo step down" | rows greyed with "(inactive)", chips `=`, `+` and `-` shown, **Add binding**/remove/reset operable; pressing `=` in Now Playing does nothing (waveform zoom only when the waveform has focus) |
| M12 | Capture rejections (US2 AS4/AS4a) | Controls: **Add binding** on "Stop", press `Tab`, then `⌃X` (physical Control), then `⇧Space` (its existing chord), then `Esc` | inline reasons "reserved for focus navigation", "Use ⌘ instead", "already bound"; capture stays open each time; `Esc` closes it with no change |
| M13 | Text field guard (FR-017 deviation, edge case) | Search view, focus the search box; type `/abc`; then `Space` | the box reads `/abc `; nothing else happens (no section jump, no pause) |
| M14 | Reset all (US2 AS7) | Rebind two actions; **Reset all to defaults** → **Cancel**; again → **Confirm** | first: nothing changes; second: both rows back to default, `[keybindings]` table gone from the file |

Deviations from the spec observed while running M1–M14 are recorded in
`tasks.md` on the scenario task and, where behaviour differs from the
spec, in this file and in [research.md](research.md).

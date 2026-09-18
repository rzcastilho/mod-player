# Quickstart: validating Markers, Loop Regions, and Cue Points

**Feature**: 006-markers-loops-and-cues | **Plan**: [plan.md](plan.md)

## Prerequisites

- Rust 1.95.0 (pinned by `rust-toolchain.toml`; if the shell exports
  `RUSTUP_TOOLCHAIN`, run with `RUSTUP_TOOLCHAIN=1.95.0`); `cargo deny`
  installed.
- For manual scenarios: a Spotify **Premium** account signed in through
  002's flow, network access, a macOS host for the constitution's
  Quartz-driven recipe, and a track long enough to hold a 30 s loop.
  Automated gates need none of these — every automated test runs on the
  synthetic/scripted source (Constitution IV).

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
| FR-001 create point/region markers, defaults | `modplayer-core tests/markers_model.rs::default_names_and_colors_by_kind`, `modplayer-ui tests/markers.rs::m_creates_point_marker_with_default_name_sorted`, `::i_then_o_creates_region_at_playhead_positions` |
| FR-002 / SC-005 64-marker limit, move paths exempt | `markers_model.rs::limit_is_64_across_all_kinds`, `::move_paths_never_hit_limit`, `markers.rs::sixty_fifth_marker_refused_inline`, `::shift_digit_on_occupied_slot_moves_at_limit` |
| FR-003 rename / recolor / move / delete | `markers.rs::f2_rename_commits_on_enter_cancels_on_esc`, `::c_cycles_palette_and_row_matches_glyph`, `::delete_focused_endpoint_makes_region_incomplete` |
| FR-004 / SC-004 marker focus keyboard table | `markers.rs::glyph_focus_arrow_nudges_by_setting_and_shift_ten_x`, `now_playing.rs::keyboard_table_matches_pointer_results` (extended), `accessibility.rs` |
| FR-004a shortcut scope | `markers.rs::shortcuts_inactive_while_rename_open` |
| FR-005 / SC-003 drag zoom-assist ≤ 5 ms | `markers.rs::drag_from_overview_zooms_detail_and_lands_within_5ms`, `::drag_esc_restores_position_and_window`, `waveform` unit `detail_window_zoom_assist_converges_to_target` |
| FR-006 current region, incomplete regions | `markers_model.rs::delete_endpoint_makes_region_incomplete_and_disarmed`, `::delete_last_endpoint_removes_region`, `markers.rs::i_then_o_creates_region_at_playhead_positions` |
| FR-007 / SC-006 swap, < 1 ms unarmable, crossfade shrink | `markers_model.rs::a_after_b_swaps_kinds_keeps_names`, `::region_under_1ms_is_not_armable`, `::region_3ms_is_armable_with_3ms_effective_crossfade`, `modplayer-engine tests/loop_seam.rs::short_region_shrinks_crossfade` |
| FR-008 one armed region, arm does not change transport, prefetch hint, uncached fallback | `markers_model.rs::only_one_region_armed`, `controller_markers.rs::arm_pushes_setters_then_commit_and_prefetch_hint`, `loop_seam.rs::uncached_seam_hard_cuts_and_reports_not_gapless` |
| FR-009 / Constitution I RT-only decision, no allocation | `modplayer-engine tests/realtime.rs::render_with_armed_loop_never_allocates`; every loop test drives only `Processor::render` |
| FR-010 / SC-002 / NFR-1.2 seam period exact after 1 000 wraps, any crossfade, A = 0 | `loop_seam.rs::period_is_exact_after_1000_wraps` (proptest), `::period_is_exact_under_resampling`, `::a_at_zero_hard_cuts` |
| SC-001 / NFR-1.3 click-free seam | `loop_seam.rs::seam_is_click_free` |
| FR-011 repeat count, wraps reset | `loop_seam.rs::repeat_count_releases_after_n`, `markers_model.rs::arm_resets_wraps`, `controller_markers.rs::loop_released_disarms_model` |
| FR-011a edits at next buffer boundary, seam completes | `loop_seam.rs::commit_is_atomic_across_renders`, `::edit_while_armed_applies_next_buffer_and_finishes_seam`, `controller_markers.rs::edit_armed_region_recommits_without_resetting_wraps` |
| FR-012 / SC-010 seek outside stays armed-inactive | `loop_seam.rs::seek_outside_keeps_armed_inactive`, `::natural_entry_from_before_a_activates` |
| FR-013 / FR-014 cues, jump keeps play state | `markers_model.rs::cue_slot_is_unique_and_set_moves`, `markers.rs::shift_digit_sets_cue_and_digit_jumps_keeping_state`, `::digit_on_empty_slot_is_noop` |
| FR-016 / SC-008 persistence & restore, armed not persisted, same-session reload | `markers_store.rs::round_trip_is_identity_for_any_state`, `controller_markers.rs::track_change_flushes_disarms_then_loads`, `::same_track_restart_reloads_and_clears_armed` |
| FR-017 / SC-009 / SC-014 / NFR-2.8 atomic write, crash mid-write, save failure | `markers_store.rs::crash_mid_write_keeps_previous_file`, `::writer_reports_save_failed_and_leaves_previous_file` |
| FR-016 read rules | `markers_store.rs::missing_file_loads_empty_without_warning`, `::unparseable_loads_empty_with_unreadable`, `::newer_schema_loads_empty_and_is_not_rewritten_until_mutation`, `::unknown_keys_ignored_and_out_of_range_clamped`, `controller_markers.rs::warnings_raised_once_per_load` |
| FR-018 / SC-012 shorter duration clamps + flags | `markers_model.rs::set_len_flags_clamped_markers_only`, `markers.rs::clamped_marker_shows_warning_glyph` |
| FR-019 clamp to end | `markers_model.rs::positions_clamp_to_len` (proptest) |
| FR-020 empty state | `markers.rs::empty_state_shows_press_i_hint` |
| FR-021 panel, clear-all two-step | `markers.rs::clear_all_two_step_confirm_and_cancel`, `::armed_region_shows_wraps_remaining`, `controller_markers.rs::clear_all_writes_empty_state_immediately` |
| FR-022 keyboard + accessible names | `modplayer-ui tests/accessibility.rs` (extended enumeration) |
| FR-023 strings externalized | `modplayer-ui tests/fluent_keys.rs` (extended; no unused keys) |
| FR-024 owner host only | `markers_store.rs::round_trip_is_identity_for_any_state` (owner always `host`), no API for other owners (review) |
| FR-026 overlays, span states | `markers.rs::overlay_paints_lines_and_span_states`, `::armed_inactive_badge_when_state_1` |
| FR-027 nudge step setting | `modplayer-core tests/settings.rs::nudge_step_setting_round_trips_and_clamps`, `settings::playback::nudge_step_drag_value_commits` |
| R5 Player follows the loop, EndOfTrack suppressed while active | `controller_markers.rs::loop_wrapped_reseeks_source_once_per_tick_then_throttled`, `::end_of_track_ignored_while_loop_active`, `::end_of_track_advances_while_loop_inactive` |
| R6 stream rebuild keeps the armed region | `controller_markers.rs::stream_rebuild_repushes_armed_region` |
| Constitution V | `modplayer tests/decoded_store_boundary.rs` unchanged (the engine's `read_frames` caller is in the allowed set) |
| Constitution IV | `modplayer tests/single_dependent.rs` unchanged; receiver `rt_feed::decoded_store_returns_current_track_store`, `worker::prefetch_hint_forwards_to_decode_ahead` |
| Constitution VIII proptests | `loop_math` (engine), `markers_model.rs::markers_stay_sorted_after_any_sequence`, `markers_store.rs::round_trip_is_identity_for_any_state`, `::track_id_hex_encoding_round_trips_and_is_case_free` |
| 001–005 regression | every existing test stays green; `command_is_copy_and_small` still passes with the new `Command` variants |

## Seam accuracy harness (engine, synthetic source)

The `loop_seam.rs` tests share one harness: build a `Processor<SyntheticSource>`
with `SyntheticSource::with_store(rate)` (the synthetic track fully
decoded into a `DecodedStore`), push `Play`, the four loop setters and
`LoopCommit`, then call `render` with a fixed device buffer until the
event queue has yielded N `LoopWrapped`s, asserting after each wrap that
`shared.clock_frames()` advanced by exactly `b − a` since the previous
wrap and that `processor.source().position() == wrap_position(a, b, clock)`.
The click test renders the same span twice — once looped, once by
seeking straight to `a − x` — and compares the max first-difference
across each seam window.

## Manual scenarios (signed-in Premium account; executed by the implementing agent per Constitution › Manual Scenario Sign-Off)

Recipe: constitution Governance › Manual Scenario Sign-Off (Quartz
`CGEventPost`, `screencapture`, helper scripts under
`target/manual-walk/`). Launch with a fresh track-state dir to start
clean: `MODPLAYER_TRACK_STATE_DIR=$(mktemp -d) ./target/debug/modplayer`
(and *without* it for M6). Record each result on its task in `tasks.md`.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | Loop a passage gaplessly (US1 AS1–AS2, SC-001) | Play a track; wait until the waveform under the playhead is solid (cached); press `I`, wait ~4 s, press `O`; press `L` | A/B brackets on both lanes; span solid; audio repeats the passage with no click or gap; wraps counter increments in the panel; `Cmd+3` Now Playing screenshot shows `loop-arm` checked |
| M2 | Repeat count releases (AS4) | Set repeat to 3 in the panel; press `L` twice (re-arm) | "3 remaining" counts down; after the 3rd wrap the toggle clears and playback continues past B |
| M3 | Second region disarms the first (AS5) | "New loop region"; `I`, `O` elsewhere; arm it from its row | first row's toggle clears; only the new span is solid |
| M4 | Swap and too-short (AS6–AS8, SC-006) | Focus A's glyph, `Shift+→` repeatedly past B | labels swap (A ≤ B); then nudge until `B − A < 1 ms`: `L` refused with `loop-region-too-short`; set crossfade 50 ms on a 20 ms region: arming works, seam audible as a short fade |
| M5 | Seek outside stays armed (SC-010) | With M1's loop armed, click the overview well after B | badge `loop-armed-inactive`, span hatched, no jump; click inside the region → jumps at B again |
| M6 | Persistence across relaunch (US2 AS1–AS2, SC-008) | Quit (`Cmd+Q`), relaunch **without** the temp dir override, play the same track | every marker/region/cue restored with names/colours; region **disarmed** |
| M7 | Crash mid-write (US2 AS3, SC-009) | With markers saved, `kill -9` the app right after a nudge (< 250 ms); relaunch | previous markers intact; a `<hex>.json.tmp` may remain and is ignored |
| M8 | Empty state and clear-all (AS5–AS6) | New track: panel reads "No markers — press I to set A"; create 3 markers; "Clear all markers" → "Clear 3 markers?" → confirm | panel returns to the empty state; relaunch shows none for that track |
| M9 | Unreadable state file (AS7) | Overwrite the track's `<hex>.json` with `{` and reload the track | one `Warning` "track-state-unreadable"; no markers; a nudge rewrites the file |
| M10 | Precise drag from the overview (US3 AS3, SC-003) | Drag a point-marker glyph on the overview lane | detail view zooms in and follows; released marker sits on the transient aimed at (verify in the panel time to the ms and by a zoomed screenshot); `Esc` mid-drag restores |
| M11 | Nudge setting (AS9) | Settings › Playback, nudge step 25; back on Now Playing, focus a marker, `→` | panel time advances exactly 25 ms; `Shift+→` 250 ms |
| M12 | Marker limit (AS6, SC-005) | Press `M` until 64 markers, then `M` again | inline "marker limit reached"; count stays 64; `Shift+1` on an occupied slot still moves it |
| M13 | Cues (US4, SC-007) | Playing: `Shift+3`, later `Shift+5`; press `3` — still playing from cue 3; pause; press `5` — paused at cue 5; press `7` — nothing | play state never changes on a jump; the jump is instant (no audible hesitation) |
| M14 | Uncached loop (AS9, SC-013) | Immediately after starting a long track, `I`/`O` far ahead in the hatched (undecoded) area, seek to just before A, arm | the first wrap may gap; the span turns solid and following wraps are gapless once the waveform there fills |
| M15 | Keyboard-only walk (FR-022) | `Tab` through lane glyphs and panel rows, `Enter` renames, `C` recolours, `Delete` removes, `L` arms — no pointer | every action reachable; VoiceOver reads role/name/time for each glyph |
| M16 | Real-time safety on device change | While looping, switch the output device in Settings › Audio | loop keeps looping after the rebuild; wraps count continues (not reset) |

## Manual walk 2026-09-17

macOS 12.6 x86_64, debug build, toolchain 1.95.0, live signed-in account,
real audio hardware. Full per-scenario results are recorded on T096 in
[tasks.md](tasks.md); PNG evidence and helper scripts live in
`target/manual-walk/` (gitignored). **14 pass, M13 pass-with-deviation,
M16 fail.**

### Driving the app from an agent shell (supersedes the 005 blocker)

005's walk was recorded as impossible because the agent shell is a
`launchctl managername == Background` session, where a bare
`./target/debug/modplayer` never gets a WindowServer connection. That is
true, but it is not the whole picture — **LaunchServices does attach the
process to the interactive Aqua session**:

1. Wrap the binary in a minimal bundle: `target/manual-walk/ModPlayer.app`
   with an `Info.plist` (`CFBundleExecutable = modplayer`) and
   `Contents/MacOS/modplayer` symlinked at `target/debug/modplayer`.
2. Launch with `open -n --stdout <log> --stderr <log> <bundle>`. The window
   is then listed by `CGWindowListCopyWindowInfo` and captured by
   `screencapture -x -o -l <window id>`.
3. Post events to the **HID tap** (`CGEventPost`) from a process that holds
   Accessibility rights. `CGEventPostToPid` delivers hover but never a
   click, and every event is dropped while the screen is locked.
4. Re-activate between steps with `open <bundle>` and confirm by z-order
   (the frontmost layer-0 window's owner pid). `NSRunningApplication::
   isActive` is stale in a process with no runloop.

### Deviations from the expectations above

- **M13** — the jump itself is correct while paused, but the position
  readout and playhead do not refresh until playback resumes (verified by
  resuming exactly at the cue). Pre-existing in the shared seek/position
  path — the engine only republishes position while rendering — and it
  affects an ordinary paused overview click-seek identically, so it is not
  a cue behaviour. Not fixed in 006.
- **M14** — not fully executable live. Decode-ahead fills a ~6 minute track
  within seconds, so the region is cached before the loop can be armed, and
  the `gapless` flag is not surfaced in the UI. The engine behaviour is
  pinned by `loop_seam.rs::uncached_seam_hard_cuts_and_reports_not_gapless`
  (`DecodeScript::Progressive`).
- **M15** — VoiceOver itself was not driven (enabling it takes over the
  whole desktop and is not safely reversible from a script). The roles and
  names it would read are asserted by `accessibility.rs`.
- **M16 — fails.** Any mid-session output-device change stops audio
  permanently (frozen position, silent peak meter), with or without a loop
  armed. Root cause is in 003's Connect receiver, not 006: `open_stream_on`
  re-`attach()`es and allocates a fresh sample ring, but the worker is
  spawned once and `handle_initialize` early-returns while it is alive, so
  the worker's `RingSink` keeps writing to the orphaned previous ring while
  the new RT half reads one nothing feeds. Deliberately left for a
  receiver-side follow-up (see T096's D3); M16 cannot pass until it lands.

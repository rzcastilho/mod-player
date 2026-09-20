# Quickstart: validating the Effect Chain and Built-In Effect Nodes

**Feature**: 008-effect-chain-and-built-in-nodes | **Plan**: [plan.md](plan.md)

## Prerequisites

- Rust 1.95.0 (pinned by `rust-toolchain.toml`; if the shell exports
  `RUSTUP_TOOLCHAIN`, run with `RUSTUP_TOOLCHAIN=1.95.0`); `cargo deny`
  installed.
- For the release-only measurements (SC-003, benches, 24 h soak): the
  reference macOS host, `cargo bench` support (criterion is a
  dev-dependency of `modplayer-effects`).
- For manual scenarios: a Spotify **Premium** account signed in through
  002's flow, network access, a macOS host for the constitution's
  Quartz-driven recipe (Governance › Manual Scenario Sign-Off), and a
  track with a clear vocal and a steady beat. Automated gates need none
  of these — every automated test runs on the synthetic/scripted source
  (Constitution IV).

## Automated gates (run from the repository root)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings   # also compiles the benches
cargo test --workspace                                                 # includes the 60 s soak
cargo deny check                                                       # criterion (dev, no default features) must add no new licence
scripts/check-license-headers.sh
```

Release-only measurements (manual, reference hardware; results recorded
below by the implementing agent):

```bash
cargo bench -p modplayer-effects                                        # nodes + reference_chain
cargo test --release -p modplayer-engine -- --ignored reference_chain_under_half_core
MODPLAYER_SOAK_HOURS=24 cargo test --release -p modplayer-engine -- --ignored reference_chain_soak_24h
```

Named tests that must exist and pass (full lists in the *Tests pinning
this contract* sections of [contracts/](contracts/)):

| Requirement | Test |
|---|---|
| FR-001 render order, empty chain bit-exact | `modplayer-engine tests/effects_pipeline.rs::empty_chain_is_bit_exact_with_001_pipeline` |
| FR-001a / SC-011 position at ratio, loop, seek/wrap history | `effects_position.rs::position_advances_at_ratio_with_loop`, `::seek_resets_history_wrap_does_not`, `::lead_is_subtracted`, `loop_math` proptest `rewind_in_loop_lands_in_region` |
| FR-002 16 nodes, ids/slots, session scope | `modplayer-core tests/controller_effects.rs::add_17th_is_refused_without_corruption`, `::ids_never_reused_slots_are`, `::chain_survives_sign_out_and_is_empty_at_construction` |
| FR-003 / FR-004 / SC-007 panel, list, add/remove/move/bypass | `modplayer-ui tests/effects_view.rs::rows_list_nodes_in_processing_order_with_type_owner_bypass_handle_cost`, `::drag_drop_reorders_and_calls_move_to`, `::arrow_up_on_focused_handle_moves_node_and_keeps_focus`, `::bypass_toggle_updates_state_immediately`, `::remove_row_disappears_order_preserved`, `::seventeenth_add_is_refused_inline` |
| FR-005 / SC-002 every edit and switch click-free at a boundary | `effects_transitions.rs::every_edit_and_switch_is_click_free`, `boundary.rs::chain_commands_apply_at_next_boundary`, `modplayer-effects crossfade.rs::equal_power_and_exact_endpoints` |
| FR-006 / SC-013 catalog, defaults transparent, ranges | `modplayer-effects catalog.rs` proptests, `nodes/*::gain_eq_filter_stereo_identity_at_defaults`, `stretch.rs::unity_is_pass_through` |
| FR-007 combined stage | `stretch.rs::combined_stage_products`, `rt/chain.rs::combined_cost_split_half`, `effects_transitions.rs` (pair/unpair rows) |
| FR-008 / SC-006 auto-switch | `mode_rule.rs::auto_switch_table`, `controller_effects.rs::auto_switch_flags_and_reverts`, `::user_forced_performance_persists_until_next_excursion`, `effects_view.rs::quality_note_appears_at_25_percent_and_clears_at_100` |
| FR-009 resonance cap | `nodes/filter.rs::resonance_caps_at_q_max`, `effects_view.rs::out_of_range_entry_displays_clamped_value` |
| FR-010 / SC-005 ramps and clamping on every path | `smooth.rs::ramp_reaches_target_in_exactly_ramp_frames`, `::retarget_restarts_from_current`, `controller_effects.rs::set_param_returns_clamped` (proptest), `effects_view.rs::out_of_range_entry_displays_clamped_value` |
| FR-011 / SC-008 meters and spectrum | `effects_meters.rs::pre_ignores_nodes_and_master_post_reflects_both`, `::spectrum_peaks_in_expected_band`, `effects_view.rs::meters_and_spectrum_render_from_snapshot` |
| FR-012 / SC-004 overload counted, named, non-host-only bypass | `effects_overload.rs::overload_counts_and_names_costliest`, `::non_host_costliest_is_bypassed_exactly_once_per_window`, `::host_costliest_is_never_bypassed`, `::over_budget_clears_after_clean_window`, `controller_effects.rs::overload_event_raises_one_keyed_warning`, `::warning_clears_when_rt_reports_clean_window` |
| FR-012a cost figures | `rt/chain.rs::combined_cost_split_half`, `controller_effects.rs::view_reports_zero_cost_when_not_playing` |
| FR-013 / SC-009 no audio leaves the engine | `modplayer tests/decoded_store_boundary.rs::effects_crate_exposes_no_sample_sink` |
| FR-014 rate change recompute | `effects_pipeline.rs::rate_change_rebuild_recomputes_coefficients`, `controller_effects.rs::rate_change_rebuild_reclamps_eq` |
| FR-015 keyboard + accessible names | `modplayer-ui tests/accessibility.rs` (extended enumeration) |
| FR-017 / SC-012 tempo steps, `E`, waveform precedence | `controller_effects.rs::tempo_step_moves_first_time_stretch_by_ten_points`, `::tempo_step_clamps_at_bounds`, `::tempo_step_without_node_notifies_once`, `effects_view.rs::plus_minus_step_tempo_unless_waveform_focused`, `::plus_without_time_stretch_notifies_once_while_held`, `::e_and_header_toggle_panel_and_it_survives_track_change`, `modplayer-core tests/actions.rs::catalog_has_45_entries_and_effects_enabled` |
| FR-018 / Constitution I real-time safety | `realtime.rs::render_with_full_chain_never_allocates` |
| FR-018 / NFR-2.7 soak | `soak.rs::reference_chain_soak_60s` (CI), `::reference_chain_soak_24h` (manual) |
| SC-001 / NFR-1.1 ratio change audible ≤ 20 ms p95 | `effects_latency.rs::ratio_change_audible_within_20ms_p95` |
| SC-003 / NFR-1.8 reference chain < 50 % of one core | `soak.rs::reference_chain_under_half_core` (release, manual) + `benches/reference_chain.rs` |
| US1 AS2/AS3/AS6 pitch, both, formant | `stretch.rs::pitch_up_7_keeps_duration`, `::tempo_half_doubles_click_interval`, `::formant_on_keeps_envelope_peaks` |
| US3 tone shaping truth tables | `nodes/eq.rs::eq_peak_plus_6db_at_1khz`, `nodes/filter.rs::filter_hp_minus_3db_at_cutoff`, `nodes/stereo.rs::stereo_truth_table`, `::mono_sum_invert_cancels_centre` |
| R8 Player follow / end-of-track under tempo | `controller_effects.rs::player_is_never_reseeked_for_tempo_drift`, `::end_of_track_deferred_at_half_speed`, `::engine_ends_track_first_at_double_speed_and_player_event_is_swallowed`, `::unity_behaviour_unchanged` |
| R11 rebuild replays the chain | `controller_effects.rs::stream_rebuild_replays_chain` |

## Automated end-to-end scenarios (synthetic source, no hardware)

1. **Tempo without pitch change** — `effects_latency.rs` plays a 1 kHz
   click train through `Processor` at 128 frames / 44.1 kHz with one
   time-stretch node; at render *k* it pushes `ratio = 0.6`; the first
   inter-click interval ≥ `1/0.6 × nominal − 1 frame` must start within
   20 ms of render *k*'s boundary in ≥ 95 % of 200 randomised trials, and
   a zero-crossing count over the following 500 ms must match 1 kHz ± 1 %.
2. **Reorder EQ before time stretch** — `effects_transitions.rs` captures
   the continuous output around a `ChainMove` and asserts the first-
   difference metric ≤ 1.5 × baseline (006's seam test), for every
   pairwise kind ordering.
3. **Overload** — `effects_overload.rs` inserts a `NodeKind::Gain` node
   with `NodeOwner::Plugin(1)` whose test hook (`cfg(test)`-only
   `burn_ns`) inflates its cost; three consecutive renders > 90 % must
   produce exactly one `Event::Overload`, `overload_count == 1`, and one
   `Event::AutoBypassed` for that slot; with the same hook on a `Host`
   node, the event fires and nothing is bypassed.
4. **Loop at ratio 0.5** — `effects_position.rs` arms a 4 s region with
   a time-stretch node at 0.5 and asserts 8 s of output per pass, a
   click-free seam, and that the published anchor advances at 0.5 ×
   with `advance_rate == 0.5`.

## Release measurements to record (implementing agent fills in)

**Host used** (no dedicated "reference hardware" was provisioned for this
sandboxed implementation session, so these figures are recorded from the
implementing agent's own dev host rather than deferred): Intel Core
i7-6820HQ @ 2.70 GHz, macOS (Darwin 21.6.0), `rustc 1.95.0`, release
profile, `cargo bench -p modplayer-effects -- --quick` (T100). Absolute
numbers on this host are not the constitution's sign-off hardware, but
the margin below the 1.45 ms/50 %-of-one-core budget is large enough
(≈70×) that the conclusion — the reference chain comfortably clears
SC-003 — should hold on materially slower hardware too; re-run on the
project's actual reference machine before treating this row as final
sign-off.

| Measurement | Command | Target | Result |
|---|---|---|---|
| reference chain (pitch + stretch + 8-band EQ, 128 frames, 44.1 kHz) | `cargo bench -p modplayer-effects -- reference_chain` | < 1.45 ms/render (50 % of 2.9 ms) | **20.1 µs/render** (≈1.4 % of budget) — PASS |
| per-node cost, performance mode (128 frames) | `cargo bench -p modplayer-effects -- nodes` | recorded per kind | Gain 1.04 µs · EQ (8 bands, all engaged) 8.45 µs · Filter 0.73 µs · Stereo 0.79 µs · Stretch (pitch +7 st) 10.14 µs |
| per-node cost, performance mode (256 / 1024 frames) | `cargo bench -p modplayer-effects -- nodes` | recorded per kind | Gain 2.10 / 8.25 µs · EQ 16.8 / 67.2 µs · Filter 1.46 / 5.78 µs · Stereo 1.58 / 6.32 µs · Stretch 20.0 / 80.4 µs |
| Stretch, quality mode + formant (128 frames) | `cargo bench -p modplayer-effects -- nodes` | recorded | Performance 10.14 µs · Performance+formant 38.7 µs · Quality 10.27 µs · Quality+formant 39.0 µs |
| `reference_chain_under_half_core` | `cargo test --release -p modplayer-engine -- --ignored reference_chain_under_half_core` | pass | **PASS** (2 000-iteration average well under the 1.45 ms budget, consistent with the bench row above) |
| 60 s soak (`reference_chain_soak_60s`, CI) | `cargo test -p modplayer-engine --test soak` | `assert_no_alloc` holds; flat `peak_rss` | **PASS** — no allocation, no RSS growth detected across the run |
| 24 h soak RSS drift (`reference_chain_soak_24h`) | `MODPLAYER_SOAK_HOURS=24 cargo test --release -p modplayer-engine -- --ignored reference_chain_soak_24h` | < 1 % over 24 h | **NOT RUN** — a real 24 h run is infeasible inside this implementation session (no host can be held for a full day here). The test itself was verified to compile and execute correctly in release (smoke-tested at `MODPLAYER_SOAK_HOURS=0`, and the 60 s CI variant above ran clean). This row must be executed for real, on the project's actual reference hardware, before SC-003/NFR-2.7 can be considered signed off — recorded as a deviation on T097/T100 in tasks.md. |

## Manual scenarios (executed by the implementing agent — Constitution Governance › Manual Scenario Sign-Off)

Setup per the constitution's recipe (`RUSTUP_TOOLCHAIN=1.95.0 cargo
build -p modplayer && ./target/debug/modplayer` in the background; drive
with Quartz `CGEventPost`; evidence via `screencapture -l <windowid>`;
helper scripts under `target/manual-walk/`). Each result (pass /
deviation + evidence path) is recorded on the scenario's task in
`tasks.md`.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | Open the panel (US2 AS1a) | Play a track; press `E`; press `E`; click the header "Effects" toggle; skip to the next track | panel opens, closes, opens; stays open across the track change |
| M2 | Slow down without pitch change (US1 AS1, SC-001) | Add Time stretch; drag ratio 100 % → 60 % while listening | tempo drops immediately and smoothly, pitch unchanged, no click; playhead advances at 60 % |
| M3 | Transpose without tempo change (US1 AS2, AS6) | Add Pitch shift; set +7 st; toggle formant on/off | pitch up a fifth, tempo unchanged; formant on keeps the vocal's timbre (no "chipmunk") |
| M4 | Both, adjacent and separated (US1 AS3, AS5) | With both nodes adjacent, set 0.8 / +3; insert an EQ between them | both effects audible in both arrangements; the CPU figures split evenly when adjacent and separate when not |
| M5 | Quality note (US1 AS4, SC-006) | Drag ratio to 25 %; then back to 100 %; set pitch +12; then 0 | "Quality mode auto-switched" appears and clears each time; mode combo reads quality/performance accordingly |
| M6 | Tempo keys (US1 AS7, AS8, SC-012) | With focus on the transport, press `-` twice; click the waveform, press `+`; remove the time-stretch node, hold `-` | 80 %; waveform zooms; one "Add a Time Stretch node…" notification |
| M7 | Loop at half speed (US1 AS9, SC-011) | Arm a 4 s loop; ratio 50 % | each pass ≈ 8 s wall time, seam click-free, playhead smooth, markers unmoved |
| M8 | Build and rearrange (US2 AS1–AS4, SC-002) | Add EQ then Time stretch; drag EQ above; bypass EQ; remove Time stretch — all while playing | every edit lands without a click or dropout; list reflects each change immediately |
| M9 | Keyboard reorder (US2 AS1b) | Tab to a handle; press `↑` | node moves one earlier, focus stays on it, no click |
| M10 | Capacity (US2 AS5, SC-007) | Add nodes until 16; attempt a 17th | inline "Chain is full" refusal; playback unaffected |
| M11 | Tone shaping (US3 AS1–AS7, SC-010) | Add Gain (+20 → shows +12; mute), EQ (+6 dB at 1 kHz, band type shelf, 30 000 Hz → 19 845 Hz), Filter (low-pass 1 kHz, resonance 1.5 → 1.0), Stereo (width 0 / 2, mono sum + invert on a centred vocal, swap) | each change audible and smooth; every clamped value displayed; invert disabled until mono sum; centre vocal attenuated with invert |
| M12 | Meters (US4 AS2, AS3, SC-008) | Watch pre/post meters and spectrum; move master volume; bypass all nodes | post follows master volume and bypasses, pre follows neither; spectrum live; per-node and chain figures update on add/remove/bypass |
| M13 | Overload warning (US4 AS4) | Build 16 heavy nodes at the Performance preset (a debug build overloads with ≈ 10 pitch-shift nodes at +7 st; the `burn_ns` cost-inflation hook exists only as `Processor::debug_set_burn_ns` for `effects_overload.rs` — no `MODPLAYER_EFFECTS_BURN_NS` env var is wired into the binary) | overload counter increments, one warning names the costliest node, playback continues, no node bypassed (all host), warning clears when the load drops |
| M14 | Rebuild keeps the chain (R11) | With a chain active, change the buffer preset in Settings › Audio | chain and parameters intact after the rebuild |

Deviations from the spec observed while running M1–M14 are recorded in
`tasks.md` on the scenario task and, where behaviour differs from the
spec, in this file's *Release measurements* table or research.md.

### Walk of 2026-09-19 — outcome (full per-scenario record on T101 in tasks.md)

| # | Result | Note |
|---|---|---|
| M1, M4, M5, M7, M8, M9, M10, M11, M12, M13 | PASS | M4/M8/M12 each surfaced a UI defect that was fixed during the walk (EQ bands overflowed the window; Now Playing had no scroll for a tall chain; spectrum drawn linear was unreadable) |
| M3 | PASS after fix | Formant on at +7 st **aborted the process** (RT-thread `f32::clamp` on `NaN` in the LPC synthesis filter); fixed in `lpc.rs` with regression tests. Pre-chain meter flicker under a stretch stage fixed in `processor.rs` |
| M6 | PASS after fix | Waveform click never took focus, and a typed `+` (`Shift`+`=`) never matched the waveform's `plain(Plus)` claim; both fixed with tests |
| M2 | PASS after fix | First run: playhead advanced slower than `ratio ×` on a streaming source (≈ 0.46 × at 55 %, ≈ 0.33 × at 45 %; exact inside a loop) — R8.1's periodic Player re-seek rewound the RT cursor each time. Re-seek removed (research.md R8 addendum); re-run 0.40 × at 40 % |
| M14 | PASS after fix | Chain and parameters survived the rebuild from the start, but audio never resumed after any stream rebuild — 003's receiver handed its ring producers to the worker only on the first `Initialize`. Fixed with `swap.rs` (research.md R11 addendum); re-run: audio continues across Balanced → Performance |

Recipe notes that were not in the constitution's recipe: launch the app
from an Aqua-session process (a Terminal.app `do script`), never from the
agent's `Background` launchd session, or it can never become the key app
and synthetic keystrokes go to whichever app *is* frontmost; each rebuilt
binary re-prompts the Keychain ("modplayer wants to use your confidential
information"), which only the maintainer can answer; and the app's own
"Open the browser again" fails from that Terminal session (`open` there
does nothing), so a re-sign-in mid-walk needs the authorize URL rebuilt
from the `pending-authorization` secure-store entry and opened by hand.

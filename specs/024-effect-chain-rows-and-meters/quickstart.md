# Quickstart & Validation: Effect Chain Rows and Meters (024)

**Contracts**: [ui-effect-chain-rows.md](./contracts/ui-effect-chain-rows.md) ·
[ui-chain-meters.md](./contracts/ui-chain-meters.md) ·
[fluent-strings.md](./contracts/fluent-strings.md) ·
**Data model**: [data-model.md](./data-model.md)

## Prerequisites

- Worktree on branch `024-effect-chain-rows-and-meters`.
- Toolchain: run cargo with `RUSTUP_TOOLCHAIN=1.95.0` (or
  `env -u RUSTUP_TOOLCHAIN`) — constitution, Manual Scenario Sign-Off.
- macOS host for the manual scenarios (Python with `pyobjc-framework-Quartz`,
  `screencapture`); helper scripts in `target/manual-walk/` (gitignored).

## 1. Automated gates

```bash
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test -p modplayer-ui
rtk cargo test --workspace
rtk cargo deny check
```

Expected: all green. Tests that must exist and pass (names indicative):

| Area | Test (file) | Proves |
|---|---|---|
| Zones | `rows_render_four_zones_in_order` (`tests/effects_view.rs`) | FR-001, SC-001: per row, handle < bypass < first param < Remove in x/y order; notes not after first param |
| Zone break rule | `zone_breaks_*` unit tests (`effects_view.rs`) | R1.2/R1.3, FR-016 |
| Units | `unit_suffix_inside_value_for_all_six_kinds` (`tests/effects_view.rs`) | FR-002, SC-002 (e.g. pitch shift +2 → value text "2.00 st") |
| Handle | `handle_name_identifies_node_and_position`, `handle_names_update_after_reorder` | FR-003/FR-004/FR-013, SC-003 |
| Existing reorder | `drag_drop_reorders_and_calls_move_to`, `arrow_up_on_focused_handle_moves_node_and_keeps_focus` (updated to prefix lookup) | FR-004 regression |
| Focus order | `tab_order_runs_identity_state_params_actions` | FR-013, SC-007 |
| Header | `header_figures_are_budget_labeled` (`tests/effects_view.rs`, `tests/accessibility.rs`) | FR-006, SC-004 |
| Readout width | `format_db_is_fixed_width` (`chain_meters.rs` unit) | FR-007 |
| Spectrum axes | `tick_label_spans_never_overlap_160_to_420`, `freq_to_frac_matches_bar_mapping`, `db_to_frac_inverts_bar_height` | FR-008, SC-005 |
| Spectrum bands | `spectrum_segments_band_boundaries` (`chain_meters.rs`), spectrum case in `tests/meter_bands.rs` | FR-009 |
| Empty state | `empty_chain_shows_explanation_and_single_primary`, `empty_primary_reveals_combo_and_focuses_it`, `empty_state_resets_after_last_node_removed` | FR-010/011/012, SC-006 |
| Narrow | `rows_fit_panel_at_960_px` | FR-016 |
| Strings | `fluent_keys.rs` updated lists + en-US ↔ pt-BR parity for this feature's keys | FR-015, NFR-7.1 |
| No behaviour change | full existing `effects_view.rs`, `controller_effects.rs` suites green | FR-014 |

## 2. Run the app

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer
```

Sign in (live account, per constitution), go to Now Playing, press `E`
to open the Effect chain panel. Locate the window via Quartz
`CGWindowListCopyWindowInfo` (owner `modplayer`, name `ModPlayer`);
capture evidence with `screencapture -x -o -l <windowid> <png>`.

## 3. Manual scenarios (executed by the implementing agent)

| # | Steps | Expected |
|---|---|---|
| M1 | Fresh chain (0 nodes), panel open | Header "Chain CPU:   0 % of real-time budget", "Budget overruns: 0"; level pairs + spectrum with 0/−30/−60 and 100/1k/10k labels visible; explanation text and one filled "Add effect node" button; no kind dropdown |
| M2 | Tab to "Add effect node", press Enter | Kind combo + primary "Add" appear, combo has focus ring; still exactly one filled button |
| M3 | Choose Pitch shift → Add; add Gain | Two rows; each shows identity │ state │ parameters │ actions with separators; pitch shift value reads "… st" inside the slider; Remove separated by a gap; explanation gone, ordinary "Add node…" row below |
| M4 | Hover the "⠿" handle; drag row 2 onto row 1 | Grab cursor on hover, grabbing during drag; after drop rows swap and position numbers read 1., 2. in new order |
| M5 | Focus a handle, press `↑`/`↓` | Node moves, focus stays on its handle, numbers update; VoiceOver (optional) reads "Reorder Gain, position 1" |
| M6 | Play a track loudly with Gain +12 dB | Spectrum bars turn amber above −6 and show a red cap at 0 dB, touching the labeled 0 dB line; level readouts do not shift horizontally as values change |
| M7 | Hover the "Chain CPU" figure | Tooltip explains callback-time share and the 90 % / 100 % overrun rule |
| M8 | Resize window to 960 × 640, add Equalizer + Stereo tools | Zones wrap as whole units; nothing overlaps or paints outside the card; long labels ellipsize with full text on hover |
| M9 | Remove all nodes | Explanation + collapsed "Add effect node" return in the same frame (not the revealed combo) |
| M10 | Tab through a full row | Order: handle → Bypass → params → Remove → next handle |

Record pass/deviation with screenshot paths on the scenario task in
`tasks.md`; record any spec deviation in this file and `research.md`.

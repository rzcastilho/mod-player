# Quickstart: Section Loop Bundled Plugin

**Feature**: 012-section-loop-plugin | **Date**: 2026-09-20
Validation guide only — design in [plan.md](plan.md),
[data-model.md](data-model.md) and [contracts/](contracts/);
implementation detail belongs to `tasks.md`.

## Prerequisites

- Rust 1.95.0 (`rust-toolchain.toml`; if the shell exports
  `RUSTUP_TOOLCHAIN`, run with `RUSTUP_TOOLCHAIN=1.95.0`).
- C++ toolchain for the vendored Luau build (unchanged from 009).
- macOS host with a signed-in Premium account for the manual scenarios
  (Constitution Governance › Manual Scenario Sign-Off); a track with an
  identifiable ≥ 8 s passage.
- No env var is needed to see Section Loop — it is a bundled package.
  `MODPLAYER_PLUGIN_FIXTURES=1` is needed only for M6 (a second
  transport-controlling plugin).

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
| `cargo test -p modplayer-capability-gateway --test api_reference` | `docs/plugin-api/v1.md` regenerated for 1.3 | FR-017, Constitution IX |
| `cargo test -p modplayer-capability-gateway --test gateway` | both new calls need `markers.write`, never focus | FR-017 |
| `cargo test -p modplayer-plugin-runtime --test bindings` | `which`/`repeat` validation; `list().regions` shape | FR-017 |
| `cargo test -p modplayer-core --test markers_model` | `set_loop_endpoint_owned` create/move/swap/clamp/limit; host `I`/`O` unchanged; proptest | FR-007, FR-017 |
| `cargo test -p modplayer-core --test controller_markers` | owner refusals, armed recommit without wrap reset, snapshot `regions` | FR-017, SC-005 |
| `cargo test -p modplayer-core --test bundled_section_loop` | package discovered/enabled, manifest permissions exact, licence copies, `@key` coverage, **schema-only call scan** | FR-001–FR-003, FR-018, SC-008 |
| `cargo test -p modplayer-core --test controller_section_loop` | every contract §7 scenario end to end on `FakeBackend` + `SyntheticSource` | US1–US3, all FR, SC-001–SC-005, SC-007 |
| `cargo test -p modplayer-ui --test plugin_panels` | Section Loop panel keyboard-operable with accessible names | SC-006 |
| `cargo test -p modplayer-ui --test plugins_view` / `-p modplayer-core --test plugins_manifest_discovery` | list shows Section Loop without fixtures; empty state only for an empty host | research R5 |

Expected: every suite green on ubuntu / macos / windows; the 009/010/011
fixture suites pass **unchanged** (SC-008 second clause).

## Launch for manual scenarios

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer &
# M6 only:
MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer &
```

Drive with the Governance recipe (Quartz `CGEventPost`, `screencapture
-l <windowid>`); helper scripts under `target/manual-walk/`. Evidence:
one screenshot per scenario plus the app log lines tagged
`plugin org.modplayer.section-loop`.

## Manual scenarios (executed by the implementing agent; results recorded in tasks.md)

| # | Steps | Expected | Spec |
|---|---|---|---|
| **M1** Fresh-install panel drill | Play a track. Open Now Playing; the docked "Section Loop" panel is present. At ~1:02 click **Set A**; at ~1:10 click **Set B**; flip **Loop** on. | A and B glyphs appear on the waveform (host rendering) plus the plugin's A/B lines, labels and accent region in the plugin lane; the marker list shows A and B; playback wraps at B to A with no audible gap/click; Loop shows on. Flip Loop off → playback continues past B. Under 10 s from first click to loop. | US1-1/2/3, SC-001 |
| **M2** Repeat count | With A/B set, drag the **Repeat count** slider to 4, flip Loop on. | After exactly 4 wraps the loop releases, Loop shows off, playback continues; never a 5th wrap. | US1-4, SC-005 |
| **M3** Keyboard after rebind | Settings › Controls: the plugin's `I`/`O`/`L` rows show a conflict with the host's; rebind Section Loop `toggle_loop` to `Shift+L`. Back in Now Playing press `]` three times, then `Shift+L`. | B moves 30 ms later (overlay/list follow); `Shift+L` arms the loop. Host `L` still toggles the host's own current region. | US1-6, FR-006, FR-016 |
| **M4** Persistence | With A, B, cue 1 (bind `set_cue_1` temporarily, press it) and repeat 4 set, quit the app; relaunch; play the same track. | A, B and Cue 1 are at their positions; the slider shows 4; flipping Loop on arms the restored region. | US2-1, SC-002 |
| **M5** Disable mid-loop | Loop armed and Section Loop holding focus (Transport panel shows it). Settings › Plugins → disable Section Loop. | Focus returns to Host immediately, loop releases, panel disappears, plugin-lane decorations vanish, **A/B/cue markers stay** on the waveform; open the host Markers panel and rename A → works. Re-enable → panel returns listing the same markers. | US2-2/3/4, SC-003 |
| **M6** Focus contention (fixtures) | Launch with fixtures; give focus to `focus-b` from the Transport panel (default policy). Flip Section Loop's Loop on. Then switch policy to Manual, take focus back, flip Loop on again; then "Give focus" to Section Loop from the Transport panel. | First flip: focus moves to Section Loop and the loop arms in one action. Manual: toggle reverts, Status reads "Needs transport focus — give Section Loop focus in the Transport panel (T)"; after Give focus nothing arms until Loop is flipped again. | US3-1/2, SC-004, SC-007 |
| **M7** Cues and ownership | Set a host cue in slot 3 with the host's own cue shortcut; press Section Loop's bound `set_cue_3`; then bound `jump_cue_3`. | Status reads "Cue 3 belongs to the host — move or delete it in the Markers panel"; jump seeks to the host's cue 3 (play state unchanged). | FR-010, EC |
| **M8** Snap and clear | Flip **Snap to beat**; then invoke a bound `clear_markers` while the loop is armed. | Snap flips straight back off beside the explanation label. Clear: A, B and the plugin's cues vanish, the loop releases, host-owned markers on the track remain; focus is unchanged. | FR-011, EC |
| **M9** Accessibility | VoiceOver on; Tab through the Section Loop panel. | Every control is reachable and announced by its label ("Set A", "Set B", "Loop", "Repeat count (0 = infinite)", "Markers", "Snap to beat", "Status"); the marker list rows are selectable by keyboard and seek. | SC-006 |

A deviation from any expected column is recorded on the scenario's
task in `tasks.md`, fixed with a regression test, and — if the
behaviour is what the spec should have said — echoed into
`research.md`.

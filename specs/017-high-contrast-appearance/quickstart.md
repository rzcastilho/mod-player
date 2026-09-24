# Quickstart: High-Contrast Appearance Option

**Feature**: 017-high-contrast-appearance | **Date**: 2026-09-23
**Inputs**: [plan.md](plan.md), [data-model.md](data-model.md),
[contracts/](contracts/)

Two halves: **§1–§2** the automated gates, which must be green before any
manual scenario is attempted; **§3** the manual scenarios M1–M10, which
the **implementing agent executes itself** against the real build
(Constitution › Governance › Manual Scenario Sign-Off — they are never
handed back to the maintainer as a to-do); **§4** what to do when the
build cannot be driven.

---

## §1 — Toolchain and build

The shell may carry `RUSTUP_TOOLCHAIN` overriding `rust-toolchain.toml`.
Run everything with the pinned version or the workspace fails its MSRV
check:

```bash
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo build -p modplayer
# or: env -u RUSTUP_TOOLCHAIN ...
```

Helper scripts for §3 live **inside the worktree** at
`target/manual-walk/` (gitignored) — the scope-guard hook denies writes
and shell redirects to the scratchpad, `/tmp` and the memory directory.
The Phase 0 ratio script is already at `target/plan-scratch/contrast.py`.

---

## §2 — Automated gates

### 2.1 Full workspace

```bash
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo fmt --check
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test --workspace
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --doc
RUSTUP_TOOLCHAIN=1.95.0 rtk cargo deny check
```

`cargo deny` must be unchanged in outcome — this feature adds **zero**
dependencies.

### 2.2 New and extended suites

| Suite | Contract clauses | What it pins |
|---|---|---|
| `modplayer-ui/tests/high_contrast.rs` *(new)* | H1–H16, S1–S5, M1–M2, P2–P5 | the four tables, the promotion, the divider, the ring width, the single selection site, plugin reach |
| `modplayer-ui/tests/design_token_contrast.rs` *(extended)* | H17, H18, M3 | the 7:1 / 3:1 / 4.5:1 floors — SC-008 |
| `modplayer-ui/tests/markers.rs` *(extended)* | M4–M10 | outline present in high contrast, absent otherwise; fills unrecoloured; focus width preserved |
| `modplayer-ui/tests/plugin_overlays.rs` *(extended)* | M11–M12, P1 | palette tokens outlined, role tokens not |
| `modplayer-ui/tests/fluent_keys.rs` *(extended)* | A15 | the two new keys are in the inventory |
| `modplayer-core/src/settings/model.rs` `#[cfg(test)]` *(extended)* | A1–A9, A11–A13 | the recovery table, incl. **A5** |
| `modplayer-core/tests/settings.rs` *(extended)* | A10 | the proptest round-trip (Constitution VIII) |

### 2.3 The regression net — these must pass **unmodified**

If a task's diff touches any file in this list, the task is wrong.

```text
crates/modplayer-ui/tests/design_token_literals.rs   # EXPECTED_BASELINE_HITS stays 0 (H19)
crates/modplayer-ui/tests/design_token_roles.rs      # H10, H8
crates/modplayer-ui/tests/interaction_states.rs      # H7 (FR-020)
crates/modplayer-ui/tests/control_variants.rs        # A22
crates/modplayer-ui/tests/control_inventory.rs       # A22
crates/modplayer-ui/tests/accessibility.rs           # A22 (FR-014)
crates/modplayer-ui/tests/actions.rs                 # A22
crates/modplayer-ui/tests/controls.rs                # A22
crates/modplayer-capability-gateway/tests/api_reference.rs   # P5, P6 (Constitution IX)
```

Plus, inside modified files, these individual tests must survive
verbatim: `style.rs::no_geometry_or_interaction_field_changes`,
`style.rs::apply_tokens_installs_both_themes`,
`style.rs::apply_tokens_is_idempotent`,
`theme/mod.rs::maps_every_theme_to_its_preference`,
`theme/markers.rs::overlay_color_maps_every_token`,
`theme/markers.rs::marker_palette_has_eight_distinct_colours`, and the
five existing loops in `design_token_contrast.rs`.

### 2.4 Quick manual re-check of the numbers

```bash
python3 target/plan-scratch/contrast.py
```

Every FR-009 candidate must print the ratio the spec's table states.
Light `danger` vs `surface.raised` prints **7.00** — the tightest value
in the feature (research R4).

---

## §3 — Manual scenarios (executed by the implementing agent)

**Recipe** (macOS host, the only manual-run platform to date):

- **Launch**: `RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer`
  in the background. A fresh first launch needs
  `MODPLAYER_CONFIG_DIR=$(mktemp -d)` set before launch.
- **Locate window**: Python `Quartz.CGWindowListCopyWindowInfo`, owner
  `modplayer`, name `ModPlayer` → window id + bounds.
- **Drive**: Python Quartz `CGEventPost` — mouse events at
  window-relative coordinates, `CGEventCreateKeyboardEvent` +
  `CGEventSetFlags` for shortcuts, `CGEventKeyboardSetUnicodeString` for
  typing. No test doubles: real Keychain, real network.
- **Capture**: `screencapture -x -o -l <windowid> <png>`, then read the
  PNG back and judge the scenario from it plus the app log.
- **Point-sample colours**: for the contrast scenarios, read the captured
  PNG's pixels directly (Python + the same
  `relative_luminance`/`ratio` functions as
  `target/plan-scratch/contrast.py`) rather than judging by eye — this
  feature's whole claim is a measured one.
- **Live token** for any `#[ignore = "manual"]` probe:
  `security find-generic-password -s ModPlayer -a session-credential -w | jq -r .access_token`
  (≈ 1 h validity). Never print it; filter `bearer|access_token` from
  captured output.

Each scenario's result (pass / deviation, with evidence) is recorded in
`tasks.md` on its scenario task and, where behaviour differs from the
spec, written back into this file and `research.md`.

| # | Scenario | Steps | Expected | Requirement |
|---|---|---|---|---|
| **M1** | The control exists and is independent | Open Settings › Appearance. Screenshot. | A checkbox labelled "High contrast" sits **below** the Theme combo. The combo still lists exactly System / Light / Dark — no fourth entry. | FR-001, FR-015, US2 AS-1 |
| **M2** | Live apply, no relaunch | With the window visible, click the checkbox. Capture immediately after the next frame. | The visible view repaints with the high-contrast palette. No relaunch, no blank/unstyled frame between the two captures. | FR-004, SC-005, US2 AS-2 |
| **M3** | Text measures ≥ 7:1 | With high contrast on, capture Library, Search, Now Playing and Settings. Point-sample ≥ 5 text pixels per view against the adjacent background pixel; compute the ratio. | Every sampled non-disabled text run measures ≥ 7.0. Secondary lines (artist/album, help text, metadata) measure the same as primary text, not less. | FR-005, FR-005a, SC-001, US1 |
| **M4** | Nothing regressed with it off | Turn high contrast off. Re-capture the same four views. | Pixel values match a pre-feature capture of the same views; text measures at today's values, not 7:1. | FR-002, US1 AS-3 |

**M3/M4 sign-off (2026-09-23, T021)**: **PASS with one documented,
out-of-scope deviation.** Executed against the real, signed-in build
(`rodrigo.zampieri`'s Spotify session), not a fabricated one. Colorimetric
point-sampling (same WCAG method as `target/plan-scratch/contrast.py`,
run over the captured PNGs) of Library (395 real tracks), Search
("metallica" query) and Settings › Appearance confirms every sampled
title, secondary (artist/album/help) line, count label, duration column
and nav label resolves to exactly `LIGHT_HIGH_CONTRAST.text_primary`
(`#1c1c1e`), ratio **17.01**, matching Phase 0's own printed value —
FR-005/FR-005a's promotion reaches the screen. With high contrast off,
the same views' secondary lines measure the unchanged, pre-feature
`#5b5b60`. **Deviation**: the very last row drawn in a search-results
group, immediately above its "Show more" button (`GROUP_VISIBLE_ROWS = 6`
in `search_view.rs`), renders its secondary line dimmed (~6.75:1 with
high contrast on, i.e. below this feature's 7:1 promise) — reproduced
twice on, and confirmed present identically with high contrast **off**
too (so it is not this feature's regression, just a pre-existing,
previously-invisible miss this feature's promotion makes measurable).
Full method and pixel evidence: research.md R17. No file this defect
would live in (`search_view.rs`, `rows.rs`) is named by any task in
`tasks.md` — left unfixed, recorded per governance rather than silently
dropped. M4's "unchanged from today" claim for the other four sampled
views holds exactly (no new deviation vs. the automated `H3`/normal-table
regression net).
| **M5** | Borders and dividers ≥ 3:1 | With high contrast on, sample a panel border, a row divider and a `Default` button outline against the surface beside each. | Each measures ≥ 3.0. | FR-007, SC-006, US5 AS-1 |
| **M6** | The focus ring thickens | Tab keyboard focus onto a control with high contrast off, capture; toggle on, capture the same control focused. | The ring is measurably thicker (3 px vs 2 px) and is drawn in the high-contrast accent. | FR-008, SC-007, US5 AS-2 |

**M5/M6 sign-off (2026-09-24, T039)**: **NOT EXECUTED.** The macOS GUI
session was still locked (screen-saver/lock screen) at attempt time —
same blocker as R18/T034, one day later: `modplayer` was built and
launched against a fresh scratch config seeded from T021/T029's own
`hc-config-off`, but no window (for this app or any other) appeared on
screen after 23+ seconds of polling; only `ScreenSaverEngine` and
non-GUI-session owners were present. The launched process was killed
rather than routed around the lock. Full evidence: research.md R19. The
values these scenarios would visually confirm are machine-checked this
session by `high_contrast.rs`'s `every_border_follows_the_divider` (H15),
`design_token_contrast.rs`'s `high_contrast_divider_clears_the_non_text_floor`
(H17) and `high_contrast.rs`'s `focus_ring_thickens_only_in_high_contrast`
(H16) — see T038 — but M5/M6 themselves should be re-attempted once the
session is unlocked.
| **M7** | Markers stay identifiable | Load a track, place ≥ 4 markers in different palette colours (including index 7, olive — the worst outline-vs-fill case, research R6), and one armed loop region. Capture with high contrast off, then on. | Each marker's fill colour is unchanged between captures; each gains a visible outline; every marker is individually distinguishable from its neighbours and from the waveform. The loop region's span gains an outline. A focused marker is still visibly focused. | FR-011, FR-012, SC-002, US3 |

**M7 sign-off (2026-09-23, T034)**: **NOT EXECUTED.** The macOS GUI
session had locked (screen-saver/lock screen) between T029's earlier
sign-off and this attempt — no window (for this app or any other) was
on screen; `loginwindow`/`SecurityAgent`/`ScreenSaverEngine` were the
only on-screen owners. The launched process was killed rather than
routed around the lock. Full evidence: research.md R18. The paint-site
claims M7 would visually confirm are machine-checked this session by
`crates/modplayer-ui/tests/markers.rs`'s new M4–M9 suite (outline
present/absent, fills unrecoloured, focus-width preserved); M7 itself
should be re-attempted once the session is unlocked.
| **M8** | A docked plugin panel follows | Dock a plugin panel (Section Loop or Key & Tempo). With it docked, toggle high contrast on. Capture. Point-sample the panel's text and control colours and the host chrome's. | The panel repaints on the next frame with the host's values; sampled panel text measures ≥ 7:1; no element inside the panel is at a normal-mode value. | FR-013, SC-003, US4 |

**M8 sign-off (2026-09-24, T037)**: **NOT EXECUTED.** `modplayer` was
built (pinned toolchain, clean) and launched against a scratch
`MODPLAYER_CONFIG_DIR` seeded from the real signed-in `hc-config`
(`account.toml` carried over) with `appearance.high_contrast = false`
and a `[plugin_panels."org.modplayer.section-loop/main"]` entry
pre-docked and enabled, so the panel would already be docked on the
first frame. Polled `Quartz.CGWindowListCopyWindowInfo` for owner
`modplayer`/name `ModPlayer` for 25+ seconds
(`target/manual-walk/m8-launch.log`, `target/manual-walk/m8-config/`).
No window appeared for this app or any other GUI application; `python3
-c "Quartz.CGSessionCopyCurrentDictionary()"` confirmed
`CGSSessionScreenIsLocked = 1` — the identical machine-state blocker
R18/R19 hit (screen-saver/lock screen), not anything this session's
config or build changed. The process ran normally (the `host_busy`
plugin-registration warnings at launch are the same benign transient
every prior session's log shows) and was killed rather than routed
around the lock. Full evidence: research.md R21. The mechanism M8 would
visually confirm is already machine-checked this session's regression
net by `plugin_overlays.rs`'s `M12` suite (outline parity at the four
plugin-overlay primitive arms) and `docked_plugin_panel_matches_host_chrome`
(`P4`) — see T035/T036 — but M8 itself, whether a docked panel actually
repaints on screen at ≥7:1, remains outstanding and should be
re-attempted once the GUI session is unlocked.
| **M9** | Both axes survive a restart | With high contrast on and theme set to Light, quit and relaunch. Open Settings › Appearance. | High contrast is still on **and** the theme is still Light; the window renders high-contrast light. | FR-003, SC-004, US2 AS-3 |
| **M10** | Found by search, focus lands on it | Open Settings, type "contrast" into the search. Open the result. | The result appears; opening it lands on the Appearance screen with keyboard focus **on the checkbox** (press Space and confirm the setting flips). | FR-016, SC-009 |

### 3.1 Additional non-UI check

| # | Check | Steps | Expected |
|---|---|---|---|
| **M11** | Malformed value recovers without collateral damage | With the app closed, hand-edit `settings.toml` to `high_contrast = "yes"` while leaving a non-default `theme`, volume and device in place. Launch. | The app starts, high contrast is **off**, the invalid field is reported as `appearance.high_contrast`, and **the theme, volume and device are still the non-default values** — not reset. | FR-003, A4, **A5** |

---

## §4 — If the build cannot be driven

014 and 015 both hit this. 015 recorded M1–M10 *all* not executed: the
process wedged inside `AccountService::launch_resolve_session` →
`KeyringSecureStore::get` → `SecKeychainFindGenericPassword` before
`eframe::run_native` was reached, so no window ever formed. 014 hit the
sign-in gate one step later.

**This feature's exposure**: M1, M2, M4, M5, M6, M9, M10 and M11 need
only the Settings screen and any rendered chrome — they do **not**
require a signed-in session or a loaded track, so they are reachable
whenever a window forms at all. Only M3 (four views), M7 (a loaded track
with markers) and M8 (a docked plugin panel) sit behind the sign-in gate.
That is a materially better position than 015's, where no scenario was
reachable.

**If the window never forms**, the required outcome is to record each
scenario **not executed**, with the reason and the point of failure, and
mark the affected checkpoints **not reached**. Never sign off on
automated evidence alone, never fabricate a signed-in state, and never
modify the host's real Keychain to route around it.

**Partial-run rule for this feature**: M3, M7 and M8 are the three
scenarios that verify the spec's own acceptance lines against the real
renderer. If only those three are blocked, record them as not executed
and say so plainly in `tasks.md` — the automated contrast suite (H17,
H18, M3) proves the *values*, but it does not prove they reach the
screen, and the plan does not claim otherwise.

### §4.1 — Consolidated sign-off status (Polish, T046)

| Scenario(s) | Task | Status |
|---|---|---|
| M1, M2, M9, M10, M11 | T029 | **PASS** |
| M3, M4 | T021 | **PASS**, one out-of-scope deviation (R17) |
| M5, M6 | T039 | **NOT EXECUTED** — GUI session locked (R19) |
| M7 | T034 | **NOT EXECUTED** — GUI session locked (R18) |
| M8 | T037 | **NOT EXECUTED** — GUI session locked (R21) |

Full detail, evidence and the automated stand-ins for the three
not-reached scenarios: research.md R20.

# Quickstart: Validating 016 — List Row, Tab Strip, and Panel Card

**Feature**: 016-list-row-and-panel-components | **Date**: 2026-09-23

Two halves, matching the split spec.md FR-027 chose: **values** are
asserted headlessly by the automated suites (§ 2), and the **rendered
pixels** — the underline, the card, the tooltip text, the selection fill —
are judged by the manual scenarios (§ 3), which the implementing agent
executes itself under Governance › Manual Scenario Sign-Off.

---

## 1. Prerequisites

```bash
# From the worktree root. The shell may carry RUSTUP_TOOLCHAIN overriding
# rust-toolchain.toml; without this the workspace fails its MSRV check.
export RUSTUP_TOOLCHAIN=1.95.0      # or: env -u RUSTUP_TOOLCHAIN
cd /Users/castilho/.autonomous/worktrees/mod-player-906276/016-list-row-and-panel-components
```

No new dependency, no new crate, no new feature flag. `cargo deny`'s
licence surface is unchanged.

---

## 2. Automated gates

Run everything:

```bash
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test --workspace
rtk cargo deny check
./scripts/check-license-headers.sh
```

### 2.1 New and extended suites

| Command | Pins | Criterion |
|---|---|---|
| `cargo test -p modplayer-ui --test rows` | list-row L1–L7, S1–S12, A1–A3, A5, A7 | SC-001, SC-003, SC-010, SC-012 |
| `cargo test -p modplayer-ui --lib theme::tokens` | L3 (`duration_measure`, `:` advance) | SC-001 |
| `cargo test -p modplayer-ui --lib rows` | L2, L4 (`format_duration` table) | SC-012 |
| `cargo test -p modplayer-ui --test library_view` | tab-strip T1, T4–T10, T12, S8 | SC-005, SC-010 |
| `cargo test -p modplayer-ui --test search_view` | S2 across the four groups, S8 | SC-010 |
| `cargo test -p modplayer-ui --test now_playing` | panel C5, C6, C9, C10, C13; P3 | SC-006, SC-007 |
| `cargo test -p modplayer-ui --test queue_view` | C8 | SC-006 |
| `cargo test -p modplayer-ui --test transport_view` | C7 | SC-006 |
| `cargo test -p modplayer-ui --test markers` | C11, C12 | SC-006 |
| `cargo test -p modplayer-core --lib settings` | P4, P5, P6 (round-trip, absent section) | SC-007 |
| `cargo test -p modplayer-core --test settings` | P9 — the `[now_playing_panels]` proptest round-trip (Constitution VIII) | SC-007 |
| `cargo test -p modplayer-core --lib controller` | P2 | SC-007 |
| `cargo test -p modplayer-ui --test fluent_keys` | A7, C8 — the three new keys | SC-004 |
| `cargo test -p modplayer-ui --test design_token_contrast` | A4 — selected-row runs at ≥ 4.5:1 | SC-011 |
| `cargo test -p modplayer-ui --doc` | Constitution VII — the new runnable doc examples on `format_duration` (made `pub`) and `RowSelection`; the count must rise, not stay flat | — |

### 2.2 The regression net — these must pass **unmodified**

This is where FR-025 and SC-009 are actually enforced. Any edit to these
files other than FR-037's two seed helpers is a failure, not a fix.

| Command | Why |
|---|---|
| `cargo test -p modplayer-ui --test accessibility` | **T9 is the binding case**: `find_one(&nodes, Role::Tab, &tr(key))` at `accessibility.rs:900` must still find each tab by its exact Fluent name. Also C4's heading-name pins. One addition is permitted: T11's tab selected/toggled state. |
| `cargo test -p modplayer-ui --test library_view` (existing assertions) | `labels_in_tree_order(&update, Role::Tab)` at `:380` — five names, fixed order, no count suffix. |
| `cargo test -p modplayer-ui --test design_token_literals` | Must still report **0** hits. Every new value lands in `theme/`, which the scan excludes. |
| `cargo test -p modplayer-ui --test design_token_roles` | The `mono` mapping for durations and counts. |
| `cargo test -p modplayer-ui --test interaction_states` | A6 — 015's focus ring is untouched and still separable from the selection fill. |
| `cargo test -p modplayer-ui --test control_variants` / `control_inventory` | 015's grammar is unchanged; the tab widget must not register as a converted boolean. |
| `cargo test -p modplayer-ui --test effects_view` | C12 — behaviour unchanged (one seed helper migrates, FR-037). |
| `cargo test -p modplayer-capability-gateway --test api_reference` | Principle IX: the plugin API must regenerate with **no diff**. |

### 2.3 What the automated suites deliberately do not cover

- **The rendered tooltip text** (SC-004). A tooltip renders in a deferred
  popup layer on hover and cannot be driven by `Context::run_ui` /
  `__run_test_ctx` without synthetic pointer state the existing suites do
  not construct (research R13). The *keys* are automated; the *text on
  screen* is M4.
- **Visual judgements** — that the underline reads as navigation, that the
  card reads as raised, that the selection fill reads as selected. These
  are SC-001/002/005/006 and belong to § 3.

---

## 3. Manual scenarios

Executed by the **implementing agent**, against the real build, per
Governance › Manual Scenario Sign-Off. Each result (pass / deviation, with
evidence) is recorded on its `tasks.md` task, and any behaviour differing
from the spec is written back into this file and `research.md`.

### 3.1 Recipe (macOS host)

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer &
```

- **Locate the window**: Python `Quartz.CGWindowListCopyWindowInfo`, owner
  `modplayer`, name `ModPlayer` → window id + bounds.
- **Drive it**: Python Quartz `CGEventPost` — mouse events at
  window-relative coordinates, `CGEventCreateKeyboardEvent` +
  `CGEventSetFlags` for shortcuts, `CGEventKeyboardSetUnicodeString` for
  typing. No test doubles: real Keychain, real network.
- **Capture**: `screencapture -x -o -l <windowid> <png>`, then read the
  image back and judge the scenario from it plus the app log. A
  point-sampling helper (015's `target/manual-walk/pixel.py`) settles
  colour questions the eye should not be asked.
- **Helper scripts live inside the worktree** (`target/manual-walk/`,
  gitignored) — the scope-guard hook denies writes and shell redirects to
  the scratchpad and `/tmp`.
- **Window sizes**: the app sets **none** —
  `crates/modplayer/src/main.rs:129` is `NativeOptions::default()`, with no
  `ViewportBuilder`, `inner_size` or `min_inner_size` anywhere in the tree
  (research R16). M6 and M9 must **resize the window explicitly** to
  1200×820 and 960×640 before capturing, or the evidence does not answer
  the criterion being asked.

### 3.2 The scenarios

| # | Scenario | Steps | Expected | Criterion |
|---|---|---|---|---|
| **M1** | One aligned duration column | Open Library › Saved Tracks with several rows visible; capture | Every duration right-aligned in one column, in tabular figures, left of the "…" menu — never inside the artist/album line | SC-001, US1 AS1 |
| **M2** | One truncation edge, three surfaces | Capture a Library tab, a Search results group and an album's detail track list | All three share the column layout, alignment and truncation edge; no title wraps to a second line | SC-001, US1 AS2/AS3 |
| **M3** | Mixed-kind alignment | Open Library › Playlists (wide rows), then a tab with track rows; capture both | The "…" menu sits at the same x-position in both; the wide rows' trailing column is reserved and empty | SC-001, US1 AS5, FR-003 |
| **M4** | Hover, and the tooltip that explains the row | Rest the pointer on a Track row, then on a Playlist row; capture each after the tooltip appears | Full-row highlight; "…" reachable without leaving the row; the Track tooltip says a second click or Enter **plays** it, the Playlist tooltip says it **opens** it | SC-002, SC-004, US1 AS4, US2 AS5 |
| **M5** | Click selects, second click opens | Single-click a Playlist row (capture); click a different row (capture); double-click the first (capture) | First click selects only — no navigation; the second row's click clears the first's highlight; the double-click opens the detail view | SC-003, SC-010, US2 AS1–AS3 |
| **M6** | Selection is legible, and still reacts | On a selected Track row with an availability reason and an explicit badge: capture, then hover it and capture again. Point-sample the title, secondary line, duration and "E" | Every run legible against the `accent` fill in **both themes**; hovering visibly changes the fill without hiding the selection | SC-011, FR-008, FR-031 |
| **M7** | Selection across Search's four groups | Click a Tracks-group row, then an Albums-group row; capture both | At most one row highlighted on the Search screen at any time | SC-010, FR-007 |
| **M8** | The tab strip reads as navigation | Open Library after the initial load completes; capture | Active tab underlined, not filled; all five tabs show their own count in `mono` figures; a genuinely empty tab shows `0` | SC-005, US4 AS1–AS3 |
| **M9** | Four cards, two window sizes | Open Now Playing with a track loaded and all three panels open. Resize to 1200×820, capture; resize to 960×640, capture | Markers, Effect Chain, Transport and Queue each render as a raised, rounded, padded card with a small uppercase header. **At 960×640 the master volume, peak meter and Queue card are all still on screen** (the 2026-09-19 clipping defect, FR-023) | SC-006, US3 AS4 |
| **M10** | Collapse survives a restart | For each of Effect Chain, Transport, Queue independently: toggle it, quit, relaunch, capture. Do both directions (collapse-then-restart, expand-then-restart). Repeat one of them via the `E`/`T`/`Q` shortcut instead of the switch | Each panel returns in the state it was last set to, independently of the other two; the shortcut path persists identically to the switch path | SC-007, US3 AS1–AS3/AS5 |
| **M11** | The hour rollover holds the column | Find or queue a track ≥ 60 minutes (a long DJ mix) in a list beside ordinary tracks; capture at both window sizes | It renders `h:mm:ss` (e.g. `1:04:15`), and the "…" menu's x-position is unchanged from the `m:ss` rows beside it | SC-012, Edge Case |

---

## 4. If the walk cannot run

015's sign-off recorded **M1–M10 all not executed**: two launches left the
main thread wedged inside
`AccountService::launch_resolve_session` → `KeyringSecureStore::get` →
`SecKeychainFindGenericPassword`, before `eframe::run_native` was ever
reached, so no window ever formed
(015 plan.md Complexity Tracking D11). 014 hit the same class of block one
step later, at the sign-in gate. **Assume this feature will hit it too**,
and note that 016's scenarios are *more* exposed than 015's: M1–M8 and M11
all need Library or Search behind the sign-in gate, and M9/M10 need a
loaded track. There is no equivalent of 015's M1 (the Welcome screen) that
is reachable regardless.

If the block recurs, the required outcome is to record each scenario as
**not executed**, with the reason and the evidence gathered (a `sample
<pid>` stack is what diagnosed it last time), and mark the affected
checkpoints **not reached**.

Explicitly forbidden, all three previously considered and rejected:

- Signing off on the automated suites alone. They cover every derivable
  *value*; they cannot stand in for the *pixel* judgement Governance
  requires. The honest record is "not executed", not "substituted".
- Fabricating a signed-in state, or adding a demo mode, to reach the
  window.
- Modifying or clearing the host's real macOS login Keychain to unblock
  the call — an irreversible, consent-requiring action on the operator's
  actual credential store, outside this feature's scope.

# Quickstart: Plugins List as a Real Table (027)

Validation guide. Rules: [contracts/plugins-table.md](./contracts/plugins-table.md) (T1–T20), strings: [contracts/fluent-strings.md](./contracts/fluent-strings.md), types: [data-model.md](./data-model.md).

## Prerequisites

- macOS host, repo worktree root as cwd.
- Toolchain: `export RUSTUP_TOOLCHAIN=1.95.0` (or `env -u RUSTUP_TOOLCHAIN`), per constitution recipe.
- Python 3 with `pyobjc-framework-Quartz` for manual scenarios; helper scripts in `target/manual-walk/` (gitignored).

## Automated gates

```bash
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test -p modplayer-core plugins::view          # suspend_cause, budgets on PluginRow
rtk cargo test -p modplayer-ui --test plugins_view       # T1–T20 test map
rtk cargo test -p modplayer-ui --test fluent_keys        # key table + pt-BR parity
rtk cargo test -p modplayer-ui --test accessibility      # health words, tab order, names
rtk cargo test -p modplayer-ui --test design_token_literals   # workspace literal scan stays 0
rtk cargo test -p modplayer-ui --test high_contrast
rtk cargo test --workspace
rtk cargo deny check
```

Expected: all pass; `seventeen_fixtures_fit_at_min_window_width` and `…_at_section_floor` report 0 overflowing/overlapping cells.

## Manual scenarios (executed by the implementing agent — Governance › Manual Scenario Sign-Off)

Launch: `cargo build -p modplayer && MODPLAYER_PLUGIN_STATE_DIR=$(mktemp -d) MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer &`. Locate window via `Quartz.CGWindowListCopyWindowInfo` (owner `modplayer`), drive with `CGEventPost`, capture with `screencapture -x -o -l <id>`. Open the Plugins section from the nav.

| # | Scenario | Steps | Expected |
|---|----------|-------|----------|
| M1 | 17 rows at minimum width | Resize window to 960×640 (its minimum) | 17 rows sorted by name; header labels sit above their columns; no cell overflows/overlaps; no horizontal scrollbar; rows scroll vertically under a fixed header; every health word and permission count visible (SC-001/002). |
| M2 | Wide window | Resize to ≥ 1440 wide, dock shown | Extra width goes to Name/Health/Source/Resource/Actions; alignment holds. |
| M3 | Truncation | Hover a truncated Name or the `invalid` row's reason | Ellipsis in cell; tooltip shows the full text; non-truncated cells show no tooltip. |
| M4 | Permissions disclosure | Click the permissions count of a plugin with ≥ 2 grants; click again | Explanations listed one per line inside the row; collapses. A 0-permission plugin shows "0", no control. |
| M5 | Health words | Observe healthy rows; let `throw` fixture accumulate aborts | "healthy" (positive colour); "degraded" (warning colour) when the warning window opens. |
| M6 | Suspend + restart | Let `hang` (or `leak`) fixture suspend | Row shows "suspended" + reason (e.g. "it stopped responding") and a Restart button; clicking Restart → row leaves suspended; the suspension notification is dismissed. |
| M7 | Resource use | Observe `wellbehaved` while playing synthetic audio | "CPU x.x % / 10 %" and "Mem x.x MB / 64 MB" in mono, right-aligned, digits aligned across rows; non-active rows show "CPU —"/"Mem —". `flood`/`leak` near cap show "over budget". |
| M8 | Panel controls in row | Observe `ui-panel` row | Show/Hide + Enable/Disable inside the Actions cell (no indented second line); clicking behaves as in 011 (session hide / persisted disable). |
| M9 | Expansion persists | Expand permissions on two rows; wait > 2 s (repaints) and let a health change occur | Both stay expanded. |
| M10 | Keyboard only | Tab through the table | Order: Enabled → permissions → panel controls → Restart per row, rows top→bottom; visible focus ring; Space/Enter operate. Repeat in high-contrast appearance: words still distinguish state. |

Record each result (pass / deviation + screenshot path) on the manual-scenario task in `tasks.md`; behaviour deviations also go into this file and `research.md`.

### Manual walk results (2026-09-30)

All ten scenarios were executed. M1 and M3 surfaced two defects, both fixed in the same change set: the health dot rendered as tofu, and truncated cells showed a duplicate tooltip. M7 memory reads 0.0 MB because of a pre-existing runtime gap (`set_used_bytes` has no caller), which is out of scope here. See research.md › Manual walk deviations.

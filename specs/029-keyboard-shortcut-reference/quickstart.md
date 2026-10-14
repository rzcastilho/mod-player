# Quickstart: Keyboard Shortcut Reference (029)

This guide shows how to check that the feature works end to end. The rules being checked are in [contracts/](./contracts/), and the types are in [data-model.md](./data-model.md).

## Prerequisites

- Rust toolchain from `rust-toolchain.toml` (1.95.0). If the shell exports `RUSTUP_TOOLCHAIN`, run commands as `RUSTUP_TOOLCHAIN=1.95.0 cargo …`.
- Font asset `crates/modplayer-ui/assets/fonts/ModPlayerKeys-Regular.ttf` present, with `OFL.txt` and `SOURCE.md` next to it (research R3, K13).
- For the manual walk: a macOS host and the constitution's Manual Scenario Sign-Off recipe (Quartz `CGEventPost` + `screencapture`, helper scripts under `target/manual-walk/`).

## Automated gates

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --workspace --doc
cargo deny check
```

Focused runs while iterating:

```bash
cargo test -p modplayer-core --test actions            # K1–K6, K10 (label table, proptests)
cargo test -p modplayer-ui --test key_labels            # K7, K8 (2 176-chord glyph coverage)
cargo test -p modplayer-ui --test controls              # C1–C17 (incl. SC-005 height)
cargo test -p modplayer-ui --test shortcut_sheet        # S6–S18
cargo test -p modplayer-ui --test shell_navigation      # S1–S3
cargo test -p modplayer-ui --test fluent_keys           # F1–F5, S5
cargo test -p modplayer-ui --test accessibility --test high_contrast --test design_token_literals
cargo test -p modplayer-ui --lib                        # key_label, shortcut_groups, file_url, printable_html unit tests
```

Expected: everything passes. `every_key_label_is_covered_by_app_fonts` reports 2 176 chords checked with 0 failures.

## Manual scenarios (executed by the implementing agent, per constitution)

Launch: `cargo build -p modplayer && ./target/debug/modplayer` in the background, signed in. Locate the window with `CGWindowListCopyWindowInfo` and capture with `screencapture -x -o -l <id>`. Record each result (pass or deviation, with the screenshot path) on the matching task in `tasks.md`.

| # | Steps | Expected |
|---|---|---|
| M1 | Settings › Controls at the default 1280×800 window | Two columns. Groups in the order Transport, Markers, Loop, Cues, Navigation, Effects, then plugin groups. Play/pause chip reads **Space**. `⌘`, `⇧`, `⌥` and arrow chips render (no empty boxes). No "trigger" labels. No `×`, Add binding or Reset visible at rest |
| M2 | Hover a row, then move the pointer away | Controls appear on hover and disappear on leave. Other rows don't shift sideways |
| M3 | Tab from the filter box into the rows | The row element shows a focus ring and its controls appear. Tab steps through `×`, Add binding, Reset, then the next row. The previous row's controls hide |
| M4 | Click Add binding on a row, then move the pointer off the row | The capture control stays visible until a chord is pressed or Esc cancels it |
| M5 | Resize the window to its minimum, 960×640 | Still two columns: content is about 760 pt, at or above the 720 threshold. The dock only renders on Now Playing, so it can't narrow this page, and one column is covered headlessly by `one_column_below_720` (research R15). Chips and conflict notes wrap, nothing is cut off, and nothing runs under the scroll bar |
| M6 | Type `L`, then `space`, then `←`, then `e` into the filter | L: Loop toggle (and any other action bound to L) is listed, with its chip outlined. space: Play/pause. ←: actions bound to Left. e: no rows matched only through Enter or Escape |
| M7 | Click the nav rail's **Keyboard shortcuts** item while Library is shown | The sheet opens and Library stays selected. The sheet is read-only, groups match the Controls page, and unbound actions show "Not bound" |
| M8 | With the sheet open, rebind an action on Controls, then use Reset all | The sheet updates on the next frame each time, without reopening |
| M9 | Focus the sheet and press Esc. Reopen it from the getting-started card's "Keyboard shortcuts" link | Esc closes the sheet. The link reopens it. The card text has no "Shortcuts:" clauses |
| M10 | Dismiss the getting-started card, then use the nav-rail item | The sheet still opens |
| M11 | Click **Open printable version** in the sheet | The default browser opens `modplayer-shortcuts.html`. Every group and action is listed with ⌘-style labels, black on white. Print preview shows tables that are not split mid-group where avoidable. The file holds no account or device data (inspect the source) |
| M12 | Make the temp file read-only (`chmod 444 $TMPDIR/modplayer-shortcuts.html`), then click Open printable version | An inline error appears in the sheet and the app keeps running. Run `chmod 644` afterwards |
| M13 | Turn on high-contrast appearance (Settings › Appearance) and repeat M1 and M6 | Chips, the key-match outline, the focus rings and the sheet are all legible |

## Done when

- All automated gates pass.
- M1–M13 are recorded as passing in `tasks.md`. Any deviation is reflected back in this file and in `research.md`.

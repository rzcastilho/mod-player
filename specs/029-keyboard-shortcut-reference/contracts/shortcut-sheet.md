# Contract: Shortcut Sheet, entry points and printable version (S1–S18)

Scope: `modplayer-ui::shortcut_sheet`, `shell::Shell::nav_rail`, `getting_started`, and `app::App` wiring. Covers FR-007 to FR-011, FR-013, SC-003, SC-004 and SC-006.

## Entry points

| # | Rule | Test |
|---|---|---|
| S1 | The nav rail draws, after the five section items, a `theme::divider` and then a "Keyboard shortcuts" `nav_item`. That item is never selected (no indicator) and is focusable, with accessible name `nav-keyboard-shortcuts` | `shell_navigation.rs::keyboard_shortcuts_item_after_settings_never_selected` |
| S2 | Activating the item (click, or Enter/Space while focused) sets `shortcut_sheet_requested`. `App` opens the sheet in the same frame and `shell.section` is unchanged | `shell_navigation.rs::shortcuts_item_opens_sheet_without_changing_section` |
| S3 | The item is present whenever the nav rail is (after the launch gates), and the getting-started dismissal state has no effect on it | `shell_navigation.rs::shortcuts_item_present_after_card_dismissed` |
| S4 | The getting-started card shows a "Keyboard shortcuts" control with `Role::Link`. Activating it returns `GettingStartedOutcome::OpenShortcuts`, and `App` opens the sheet. Dismiss and Tutorial behave as before | `first_launch.rs` / `library_view.rs` getting-started tests |
| S5 | `getting-started-section-loop` and `getting-started-key-tempo` contain no shortcut prose in en-US or pt-BR (no `Shortcuts:` / `Atalhos:` and no key list) | `fluent_keys.rs::getting_started_has_no_shortcut_prose` |

## Sheet window

| # | Rule | Test (`tests/shortcut_sheet.rs`) |
|---|---|---|
| S6 | An `egui::Window` with a fixed id `shortcut-sheet`, titled `shortcuts-sheet-title`. It is non-modal: the rest of the UI keeps receiving input. There is at most one instance, and opening it again brings it to the front | `sheet_single_instance_non_modal` |
| S7 | Content has the same groups, order and rows as the controls page with an empty filter. Each row shows the action name, chips (`KeyLabel::visible`), "Not bound", "(inactive)" and the `⚠` conflict marker | `sheet_matches_controls_groups_and_rows` |
| S8 | The sheet has no editing controls: no `×`, no Add binding, no Reset, no capture | `sheet_has_no_editing_controls` (AccessKit names absent) |
| S9 | The same two-column rule and break as C3/C4, applied to the sheet's own available width | `sheet_two_columns_when_wide` |
| S10 | **Live**: change, remove, reset, reset-all or plugin activation while the sheet is open shows on the next frame, without reopening | `sheet_reflects_binding_change_next_frame` (SC-004) |
| S11 | Clicking the close button closes the sheet. Esc closes it only when focus is inside the sheet, and the Esc key is consumed so no action fires in that frame. Esc with focus elsewhere does nothing to the sheet | `sheet_esc_closes_when_focused_only` |
| S12 | Open/closed state is not persisted: a fresh `App` starts with the sheet closed | `sheet_starts_closed` |
| S13 | Every sheet control has an accessible name and a visible focus ring. Colours come from tokens only and stay legible in high contrast | `accessibility.rs`, `high_contrast.rs` |

## Printable version

| # | Rule | Test |
|---|---|---|
| S14 | "Open printable version" writes `temp_dir()/modplayer-shortcuts.html` (overwriting any existing file) and returns `SheetOutcome::OpenPrintable(file_url)`. **Only `App`** calls `ctx.open_url(OpenUrl::new_tab(url))` | `printable_writes_file_and_returns_url` (temp dir set through env in the test) |
| S15 | The HTML is self-contained: a single file with inline `<style>` (including `@media print`), no `<script>`, no external `src`/`href`, and `<html lang>` set to the locale. It holds a title, the generated date, and one `<h2>` + `<table>` per group with every action and its visible binding labels or "Not bound" | `printable_html_*` unit tests on the pure builder |
| S16 | Every interpolated string is HTML-escaped. A plugin action labelled `<script>x</script>` or `a&b"` appears escaped | `printable_html_escapes_plugin_labels` |
| S17 | The file holds no account, credential, device, token or path data: it is built only from `ShortcutGroup`s and the date | review + `credential_leak.rs` style assertion that the builder signature takes no session or settings |
| S18 | A write failure (simulated with an unwritable directory) sets `export_error`. The sheet shows `shortcuts-printable-error` inline, nothing panics, the sheet stays open, and no URL is opened | `printable_write_failure_shows_inline_error` |

`file_url(&Path)` unit tests: `/tmp/modplayer-shortcuts.html` → `file:///tmp/modplayer-shortcuts.html`; `C:\Users\A B\AppData\Local\Temp\modplayer-shortcuts.html` → `file:///C:/Users/A%20B/AppData/Local/Temp/modplayer-shortcuts.html`; non-ASCII bytes are percent-encoded.

# Research: Keyboard Shortcut Reference (029)

Phase 0 output for [plan.md](./plan.md). Every Technical Context unknown is resolved below. Each decision lists what was chosen, why, and the rejected alternatives. Facts come from the current code (`modplayer-core::actions`, `modplayer-ui::settings::controls`, `shell.rs`, `getting_started.rs`, `app.rs`, `theme/`) and from checking egui 0.36.2's bundled fonts and the eframe/egui-winit 0.36 sources in the local cargo registry on 2026-10-09.

---

## R1 — Where the label table lives (core vs UI)

**Decision**: Keep the label table in one place, `modplayer-core::actions::chord`. A new pure method `Chord::display_parts(platform) -> DisplayParts` returns structured tokens: modifier tokens plus one key token, each either a `Symbol(&'static str)` (painted as-is: `⌘ ⇧ ⌥ ← → ↑ ↓`, punctuation characters, letters, digits, F-keys) or a `Word(KeyWord)` (a closed enum: Space, Return, Enter, Esc, Tab, Backspace, Delete, Insert, Home, End, PageUp, PageDown, Copy, Cut, Paste, Left/Right/Up/Down (non-Mac), Ctrl, Shift, Alt, and the physical-only names). `KeyWord::fluent_key()` maps each word to a Fluent key. `Chord::display(platform)` stays the non-localised **en-US** rendering of the same parts (used by core tests, logs and the plugin console). The UI's new `key_label` module renders the parts through `tr`, so painted labels follow the active locale.

**Rationale**: FR-012 says only *display* changes, and core already owns `display`. Splitting it into "what to show" (core, pure, proptest-able, no egui or Fluent dependency) and "how to word it" (UI, `tr`) means the two can never drift apart. A UI test pins `key_label(chord, p).visible == chord.display(p)` under en-US for every enumerated chord.

**Alternatives rejected**:
- *Call `tr` inside `Chord::display`*: core tests would depend on the locale, and plugin-console and log output would change with the UI language.
- *Duplicate the table in the UI*: two sources of truth, and the existing `display` tests would stop guarding what the user sees.

## R2 — Glyph coverage of the fonts the app loads today

**Finding (measured)**: The app installs no fonts of its own (`widgets/controls.rs`: "egui's default fonts; the app installs no others"). egui 0.36's `FontDefinitions::default()` uses Proportional = `[Ubuntu-Light, NotoEmoji-Regular, emoji-icon-font]` and Monospace = `[Hack, Ubuntu-Light, NotoEmoji-Regular, emoji-icon-font]`. A cmap scan of `epaint_default_fonts-0.36.2/fonts` gives:

| Glyph | Ubuntu-Light | Hack | NotoEmoji | emoji-icon-font |
|---|---|---|---|---|
| `⌘` U+2318 | – | – | – | ✓ |
| `⇧` U+21E7 | – | ✓ | – | – |
| `⌥` U+2325 | – | – | – | – |
| `← → ↑ ↓` U+2190–2193 | – | ✓ | – | – |
| `␣` U+2423, `⏎` U+23CE, `⎋` U+238B | – | – | – | – |

In the Proportional family (the one chips use), `⇧ ⌥ ← → ↑ ↓ ␣ ⏎ ⎋` all draw as tofu today. This is the UX-32 defect. Only `⌘` resolves, through the icon font.

**Decision**: R3 (font fallback) plus R1's table (Space/Return/Esc become words). Afterwards every symbol the table can emit (`⌘ ⇧ ⌥ ← → ↑ ↓` and ASCII punctuation) is covered in the Proportional family.

## R3 — Making `⌘ ⇧ ⌥ ← → ↑ ↓` render

**Decision**: Add a new `theme::fonts` module with one entry point, `ensure_installed(ctx)`. It is called from `theme::apply_tokens_for`, and a one-shot flag in `ctx` data makes it cheap and idempotent, so the app (`App::new` plus every frame) and all existing test helpers (`fresh_ctx` → `apply_tokens`) use the same fonts. It starts from `FontDefinitions::default()` and:

1. registers `ModPlayerKeys`, a **subset of Noto Sans Symbols 2 Regular (SIL OFL 1.1)** with exactly U+2318 `⌘`, U+21E7 `⇧`, U+2325 `⌥`. A cmap check on 2026-10-09 confirmed the upstream file covers all three. It is committed as `crates/modplayer-ui/assets/fonts/ModPlayerKeys-Regular.ttf` with `OFL.txt` and a `SOURCE.md` (upstream URL, version, `pyftsubset` command), and embedded with `include_bytes!`;
2. **appends** `ModPlayerKeys`, then egui's existing `Hack` data (already embedded by `epaint_default_fonts`, so no extra bytes), to the **end** of both the Proportional and Monospace fallback lists.

Appending at the end means no glyph that already resolves today changes its font. Only glyphs that are tofu today gain a source. The arrows resolve through Hack, `⌘` keeps resolving through emoji-icon-font (already covered), and `⇧ ⌥` resolve through `ModPlayerKeys` (Hack also has `⇧`).

**Rationale**: The spec explicitly allows an OFL font or subset (Clarifications, label table). A 3-glyph subset is a few KB. Reusing Hack for arrows costs nothing. A font file is not a crate, so `cargo deny` is unaffected and Constitution X's new-crate rule does not apply.

**Fallback (implementation-time)**: If `pyftsubset` (fontTools) is not available to the implementer, embed the full upstream `NotoSansSymbols2-Regular.ttf` (~670 KB, same licence) under the same name and record the deviation in `SOURCE.md`. The coverage test (R13) is the arbiter either way.

**Alternatives rejected**:
- *Noto Sans Math subset for arrows*: it covers arrows and `⇧` but not `⌘ ⌥`, so a second file would be needed. Hack already covers the arrows.
- *Insert the new fonts before NotoEmoji*: this could change how existing glyphs such as `⚠` resolve. Appending is strictly additive.
- *Use textual "Cmd/Opt" on macOS*: this contradicts the clarified label table, which requires `⌘ ⇧ ⌥`.
- *Monospace-only chips*: the Monospace family still lacks `⌥`, and chips would no longer match body type.

## R4 — Hover/focus-revealed row controls with zero width at rest

**Decision**: Each controls row gets a stable `row_id = Id::new("controls-row").with(&action_id)`. At the start of the row, the UI registers a focusable row element by calling `ui.interact(prev_rect, row_id, Sense::focusable_noninteractive() | Sense::hover())`, using the row rect stored in `ctx` memory from the previous frame. On the first frame there is no rect yet, so it uses a zero-size rect at the cursor. Registering the element first puts it ahead of the row's controls in Tab order, because egui's focus order follows registration order. Its accessible name is set with `widget_info`. The row is **revealed** when any of these holds:

- the pointer is inside the row's previous-frame rect;
- `ctx.memory(|m| m.focused())` is `row_id`, or is one of the row's control ids. Those ids are derived from `row_id` (`row_id.with("remove", chord)`, `row_id.with("add")`, `row_id.with("reset")`) by passing explicit `id_salt`s, so the check is a cheap id comparison;
- `screen.capture == Some(action)` or a capture rejection is showing for this row.

When the row is not revealed, the controls are simply **not added**, so they take zero width. When the reveal state changes, the row calls `ctx.request_repaint()`, so the one-frame latency of using last frame's rect is never visible as a stale frame.

**Tab walk**: Tab onto the row element reveals the controls. On the next frame they exist, so the next Tab moves to the first `×`, and so on. When focus leaves the row, the next frame hides them. Because hidden controls are not registered, they never steal focus.

**Rationale**: egui is immediate-mode, and the previous-frame-rect pattern is what egui itself uses for hover. Not adding the widgets meets "zero width" exactly. A transparent placeholder would still take layout space.

**Alternatives rejected**:
- *Paint controls with `set_invisible`*: they keep their width, which fails FR-004.
- *Overlay controls on top of the chips*: they would cover chip content and be hard to reach by keyboard.
- *Always show controls while the filter is focused*: this does not match the spec's reveal rules.

## R5 — Two-column rule and column split

**Decision**: Use a pure function `shortcut_groups::column_break(weights: &[usize]) -> usize`. The weight of a group is its heading plus its row count (`1 + rows`). The function returns the boundary `k` (in `0..=n`) that minimises `|Σweights[..k] − Σweights[k..]|`; ties go to the smaller left column, which is deterministic. The layout uses `ui.columns(2, …)` when `ui.available_width() >= TWO_COLUMN_MIN_WIDTH` (720.0 logical px, a named constant on the controls/sheet module, not a magic literal). Otherwise it uses one column. The controls page and the sheet share the same function, each applied to its own available width. The filter box and "Reset all to defaults" are drawn before the columns, at full width. Chips use `horizontal_wrapped`, so they wrap and are never elided.

**Rationale**: Splitting at a group boundary keeps groups whole (FR-003). Row count is the stable proxy for height and is testable without layout. A proptest pins the invariants: `k` is in `0..=n`, and no other `k` gives a smaller imbalance.

**Alternatives rejected**:
- *egui `Grid` with alternating groups*: this interleaves groups and breaks catalog reading order.
- *Measuring real heights from the previous frame*: this is non-deterministic for tests and gives little gain, because row heights are uniform unless chips wrap.

## R6 — Filter by key name

**Decision**: Add a pure function `key_label::aliases(chord, platform) -> Vec<String>` that returns lowercased strings:

1. the canonical key name (`KeyName::as_str()`), for example `l`, `space`, `left`, `openbracket`;
2. the localised displayed key label (`L`, `Space`, `←`, `[`, `Page Up`);
3. the whole chord's displayed form (`⌘l`, `ctrl+shift+l`).

When the chord has no modifiers, item 3 equals item 2. A row matches the filter when the group heading or the action label contains the query (existing substring behaviour), **or** when any binding has an alias that equals the query exactly. The query is trimmed and lower-cased. Chips that match by key get the accent focus-style outline. This is a stroke, not a fill, so it gives a non-colour cue that survives high contrast.

**Rationale**: Exact matching is what makes the spec's "e must not match Enter/Escape" rule true. Pure functions are unit-testable without egui.

**Alternatives rejected**:
- *Substring on key labels*: "e" would match "Esc" and "Delete".
- *Fuzzy match*: unpredictable, and not requested.

## R7 — Shortcut sheet surface

**Decision**: Use an `egui::Window` with the title `tr("shortcuts-sheet-title")`, `.open(&mut state.open)`, `.collapsible(false)`, `.resizable(true)`, `.vscroll(true)`, `.default_size` derived from tokens (clamped to the viewport), and a fixed `Id` (`"shortcut-sheet"`), which gives at most one instance. The open/closed state lives in `App` as `ShortcutSheetState`, which is not persisted. Each frame the content is rebuilt from `shortcut_groups::snapshot(controller.actions())`, the same snapshot the controls page uses, with no filter. That makes live updates automatic (FR-009, SC-004).

**Esc**: The window handles Esc only when the focused widget is in the window's layer, or when the window's layer is the top layer and the pointer was last pressed inside it. In that case it calls `ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))` and closes. Consuming the key stops it from reaching the action dispatcher in the same frame. No default catalog action binds bare Escape, but consuming keeps a user binding from double-firing.

**Rationale**: `egui::Window` is non-modal, scrollable and draggable, and `plugin_panels.rs` (L2) already uses it for floating panels. A fixed id gives uniqueness for free.

**Alternatives rejected**:
- *`egui::Modal`*: blocks the app, but the spec says non-modal.
- *A new nav section*: the spec says the item is an action, not a section, and it must not change the current section.
- *A native OS window (viewport)*: platform-dependent focus behaviour, and multi-viewport is not used anywhere else.

## R8 — "Keyboard shortcuts" nav-rail item

**Decision**: `Shell::nav_rail` draws `theme::divider` after the `SECTIONS` loop, then one `widgets::controls::nav_item(ui, false, &tr("nav-keyboard-shortcuts"))`. `selected` is always `false`, so it never shows the indicator. On click it sets a new `Shell::shortcut_sheet_requested: bool`, mirroring the existing `focus_search_requested` pattern. `App` consumes the flag each frame and opens the sheet, leaving `shell.section` unchanged. `nav_item` already sets the accessible name and focusability.

**Rationale**: This reuses the existing nav widget and a request-flag pattern already used in the shell. It is identical on all platforms (Constitution X) and is present wherever the nav rail is: `show_chrome` draws the rail only after the launch gates.

**Alternatives rejected**:
- *Native menu bar*: platform-specific, and out of scope per the clarifications.
- *New global hotkey action*: adds a catalog action, which is out of scope per the clarifications.

## R9 — Printable version

**Decision**:
- `shortcut_sheet::printable_html(groups, platform, generated: &str) -> String` is pure. It produces a self-contained HTML5 document with `<html lang>` set from the active locale tag, an inline `<style>` (black on white, `@media print` with `break-inside: avoid` per group, system font stack), `<h1>` for the title, a generation-date line, and per group an `<h2>` plus a two-column `<table>` (Action, Shortcut) listing bindings or "Not bound", with "(inactive)" and a conflict mark when relevant. There is no script, no external resource and no network fetch.
- **Every interpolated string is HTML-escaped** (`& < > " '`). Plugin action and group labels are plugin-supplied literals (`RowLabel::Literal`), so leaving them unescaped would allow markup injection into a page the system browser opens (Constitution II/VI).
- The date is `chrono::Local::now()` formatted as `%Y-%m-%d %H:%M`, passed into the template through `tr_args("shortcuts-printable-generated", date)`. `chrono` is already a direct dependency (028).
- The file is written to `std::env::temp_dir().join("modplayer-shortcuts.html")` with `std::fs::write`, overwriting any previous file. On `Err`, the sheet stores `ExportError` and shows `tr("shortcuts-printable-error")` inline. The error is logged without the file contents, and nothing panics.
- On success the sheet returns `SheetOutcome::OpenPrintable(url)`, and **`App`** calls `ctx.open_url(OpenUrl::new_tab(url))`, keeping design note 11: only `App` opens URLs. The URL comes from a small pure function, `file_url(&Path) -> String`. It turns `\` into `/`, prefixes `file:///` (Windows `C:/…`) or `file://` (absolute Unix paths), and percent-encodes every byte outside RFC 3986 unreserved characters plus `/` and `:`. Unit tests cover a Unix path, a Windows drive path, and a path with a space or non-ASCII characters.
- Verified 2026-10-09: egui-winit 0.36 opens URLs through `webbrowser::open` (crate 1.2.4), and the workspace does not enable its `hardened` feature, so `file://` URLs are allowed ("guarantees that the browser is opened, even for local files"). An `open_url` failure is only logged by egui-winit and cannot be observed. The spec's inline error therefore covers only the write failure, which is the observable one.

**Rationale**: egui has no print API. A browser handles printing, pagination and PDF output well, and the file holds only action names, groups and binding labels (Constitution VI).

**Alternatives rejected**:
- *PDF crate*: a new dependency, against YAGNI.
- *Native print dialog*: platform adapters would be needed.
- *Copy to clipboard*: does not give a printable page.
- *Add the `url` crate as a direct dependency*: it is in the lockfile, but a 20-line tested function avoids touching the dependency list.

## R10 — Getting-started card

**Decision**: In en-US, delete the "Shortcuts: …" sentence from `getting-started-section-loop` and `getting-started-key-tempo` in `locales/en-US/app.ftl`. Add `getting-started-shortcuts` ("Keyboard shortcuts"). Create `locales/pt-BR/app.ftl` containing this feature's getting-started keys, translated with no shortcut prose; pt-BR has no `app.ftl` today. Add `GettingStartedOutcome::OpenShortcuts`, drawn as a button with the `Role::Link` override (same as the tutorial link). `App::show_library` handles it by setting `shortcut_sheet.open = true`. Dismiss behaviour is unchanged.

**Rationale**: Uses the same outcome pattern as `OpenTutorial`. SC-006 is checked by a test that both `.ftl` files have no `Shortcuts:` / `Atalhos:` text in any `getting-started-*` value.

## R11 — Strings and locales

**Decision**: Put all new keys in a new `locales/en-US/shortcuts.ftl` and `locales/pt-BR/shortcuts.ftl`. The `static_loader!` picks up every `.ftl` file in a locale directory automatically. Changed keys stay in the files they live in now. `fluent_keys.rs` gains a `SHORTCUTS_029_KEYS` table, a test that every listed key resolves, and an en-US ↔ pt-BR parity test (the pattern `effects_024_keys_have_en_us_and_pt_br_parity` uses). As established in 024 and 028, `tr` still resolves against en-US; pt-BR is shipped for parity. The chord separator `+` and the mac symbols are typographic constants, not translatable text. Words, spoken modifier names and spoken punctuation names are Fluent strings. The full table is in [contracts/fluent-strings.md](./contracts/fluent-strings.md).

## R12 — SC-005 height measurement

**Decision**: The first test task for US1, before any layout change, writes a headless measurement in `tests/controls.rs`. It takes a 1280×800 viewport, the full host catalog, default bindings and an empty filter, draws `settings::controls::show` inside a `ScrollArea`, and records `content_size.y`. Run against the pre-feature code, it gives the **baseline**, which is pinned as `const PRE_029_CONTROLS_HEIGHT: f32` with a comment giving the commit it was measured on. The SC-005 test then asserts `height <= 0.6 * PRE_029_CONTROLS_HEIGHT`.

**Rationale**: "Same harness" in SC-005 needs a number captured from the old layout. Pinning it as a constant keeps the assertion reproducible after the old code is gone.

**Expected outcome (sanity check)**: Today a row is label + kind + chips + "Add binding" + a long "Reset … to default" button, and the reset button often wraps. Removing the kind label and the revealed controls makes each row one line. Two columns then roughly halve the count, so ≤ 60 % is reachable. If the measurement misses, the fallback is to tighten row spacing to `theme::Spacing::XS` (tokens only).

## R13 — Glyph-coverage test (FR-001, SC-001)

**Decision**: Add a new `crates/modplayer-ui/tests/key_labels.rs`. After `fresh_ctx()` and one frame (so the fonts from R3 are installed), it loops over every `KEY_NAMES` entry × all 8 `Mods` combinations × `[Platform::Mac, Platform::Other]`, which is 136 × 8 × 2 = 2 176 chords. For each chord it builds `key_label::KeyLabel` and asserts `fonts.has_glyphs(&FontId::proportional(body_size), &label.visible)`. It also asserts that the label contains no U+FFFD or U+25A1 and is non-empty. A second assertion checks that `label.visible == chord.display(platform)` (R1, en-US).

The test uses egui's own `has_glyphs`, the same mechanism as `row_action_glyphs_covered_by_fonts` (023) and the panel-card disclosure test.

## R14 — Out of scope (confirmed)

MIDI mappings (the breakdown makes them a prerequisite in 004-performance/001, which is not built), a native OS menu bar, a new catalog action or hotkey to open the sheet, persisting sheet open state, and restoring a dismissed getting-started card are all out of scope. No category is renamed or added; panel toggles stay under Navigation.

## R15 — Manual-walk findings (T051, 2026-10-10)

Running M1–M13 against the real app on macOS turned up four defects and one scenario that can't be run as written. All are fixed or recorded here.

- **Column break tie direction.** `column_break` broke ties towards the smaller `k`, so a filter that left one group (`space` → Transport) put it alone in the right column with the left column empty. Ties now go to the larger `k`: the left column fills first and is never empty while there is a group. The proptest asserts this, and `controls.rs::lone_filtered_group_sits_in_left_column` covers the page.
- **Scroll-bar gutter.** egui's floating scroll bar overlays the page's right edge, so right-column conflict notes ran under it ("…Tempo step u|p" at 960 × 640). The groups are now laid out in a child UI narrowed by `bar_width + bar_outer_margin`, the same reduction `plugins_view` uses. `set_max_width` could not do this: the full-width divider above the groups had already widened the UI's min rect. Covered by `controls.rs::groups_keep_clear_of_scroll_bar_gutter`.
- **Esc after clicking the sheet (S11).** Clicking the sheet's body focuses no widget, and Tab from the app lands in the nav rail, so a mouse user could never make Esc close the sheet. A press inside the sheet's rect now counts as focus inside until focus or a click goes elsewhere. Closing the sheet clears this, so a reopened sheet needs a fresh click. Esc with focus elsewhere still does nothing. Covered by `shortcut_sheet.rs::sheet_esc_closes_after_clicking_its_body`.
- **Printable column alignment.** Each group's table sized its own columns, so the Shortcut column started at a different x in every group. The print CSS now uses `table-layout:fixed` with a 60 % action column.
- **M5 one-column case can't be reached in the live app.** The window's minimum inner size is 960 × 640. The plugin dock only renders on Now Playing, and below 1024 it becomes an overlay that doesn't narrow content. So Settings › Controls always has about 760 pt of content and shows two columns (C3 behaves correctly). The one-column branch is covered only by `controls.rs::one_column_below_720` (headless, 700 px). M5 in `quickstart.md` now checks the narrowest live layout instead.

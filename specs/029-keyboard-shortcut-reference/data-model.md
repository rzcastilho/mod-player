# Data Model: Keyboard Shortcut Reference (029)

Phase 1 output for [plan.md](./plan.md). This feature adds **no persisted state**. `keybindings` in `settings.toml`, the chord encoding and the registry are unchanged (FR-012). The entities below are presentation types: pure values in core, plus UI-session state.

## 1. Core: `modplayer-core::actions::chord` (additive)

### 1.1 `DisplayParts`

The platform-specific structure of a chord's label (research R1).

| Field | Type | Notes |
|---|---|---|
| `mods` | `Vec<KeyToken>` (≤ 3) | Fixed order: primary, shift, alt. Mac: `Symbol("⌘")`, `Symbol("⇧")`, `Symbol("⌥")`. Other: `Word(Ctrl)`, `Word(Shift)`, `Word(Alt)` |
| `key` | `KeyToken` | Comes from the label table below |
| `platform` | `Platform` | Mac → parts are joined with no separator. Other → parts are joined with `+` |

`KeyToken = Symbol(&'static str) | Word(KeyWord)`

`KeyWord` is a closed enum. It has one Fluent key per variant for the visible word (`KeyWord::fluent_key()`) and one for the spoken word (`KeyWord::spoken_key()`; this may be the same key):

`Space, Return, Enter, Esc, Tab, Backspace, Delete, Insert, Home, End, PageUp, PageDown, Copy, Cut, Paste, ArrowLeft, ArrowRight, ArrowUp, ArrowDown, Ctrl, Shift, Alt, BrowserBack, ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight, SuperLeft, SuperRight, IntlBackslash`

### 1.2 Label table (`key` token by `KeyName` × `Platform`)

| KeyName | Mac | Other |
|---|---|---|
| `Space` | Word(Space) "Space" | Word(Space) "Space" |
| `Enter` | Word(Return) "Return" | Word(Enter) "Enter" |
| `Escape` | Word(Esc) "Esc" | Word(Esc) "Esc" |
| `Tab` | Word(Tab) "Tab" | Word(Tab) "Tab" |
| `Left/Right/Up/Down` | Symbol `← → ↑ ↓` | Word(ArrowLeft…) "Left/Right/Up/Down" |
| `PageUp/PageDown` | Word "Page Up"/"Page Down" | same |
| `Backspace, Delete, Insert, Home, End, Copy, Cut, Paste` | Word (same text) | same |
| `Colon , Comma , Minus , Period , Plus , Equals , Semicolon , Backslash , Slash , Pipe , Questionmark , Exclamationmark , OpenBracket , CloseBracket , OpenCurlyBracket , CloseCurlyBracket , Backtick , Quote` | Symbol `: , - . + = ; \ / \| ? ! [ ] { } `` ` `` '` | same |
| `0`–`9`, `A`–`Z`, `F1`–`F35` | Symbol (the name itself) | same |
| `BrowserBack, ShiftLeft … IntlBackslash` (physical-only) | Word ("Browser Back", "Left Shift", …, "Intl Backslash") | same |

Invariants (core unit tests + proptest over all `KEY_NAMES` × `Mods` × `Platform`):

- `display_parts` is total and never empty.
- `display(p)` equals the parts joined (Mac: concatenated; Other: `+`-joined) with en-US words.
- Every `Symbol` is in the **covered set**: ASCII printable, plus `⌘ ⇧ ⌥ ← → ↑ ↓`.
- `encode`/`parse` round-trip is unchanged. The existing proptest is kept.

## 2. UI: `modplayer-ui::key_label` (new)

### 2.1 `KeyLabel`

Built once per chord per frame by `KeyLabel::new(chord, platform)`. It is cheap: at most 4 `tr` lookups.

| Field | Type | Rule |
|---|---|---|
| `visible` | `String` | The parts, with `Word`s resolved through `tr(word.fluent_key())`. Painted in the chip |
| `spoken` | `String` | Spelled out: mac modifiers → "Command", "Shift", "Option"; other → "Ctrl", "Shift", "Alt"; punctuation symbols → their spoken word (`key-spoken-*`); arrows → "Left Arrow" etc.; joined with `+`. Used in `controls-binding-chip { $binding }`, `controls-remove-binding`, the row's accessible name, and the sheet |
| `aliases` | `Vec<String>` (lowercased, deduplicated) | `[canonical KeyName, visible key label, whole visible chord]` (research R6) |

`fn matches_key(&self, query: &str) -> bool`: `query` is already trimmed and lowercased. Returns true when it is non-empty **and** equals one of the `aliases`.

## 3. UI: `modplayer-ui::shortcut_groups` (new, shared by controls page and sheet)

### 3.1 `ShortcutRow` (moved from `controls::RowSnapshot`, unchanged fields)

| Field | Type | Notes |
|---|---|---|
| `id` | `ActionId` | host or plugin |
| `label` | `String` | `RowLabel` resolved through `tr` or a literal |
| `kind` | `ActionKind` | kind label shown only for `Continuous` |
| `enabled` | `bool` | false → "(inactive)", de-emphasised |
| `bindings` | `Vec<Chord>` | empty → "Not bound" |
| `conflicts` | `Vec<Chord>` | subset of `bindings` |
| `group` | `RowGroup` | `Host(ActionCategory)` or `Plugin(String)` |

### 3.2 `ShortcutGroup`

| Field | Type | Notes |
|---|---|---|
| `heading` | `String` | `tr(category.label_key())` or `tr_args("controls-plugin-group", plugin)` |
| `rows` | `Vec<ShortcutRow>` | in `rows()` order |

`fn snapshot(registry) -> Vec<ShortcutGroup>` lists host groups in `ActionCategory::ALL` order, then plugin groups in `rows()` order (already sorted by plugin display name). Empty groups are dropped.

`fn filter(groups, query, platform) -> Vec<FilteredGroup>` (research R6): a `FilteredGroup` holds its rows plus, for each row, the set of chords that matched by key (`key_hits: Vec<Chord>`), so the matching chips can be outlined.

### 3.3 Column layout

| Item | Value |
|---|---|
| `TWO_COLUMN_MIN_WIDTH` | `720.0` logical px (named const) |
| `fn column_break(weights: &[usize]) -> usize` | weights = `1 + rows.len()` per group. Returns the `k ∈ 0..=n` that minimises the imbalance; on a tie, picks the smaller `k` |
| `fn use_two_columns(available_width: f32) -> bool` | `available_width >= TWO_COLUMN_MIN_WIDTH` |

## 4. UI state

### 4.1 `ControlsScreen` (existing, extended)

| Field | Change |
|---|---|
| `filter: String` | unchanged |
| `capture: Option<ActionId>` | unchanged; while set, also forces the row's reveal state |
| `capture_error` | unchanged; while showing, also forces the row's reveal state |

Per-row reveal state is **derived** each frame and not stored on the screen (research R4). The only data kept between frames is the row rect, stored in egui temp memory under `Id::new("controls-row").with(&action_id)`.

`RowReveal = hovered(prev_rect) || focus ∈ {row_id, row_id.with(..) controls} || capturing(row) || rejecting(row)`

### 4.2 `Shell` (existing, extended)

| Field | Type | Notes |
|---|---|---|
| `shortcut_sheet_requested` | `bool` | Set by the nav-rail item. `App` consumes it and clears it the same frame. `section` is not changed |

### 4.3 `ShortcutSheetState` (new, owned by `App`, not persisted)

| Field | Type | Notes |
|---|---|---|
| `open` | `bool` | Window open flag, passed to `Window::open` |
| `export_error` | `Option<ExportError>` | Set when the write fails, shown inline. Cleared on the next successful export or when the sheet closes |

State transitions:

```text
Closed --(nav item | getting-started link)--> Open
Open --(nav item | link)--> Open                       (idempotent; brought to front)
Open --(close button | Esc while sheet focused)--> Closed (export_error cleared)
Open --(Open printable: write Ok)--> Open + SheetOutcome::OpenPrintable(url) → App open_url
Open --(Open printable: write Err)--> Open + export_error = Some(..)
```

### 4.4 `GettingStartedOutcome` (existing, extended)

New variant: `OpenShortcuts`. `App` handles it by setting `shortcut_sheet.open = true`.

### 4.5 `SheetOutcome` (new)

`None | OpenPrintable(String /* file URL */)`. Returned by `shortcut_sheet::show`, so that only `App` calls `open_url`.

## 5. Printable document (transient file)

| Property | Value |
|---|---|
| Path | `std::env::temp_dir()/modplayer-shortcuts.html`, overwritten each time |
| Content | title, generated date (local), one `<h2>` + `<table>` per group with action → binding labels (`visible`, for the current platform) or "Not bound", plus "(inactive)" and the conflict mark |
| Excludes | account, credential, device, token, file paths, plugin settings (Constitution VI) |
| Escaping | every interpolated string is HTML-escaped (`& < > " '`) |
| Builder | `printable_html(groups, platform, generated) -> String` (pure); `file_url(&Path) -> String` (pure) |

## 6. Fonts (asset, not state)

| Item | Value |
|---|---|
| `ModPlayerKeys-Regular.ttf` | Noto Sans Symbols 2 subset: U+2318, U+21E7, U+2325 (OFL 1.1) |
| Families | `Proportional` and `Monospace`: existing list + `ModPlayerKeys` + `Hack` (appended) |
| Install | `theme::fonts::ensure_installed(ctx)`, idempotent through a ctx temp flag; called from `apply_tokens_for` |

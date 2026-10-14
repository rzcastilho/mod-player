# Feature Specification: Keyboard Shortcut Reference

**Feature Branch**: `feature/029-keyboard-shortcut-reference`

**Created**: 2026-10-09

**Status**: Draft (clarified 2026-10-09)

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/011-browse-and-manage-polish/005-keyboard-shortcut-reference.md (id 005, keyboard-shortcut-reference)."

## Clarifications

### Session 2026-10-09 (clarify reviewer — no human escalation needed)

Sources: constitution v1.1.1 (X: identical behaviour across platforms, keyboard-operable, accessible names, externalised strings en-US + pt-BR, YAGNI; VI: no credentials in user files), breakdown `011-browse-and-manage-polish/005` (intent, acceptance, scope boundary), UI/UX review §3.1 UX-10 and §3.7 UX-32, and current code (`modplayer-core::actions::{ActionCategory, Chord::display, registry::ActionRow}`, `modplayer-ui::settings::controls`, `getting_started.rs`, `shell.rs` nav rail, `layout.rs`, `main.rs` min window 960×640). Items marked **[derived]** come from a cited source; **[default]** items are assumed conventional defaults.

- Q: Which categories are the groups? The breakdown lists "transport, markers, loop, cues, navigation, panels". → A **[derived]** (breakdown: "the action categories the catalog already defines"; `ActionCategory::ALL`): host groups are exactly `ActionCategory::ALL` in catalog order — **Transport, Markers, Loop, Cues, Navigation, Effects**. There is no "Panels" category; panel toggles (`ToggleQueue`, `ToggleEffectChain`, `ToggleTransportPanel`) stay under Navigation. Plugin-contributed actions keep their existing per-plugin groups ("{plugin} actions", contracts/action-registry-plugins.md G15), placed after the host groups in `rows()` order. No new category is introduced.
- Q: Why do keys render as empty boxes, and what is the rule? → A **[derived]** (UX-32 + `chord.rs`): `Chord::display` emits symbols (e.g. `␣`, `⏎`, `⎋`, arrows) that the app's loaded fonts do not cover. Rule **[default]**: every character of every key label the UI paints MUST be covered by the fonts the app actually loads; this is verified by an automated test that enumerates every `KEY_NAMES` entry × every modifier combination × both `Platform` values and asserts glyph coverage against the app's real font set (no tofu/replacement glyph). Where a symbol cannot be covered, the label falls back to its textual name — never a box.
- Q: Which exact labels? → A **[default]** (platform conventions; acceptance requires "Space"):
  - **Space** renders as the word "Space" on every platform (replaces `␣`).
  - **macOS** modifiers render as `⌘` `⇧` `⌥` (existing order: ⌘ ⇧ ⌥, no separator) and arrows as `←` `→` `↑` `↓`; the plan MUST ensure these glyphs are covered (bundling an OFL-licensed font/subset is acceptable — a font file is not a new crate). Enter → "Return", Escape → "Esc"; every other key without a covered symbol uses its textual name.
  - **Windows/Linux** keep the existing "Ctrl+Shift+Alt+Key" textual form; Escape → "Esc".
  - **All platforms**: punctuation keys render as their printed character (`OpenBracket` → `[`, `CloseBracket` → `]`, `Comma` → `,`, `Period` → `.`, `Semicolon` → `;`, `Quote` → `'`, `Backtick` → `` ` ``, `Backslash` → `\`, `Slash` → `/`, `Minus` → `-`, `Equals` → `=`, `Plus` → `+`, `Colon` → `:`, `Pipe` → `|`, `Questionmark` → `?`, `Exclamationmark` → `!`, curly brackets → `{` `}`); multi-word names are spaced ("Page Up", "Page Down"); letters upper-case; digits as digits.
  - Textual key words (Space, Return, Esc, Page Up, Home, End, Delete, Backspace, Insert, Ctrl, Shift, Alt, arrow words, etc.) are Fluent strings in **en-US and pt-BR**.
  - This changes only `Chord`'s *display* (presentation); chord encoding, parsing, persistence, conflict detection and capture are unchanged (scope boundary). Existing tests asserting `␣` are updated accordingly.
- Q: Chip shape and accessible name? → A **[default]**: one chip per binding (whole chord in one chip, e.g. `⌘⇧L` / `Ctrl+Shift+L`), drawn with the existing chip/row components and theme tokens (no colour literals). Accessible name is `controls-binding-chip` with a **spelled-out** form ("Binding Command+Shift+L", "Binding Space") so screen readers never read raw symbols.
- Q: What does a row show at rest, and which controls are hover/focus-revealed? → A **[default]**: at rest a row shows the action name (+ the existing "(inactive)" de-emphasis when disabled), its binding chips (or "Not bound" in secondary text when it has none), and any conflict `⚠` + "conflicts with …" note (conflict state is information, always visible). The "Trigger" kind label is removed from the row; a kind label is shown only for `Continuous` actions (none ship today). **Revealed controls** = each chip's `×` remove button, "Add binding" and "Reset {action} to default". They occupy **zero width** at rest and appear when (a) the pointer is over the row, (b) keyboard focus is anywhere in the row, or (c) the row is in capture mode or showing a capture rejection (capture never collapses mid-use). Each row exposes one focusable row element in tab order (accessible name: action name + its bindings); focusing it reveals the controls and the next Tab moves into them; focus leaving the row hides them. Capture, add, remove and reset behaviour are otherwise unchanged (007).
- Q: Two-column rule? → A **[default]**: two columns when the controls page's available content width is ≥ **720 logical px**, otherwise one column. Groups are never split across columns: groups flow in order (host categories, then plugin groups) and the column break is placed at the group boundary that best balances row counts between the columns. Category headings keep the existing `section_heading`. Filter box and "Reset all to defaults" stay above the columns, full width. No binding chip is truncated or elided — chips wrap within their row.
- Q: How does the filter match key names? → A **[default]**: query = filter text trimmed and lower-cased; empty query shows everything. A row is shown when **any** of: (1) its group heading contains the query (existing), (2) its action label contains the query (existing), or (3) the query **equals** (case-insensitive) one of a binding's key aliases: the canonical key name (`l`, `space`, `left`, `1`, `openbracket`), the displayed key label in the current locale (`L`, `Space`, `←`, `[`, `Page Up`), or the whole chord's displayed form on the current platform (`⌘l`, `ctrl+shift+l`). Key matching is exact (not substring), so "e" does not match Enter/Escape. Chips matched by (3) are highlighted with the accent/focus outline (non-colour cue: outline). Results keep group order. No match → existing `controls-no-match` message.
- Q: What is "the application menu"? The app has no menu bar. → A **[default]** (constitution X: identical across platforms, no new platform adapter for one feature): a **"Keyboard shortcuts"** item in the shell's nav rail, placed after Settings and visually separated from the section items. It is an action, not a section: activating it opens the sheet without changing the current section and it never shows the selected indicator. It is keyboard-focusable with accessible name "Keyboard shortcuts" and is present whenever the nav rail is present (main shell, after launch gates). A native OS menu bar and a dedicated global hotkey (new catalog action) are out of scope for this feature.
- Q: What kind of surface is the shortcut sheet? → A **[default]**: a non-modal, scrollable, in-app floating window titled "Keyboard shortcuts", at most one instance, closed by its close button or Esc while it has focus; open/closed state is not persisted across restarts. It reads the live registry every frame, so any binding change (from Controls, reset, reset-all, plugin activation) appears on the next frame with no reopen. Content: the same groups, order and rows as the controls page with no filter — action name, binding chips (same labels), "Not bound" for unbound actions, "(inactive)" for disabled ones, conflict `⚠` marker — and **no** editing controls. Same two-column rule as the controls page, applied to the sheet's width.
- Q: How is printing done (egui has no system print)? → A **[default]**: the sheet has an **"Open printable version"** button that writes a self-contained, localised HTML file (`modplayer-shortcuts.html` in the OS temp directory, overwritten each time; inline print stylesheet, black-on-white, grouped tables, current platform labels, generation date) reflecting the current bindings, and opens it in the system's default browser via `App`'s existing `open_url` path, where the user prints. The file contains only action names, groups and bindings — no account, credential or device data (constitution VI). A write failure shows an inline error in the sheet; no panic.
- Q: What changes on the getting-started card? → A **[derived]** (breakdown + UX-10): the "Shortcuts: …" clauses are removed from both plugin lines (`getting-started-section-loop`, `getting-started-key-tempo`, en-US and pt-BR); the plugin descriptions stay. A new link-role control "Keyboard shortcuts" (accessible role Link, like the tutorial link) opens the sheet via a new `GettingStartedOutcome` variant handled by `App`. Dismiss behaviour and dismissal persistence are unchanged; restoring a dismissed card is out of scope (the nav-rail item is the persistent route).
- Q: MIDI mappings? → A **[derived]** (breakdown prerequisites: MIDI belongs to 004-performance/001, not built): no MIDI handling in this feature; only keyboard chords render as chips.
- Q: Locales and accessibility? → A **[derived]** (constitution X): every new string ships in en-US and pt-BR (Fluent key-parity test passes); every new control has an accessible name; colours come from theme tokens and remain legible in high-contrast appearance (017).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Read bindings legibly on the controls page (Priority: P1)

A musician opens Settings › Controls to check which key does what. Every key is shown by a readable label (Space, ⌘, ⇧, ←, digits, `[`) instead of empty boxes. Actions are grouped by the catalog's categories (Transport, Markers, Loop, Cues, Navigation, Effects, then plugin groups) in a two-column layout. Each row shows the action name and its bindings as key chips; the per-action controls (chip remove, Add binding, Reset) appear only on hover or keyboard focus.

**Why this priority**: Today the binding a user most wants to check is unreadable. Legible keys are the core value and unblock everything else.

**Independent Test**: Open the controls page at ≥ 720 px content width; confirm Play/pause reads "Space", all chips pass the glyph-coverage test, groups render in two columns, and per-action controls take zero width until hover/focus.

**Acceptance Scenarios**:

1. **Given** the controls page is shown, **When** the user looks at Play/pause, **Then** its binding chip reads "Space", not an empty box.
2. **Given** any binding using modifiers, arrows, digits or punctuation, **When** displayed, **Then** each label follows the Clarifications label table and every painted glyph is covered by the loaded fonts.
3. **Given** a row at rest, **When** the pointer hovers it or keyboard focus enters it, **Then** `×`, Add binding and Reset become visible and operable; at rest they take zero width.
4. **Given** the page renders with content width ≥ 720 px, **Then** groups appear in two columns in catalog order, no group split across columns; below 720 px, one column.
5. **Given** a row is in capture mode, **When** the pointer leaves it, **Then** the capture control stays visible until capture completes or is cancelled.

---

### User Story 2 - Filter by key name (Priority: P2)

A user wonders what the L key does. They type "L" into the existing filter and see the actions bound to L (chip highlighted), alongside name-based matches.

**Why this priority**: Answers the reverse lookup ("what does this key do?"), valuable but secondary to legibility.

**Independent Test**: Type "L", "space", "←" or "ctrl+shift+l" into the filter and verify the bound actions are listed with the matching chip highlighted.

**Acceptance Scenarios**:

1. **Given** the controls page, **When** the filter equals a key alias (canonical name, displayed label or whole chord display), **Then** every action bound to that key/chord is listed and the matching chip is outline-highlighted.
2. **Given** a filter by action name or group name, **When** applied, **Then** it behaves as before (substring).
3. **Given** the filter "e", **Then** actions bound to Enter or Escape are not included by key matching (key matching is exact).
4. **Given** a filter matching nothing, **When** applied, **Then** the existing no-match message is shown.

---

### User Story 3 - Shortcut sheet reachable without settings (Priority: P2)

A user opens a read-only shortcut sheet from the nav rail's "Keyboard shortcuts" item or from the getting-started card. It lists the same grouped bindings, offers a printable version, and reflects rebindings immediately. The getting-started card no longer inlines shortcuts as prose; it links to the sheet. After the card is dismissed, the nav-rail item still opens the sheet.

**Why this priority**: Gives a quick reference without entering settings and removes duplicated prose that goes stale.

**Independent Test**: Open the sheet from the nav rail; change a binding on Controls with the sheet open and see it update; dismiss the card and confirm the nav-rail item still opens the sheet; open the printable version and confirm the HTML lists every group.

**Acceptance Scenarios**:

1. **Given** a binding was changed, **When** the sheet is opened, **Then** it shows the new binding.
2. **Given** the getting-started card was dismissed, **When** the user activates the nav rail's "Keyboard shortcuts" item, **Then** the sheet opens.
3. **Given** the getting-started card is visible, **When** the user activates its "Keyboard shortcuts" link, **Then** the sheet opens; the card text contains no shortcut prose.
4. **Given** the sheet is open, **When** the user activates "Open printable version", **Then** an HTML file with every group, action and current binding label is written and opened in the default browser; on write failure an inline error is shown.
5. **Given** the sheet is open, **When** a binding is changed, reset or reset-all elsewhere, **Then** the sheet shows the change on the next frame.
6. **Given** the sheet is open, **Then** it contains no editing controls (no `×`, Add binding, Reset).

---

### Edge Cases

- Action with no binding: row (and sheet entry) shows "Not bound" in secondary text.
- Action with several bindings: all shown as chips, wrapping within the row, never clipped or elided.
- Platform differences: labels follow the running platform per the Clarifications label table (⌘/⇧/⌥ on macOS; Ctrl/Shift/Alt elsewhere).
- Keys with no covered symbol: fall back to their localised textual name, never a box.
- Narrow content (< 720 px, e.g. 960×640 window with dock open): one column, no truncation.
- Plugin-contributed actions: shown under their plugin group after host groups; inactive plugin rows de-emphasised with "(inactive)" on both surfaces.
- Keyboard-only users reach revealed controls by focusing the row element then tabbing.
- Rebinding while the sheet is open: updates on the next frame without reopening.
- Conflicting bindings: `⚠` marker visible at rest on both surfaces.
- Printable-version file write fails: inline error in the sheet, app keeps running.

## Requirements *(mandatory)*

### Functional Requirements

Source requirement IDs from `docs/ModPlayer-Software-Specification.md`: FR-1.4.1 (getting-started introduces shortcuts), FR-7.4.1 (central shortcut map), FR-9.2.2 (rebind/conflicts), NFR-6.1 (keyboard operability), NFR-6.2 (accessible names/focus), NFR-7.1 (externalised strings), NFR-9.2 (cross-platform shortcut conventions). Each FR below cites the IDs it implements.

- **FR-001**: Every painted key label MUST use the label table in Clarifications (Space as "Space"; macOS ⌘ ⇧ ⌥ and arrow symbols; punctuation as printed characters; textual names otherwise) and MUST consist only of glyphs covered by the app's loaded fonts — verified by an automated test over all `KEY_NAMES` × modifier combinations × both platforms (UX-32). *(Source: NFR-9.2, NFR-6.2)*
- **FR-002**: Each binding MUST render as one key chip per chord, with a spelled-out accessible name ("Binding Command+Shift+L"); unbound actions show "Not bound". *(Source: FR-7.4.1, NFR-6.2)*
- **FR-003**: The controls page MUST group rows by `ActionCategory::ALL` in catalog order (Transport, Markers, Loop, Cues, Navigation, Effects), then plugin groups, laid out in two columns at content width ≥ 720 px and one column below, never splitting a group across columns. *(Source: FR-7.4.1)*
- **FR-004**: Each row MUST show at rest only the action name (+ inactive marker), binding chips, conflict `⚠` note and — for Continuous actions only — the kind label; the chip `×`, "Add binding" and "Reset to default" controls MUST take zero width at rest and appear on pointer hover, on keyboard focus within the row, or while the row is capturing / showing a capture rejection. *(Source: FR-9.2.2, NFR-6.1)*
- **FR-005**: Each row MUST expose a focusable row element in tab order whose focus reveals the row's controls, which are then reachable by Tab; all revealed controls keep their existing accessible names. *(Source: NFR-6.1, NFR-6.2)*
- **FR-006**: The filter MUST keep its existing substring match on group headings and action names and additionally include rows with a binding whose key or chord exactly equals (case-insensitive) the query by canonical name, displayed label or whole-chord display; key-matched chips MUST be outline-highlighted. *(Source: FR-7.4.1)*
- **FR-007**: The system MUST provide a read-only, non-modal "Keyboard shortcuts" sheet listing the same groups, order and rows as the controls page (no filter, no editing controls), at most one instance, closable by its close button or Esc. *(Source: FR-7.4.1, NFR-6.1)*
- **FR-008**: The sheet MUST be openable from a "Keyboard shortcuts" item in the nav rail (after Settings, not a section, no selected state, present whenever the nav rail is) and from a link-role "Keyboard shortcuts" control on the getting-started card; the nav-rail route MUST work after the card is dismissed. *(Source: FR-1.4.1, NFR-6.1)*
- **FR-009**: The sheet MUST read the live action registry each frame so binding changes, resets and plugin action changes appear on the next frame without reopening or restart. *(Source: FR-7.4.1, FR-9.2.2)*
- **FR-010**: The sheet MUST offer "Open printable version", which writes a self-contained localised HTML file (`modplayer-shortcuts.html` in the OS temp directory, overwritten) with every group, action and current binding label and a print stylesheet, containing no account/credential/device data, and opens it via `App`'s `open_url`; a write failure MUST show an inline error and MUST NOT panic. *(Source: FR-7.4.1)*
- **FR-011**: The getting-started card's plugin lines MUST contain no shortcut prose (en-US and pt-BR) and the card MUST include the "Keyboard shortcuts" link; dismissal behaviour is unchanged. *(Source: FR-1.4.1, NFR-7.1)*
- **FR-012**: Chord encoding, parsing, persistence, conflict detection and capture behaviour MUST remain unchanged; only `Chord` display labels and presentation change. *(Source: FR-9.2.2, FR-7.4.1)*
- **FR-013**: All new strings (key words, nav item, sheet title, "Not bound", "Open printable version", write-error message, card link) MUST ship in en-US and pt-BR; all new controls MUST have accessible names and visible focus; colours MUST come from theme tokens and be legible in high-contrast appearance. *(Source: NFR-7.1, NFR-6.1, NFR-6.2)*

### Key Entities

- **Action**: A registry row (host catalog or plugin) with label, group and enabled state.
- **Binding**: A chord assigned to an action; may be user-modified.
- **Key label**: Platform- and locale-specific display of a chord, glyph-covered, plus a spelled-out accessible form and a set of filter aliases.
- **Shortcut sheet**: Read-only, live view of all groups with current bindings, plus a printable HTML export.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of key labels across all `KEY_NAMES` × modifier combinations × both platforms pass the glyph-coverage test; zero empty boxes.
- **SC-002**: Typing a single key name ("L", "Space") into the filter lists every action bound to that key, with the matching chip highlighted.
- **SC-003**: From any section of the main shell the sheet opens in one activation (nav-rail item), with or without the getting-started card.
- **SC-004**: A binding change made while the sheet is open is visible on the sheet on the next rendered frame.
- **SC-005**: At a 1280×800 test viewport with the full host catalog and default bindings, the controls page's total content height is ≤ 60% of the pre-feature single-column layout's height measured in the same harness.
- **SC-006**: The getting-started card's rendered text contains zero shortcut clauses (no "Shortcuts:" in en-US or pt-BR strings).

## Assumptions

- The action catalog, categories, registry, rebinding and conflict handling come from 007 / 011-plugin-ui-contributions and are reused as-is (display labels excepted, FR-012).
- Card, chip and row components and theme tokens from earlier polish waves are reused.
- Printing happens from the system browser via the HTML export; no native print dialog, PDF crate or OS menu bar is added.
- The sheet is read-only; rebinding stays on the controls page.
- No new catalog action (e.g. a "show shortcuts" hotkey) is added in this feature.

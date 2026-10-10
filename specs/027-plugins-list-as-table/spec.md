# Feature Specification: Plugins List as a Real Table

**Feature Branch**: `feature/027-plugins-list-as-table`

**Created**: 2026-09-30

**Status**: Draft

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/011-browse-and-manage-polish/003-plugins-list-as-table.md (id 003, plugins-list-as-table)."

**Source**: UI/UX review §3.6 Plugins (UX-27, UX-28, UX-29), §4.6, §5.4.

## Clarifications

### Session 2026-09-30 (clarify reviewer)

Sources: macro-spec breakdown `011-browse-and-manage-polish/003-plugins-list-as-table.md` (B), constitution (C), existing code — `crates/modplayer-ui/src/plugins_view.rs`, `crates/modplayer-core/src/plugins/view.rs`, `controller.rs::plugin_restart`, `locales/*/plugins.ftl`, `modplayer/src/main.rs` (`with_min_inner_size([960, 640])`), `modplayer-ui/src/layout.rs` (`AUTO_HIDE_THRESHOLD = 1024`, `HOST_CONTENT_FLOOR = 560`), and 009 `contracts/ui-plugins.md`.

- Q: What exactly are the table's columns? → A: Seven, in this order: **Name** (name, with version as secondary text in the same cell), **Source**, **Enabled**, **Health**, **Permissions**, **Resource use** (CPU and memory in one cell), **Actions**. The breakdown lists "name and version … resource use" as the data columns (B); the trailing Actions column is the conventional home for the row-owned controls B requires. Separate Version/CPU/Memory header columns are removed.
- Q: What does "minimum width" mean for the 17-plugin acceptance test? → A: *(Derived + default)* The window's minimum inner size is 960×640 logical points (`main.rs`); below 1024 pt the dock auto-hides (018). The test runs at a 960 pt wide window. Additionally, as a stronger default, the table MUST lay out without overflow or overlap at any available section width ≥ 560 pt (`HOST_CONTENT_FLOOR`, the narrowest host content any dock state leaves). Columns have minimum widths whose sum plus spacing ≤ 560 pt; extra width is distributed to Name, Source and Permissions/Resource use as the plan chooses.
- Q: Vertical overflow with 17 rows? → A: *(Default)* Rows sit in a vertical scroll area under a header that stays aligned with (and does not scroll horizontally away from) its columns. Horizontal scrolling MUST NOT be needed or present.
- Q: How is truncation revealed? → A: *(Default, egui convention)* Truncated text ends in an ellipsis; hovering shows a tooltip with the full value; the cell's accessible name/value is always the full, untruncated text. Non-truncated cells show no tooltip.
- Q: Health words? → A: *(Derived from B)* Displayed words become **healthy**, **degraded**, **suspended** (replacing today's "ok"/"warning"). Mapping: `Health::Ok` → healthy, `Health::Warning` → degraded, `Health::Suspended` → suspended. The `Health` enum is unchanged; only `plugins-health-*` strings change, in both `en-US` and `pt-BR`. Invalid-manifest rows show no health word (unchanged).
- Q: Health colour source — is FR-009 already met? → A: *(Derived from code)* `health_color` already maps to theme roles (`positive`/`warning`/`danger`). This feature keeps that mapping, keeps the state word as the primary carrier (dot/colour decorative), and adds a regression test asserting no `Color32` literal (`from_rgb*`, `Color32::<NAMED>` constants other than `TRANSPARENT`) appears in `plugins_view.rs`. High-contrast appearance resolves the same roles from its own palette (017).
- Q: Suspension reason — where does it come from, and can it be missing? → A: *(Derived)* `Lifecycle::Suspended { cause: SuspendCause }` always carries a cause. The row model gains `suspend_cause: Option<SuspendCause>` (Some iff Suspended). The row shows the existing `plugin-suspended-cause-*` string (e.g. "it used too much CPU") next to/under the health word within the Health cell (truncating with tooltip). The "reason unavailable" fallback string `plugins-suspended-reason-unknown` is kept only for defensive completeness (Suspended health with no cause), and is tested via the pure mapping function.
- Q: Restart action behaviour? → A: *(Derived)* Actions cell shows a **Restart** button only when health is suspended; it calls the existing `PlaybackController::plugin_restart(id)` (which also dismisses the `plugin-suspended` notification). Accessible name includes the plugin name (e.g. "Restart {plugin}"). A plugin auto-disabled after its third suspension is `Disabled`, not suspended, so shows no Restart (existing 009 rule; re-enable via the Enabled switch).
- Q: How do panel controls fit into one row when a plugin has several panels? → A: *(Default)* Per-panel Show/Hide (session-only) and Enable/Disable (persisted) semantics from 011 FR-006 are preserved unchanged. Actions cell: a plugin with **one** panel shows that panel's Show/Hide and Enable/Disable buttons inline; a plugin with **two or more** panels shows a "Panels (N)" disclosure control that expands, inside the same row's expansion area (same mechanism as permissions), one line per panel with its title and both buttons. A plugin with no panels shows no panel controls. No panel controls ever render as a separate indented line outside the row's frame. (All bundled plugins/fixtures today register ≤ 1 panel, so the inline case is the common one.)
- Q: Expansion state semantics? → A: *(Default)* Per-row, session-only, not persisted; several rows may be expanded at once; permissions and panels disclosures toggle independently. Expansion is keyed by `PluginId` so it survives live repaints (500 ms) and health changes. The expanded area grows the row in place, spans the row's width below its cells, and wraps text within the window (no horizontal overflow).
- Q: Permissions count wording and zero case? → A: *(Default)* Collapsed cell shows the count as a number with a disclosure control (e.g. "3 ▸", accessible name "Show 3 permissions of {plugin}" / "Hide …"). Expanded list uses the existing `permission-*` explanation strings, one per line, in catalog order. With 0 permissions the cell shows "0" and no disclosure control. Permissions are read-only here.
- Q: Resource-use format and budgets? → A: *(Default, consistent with existing memory format)* Both figures use a "used / budget" form in mono (tabular) digits: CPU as `{used} % / {budget} %` where `used = cpu_pct_of_share × share_budget_pct / 100` (one decimal) and budget is the plugin runtime's CPU share (today 10 %); memory as `{used} MB / {budget} MB` (one decimal used, whole budget; today 64 MB). Budget values MUST be read from the runtime/host constants, not hard-coded in the locale string (today's `plugins-memory` literal "64 MB" is replaced by a `$budget` argument). Both lines render inside the single Resource use cell, labelled "CPU"/"Mem", right-aligned so digits align down the column.
- Q: Not-running / no data yet? → A: *(Derived: 009 FR-023)* Unless the plugin is `Active` with gauges, the Resource use cell shows "—" for each figure (existing `plugins-dash`).
- Q: Over-budget indication? → A: *(Default)* When a figure is ≥ its budget (used ≥ 100 % of share, or memory ≥ limit), the cell appends the text "over budget" (`plugins-over-budget`) and may tint with the `warning` role; the text is the carrier.
- Q: Invalid-manifest rows? → A: *(Derived: 009 contract)* Name/Source/inert Enabled switch render normally; the invalid-manifest sentence occupies the Health→Resource use span as a single truncating cell with full text on hover. No actions.
- Q: Sort order, keyboard order? → A: *(Derived)* Rows stay sorted by name case-insensitively (existing). Tab order within a row runs left→right: Enabled switch, permissions disclosure, panel controls/disclosure, Restart. Header labels are not focusable.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Scan many plugins in aligned columns (Priority: P1)

A user with a dozen or more plugins installed opens the plugins screen and sees a real table: column headers sit directly above aligned columns for name and version, source, enabled, health, permissions and resource use. Every cell stays inside its column; long values truncate and reveal the full value on hover.

**Why this priority**: Today the screen is unusable past two plugins. Legible layout is the core value.

**Independent Test**: Install seventeen plugins, set the window to its minimum width, and check that no cell overflows its column or overlaps another.

**Acceptance Scenarios**:

1. **Given** seventeen installed plugins, **When** the plugins screen is shown at minimum window width, **Then** every column stays within the window and no cell overlaps another.
2. **Given** a plugin with a very long name or source, **When** it is shown, **Then** the value truncates within its cell and the full value appears on hover.
3. **Given** the table, **When** viewed, **Then** each header label aligns with its column of cells.

---

### User Story 2 - Read permissions as a list (Priority: P1)

A user sees each plugin's permissions as a count with a control to expand. Expanding lists the granted permissions one per line instead of a comma-joined sentence.

**Why this priority**: The run-on permissions cell is the worst overflow offender and matters for trust decisions.

**Independent Test**: Expand a plugin with several permissions; each appears on its own line, inside the window.

**Acceptance Scenarios**:

1. **Given** a plugin with N granted permissions, **When** the row is collapsed, **Then** the cell shows the count N and an expand control.
2. **Given** the control is activated, **When** expanded, **Then** the permissions are listed one per line; activating again collapses them.
3. **Given** a plugin with no permissions, **When** shown, **Then** the count reads 0 and nothing is expandable to an empty list.

---

### User Story 3 - Understand health and act on a suspended plugin (Priority: P2)

Health shows as a state word (healthy, degraded, suspended) with a colour from the shared theme tokens; colour is never the only carrier. A suspended plugin's row says why it was suspended and offers a restart action in the row.

**Why this priority**: Makes problems recognisable and recoverable without hunting.

**Independent Test**: Suspend a plugin; its row shows "suspended" in words and colour, the reason, and a restart action that restarts it.

**Acceptance Scenarios**:

1. **Given** a suspended plugin, **When** the list is shown, **Then** the row shows the word "suspended", its state colour (theme `danger` role), the suspension reason from its `SuspendCause` (e.g. "it used too much CPU"), and a Restart action in its Actions cell; activating Restart calls `plugin_restart` and the row leaves the suspended state.
2. **Given** healthy and degraded plugins, **When** shown, **Then** each shows the word "healthy" or "degraded" respectively; no restart action is offered.
3. **Given** high-contrast appearance, **When** health is shown, **Then** state remains distinguishable by word.

---

### User Story 4 - Resource use against budgets, row-owned controls (Priority: P2)

Each row shows CPU share and memory in tabular figures against their budgets, so a figure is meaningful without knowing the budget. Row controls (show/hide panels, enable/disable, restart when suspended) sit in the row itself, not on an indented second line.

**Why this priority**: Completes the table; each part independently improves the screen.

**Independent Test**: Check an active row shows e.g. "CPU 1.2 % / 10 %" and "Mem 3.2 MB / 64 MB" in mono digits, and all controls are inside the row's own frame.

**Acceptance Scenarios**:

1. **Given** a plugin with CPU and memory budgets, **When** shown, **Then** both figures are shown with their budgets in tabular figures that align down the column.
2. **Given** any plugin row, **When** shown, **Then** its controls appear within the row, with no second indented line.
3. **Given** a plugin at or over a budget, **When** shown, **Then** the cell includes the text "over budget"; colour is not the only carrier.
4. **Given** a plugin with two or more panels, **When** its "Panels (N)" disclosure is activated, **Then** one line per panel with its Show/Hide and Enable/Disable controls appears inside that row's expansion area.

---

### Edge Cases

- Zero plugins installed: an empty-state message replaces the table body.
- Window at minimum width (960 pt) or section width down to 560 pt: columns shrink/truncate; nothing overlaps; no horizontal scroll.
- Expanded permissions with many entries: expanded area stays within the row's column and window.
- Plugin with no resource data yet (just started): placeholder shown, not a blank or misleading figure.
- Suspended plugin with no recorded reason (defensive only; the lifecycle always carries a cause): the fallback text "reason unavailable" is shown.
- Plugin changes health while list is open: row updates in place on the next 500 ms repaint, keeping expansion state (keyed by plugin id).
- Plugin auto-disabled after its third suspension: shows as disabled (Enabled switch off), no Restart action.
- Invalid-manifest row: reason spans Health→Resource use as one truncating cell; no actions.
- Keyboard-only use: expand control, row controls and restart are reachable and operable, with visible focus.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The plugins list MUST render as a table whose column headers align (same x-extent) with the cells beneath, with columns in this order: Name (name + version), Source, Enabled, Health, Permissions, Resource use, Actions.
- **FR-002**: Every cell MUST stay within its column; long values MUST truncate with an ellipsis and show the full value in a hover tooltip; accessible text MUST be the full value.
- **FR-003**: With seventeen plugins at the 960 pt minimum window width, and at any section width ≥ 560 pt, all columns MUST stay within the available width with no overlapping cells and no horizontal scrolling; rows scroll vertically under an aligned header.
- **FR-004**: The permissions column MUST show a count and (when count > 0) a disclosure control that expands the plugin's granted permissions, one `permission-*` explanation per line in catalog order, inside the row; it MUST be collapsible. Expansion state is per-row, session-only, keyed by plugin id.
- **FR-005**: The resource-use column MUST show CPU as `used % / budget %` and memory as `used MB / budget MB` in mono digits, with budget values taken from runtime/host constants (not locale literals); "—" when not Active; the text "over budget" when used ≥ budget.
- **FR-006**: Health MUST display as a state word — healthy (`Health::Ok`), degraded (`Health::Warning`), suspended (`Health::Suspended`) — in all shipped locales; colour MUST come from theme roles (`positive`/`warning`/`danger`), never literals in the view, and MUST NOT be the only carrier of state.
- **FR-007**: Each row's controls (per-panel show/hide and enable/disable, restart when suspended) MUST be inside the row's Actions cell or its in-row expansion area, never a separate indented line. One panel → inline buttons; ≥ 2 panels → a "Panels (N)" disclosure. 011 FR-006 panel semantics are unchanged.
- **FR-008**: A suspended plugin's row MUST show its `SuspendCause` reason text (existing `plugin-suspended-cause-*` strings) and a Restart action calling `plugin_restart`; non-suspended rows MUST NOT show Restart.
- **FR-009**: `plugins_view.rs` MUST contain no colour literals; a test MUST guard this (health colours come only from theme roles).
- **FR-010**: All interactive elements MUST be keyboard operable with visible focus and accessible names carrying the plugin name (and panel title for panel controls), including in high-contrast appearance; tab order runs left→right within a row, rows top→bottom.
- **FR-011**: Granting, revoking and auditing permissions, and plugin panel/floated-window chrome, are out of scope and MUST be unchanged.

### Key Entities

- **Plugin row** (`PluginRow`, extended): name, version, source, enabled flag, health state, `suspend_cause: Option<SuspendCause>` (new), invalid reason, granted permissions, CPU (% of share) and memory use, panel controls. Budgets come from runtime constants. UI-only expansion state (permissions / panels open) is keyed by `PluginId`, not stored in the row.
- **Health state**: `Health::{Ok, Warning, Suspended}` displayed as healthy / degraded / suspended; each has a word and a theme role colour.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With 17 plugins at minimum window width, 0 cells overflow their column or overlap another.
- **SC-002**: Every plugin's health word and permission count are visible in its collapsed row at 960 pt window width, with no horizontal scrolling.
- **SC-003**: A suspended plugin can be identified, its reason read, and restarted in one step from its own row.
- **SC-004**: A search of `plugins_view.rs` (and the source outside the theme module) for plugin health colour literals returns 0 results, enforced by a test.
- **SC-005**: 100% of a plugin's permissions are visible one per line when expanded, with none clipped.

## Assumptions

- The plugin list, row/panel components and design tokens from earlier features exist and are reused.
- Budget values are already exposed by the plugin runtime; no new budgets are defined here.
- Permission data shown is read-only; management stays with the community-registry permission feature.
- Expanding permissions (or panels) grows the row in place rather than opening a separate window.
- The only model change is adding `suspend_cause` to `PluginRow` and exposing the CPU-share and memory budget constants to the UI; no runtime or lifecycle behaviour changes.
- Plugin runtime/engine behaviour is unchanged; this is presentation only, so real-time audio path is untouched.

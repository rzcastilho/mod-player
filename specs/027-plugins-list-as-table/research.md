# Research: Plugins List as a Real Table (027)

**Date**: 2026-09-30 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Every Technical Context unknown is resolved below. Format per item: Decision / Rationale / Alternatives considered.

Code read for this research: `crates/modplayer-ui/src/plugins_view.rs` (277 lines, `ui.horizontal` rows, 8 header labels not tied to cell widths, panel controls as an indented second line), `crates/modplayer-core/src/plugins/view.rs` (`PluginRow`, `row()`), `crates/modplayer-core/src/plugins/mod.rs` (`PluginRecord::budgets: Budgets`, `Lifecycle::Suspended { cause }`), `crates/modplayer-capability-gateway/src/budgets.rs` (`Budgets::DEFAULT`: `share = 100 ms`, `window = 1 s`, `memory = 64 MiB`), `crates/modplayer-plugin-runtime/src/budget.rs` (`cpu_permille_of_share`, 1000 = at cap), `crates/modplayer-ui/src/app.rs` (`Section::Plugins` wraps `plugins_view::show` in the section `ScrollArea`), `crates/modplayer-ui/src/search_view.rs` (026: the view owns its own results `ScrollArea`), `crates/modplayer-ui/src/layout.rs` (`HOST_CONTENT_FLOOR = 560`), `crates/modplayer-core/src/i18n.rs` (en-US is the only runtime locale), `locales/pt-BR/` (only `effects.ftl`, `library.ftl`), `crates/modplayer-ui/tests/design_token_literals.rs` (workspace-wide literal scan, baseline 0), `crates/modplayer-ui/tests/plugins_view.rs` (AccessKit harness with `bounds`), `plugins/fixtures/` (17 packages).

---

## R1. Table mechanism: hand-rolled column layout vs `egui_extras::TableBuilder` vs `egui::Grid`

- **Decision**: A small UI-local, pure `PluginsColumns` layout (`plugins_view.rs`) computes seven column x-extents from the available width once per frame. The header and every row place each cell with `ui.allocate_ui_at_rect` / `ui.scope_builder(UiBuilder::new().max_rect(cell_rect))` into that exact rect, with `Label::truncate()` for text. Header labels use the same rects, so alignment is by construction.
- **Rationale**: No new dependency (Constitution X / VII — `egui_extras` is not in the workspace). A pure `fn layout(available: f32) -> [ColumnExtent; 7]` is unit-testable and property-testable (sum ≤ available, mins honoured, no overlap) without a `Ui`. Cell rects are fixed-width so `truncate()` has a real width to truncate against — the root cause of today's overflow is that `ui.horizontal` gives each label unbounded width. Rows remain ordinary widgets in reading order, so egui's default Tab order and AccessKit tree still apply (the reason 009 avoided a custom grid).
- **Alternatives considered**: `egui_extras::TableBuilder` — built-in resizable columns and sticky header, but adds a crate (needs PR justification), its row heights must be known up front (our rows grow when expanded), and resizable columns are not asked for. `egui::Grid` — aligns columns but sizes them to content (a long name widens the column past the window), no truncation width, no in-row spanning expansion area.

## R2. Column minimum widths and extra-width distribution

- **Decision**: Column gap `theme::space::XS` (4 pt; 6 gaps = 24 pt). Minimum widths (pt): Name 104, Source 56, Enabled 44, Health 80, Permissions 48, Resource use 128, Actions 76 → Σ 536 + 24 = **560** = `HOST_CONTENT_FLOOR`. Extra width above the floor goes by weight: Name 3, Health 2, Source 1, Resource use 1, Actions 1; Enabled and Permissions stay at their minimums. Below 560 pt (not reachable by any dock state, defensive) every column scales proportionally to its minimum so the sum still equals the available width, and cells truncate.
- **Rationale**: Clarification 2 requires mins + spacing ≤ 560. Name carries the longest values; Health carries the suspension reason (the second-longest text); Enabled (a switch) and Permissions (a count + disclosure) have fixed-size content. Actions gets some growth so the one-panel case's two buttons sit on one line at 960 pt window width. Values are constants in one place (`PluginsColumns::MIN`/`WEIGHT`) and are verified by the layout test, not by eye; tuning them later is a one-line change.
- **Alternatives considered**: Equal weights (wastes width on Enabled/Permissions); percentages without minimums (Health word truncates at the floor, violating SC-002); content-measured widths (unstable per frame as gauges change, and reintroduces overflow).

## R3. Vertical scrolling with an aligned header

- **Decision**: `plugins_view::show` gains a `&mut SectionMemory` parameter and owns the rows' `ScrollArea` (`section_memory.scroll_area(&ViewKey::Plugins)`), drawing heading + header above it — the exact pattern 026 introduced for Search. `app.rs`'s `Section::Plugins` arm stops wrapping the view in its own scroll area and records nothing itself. The `ScrollArea` is `vertical()` only; horizontal scrolling cannot occur because every cell is width-bounded (R1).
- **Rationale**: Clarification 3 (header stays above aligned columns).
- **Scrollbar gutter (R3a)**: Columns are computed once per frame from `ui.available_width() − gutter`, where `gutter = spacing.scroll.bar_width + spacing.scroll.bar_outer_margin`, and the rows' `ScrollArea` uses `scroll_bar_visibility(AlwaysVisible)` with non-floating bars, so the header (outside) and the rows (inside) use identical widths every frame whether or not content overflows.
- **Alternatives considered**: Keep the app-level scroll area (header scrolls away with 17 rows — fails clarification 3); `ScrollArea::both` (introduces horizontal scroll — forbidden by FR-003); computing header widths from the previous frame's inner width (one-frame jitter on first show and resize).

## R4. Truncation, tooltips and accessible text

- **Decision**: One helper `truncating_cell(ui, rect, text, style) -> Response` — `Label::new(text).truncate()`; if `response.intrinsic_size.x > rect.width()` (egui's galley reports elided), attach `on_hover_text(full)`. The AccessKit label of a truncated `Label` is already the full galley text (egui uses the job text, not the elided glyphs); a test asserts it.
- **Rationale**: Mirrors `now_playing.rs`/`detail_view.rs`/`rows.rs` existing usage; clarification 4 says non-truncated cells show no tooltip.
- **Alternatives considered**: Always attaching a tooltip (noise, contradicts clarification 4); manual char-count ellipsis (wrong for proportional fonts, loses AccessKit full text).

## R5. Health words and pt-BR locale

- **Decision**: Change `plugins-health-ok` → "healthy", `plugins-health-warning` → "degraded" (key names unchanged; `-suspended` stays "suspended"). Create `locales/pt-BR/plugins.ftl` carrying pt-BR values for every key this feature changes or adds (health words, new column/action/disclosure/resource keys, `plugins-over-budget`, `plugins-suspended-reason-unknown`), following 025's precedent (`locales/pt-BR/library.ftl`: "Not yet a shipped locale … this bundle exists so the `fluent_keys` parity test proves …"). Add a `fluent_keys` parity test for this key subset.
- **Rationale**: FR-006 says "all shipped locales"; en-US is the only runtime locale (`i18n.rs`), but NFR-7.1 / Constitution X require pt-BR to ship first and the repo already stages pt-BR per feature. Keeping key names avoids churn in `accessibility.rs`/`fluent_keys.rs` beyond value assertions.
- **Alternatives considered**: Rename keys to `plugins-health-healthy/-degraded` (pure churn); skip pt-BR because it is not loaded (contradicts spec wording and 025/026 precedent); translating the entire existing `plugins.ftl` into pt-BR (out of scope — only this feature's keys).

## R6. Suspension cause in the row model

- **Decision**: `PluginRow.suspend_cause: Option<SuspendCause>` = `Some(cause)` iff `record.lifecycle` is `Lifecycle::Suspended { cause }`. The UI maps it with a pure `fn suspension_reason(health: Option<Health>, cause: Option<SuspendCause>) -> Option<String>`: `Some(cause)` → the existing `plugin-suspended-cause-*` string (same mapping `plugin_panels.rs` uses for its placeholder — reuse that key-mapping fn, moved to a shared `pub(crate)` helper if it is private); `Suspended` with `None` → `plugins-suspended-reason-unknown` ("reason unavailable"); otherwise `None`.
- **Rationale**: Clarification 7. `SuspendCause` is `Copy`; `modplayer-ui` already depends on `modplayer-plugin-runtime`.
- **Alternatives considered**: Pre-rendering the reason string in core (core would call `tr` for a UI-only string; the other `PluginRow` fields stay typed).

## R7. Budgets in the row model

- **Decision**: `PluginRow` gains `cpu_budget_pct: f32` and `memory_budget_bytes: u64`, derived in `view.rs::row()` from `record.budgets`: `cpu_budget_pct = share.as_secs_f64() / window.as_secs_f64() × 100` (= 10.0 for `Budgets::DEFAULT`), `memory_budget_bytes = budgets.memory as u64` (= 64 MiB). The UI formats `used` for CPU as `cpu_pct_of_share × cpu_budget_pct / 100` (one decimal) and budget with `{:.0}`; memory used `{:.1}` MB, budget whole MB (`bytes / 1_048_576`). Locale strings: `plugins-resource-cpu = CPU { $used } % / { $budget } %`, `plugins-resource-memory = Mem { $used } MB / { $budget } MB` (replacing `plugins-cpu` / `plugins-memory`; the literal "64 MB" disappears). Over budget: `cpu_pct_of_share ≥ 100.0` or `memory_bytes ≥ memory_budget_bytes` → append `plugins-over-budget` ("over budget") on that line and tint with `roles.warning`.
- **Rationale**: Clarification 11; the record already carries the per-plugin `Budgets` (its doc says a future per-plugin override needs no call-site change) — reading from the record honours that seam. No Capability Gateway change → no GOV-3.2 sign-off needed.
- **Alternatives considered**: UI reads `Budgets::DEFAULT` directly (ignores the per-record seam; UI would duplicate the share/window arithmetic); a `Budgets::share_pct()` method in the gateway (touches a GOV-3.2 crate for a one-line computation).

## R8. Expansion state (permissions / panels)

- **Decision**: UI-local `PluginsViewState { permissions_open: HashSet<PluginId>, panels_open: HashSet<PluginId> }`, owned by `App` next to `search_view` state, passed `&mut` into `plugins_view::show`. Not persisted; pruned of ids no longer in `view.rows` each frame (cheap: ≤ tens of rows).
- **Rationale**: Clarification 10: per-row, session-only, keyed by `PluginId`, independent toggles, survives the 500 ms repaint. Keeping it out of `PluginRow` keeps the core model pure (spec Key Entities).
- **Alternatives considered**: `egui::CollapsingState` keyed by `Id` (works, but ties state to egui memory that `SectionMemory`'s epoch resets and is harder to assert in tests); storing in core (UI concern leaking into the model).

## R9. In-row expansion area and Actions cell

- **Decision**: A row is `row_frame(ui, id, |ui| { cells_line; if any_open { expansion_area } })`. The cells line allocates the seven cell rects at the row's top; row height = max cell content height (the Actions cell uses `horizontal_wrapped` inside its rect so two buttons wrap onto a second line when narrow instead of overflowing). The expansion area is a full-width (`row_rect.width()`) vertical block below the cells, indented to the Name column's left edge, containing (in order) the permissions list (one `Label::wrap()` per `permission-*` explanation) and the panels list (one line per panel: truncating title + Show/Hide + Enable/Disable). Text wraps; nothing extends past the row rect.
- **Rationale**: Clarifications 8–10; FR-007 (controls inside row's frame). `row_frame` already provides hover/pressed fill beneath content with zero layout change (014 I6).
- **Alternatives considered**: Expansion inside the Permissions column only (a 48 pt column cannot hold sentences); a popup (spec Assumption: grows in place, no separate window).

## R10. Keyboard order and accessible names

- **Decision**: Widgets are created left→right within the row (Enabled switch, permissions disclosure, panel inline buttons or "Panels (N)" disclosure, Restart), then expansion-area controls top→bottom, so egui's creation-order Tab traversal gives FR-010's order. Accessible names: switch `plugins-enable-toggle` (unchanged, "Enable {plugin}"); disclosure `plugins-permissions-show`/`-hide` ("Show {count} permissions of {plugin}" / "Hide …") with visible text "{count} ▸/▾"; panels disclosure `plugins-panels-show`/`-hide` ("Show panels of {plugin}" …) with visible text `plugins-panels-count` ("Panels ({ $count })"); panel buttons get `WidgetInfo` labels `plugins-panel-show-a11y` etc. ("Show {title} panel of {plugin}") while the visible text stays the short `plugin-panel-show`/`-hide`/`-enable`/`-disable`; Restart visible `plugin-panel-restart` ("Restart"), accessible `plugins-restart-a11y` ("Restart {plugin}"). Header labels are plain `Label`s (not focusable). Focus ring comes from existing `paint_focus_ring` / theme (visible in high contrast, 017).
- **Rationale**: FR-010; the current panel buttons rely on a neighbouring title label for context, which fails "accessible names carrying the plugin name".
- **Alternatives considered**: Visible long labels ("Restart Wellbehaved") — too wide for the Actions column.

## R11. Colour-literal regression test

- **Decision**: New test `plugins_view_has_no_colour_literals` in `tests/plugins_view.rs` reads `src/plugins_view.rs` via `include_str!`, strips `//` comments and `use` lines, and asserts no match for `Color32::from_`, `Color32::[A-Z]` (except `Color32::TRANSPARENT`), `rgb(`, `hex_color!`. It also asserts `health_color` returns `roles.positive/warning/danger` for both normal and high-contrast `Roles`.
- **Rationale**: FR-009/SC-004 name a test scoped to this file; the existing workspace scan (`design_token_literals.rs`, baseline 0) already covers the rest of the source outside `theme/`, so SC-004's "and outside the theme module" clause is satisfied by keeping that scan green.
- **Alternatives considered**: Relying on the workspace scan alone (spec explicitly asks for a guard named on this file).

## R12. 17-plugin minimum-width acceptance test

- **Decision**: Use the discovered `plugins/fixtures/` set — exactly 17 packages (effects-observer, flood, focus-a, focus-b, hang, invalid, leak, noready, observer, throw, ui-icons, ui-notify, ui-overlay, ui-panel, ui-settings, ui-shortcuts, wellbehaved) — rendered via the existing AccessKit harness with `screen_rect` widths 960 (window minimum) and 560 (section floor; the view rendered directly into a 560 pt `Ui`). Assertions from node `bounds`: every cell node's rect ⊆ its column's extent and ⊆ the available rect; no two cell rects in the same row intersect; header label rect x-range == its column extent; no horizontal `ScrollArea` present. Plus a proptest over `PluginsColumns::layout(w)` for `w ∈ [560, 4000]` (sum ≤ w, each ≥ min, monotone non-overlap) — Constitution VIII (layout arithmetic).
- **Rationale**: SC-001/SC-002 are measurable with existing test infrastructure; the fixture count matching 17 removes the need for synthetic rows. `invalid` gives the invalid-manifest span case for free.
- **Alternatives considered**: Synthetic `PluginRow`s (would bypass `from_records`, less end-to-end); screenshot diffing (not in repo tooling).

## R13. Invalid-manifest span

- **Decision**: For `invalid_reason.is_some()`, the Health, Permissions and Resource use extents are merged into one rect (Health.min → Resource.max) holding one truncating label `plugins-invalid-manifest`; the Actions cell is empty.
- **Rationale**: Clarification 14 (009 contract example table).
- **Alternatives considered**: Putting the reason in the Health cell only (truncates to near-nothing at the floor).

## Baseline (T001)

Green before changes: `modplayer-core --lib view` 5 passed; `modplayer-ui` tests `plugins_view`, `accessibility`, `fluent_keys`, `design_token_literals` 78 passed. No pre-existing failures.

## Caller audit of legacy plugin strings (T002)

Keys `plugins-col-version`, `plugins-col-cpu`, `plugins-col-memory`, `plugins-cpu`, `plugins-memory`, `plugins-list-separator`: only referenced by `crates/modplayer-ui/src/plugins_view.rs`, `locales/en-US/plugins.ftl`, and tests `crates/modplayer-ui/tests/{fluent_keys,plugins_view,notification_stack}.rs`. `modplayer/` has none. Safe to remove once view and tests are rewritten (notification_stack.rs key list at ~316-322 must be updated too).

## Pattern reference (T003)

026 `search_view.rs` uses `SectionMemory` + `ViewKey::Search` with an owned `ScrollArea`. `app.rs` `Section::Plugins` arm (~line 711) uses `ViewKey::Plugins`; copy that pattern. No edits.

## Manual walk deviations (T042, 2026-09-30)

- **Health dot drew as tofu (M1, fixed).** `●` (U+25CF) is not in the app's configured fonts (neither the proportional nor the monospace family), so every Health cell showed a hollow box `□` instead of a coloured dot. **Fix**: the dot is now painted with `circle_filled` (diameter `theme::space::SM`, centred on the body line, health colour), following the onboarding step-dot idiom in `shell.rs`. It no longer depends on any font. Pinned by `health_dot_is_painted_not_a_font_glyph`, which installs the app fonts and checks for one painted positive-colour circle per healthy row and no `●` text shape. Contract T9's "`●`" now means a painted dot.
- **Truncated cells showed two tooltips (M3, fixed).** egui's `Label::truncate()` adds its own full-text tooltip when a label is elided (`show_tooltip_when_elided` is on by default). On top of the view's T6 tooltip, that stacked two copies. **Fix**: `show_tooltip_when_elided(false)` in `truncating_cell` and on the Resource lines. `long_name_truncates_with_tooltip_and_full_accessible_name` now asserts exactly one tooltip (`== 2` full-text nodes, where it used to accept `>= 2`).
- **Memory gauge is always 0 (M7, pre-existing, not fixed here).** `PluginGauges::set_used_bytes` (`crates/modplayer-plugin-runtime/src/budget.rs`) has no caller anywhere in the workspace, so `used_bytes()` stays 0. Every active row reads `Mem 0.0 MB / 64 MB`, even the `leak` fixture on its way to its 64 MiB memory suspension. This gap has existed since 009. Fixing it means changing the runtime, which this feature scopes out ("no runtime change", no GOV-3.2 sign-off). Needs a follow-up issue. The 027 formatting and over-budget logic for memory are covered by unit tests (T031/T11/T12).
- **"over budget" not observed live (M7).** `hang` went from `degraded` (CPU 4.1 % / 10 %) to `suspended` between two 500 ms repaints, so the over-budget suffix never reached the screen. Unit tests cover it (`cpu_line`/`memory_line`, T12).

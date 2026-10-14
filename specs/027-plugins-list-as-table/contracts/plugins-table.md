# Contract: Plugins table UI (027)

Supersedes 009 `contracts/ui-plugins.md` §2 "Layout" and 011 `contracts/ui-panels.md` L6 (panel controls placement). 009 §1 placement, §3 notifications, and 011 FR-006 panel semantics are unchanged. Types: [data-model.md](../data-model.md). Test file: `crates/modplayer-ui/tests/plugins_view.rs` unless noted.

## Entry point

```rust
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    state: &mut PluginsViewState,
    section_memory: &mut SectionMemory,
)
```

`app.rs`'s `Section::Plugins` arm keeps its 500 ms `request_repaint_after`, calls `show`, and no longer wraps it in a `ScrollArea` (the view owns it and records `ViewKey::Plugins`, like 026 Search).

## Layout rules (T = table)

| ID | Rule | Spec |
|----|------|------|
| T1 | Heading `plugins-title`; zero rows → `plugins-empty` label, no header, no scroll area. | Edge case |
| T2 | Header row (outside the scroll area) shows 7 labels in order: `plugins-col-name`, `-source`, `-enabled`, `-health`, `-permissions`, `-resource`, `-actions`. `plugins-col-version`, `-cpu`, `-memory` are removed. Header labels are non-focusable `Label`s and truncate like cells. | FR-001 |
| T3 | Header label i and every row's cell i share the same x-extent from `PluginsColumns::layout(width)` (±0.5 pt). | FR-001, US1-3 |
| T4 | Every cell's content rect lies within its column extent; no two cells of one row intersect; every rect lies within the section's available rect. | FR-002, FR-003, SC-001 |
| T5 | Rows live in a vertical-only `ScrollArea`; no horizontal scroll area exists at any width ≥ 560 pt. | FR-003 |
| T6 | Text cells: `Label::truncate()`; tooltip with full text **only** if truncated; AccessKit label is always the full text. | FR-002, US1-2 |
| T7 | Name cell: name (body) + version (secondary style) on one truncating line, e.g. `Wellbehaved  1.0.0`. Invalid rows' version is `plugins-dash`. | FR-001 |
| T8 | Enabled cell: existing `switch(Checkbox)` named `plugins-enable-toggle`; inert for invalid rows (unchanged 009 behaviour). | FR-001 |
| T9 | Health cell: `●` (colour `health_color`) + word (`plugins-health-*`); Suspended adds reason (`suspension_reason`) after the word, same truncating line. Word always present when `health.is_some()`. | FR-006, FR-008 |
| T10 | Permissions cell: count in mono digits. `count > 0` → disclosure button, visible text `"{count} ▸"` / `"{count} ▾"`, accessible name `plugins-permissions-show`/`-hide` (`$count`, `$plugin`). `count == 0` → `"0"` label only. | FR-004, US2 |
| T11 | Resource cell: two right-aligned mono lines `plugins-resource-cpu` (`$used` 1 dp, `$budget` 0 dp) and `plugins-resource-memory` (`$used` 1 dp MB, `$budget` whole MB), or `plugins-resource-cpu-none` / `plugins-resource-memory-none` ("CPU —" / "Mem —") when not Active. Budget args come from `row.cpu_budget_pct` / `row.memory_budget_bytes`. | FR-005, US4-1 |
| T12 | Over budget (`cpu_pct_of_share ≥ 100` or `memory_bytes ≥ memory_budget_bytes`): that line appends `plugins-over-budget` and uses `roles.warning`. | FR-005, US4-3 |
| T13 | Actions cell (`horizontal_wrapped` within its rect): 1 panel → Show/Hide + Enable/Disable buttons; ≥ 2 panels → `plugins-panels-count` disclosure (`$count`), accessible `plugins-panels-show`/`-hide` (`$plugin`); Suspended → Restart button (`plugin-panel-restart`, accessible `plugins-restart-a11y` `$plugin`) calling `controller.plugin_restart(row.id)`. No Restart when not Suspended. | FR-007, FR-008 |
| T14 | Expansion area: inside the same `row_frame`, below the cells line, from Name.min_x to row right edge. Permissions (if open): one wrapping label per `permission-*` explanation, catalog order. Panels (if open): one line per panel — truncating title, Show/Hide, Enable/Disable. Nothing outside the row frame; no legacy indented line. | FR-004, FR-007, SC-005 |
| T15 | Panel buttons: visible `plugin-panel-show`/`-hide`/`-enable`/`-disable`; accessible names `plugins-panel-show-a11y` / `-hide-a11y` / `-enable-a11y` / `-disable-a11y` (`$title`, `$plugin`). Semantics unchanged: Show/Hide → `plugin_panel_show`/`plugin_panel_close`; Enable/Disable → `plugin_panel_set_disabled`. | FR-007, FR-010 |
| T16 | Invalid row: Name/Source/inert switch, then one truncating `plugins-invalid-manifest` label over `span(Health, Resource)`; Actions empty. | Clarif. 14 |
| T17 | Tab order: rows top→bottom; within a row Enabled → permissions disclosure → panel controls/disclosure → Restart → expansion-area controls. Focus ring visible (normal + high contrast). | FR-010 |
| T18 | Expansion state from `PluginsViewState`, keyed by `PluginId`, survives repaint and health change; pruned for vanished ids. | FR-004, Edge |
| T19 | `plugins_view.rs` contains no colour literal (`Color32::from_*`, `Color32::<NAMED>` except `TRANSPARENT`, `rgb(`, `hex_color!`). | FR-009, SC-004 |
| T20 | No uninstall control (009 FR-013, unchanged). Permission grant/revoke UI absent. | FR-011 |

## Pure functions (unit/prop tested)

| Fn | Contract |
|----|----------|
| `PluginsColumns::layout(w)` | data-model §3 L1–L4; proptest `w ∈ [560, 4000]` and `w ∈ [0, 560)`. |
| `health_color(roles, h)` | positive/warning/danger for `Roles` of normal and high-contrast palettes. |
| `health_word_key(h)` | Ok→`plugins-health-ok`, Warning→`-warning`, Suspended→`-suspended`. |
| `suspension_reason(h, cause)` | data-model §2. |
| `cpu_line(row)` / `memory_line(row)` | formats + over-budget flag, e.g. permille 120, budget 10 → `"CPU 1.2 % / 10 %"`, not over; permille 1000 → over. `memory_bytes = 3.2 MiB`, budget 64 MiB → `"Mem 3.2 MB / 64 MB"`. |

## Test map

| Test | Rules |
|------|-------|
| `seventeen_fixtures_fit_at_min_window_width` (960) and `…_at_section_floor` (560) | T3, T4, T5, SC-001, SC-002 |
| `header_aligns_with_columns` | T2, T3 |
| `long_name_truncates_with_tooltip_and_full_accessible_name` | T6, T7 |
| `permissions_count_and_disclosure` / `permissions_expand_one_per_line` / `zero_permissions_no_disclosure` | T10, T14, US2 |
| `health_words_healthy_degraded_suspended` | T9, US3-2 |
| `suspended_row_shows_reason_and_restart_restarts` | T9, T13, US3-1, SC-003 |
| `resource_cell_shows_budgets_and_dashes` / `over_budget_text` (pure) | T11, T12 |
| `single_panel_controls_inline_in_row` / `multi_panel_disclosure` (pure-rendered with a synthetic 2-panel registry) | T13, T14, T15 |
| `expansion_survives_repaint_and_health_change` | T18 |
| `invalid_row_span` | T16 |
| `tab_order_within_row` (accessibility.rs) | T17 |
| `plugins_view_has_no_colour_literals` | T19 |
| existing `no_uninstall_control`, `toggle_disables_in_one_action`, `notification_actions_call_facade` stay green | T8, T20 |

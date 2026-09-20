# Contract: Plugins section (UI)

Crate: `modplayer-ui` (`src/plugins_view.rs`, `src/notifications.rs`,
`src/app.rs`, `locales/en-US/plugins.ftl`). Requirement ids: FR-011,
FR-013, FR-023, FR-024; 001 FR-015; NFR-6.1, NFR-6.2, NFR-7.1;
Constitution X. Decisions: [../research.md](../research.md) R14, R15.

## 1. Placement

`App::update`'s `Section::Plugins` arm calls `plugins_view::show(ui,
&mut controller)` instead of `shell::plugins_placeholder`. The
`placeholder-plugins` key is removed from `app.ftl`; `shell.rs` keeps the
rail entry unchanged (`nav-plugins`, `action-nav-plugins` from 007).
Settings › Plugins stays its existing placeholder.

## 2. Layout

```text
Plugins                                   (heading, `plugins-title`)
┌──────────────┬─────────┬─────────┬─────────┬───────────┬──────────────────────────┬────────┬──────────────┐
│ Name         │ Version │ Source  │ Enabled │ Health    │ Permissions              │ CPU    │ Memory       │
├──────────────┼─────────┼─────────┼─────────┼───────────┼──────────────────────────┼────────┼──────────────┤
│ Hang fixture │ 1.0.0   │ bundled │ [x]     │ suspended │ Observe playback, Control│ —      │ —            │
│ Invalid fix… │ —       │ bundled │ [ ]     │ invalid manifest: The required permission 'teleport.everywhere' is not in the permission catalog. │
│ Observer     │ 1.0.0   │ bundled │ [x]     │ ok        │ Observe playback         │ 3 %    │ 1.2 MB / 64 MB│
└──────────────┴─────────┴─────────┴─────────┴───────────┴──────────────────────────┴────────┴──────────────┘
```

- Rows come from `controller.plugins_view().rows` (already sorted by
  name); an empty list renders `plugins-empty` and no table.
- **Enabled**: an `egui::Checkbox` bound to `row.enabled`; toggling calls
  `plugin_enable`/`plugin_disable` immediately (no confirmation, FR-024);
  disabled and unchecked-and-inert for `Invalid` rows. Accessible name
  `plugins-enable-toggle` with `$plugin`.
- **Health**: `plugins-health-ok|warning|suspended` label with a colored
  dot; nothing for invalid rows (the invalid-manifest sentence spans the
  remaining columns).
- **Permissions**: the granted permissions' `permission-*` explanation
  strings joined with `plugins-list-separator` (", "), in catalog order.
- **CPU / Memory**: `plugins-cpu { $pct }` as a percentage of the 10 %
  share (e.g. "42 %") and `plugins-memory { $used }` as
  "<used> MB / 64 MB" (one decimal); `plugins-dash` ("—") unless
  `health.is_some()` and the row is `Active`.
- **No uninstall control** anywhere (FR-013); test asserts no widget with
  an uninstall label exists.
- Repaint: the section requests a repaint every 500 ms while visible so
  gauges stay live, as the Effects panel does for costs.

## 3. Notifications

`notifications.rs` renders `RestartPlugin(id)` as
`notification-action-restart-plugin` and `DisablePlugin(id)` as
`notification-action-disable-plugin`, calling `plugin_restart`/
`plugin_disable`. The toast text is `plugin-suspended` with `$plugin` and
`$cause` (from `plugin-suspended-cause-*`), or `plugin-auto-disabled`.

## 4. Keyboard and accessibility (Constitution X)

Every row's checkbox is focusable in tab order; the table is a plain
vertical list of `ui.horizontal` rows (no custom widget), so egui's
default keyboard navigation applies; every control has an accessible name
carrying the plugin name; all strings come from `plugins.ftl`;
`fluent_keys.rs` gains the new file and `accessibility.rs` gains the
Plugins section walk.

## 5. Tests (`crates/modplayer-ui/tests/plugins_view.rs` + existing suites)

`empty_state_without_fixtures`, `rows_show_every_column_sorted_by_name`,
`invalid_row_shows_reason_and_inert_toggle`, `toggle_disables_in_one_action`,
`suspended_row_shows_dash_gauges`, `no_uninstall_control`,
`notification_actions_call_facade`; `accessibility.rs::plugins_section_controls_named`;
`fluent_keys.rs::plugins_ftl_keys_used_exist`.

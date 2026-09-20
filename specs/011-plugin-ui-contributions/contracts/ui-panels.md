# Contract: Plugin Panel Host (panels, widgets, placement, a11y)

**Feature**: 011-plugin-ui-contributions | **Source**: AR-19 Plugin Panel Host; PL-7.1–7.4; NFR-6.1–6.3; NFR-4.7 | **Consumers**: `modplayer-ui/src/plugin_panels.rs`, `plugins_view.rs`, `now_playing.rs`; `modplayer-core/src/plugins/ui/panel.rs`

## 1. Core rules (`PanelRegistry`, FR-001–FR-007, FR-025)

- **P1** A `register_panel` call is validated whole (`validate_panel`) before any state changes; on refusal the previously registered panel of that id is untouched.
- **P2** Panels per plugin are kept in registration order; re-registering an id keeps its position and replaces its layout; the 17th distinct id is `panel_limit`.
- **P3** `update_widget` applies on the next UI frame, never emits an event, and works while the panel is closed, disabled or the Now Playing view is hidden.
- **P4** `interact(...)` (UI-driven) stores the value and returns it; the controller delivers exactly one `panel_interaction` to the owning handle. A `button` with `action` never reaches `interact` — it goes to `invoke_plugin_action(.., Ui)`.
- **P5** `on_stop(Suspend)` keeps panels (view renders `Placeholder{cause}`); `on_ready` clears them; `on_stop(Disable | Shutdown)` removes them. `closed` (session) and `[plugin_panels].disabled` (persisted) survive all of these and re-apply on re-registration.
- **P6** Widget/panel refusals use FR-024's shape; messages name `widgets[<i>] (<id>)`.

## 2. Placement and persistence (FR-005, FR-006)

- **L1** Dock = fixed 280 px column at the right edge of Now Playing's content, drawn only when ≥ 1 docked panel is visible; panels stacked vertically in (plugin name ↑, registration seq ↑) order inside one `ScrollArea`.
- **L2** Floated = `egui::Window` titled by the panel, `.constrain(true)`, `.resizable(true)`, min 200 × 120, default size = docked size, default pos = centre of the Now Playing content rect; drawn only while `Section::NowPlaying` is shown, after the central panel and before `notifications::show`.
- **L3** Every placement/geometry change writes `[plugin_panels."<identifier>/<panel>"]` through `controller.plugin_panel_set_placement` (settings.toml atomic write, coalesced to at most one write per 500 ms while dragging).
- **L4** On restore, a floated rect outside the current window is clamped inside; the persisted entry is updated on the next move.
- **L5** Header (every placement): plugin icon (or generic glyph, 16 px) · plugin name · panel title · controls `[Float|Dock] [Close] [Disable]`.
- **L6** Plugins-list row (009 FR-023) lists each registered panel with `Show/Hide` (session) and `Enable/Disable` (persisted) controls.

## 3. Widget behaviour and accessibility (FR-002, FR-007a, NFR-6.3)

| kind | egui | AccessKit role · name · value | keys it consumes (`Claim::Keys`) | event |
|---|---|---|---|---|
| `label` | `Label` | `Label` · text | — | — |
| `button` | `Button` | `Button` · label | `Space`, `Enter` | `panel_interaction{value=true}` or `action_invoked{source=ui}` |
| `toggle` | `Checkbox` | `CheckBox` · label · on/off | `Space` | on flip |
| `slider` | `Slider` | `Slider` · label · number | `←/→/↑/↓` (±1 step), `PageUp/PageDown` (±10 steps) | each key step; pointer drag once on release |
| `knob` | `widgets::knob` | `Slider` · label · number | same as slider | same as slider |
| `list` | rows of `selectable_label` in a `ScrollArea` | `ListBox` · label; rows `ListBoxOption` · item label · selected | `↑/↓`, `Home/End` | on selection change (focus follows) — `Enter` no-op |
| `marker_list` | 006 marker rows filtered by `Owner::Plugin(id)` | `ListBox` · label; rows `ListBoxOption` · marker name + m:ss | `↑/↓`, `Enter` (seek) | none (seek is a host user command) |
| `text` | `Label` (wrap) | `Label` · label + text | — | — |
| `meter` | `widgets::peak_meter` single bar | `Meter` · label · 0–100 % | — | — |

- **A1** `WidgetInfo::labeled(role, enabled, label)` on every widget; value text set for slider/knob/toggle/list/meter so a reader announces "Tempo, slider, 120".
- **A2** Focus order: dock follows Now Playing's existing controls in `Tab` order; each panel is one group — header controls, then widgets in layout order. Floated windows follow the dock.
- **A3** Any key a widget's kind does not claim falls through to `actions::dispatch` (007 FR-019 step 2). No panel widget is `Claim::TextLike`.
- **A4** Theme: every colour/font comes from `ui.visuals()`/`ui.style()`; the panel module contains no colour literals (theme.rs owns `overlay_color` only). A theme change repaints on the next frame with zero controller calls into any plugin.
- **A5** A widget draws only from its `WidgetState`; no string from a plugin is ever interpreted as markup, a URL, a Fluent pattern or a file path (FR-022).

## 4. Fluent keys (`plugins.ftl`, en-US)

`plugin-panel-float`, `plugin-panel-dock`, `plugin-panel-close`,
`plugin-panel-disable`, `plugin-panel-enable`, `plugin-panel-show`,
`plugin-panel-hide`, `plugin-panel-suspended = { $plugin }, suspended: { $cause }`,
`plugin-panel-restart`, `plugin-panel-header = { $plugin } — { $title }`,
`plugin-generic-glyph-desc`, `plugin-marker-list-empty`,
`plugin-notification = { $plugin }: { $text }`.

## 5. Named tests

- core `tests/plugin_ui_registry.rs::{register_replaces_atomically, seventeenth_panel_refused, update_label_not_updatable, update_slider_out_of_range, meter_clamps, list_replace_items, closed_survives_reregister, suspend_keeps_disable_removes}`
- core `tests/controller_plugin_ui.rs::{unlabeled_widget_refused_with_path, interaction_delivers_once, update_widget_no_event, button_action_source_ui, request_focus_in_handler_auto_grants, request_focus_from_timer_recorded_only, handler_throw_contained}`
- core `tests/settings.rs::{plugin_panels_round_trip_proptest, plugin_panels_bad_placement_dropped}`
- ui `tests/plugin_panels.rs::{every_kind_has_role_and_name, tempo_slider_announces, arrows_step_and_emit, drag_emits_once, list_selection_follows_focus, unclaimed_key_falls_through, theme_switch_no_events, dock_order_by_name_then_seq, float_clamped_into_window, placeholder_on_suspend, header_attribution}`
- ui `tests/plugins_view.rs::{panel_controls_listed}`; `accessibility.rs` and `fluent_keys.rs` extended.

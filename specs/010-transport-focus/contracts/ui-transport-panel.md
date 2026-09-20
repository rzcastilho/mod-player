# Contract: Transport panel (UI)

**Crate**: `modplayer-ui` (`src/transport_view.rs` new, `src/
now_playing.rs`, `src/actions.rs`, `src/lib.rs`; `tests/transport_view.rs`
new, `tests/{accessibility,fluent_keys,actions,now_playing}.rs`
extended), `locales/en-US/transport.ftl` new, `locales/en-US/
controls.ftl` (+1 action label). Implements FR-008, FR-009, FR-012
(selector), FR-013; research R8, R14, R16. Follows 008's
[ui-effect-chain.md](../../008-effect-chain-and-built-in-nodes/contracts/ui-effect-chain.md)
for placement/toggle and 007's ui-actions.md for dispatch.

## 1. Placement and toggle (FR-013)

- The Now Playing header row gains `selectable_label(transport_open,
  tr("transport-toggle"))` beside "Queue" and "Effects". Open state lives
  in egui temp memory under `now-playing-transport-open`
  (`transport_view::panel_open_id()`), exactly like
  `effects_view::panel_open_id()`: survives track changes, dropped with
  the process.
- `transport_view::toggle_transport_panel(ctx)` flips the flag; it is
  what `invoke(HostAction::ToggleTransportPanel)` calls. Default chord
  `T`, Now Playing scope; when a text field owns keyboard focus the 007
  dispatcher already swallows it (spec edge case).
- When open, `transport_view::show(ui, controller)` is drawn after the
  Effect Chain panel (if open) and before the Queue panel, separated by
  `ui.separator()`.
- Rendered regardless of whether a track is loaded (holder "host", rows
  may be empty).

## 2. Layout

```
[Transport focus]      Holder: host                       Policy [Auto on interaction ▾]   [Take back]
 ┌ Focus fixture A   · holds focus                                              [Give focus] ┐
 ┌ Focus fixture B   · requesting (1st)                                          [Give focus] ┐
 ┌ Well-behaved fixture                                                          [Give focus] ┐
 (no plugin can hold transport focus)            ← only when rows is empty
```

| Element | Widget | Accessible name / role / state |
|---|---|---|
| title | `heading` | "Transport focus" |
| holder | label | "Focus holder: host" / "Focus holder: Focus fixture A" |
| policy | `ComboBox` of `FocusPolicy::ALL` | "Focus policy", value = selected label; change → `controller.set_focus_policy` |
| take back | `Button`, `enabled = holder.is_some()` | "Take focus back to host" |
| row | `ui.horizontal` in a `Frame::group` | group "Focus fixture A, holds focus" / ", requesting, 1st" / (nothing) |
| requesting badge | label, only when `request_order` | "requesting (1st)" (`transport-requesting` with `$order` ordinal via Fluent `NUMBER(..)`) |
| give | `Button`, `enabled = !row.holds` | "Give focus to Focus fixture A" |
| empty state | label | text of `transport-empty` |

Every control reachable with Tab in the order above; `Enter`/`Space`
activates; the ComboBox is keyboard-operable as egui provides. Read from
`controller.transport_focus_view()` every frame; nothing is cached in
the UI beyond the open flag.

## 3. Fluent keys (`locales/en-US/transport.ftl`)

```
transport-toggle = Transport
transport-panel-title = Transport focus
transport-holder = Focus holder: { $holder }
transport-holder-host = host
transport-policy = Focus policy
transport-policy-manual = Manual
transport-policy-auto = Auto on interaction
transport-policy-first = First request wins
transport-take-back = Take focus back to host
transport-give-focus = Give focus to { $plugin }
transport-holds = holds focus
transport-requesting = requesting ({ $order })
transport-row-a11y = { $plugin }, { $state }
transport-empty = No enabled plugin can hold transport focus.
```
`controls.ftl`: `action-nav-toggle-transport-panel = Toggle Transport panel`.

## 4. Behaviour rules

| # | Rule | Spec |
|---|---|---|
| U1 | "Give focus" → `controller.focus_give(row.id)`; "Take back" → `controller.focus_take_back()`; policy change → `controller.set_focus_policy(p)`. One click each; no confirmation. | FR-008, SC-004 |
| U2 | The panel never mutates markers, playback or plugin lifecycle; "Give focus"/"Take back" are not transport actions (they do not trigger the auto-policy hook — they *are* the focus change). | FR-002a |
| U3 | A plugin that gets suspended/disabled disappears from the rows on the next frame; the holder label reads "host" on the same frame the controller's `tick` drained the event. | FR-007, SC-003 |
| U4 | Strings only through `tr`/`tr_args`; every key above guarded by `tests/fluent_keys.rs`. | FR-009 |

## 5. Tests

`tests/transport_view.rs` (offscreen `egui::Context`, `FakeBackend` +
`SyntheticSource`, `MODPLAYER_PLUGIN_FIXTURES=1`):
- `panel_lists_only_transport_control_plugins`
- `holder_and_requesting_badges_render`
- `give_focus_click_changes_holder`
- `take_back_click_returns_host`
- `policy_combo_persists_selection`
- `empty_state_when_no_eligible_plugin`
- `suspended_holder_row_disappears_and_holder_reads_host`

`tests/actions.rs`: `t_toggles_transport_panel_in_now_playing_scope`,
`t_ignored_while_text_field_focused`, `t_ignored_outside_now_playing`.
`tests/now_playing.rs`: `transport_toggle_beside_queue_and_effects`,
`transport_panel_survives_track_change`.
`tests/accessibility.rs`: every element in §2 has a non-empty
`WidgetInfo` label; `tests/fluent_keys.rs`: all §3 keys present.

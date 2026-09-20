# UI Panel fixture

011-plugin-ui-contributions (US1 T059, US2 T078, contracts/ui-panels.md,
contracts/action-registry-plugins.md), holds `ui.panel`,
`transport.control` and `ui.shortcuts`. On `ready_ack`, registers a panel
`"main"` titled "Controls" with one widget of every kind (`label`,
`button` × 3, `toggle`, `slider`, `knob`, `list`, `text`, `marker_list`,
`meter`), registers two `ui.shortcuts` actions (below), then schedules a
20 ms timer.

Its `panel_interaction` handler counts every committed interaction and,
by the activated button's widget id:

- **`hang`** — a deliberate busy loop; the scheduler's per-handler budget
  interrupt (RT3) aborts the call (never suspends the plugin outright on
  its own, L5).
- **`throw`** — a deliberate Lua error (`error("boom")`); RT3's own
  `AbortCause::Exception` contains it (FR-023/SC-007) — the plugin keeps
  running.

The `"take_over"` button instead declares `action = "take_over"` (D3):
it never produces `panel_interaction` at all — the host resolves
`"org.modplayer.fixture.ui-panel.take_over"` against the action registry
and delivers `action_invoked` (`source = "ui"`) instead. Its
`action_invoked` handler calls `api.transport.request_focus()`
synchronously (R16/FR-026): counts as a user interaction, so the default
`AutoOnInteraction` focus policy grants it immediately. The second
registered action, `"focus_me"` (default binding `Shift+K`), exists only
to collide with the `ui-shortcuts` fixture's own `nudge` (M6) — its own
`action_invoked` handling is a no-op beyond the shared counter below.

Its `timer` handler also calls `request_focus()` — outside any
interaction handler, so it is only ever recorded, never auto-granted
(the R16 contrast case).

`debug_probe`:
- `"interaction_count"` — the running count of delivered
  `panel_interaction`s.
- `"last_interaction"` — the last `panel_interaction` payload received.
- `"action_invoked_count"` — the running count of delivered
  `action_invoked`s.
- `"last_action_invoked"` — the last `action_invoked` payload received.
- `"register_bad"` — calls `register_panel` again with a deliberately
  unlabeled widget and returns its refusal (`{ ok = false, code, reason,
  message }`) or `{ ok = true }` — proves M2 (an unlabeled widget is
  refused whole, naming its path, with the real "Controls" panel left
  untouched).

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

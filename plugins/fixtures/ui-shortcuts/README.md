# UI Shortcuts fixture

011-plugin-ui-contributions (US2 T077, contracts/action-registry-plugins.md),
holds `ui.shortcuts` only. On `ready_ack`, registers three actions:

- **`take_over`** — trigger, default binding `L`, deliberately colliding
  with the host's own `ToggleLoop` (also `L`) for M5: the host always
  wins a cross-tier conflict, so this binding stays flagged and never
  fires until the user rebinds it to a free key.
- **`nudge`** — continuous, default binding `Shift+K`, colliding with the
  `ui-panel` fixture's own `focus_me` (also `Shift+K`, both `Bundled`
  tier) for M6: a same-tier conflict flags both, neither fires until the
  user resolves it.
- **`tab_bound`** — trigger, default binding `Tab`, which 007 FR-007's
  own interactive capture would itself reject (G10): registers
  successfully but unbound, with a console warning.

Every `action_invoked` delivery increments a per-action counter and
records the delivered `value`.

`debug_probe`:

- `"invoked_take_over"` / `"invoked_nudge"` / `"invoked_tab_bound"` — the
  running invocation count for that action.
- `"last_value"` — the last delivered `action_invoked`'s `value` field
  (`nil` for a `Trigger`, a number for `nudge`'s `Continuous`).

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

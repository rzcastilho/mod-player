# UI notify fixture

011-plugin-ui-contributions (US5 T113, contracts/overlays-settings-
notify.md §3), holds `ui.notify` only. Posts nothing on its own until
armed by a `debug_probe`; once armed, it posts one `api.ui.notify(..)`
call per 5ms timer tick — not all at once from inside `debug_probe`
itself, since that would nest a real RPC inside the host's own bounded
wait for this plugin's `debug_probe` reply and deadlock/time out
(`host_busy`) before completing. See the comment at the top of
`main.luau` for the full explanation.

`debug_probe`:

- `"arm:<level>:<n>"` — queues `n` valid `notify(level, "fixture
  notification")` calls (`level` one of `critical`, `warning`, `info`),
  resetting `posted`/`refused`/`last_reason` to zero/`nil`.
- `"arm_invalid:<n>"` — the same, but with a 201-character text (over
  `MAX_NOTIFY_TEXT_CHARS = 200`), proving N1's "admitted, then refused by
  `validate_notify`, but the slot is already gone" rule.
- `"counts"` — `{ posted, refused, last_reason, done }`: poll this until
  `done` is `true` (every queued call has been attempted) before reading
  `posted`/`refused`. `last_reason` is the most recent refusal's
  `err.reason` (`"rate_limited"` once the window's 6 slots are gone,
  `"invalid_value"` for `arm_invalid`'s own over-length text).

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

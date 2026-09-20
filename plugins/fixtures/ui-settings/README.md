# UI settings fixture

011-plugin-ui-contributions (US4 T105, contracts/overlays-settings-
notify.md §2), holds `ui.settings` and `state.plugin` (the latter only to
drive the `state_shift` probe below). On `ready_ack`, registers a 4-field
settings schema:

- **`snap`** — boolean, "Snap to beat", default `false`.
- **`shift`** — number (`-12..12`, step `1`), "Semitone shift", default
  `0` — the field manual scenario M8 sets to `3` and checks again after a
  restart.
- **`note`** — string, "Label", default `""`.
- **`mode`** — choice (`a`/`b`), "Mode", default `"a"`.

Every `settings_changed` delivery increments a counter and records the
changed keys.

`debug_probe`:

- `"get"` — `api.ui.get_settings()`'s own values table verbatim (M8: after
  a restart, this reflects `settings.json` as last written to disk, not
  anything this session remembers).
- `"settings_changed_count"` — the running delivery count.
- `"last_changes"` — the most recent delivery's `changes` table.
- `"state_shift"` — `{ found, value }` from `state.plugin.get("shift")`:
  always `{ found = false }`, proving `Scope::Settings` is unreachable
  through `state.plugin` (FR-018) regardless of what the settings page's
  own "shift" field currently holds.

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

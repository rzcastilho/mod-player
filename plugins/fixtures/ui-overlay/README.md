# UI Overlay fixture

011-plugin-ui-contributions (US3 T090, contracts/overlays-settings-
notify.md §1.1), holds `ui.overlay` and `playback.observe`. On every
`track_changed`, draws one primitive of every kind (`line`, `region`,
`label`, `glyph`) — the host has already cleared this plugin's previous
overlay set before the handler runs (O3, FR-016), so re-adding the same
ids each time is the ordinary usage pattern.

`debug_probe`:
- `"add_501st"` — a single `add_overlays` call with 501 primitives,
  proving the whole-batch `overlay_limit` refusal (O1).
- `"bad_region"` — `add_overlays` with `from >= to`, proving
  `invalid_value`.
- `"bad_icon"` — `add_overlays` naming an icon that is neither a host
  glyph nor a declared manifest glyph key, proving `invalid_value`.

Every probe returns `{ ok = true }` or `{ ok = false, code, reason,
message }`.

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

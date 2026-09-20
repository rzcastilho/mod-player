# Focus fixture B

010-transport-focus contention fixture (data-model.md §5, priority
P1/P2 manual scenarios). Holds `playback.observe` and
`transport.control`. On `ready_ack`, requests transport focus. On every
`focus_granted`, seeks to 2000 ms. Re-requests focus on every
`track_changed` (a fresh per-track arbitration).

While it does **not** currently hold focus, attempts `seek(0)` on every
`play_state_changed` and logs the resulting `no_focus` refusal — paired
with focus fixture A (which does hold focus and seeks successfully),
this is the single-holder contention proof: focus-b's seek must never
move the playhead while a different plugin holds focus (quickstart.md
M2, SC-001).

`debug_probe("log")` returns the accumulated log (an array of strings).

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

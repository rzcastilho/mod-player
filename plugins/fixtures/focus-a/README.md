# Focus fixture A

010-transport-focus contention fixture (data-model.md §5, priority P1/P2
manual scenarios). Holds `playback.observe`, `transport.control`,
`markers.read` and `markers.write`. On `ready_ack`, requests transport
focus. On every `focus_granted`, seeks to 1000 ms, creates a transient
loop region (`1000`-`3000` ms) and arms it. Re-requests focus on every
`track_changed` (a fresh per-track arbitration). Logs every
`focus_granted`/`focus_revoked`/`play_state_changed`/`loop_armed`/
`loop_disarmed` event it receives.

Deliberately hangs (a busy loop) on the **third** `play_state_changed`
it receives — the scheduler's per-handler budget interrupt (RT3) aborts
that call, and the resulting retries blow the aggregate 1 s CPU share,
suspending this plugin (RT4). This makes the "holder suspended -> focus
returns to the host within one second, loop disarmed" scenario
(quickstart.md M5b, SC-003) reproducible from the transport rather than
racing a machine-dependent CPU-share suspension.

`debug_probe("log")` returns the accumulated log (an array of strings).

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

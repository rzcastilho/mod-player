# Well-behaved fixture

US3 full-behavior-cycle fixture (spec.md priority P3, research R18).
Holds all 9 operable permissions and, on `ready_ack`, drives one
complete cycle through this slice's whole surface: subscribes to
`position` at 30/s, reads `playback.state()`, lists the queue, requests
transport focus, creates and arms its own loop region, lists markers
back, creates a `pitch_shift` node suggested `before` a `time_stretch`
node and sets one of its parameters, writes both per-plugin and
per-track state, and schedules a `schedule_at_position` timer — logging
each step's outcome (`<step>: ok` or `<step>: <code>/<reason>`) so a
manual run (quickstart.md M8) or a test reading `plugin_log()` can
confirm every one of them worked exactly as declared.

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

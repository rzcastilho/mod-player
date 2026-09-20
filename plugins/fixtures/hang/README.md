# Hang fixture

US1 fault-isolation fixture (spec.md priority P1, research R18). Requires
only `playback.observe`. Its `play_state_changed` handler runs
`while true do end`, which never returns on its own — the scheduler's own
4 ms interrupt (RT3) is the only thing that ever ends the call, aborting
it with `AbortCause::Deadline`. Enough repeats of this abort within the
rolling 1 s CPU-share window push the plugin over its share and suspend
it (`SuspendCause::Hang`, RT4) — audio and transport are never affected,
since the hang runs entirely on this plugin's own thread (Constitution
I).

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

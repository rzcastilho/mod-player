# Leak fixture

US1 fault-isolation fixture (spec.md priority P1, research R18). Requires
only `playback.observe`. Its `position` handler grows an ever-larger Lua
table by 1 MB per event; once the plugin's own 64 MiB `Lua` heap cap
(`Lua::set_memory_limit`) is hit, the scheduler suspends it
(`SuspendCause::Memory`, RT4) — the host process's own memory is never at
risk (Constitution I).

`debug_probe("position_count")` returns how many `position` events this
fixture has received before being suspended, for automated tests. Only
used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

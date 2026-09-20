# Throw fixture

US1 fault-isolation fixture (spec.md priority P1, research R18). Requires
only `playback.observe`. Its `position` handler raises `error("boom")`
every time it runs — the scheduler aborts just that one handler call
(`AbortCause::Exception`, RT3) and logs the error to the plugin's own
console; the plugin itself is never suspended by this alone (a single
exception is not the same as the Hang/CpuShare/Memory suspension causes),
and neither audio nor transport are affected.

`debug_probe("position_count")` returns how many `position` events this
fixture has received, for automated tests. Only used when
`MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

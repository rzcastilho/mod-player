# Never-ready fixture

US1 fault-isolation fixture (spec.md priority P1, research R18). Minimal
manifest, no permissions. Its entry script never calls `api.ready()`, so
no event is ever delivered to it (contract §1: "before it no event is
delivered"). After the 5 s ready-timeout budget elapses the scheduler
suspends it with `SuspendCause::DidNotStart` (RT5) without running an
`unloading` handler, since the plugin was never ready to receive one.

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

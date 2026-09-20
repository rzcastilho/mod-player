# Observer fixture

US2 least-privilege fixture (spec.md priority P2, acceptance scenario 1).
Requires only `playback.observe`. On `ready_ack` it calls
`api.transport.seek(0)`, a `transport.control`-only request it was never
granted — the Capability Gateway refuses it with `permission_denied`/
`not_granted` before it ever reaches the host (no side effect), and the
fixture logs the refusal to its own console entry.

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

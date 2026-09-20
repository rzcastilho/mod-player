# Fixture plugins

`plugins/fixtures/<name>/` holds the eight bundled-source test plugins
FR-003 requires (research R8), embedded the same way as
`../bundled/<identifier>/` but *discovered* only when
`MODPLAYER_PLUGIN_FIXTURES=1` is set at launch. Without the variable the
plugin list is empty (FR-023 empty state).

Each package folder has the same shape as a bundled package
(`plugin.toml`, `main.luau`, `README.md`; see `../bundled/README.md` and
`contracts/manifest.md` §1).

Fixtures land with the user story that exercises them (tasks.md):

| Name | Story | Behavior |
|---|---|---|
| `observer` | US2 | `playback.observe` only; probes a refused call |
| `invalid` | US2 | manifest names a permission outside the catalog |
| `flood` | US2 | issues far-above-normal-rate requests |
| `hang` | US1 | infinite loop in an event handler |
| `throw` | US1 | throws on every event |
| `leak` | US1 | grows unbounded memory on every event |
| `noready` | US1 | never calls `ready()` |
| `wellbehaved` | US3 | every operable permission, one full behavior cycle |

This directory is empty of packages until then.

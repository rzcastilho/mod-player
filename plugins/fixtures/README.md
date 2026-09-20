# Fixture plugins

`plugins/fixtures/<name>/` holds the bundled-source test plugins FR-003
requires (research R8), embedded the same way as
`../bundled/<identifier>/` but *discovered* only when
`MODPLAYER_PLUGIN_FIXTURES=1` is set at launch. Without the variable the
plugin list is empty (FR-023 empty state).

Each package folder has the same shape as a bundled package
(`plugin.toml`, `main.luau`, `README.md`; see `../bundled/README.md` and
`contracts/manifest.md` §1).

Fixtures land with the user story that exercises them (tasks.md);
`crates/modplayer-core/src/plugins/bundled.rs::fixtures()` is the
registration of record:

| Name | Story | Behavior |
|---|---|---|
| `hang` | US1 (009) | infinite loop in an event handler |
| `throw` | US1 (009) | throws on every event |
| `leak` | US1 (009) | grows unbounded memory on every event |
| `noready` | US1 (009) | never calls `ready()` |
| `observer` | US2 (009) | `playback.observe` only; probes a refused call |
| `invalid` | US2 (009) | manifest names a permission outside the catalog |
| `flood` | US2 (009) | issues far-above-normal-rate requests |
| `wellbehaved` | US3 (009) | every operable permission, one full behavior cycle |
| `focus-a` | 010 | requests transport focus; hangs on its third event (RT3/RT4 suspension) |
| `focus-b` | 010 | requests transport focus; proves a non-holder's `seek` is refused |
| `ui-panel` | 011 US1 | registers a panel with every widget kind; request-focus-in-handler/timer, hang and throw probes |
| `ui-shortcuts` | 011 US2 | registers `take_over`/`nudge`/`tab_bound` actions (host- and same-tier conflicts) |
| `ui-overlay` | 011 US3 | draws one overlay primitive of every kind per `track_changed`; `add_501st`/`bad_region`/`bad_icon` probes |
| `ui-icons` | 011 US3 | declares a manifest icon and two glyphs, one deliberately over the size cap |
| `ui-settings` | 011 US4 | registers a 4-field settings schema; logs `settings_changed`; `get` probe |
| `ui-notify` | 011 US5 | `arm:<level>:<n>`/`arm_invalid:<n>` queue posts, one per timer tick; `counts` probe reads back posted/refused, rate-limited to 6/60s |
| `effects-observer` | 013 Foundational | `audio.effects` only; logs a summary of every `effect_chain_changed` (API 1.4 fan-out) |

This directory is empty of packages until `MODPLAYER_PLUGIN_FIXTURES=1`
is set.

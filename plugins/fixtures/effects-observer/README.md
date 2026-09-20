# Effects observer fixture

`org.modplayer.fixture.effects-observer` holds only `audio.effects` and
subscribes to `effect_chain_changed` (API 1.4). On every delivery it logs
one line, `effect_chain_changed#<seq>:<node count>:<node summaries>`,
where each node's summary is `<kind>[<sorted name=value pairs>]auto_switched=<bool>`.

Used by `crates/modplayer-core/tests/controller_effects.rs` to prove the
API 1.4 fan-out mechanism (013-key-and-tempo-plugin, research R1/R3, the
R4 delivery fix) end to end — a real subscribed plugin thread actually
receiving the coalesced event with its `params`/`auto_switched` payload
— without depending on the bundled Key & Tempo package.

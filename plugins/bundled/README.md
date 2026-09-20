# Bundled plugins

`plugins/bundled/<identifier>/` holds the "bundled" plugin source, one
folder per plugin (FR-001, FR-003, research R8). Bundled plugins ship
with the host, are audited by the host project, and are installed and
enabled by default with their manifest's declared permissions
pre-approved — no approval-sheet prompt.

Each package folder is:

```text
<identifier>/
├── plugin.toml        # manifest (see contracts/manifest.md §2)
├── main.luau           # entry script (`entry` in plugin.toml may rename it)
├── README.md           # readme (required by FR-001; content not validated)
└── …                    # optional scripts/resources (not loaded this slice)
```

Packages here are embedded into the `modplayer-core` binary at compile
time via `include_str!` (`crates/modplayer-core/src/plugins/bundled.rs`);
there is no runtime filesystem read and no install/uninstall action —
`can_uninstall` is always `false` for a bundled plugin (FR-013).

Packages currently shipped here:

- `org.modplayer.section-loop` — Section Loop (012-section-loop-plugin):
  A/B loop marking, gapless repeat, and cue navigation for hands-free
  drilling of a track section.
- `org.modplayer.key-tempo` — Key & Tempo (013-key-and-tempo-plugin):
  independent semitone transpose and tempo/time-stretch control, with
  optional per-track memory of the chosen key and tempo.

The eight test-only fixture packages used to exercise and demonstrate
this slice live under `../fixtures/` instead (discovered only when
`MODPLAYER_PLUGIN_FIXTURES=1` is set at launch), not here.

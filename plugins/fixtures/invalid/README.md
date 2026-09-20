# Invalid fixture

US2 manifest-rejection fixture (spec.md priority P2, acceptance scenario
2, contracts/manifest.md rule 4). Its `plugin.toml` names
`teleport.everywhere` — a permission that does not exist in the 25-entry
catalog — so `manifest::validate` rejects it with
`ManifestError::UnknownPermission`. The plugin is discovered and listed
("invalid manifest: …teleport.everywhere…") but never loaded: `lifecycle
= Invalid(_)`, `enabled = false`, `health = None`.

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.

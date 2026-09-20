# Contract: Plugin package and `plugin.toml` manifest

Requirement ids: FR-001, FR-002, FR-003, FR-015; PL-2.1–2.3, PL-4.2;
EC-6.1. Parser/validator live in `modplayer-capability-gateway::manifest`
(research R7); the permission catalog it validates against is generated
from `api/v1.toml` (research R6).

## 1. Package layout

```text
<identifier>/
├── plugin.toml        # manifest (required)
├── main.luau          # entry script (required; `entry` may rename it)
├── README.md          # readme (required by FR-001; content not validated)
└── …                  # optional scripts/resources (not loaded this slice — no `require`)
```

Bundled packages are embedded from `plugins/bundled/<identifier>/`;
fixture packages from `plugins/fixtures/<name>/` (research R8). The
embedded form is `BundledPackage { identifier, manifest_toml, entry,
readme, fixture }`.

## 2. `plugin.toml`

```toml
identifier   = "org.modplayer.fixture.wellbehaved"   # reverse-domain, immutable, [a-z0-9-] labels, ≥ 2 labels, ≤ 128 bytes
name         = "Well-behaved fixture"
description  = "Exercises every operable capability."
version      = "1.0.0"                                # major.minor.patch, each u32
api          = "1.0"                                  # supported API range: ≥ 1.0, < 2.0
author       = "ModPlayer project"
license      = "MIT OR Apache-2.0"
homepage     = "https://example.invalid/wellbehaved"  # optional
source       = "bundled"                              # free text recorded in the list ("bundled")
entry        = "main.luau"                            # optional, default "main.luau"
min_host_version = "0.9.0"                            # optional

[[permissions.required]]
permission    = "playback.observe"
justification = "Follows the playhead to place its markers."

[[permissions.optional]]
permission    = "audio.meter"
justification = "Shows a level readout in a later slice."

network_hosts = []                                    # recorded; must be non-empty if `network` is requested

[[ui]]                                                # recorded, never rendered this slice
kind  = "panel"
id    = "main"
title = "Well-behaved"

[[effect_nodes]]                                      # intended nodes (type + suggested position)
kind = "pitch_shift"
suggested_position = { before = "time_stretch" }      # or { after = "…" } or { index = 0 }
```

## 3. Validation (FR-002) — first failure wins

| # | Rule | `ManifestError` | Fluent key / sentence |
|---|---|---|---|
| 1 | TOML parses | `UnreadableManifest(detail)` | `manifest-error-unreadable` — "The manifest could not be read: {detail}." |
| 2 | `identifier`, `name`, `version`, `api`, `author`, `license`, `source` present | `MissingField(name)` | `manifest-error-missing-field` — "The manifest is missing the required field '{field}'." |
| 3 | identifier grammar; version triple; api `major.minor`; non-empty name/author/license; entry non-empty | `MalformedField { field, detail }` | `manifest-error-malformed-field` — "The field '{field}' is not valid: {detail}." |
| 4 | every `permission` string is in the 25-entry catalog | `UnknownPermission { permission, list }` | `manifest-error-unknown-permission` — "The {list} permission '{permission}' is not in the permission catalog." |
| 5 | `network` never in `required` | `NetworkMustBeOptional` | `manifest-error-network-required` — "The 'network' permission may only be requested as optional." |
| 6 | `network` in `optional` ⇒ `network_hosts` non-empty | `NetworkWithoutHosts` | `manifest-error-network-hosts` — "The 'network' permission requires at least one declared network host." |
| 7 | no permission listed twice across both lists | `DuplicatePermission(p)` | `manifest-error-duplicate-permission` — "The permission '{permission}' is listed more than once." |
| 8 | the entry file exists in the package | `EntryMissing(path)` | `manifest-error-entry-missing` — "The entry script '{path}' is not in the package." |

An invalid package still produces a `PluginRecord` with `lifecycle =
Invalid(err)`, `enabled = false`, `health = None`; the list row reads
"invalid manifest: <sentence>" (FR-002). A catalog permission that this
slice does not implement is **not** an error (FR-015) — the grant is
recorded and every call needing it is refused at call time.

## 4. Grants for the bundled source (FR-003, DM-11)

`Grants::from_bundled(&manifest)` marks every permission in `required`
and `optional` as `Granted` (`granted_by = Install`, `granted_at = load
time`), everything else `NotRequested`. No approval sheet, no revocation,
no `permission_changed` this slice.

## 5. Tests (Constitution VIII — proptest required for manifest parsing)

- `manifest::tests::round_trip` (proptest): any generated valid manifest
  serialises to TOML and re-parses equal.
- `manifest::tests::rejects_unknown_permission` (proptest over random
  strings not in the catalog).
- `manifest::tests::network_rules`, `duplicate_permission`,
  `missing_field_names_the_field`, `malformed_identifier_cases`
  (`"foo"`, `"Foo.bar"`, `"a..b"`, 129 bytes).
- `manifest::tests::fixture_packages_parse` — the eight embedded fixtures
  yield exactly one `Invalid` (the `…invalid` package) with
  `UnknownPermission { permission: "teleport.everywhere" }`.

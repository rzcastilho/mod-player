# Contract: `[keybindings]` table in `settings.toml` (extends 001 contracts/settings-file.md)

**Crate**: `modplayer-core::settings` (`model.rs`, `store.rs`), `modplayer-core::notifications` | **Traces**: FR-013, FR-015, SC-004, SC-008; 001 contracts/settings-file.md (location, write rules, `MODPLAYER_CONFIG_DIR`) | **Design**: [research.md](../research.md) R7, [data-model.md](../data-model.md) §2, §5, §6

Everything in 001's contract — location, `MODPLAYER_CONFIG_DIR`,
schema version 1, the read-rules table and the five write rules — is
unchanged. This file adds one optional table and one warning.

## Schema delta

```toml
schema_version = 1

# … [audio], [appearance], [disclosure], [playback], [markers] unchanged …

[keybindings]
# One entry per action whose bindings differ from the shipped default.
# key   = the action's stable id (quoted, it contains dots)
# value = array of chord strings: ("Primary+")?("Shift+")?("Alt+")?<KeyName>
#         (KeyName = egui logical key name: Space, Left, Right, Up, Down,
#          Plus, Equals, Minus, Slash, 0-9, A-Z, F1-F35, …)
#         an empty array means "deliberately unbound".
"host.loop.toggle" = ["K"]
"host.transport.stop" = []
"host.nav.toggle_queue" = ["Q", "Primary+Q"]
```

The table is **absent** whenever no customisation exists (a freshly
reset map writes no `[keybindings]` table at all).

## Read rules (delta to 001's table)

| Situation | Result |
|---|---|
| `[keybindings]` absent | every action at its shipped default; **no** notification (US4 AS4) |
| Entry whose key is not a known action id | that entry dropped; the rest apply; one `Warning` `keybindings-invalid-entries` per load listing every dropped key |
| Entry whose value is not an array of strings (e.g. a bare string, an integer, an array containing a non-string) | same as above |
| Entry with a chord string that fails the grammar (unknown key name, unknown/duplicated/misordered modifier, empty) | the **whole entry** is dropped (never a partial binding list); same warning |
| Entry with duplicated chords | deduplicated silently, first occurrence kept; no notification |
| Entry equal to the shipped default | applied (harmless); rewritten away on the next save because `KeymapOverrides::set` is sparse |
| File as a whole unparseable | 001's `settings-unreadable`: defaults for everything including bindings |
| `schema_version` > 1 | 001's `settings-newer-version`: defaults, not rewritten |

Dropped entries are gone from memory immediately; the file keeps them
until the next successful save of *any* setting (full-struct
serialisation), which writes the table clean (US4 AS2).

## Write rules

Identical to 001: full-struct serialisation, `settings.toml.tmp` +
`sync_all` + `rename`, previous file intact on failure with
`settings-save-failed`, on the controller thread only. A binding change
(`add_binding`, `remove_binding`, `reset_binding`, `reset_all_bindings`)
is one save each.

## Types

```rust
// model.rs
pub struct RawSettings { /* … */ #[serde(default)] pub keybindings: BTreeMap<String, toml::Value> }
pub struct AudioSettings { /* … */ pub keybinding_overrides: KeymapOverrides }  // default: empty
impl RawSettings {
    pub fn from_settings(&AudioSettings) -> Self;                        // encodes only override entries, chords via Chord::encode, sorted by id
    pub fn into_settings(self) -> (AudioSettings, Vec<InvalidField>, Vec<String>);  // third: dropped keybinding ids
}
// store.rs
pub enum SettingsWarning { Unreadable, NewerVersion, InvalidValue(Vec<InvalidField>), InvalidKeybindings(Vec<String>) }
impl SettingsWarning { pub fn message_key(&self) -> &'static str }        // InvalidKeybindings → "keybindings-invalid-entries"
pub struct LoadOutcome { pub settings: AudioSettings, pub warnings: Vec<SettingsWarning> }   // was `warning: Option<_>`
// notifications.rs
pub const KEY_KEYBINDINGS_INVALID_ENTRIES: &str = "keybindings-invalid-entries";
```

`LoadOutcome.warnings` ordering: `Unreadable`/`NewerVersion` alone
(early return, as today); otherwise `InvalidValue` (if any) then
`InvalidKeybindings` (if any). Callers: `PlaybackController::new` raises
each; every test that asserted `outcome.warning == None` asserts
`outcome.warnings.is_empty()`.

## Tests that pin this contract

- `settings.rs::keybindings_table_absent_loads_defaults_silently`
- `settings.rs::keybindings_round_trip_is_sparse` — set one override, save, reload: only that entry in the file; reset it: no `[keybindings]` table.
- `settings.rs::keybindings_invalid_entries_dropped_in_isolation` — a file with one good, one unknown-id, one bad-chord and one wrong-shape entry loads the good one and exactly one `InvalidKeybindings` listing the three others.
- `settings.rs::keybindings_bad_entries_rewritten_clean_on_next_save`
- `settings.rs::crash_mid_write_keeps_previous_keybindings` (SC-008)
- `settings.rs::whole_file_garbage_still_single_unreadable_warning` (no second warning for bindings)
- `modplayer-core tests/actions.rs::overrides_round_trip_through_raw_settings` (proptest over random `KeymapOverrides`)

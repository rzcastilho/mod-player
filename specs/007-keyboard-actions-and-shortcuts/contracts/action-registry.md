# Contract: Action & Binding registry (`modplayer_core::actions`)

**Crate**: `modplayer-core` (`src/actions/{mod,catalog,chord,keymap,registry}.rs`, controller façade in `src/controller.rs`) | **Traces**: FR-001, FR-002, FR-003, FR-004, FR-004a, FR-005, FR-008, FR-009, FR-010, FR-011, FR-012, FR-016, FR-017, FR-018; DM-14, DM-15; AR-8 | **Design**: [research.md](../research.md) R1–R3, R5–R9, [data-model.md](../data-model.md) §1–§3

No egui type appears in this module. Nothing here runs on, or is
reachable from, the real-time path.

## 1. Catalog (`catalog.rs`)

```rust
pub const CATALOG: [ActionDef; 44];
pub fn def(action: HostAction) -> &'static ActionDef;
impl HostAction {
    pub const ALL: [HostAction; 44];
    pub fn id(self) -> &'static str;             // "host.<category>.<name>"
    pub fn parse(id: &str) -> Option<HostAction>;
    pub fn label_key(self) -> &'static str;      // "action-<category>-<name>"
    pub fn category(self) -> ActionCategory;
}
impl ActionCategory { pub const ALL: [ActionCategory; 6]; pub fn label_key(self) -> &'static str; } // "action-cat-transport" …
```

Guarantees:
- `CATALOG[i].action == HostAction::ALL[i]`; ids unique; `parse(id()) == Some(self)`.
- Default bindings, scope, `repeats_while_held`, `enabled_by_default`
  match the spec's § Default Action Catalog row for row (a test
  literally re-encodes that table).
- Exactly `TempoStepUp`/`TempoStepDown` have `enabled_by_default == false`.
- Every `kind == Trigger` (zero `Continuous`).
- The shipped defaults are conflict-free: `ActionRegistry::new(KeymapOverrides::default()).conflicts` is empty.

## 2. Chords (`chord.rs`)

```rust
impl KeyName { pub fn parse(s: &str) -> Option<KeyName>; pub fn as_str(&self) -> &'static str; }
pub const KEY_NAMES: &[&str];            // egui 0.36 Key::name() vocabulary, sorted, deduplicated
impl Chord {
    pub fn new(mods: Mods, key: KeyName) -> Chord;
    pub fn parse(s: &str) -> Result<Chord, ChordParseError>;
    pub fn encode(&self) -> String;
    pub fn display(&self, platform: Platform) -> String;
}
```

Grammar (FR-003, R2): `chord := ("Primary+")? ("Shift+")? ("Alt+")? key`,
`key ∈ KEY_NAMES`, case-sensitive, no whitespace. Examples that parse:
`Space`, `Shift+Space`, `Primary+Shift+Right`, `Plus`, `Slash`, `F5`.
Examples that fail: `""`, `Ctrl+A` (`UnknownModifier`), `Shift+Primary+A`
(order — `UnknownKey("Primary+A")`), `Shift+Shift+A` (`DuplicateModifier`),
`Primary+` (`UnknownKey("")`), `Spacebar` (`UnknownKey`).

Display (R14): `Platform::Mac` → modifiers `⌘`, `⇧`, `⌥` in that order,
then the key symbol (`←`, `↑`, `→`, `↓`, `␣` for Space, `⏎` Enter, `⇥`
Tab, `⎋` Escape, `+`, `=`, `-`, `/`, digits, letters, `F5`) with no
separator (`⌘⇧→`, `⇧␣`, `I`); `Platform::Other` → `Ctrl`, `Shift`,
`Alt`, then the name, joined by `+` (`Ctrl+Shift+Right`, `Shift+Space`,
`I`).

## 3. Keymap overrides (`keymap.rs`)

```rust
impl KeymapOverrides {
    pub fn get(&self, action: HostAction) -> Option<&[Chord]>;
    pub fn set(&mut self, action: HostAction, chords: Vec<Chord>);   // no entry when equal to default (as a set)
    pub fn remove(&mut self, action: HostAction);
    pub fn iter(&self) -> impl Iterator<Item = (HostAction, &[Chord])>;
    pub fn is_empty(&self) -> bool;
}
```

`set` deduplicates `chords` preserving first occurrence, then removes
the entry when the resulting set equals the catalog default set.
Sparse by construction: `KeymapOverrides::default()` ⇔ "every action at
its shipped default".

## 4. Registry (`registry.rs`)

```rust
impl ActionRegistry {
    pub fn new(overrides: KeymapOverrides) -> Self;
    pub fn overrides(&self) -> &KeymapOverrides;
    pub fn bindings(&self, action: HostAction) -> &[Chord];
    pub fn is_enabled(&self, action: HostAction) -> bool;
    pub fn set_enabled(&mut self, action: HostAction, enabled: bool);
    pub fn add_binding(&mut self, action: HostAction, chord: Chord) -> Result<(), BindingError>;
    pub fn remove_binding(&mut self, action: HostAction, chord: Chord);
    pub fn reset(&mut self, action: HostAction);
    pub fn reset_all(&mut self);
    pub fn is_conflicting(&self, action: HostAction, chord: Chord) -> bool;
    pub fn conflict_partner(&self, action: HostAction, chord: Chord) -> Option<HostAction>;
    pub fn resolve(&self, chord: Chord, state: &ScopeState) -> Option<HostAction>;
    pub fn rows(&self) -> impl Iterator<Item = ActionRow<'_>> + '_;
}
```

Rules (FR-008–FR-012, R6):

| # | Rule |
|---|---|
| G1 | `bindings(a)` = override if present, else the parsed catalog default; may be empty (FR-008). |
| G2 | The chord index contains only **enabled** actions; `set_enabled(a, false)` removes `a`'s chords from it and from every conflict (FR-012); `set_enabled(a, true)` re-adds them and re-evaluates conflicts at that moment (FR-009). |
| G3 | `(a, c)` is conflicting iff some enabled `b ≠ a` has `c` and `Scope::can_coexist(scope(a), scope(b))`. Symmetric: `is_conflicting(a, c) ⇔ is_conflicting(b, c)` for such a pair (FR-009). |
| G4 | `resolve(c, s)` returns the unique enabled action `a` with `c ∈ bindings(a)`, `scope(a).is_live(s)` and `!is_conflicting(a, c)`; `None` when there is none or when the only candidates conflict (FR-009 "neither fires"). Ties among non-conflicting candidates (impossible with this slice's nested scopes) go to catalog order. |
| G5 | `add_binding(a, c)` with `c ∈ bindings(a)` → `Err(BindingError::Duplicate)`, nothing changes (FR-007 duplicate). Otherwise appends `c`, rebuilds, `Ok(())` — even when `c` is held by another enabled action (that pair is now conflicting; US3 AS1). |
| G6 | `remove_binding(a, c)` with `c ∉ bindings(a)` is a no-op; otherwise removes it, rebuilds (conflict on both sides clears, US3 AS3). |
| G7 | `reset(a)` / `reset_all()` restore catalog defaults for that action / every action, never touching `enabled` (FR-011); after `reset_all()`, `conflicts` is empty (defaults conflict-free). |
| G8 | Every mutator is O(total bindings) and allocation-bounded; `resolve` is two hash lookups; nothing blocks. |
| G9 | `rows()` yields catalog order with each row's `enabled`, effective bindings and the subset of them that conflict. |

## 5. Controller façade (`controller.rs`)

```rust
impl<B, H> PlaybackController<B, H> {
    pub fn actions(&self) -> &ActionRegistry;
    pub fn add_binding(&mut self, action: HostAction, chord: Chord) -> Result<(), BindingError>;   // persists on Ok
    pub fn remove_binding(&mut self, action: HostAction, chord: Chord);                            // persists
    pub fn reset_binding(&mut self, action: HostAction);                                           // persists
    pub fn reset_all_bindings(&mut self);                                                          // persists
    pub fn set_action_enabled(&mut self, action: HostAction, enabled: bool);                       // not persisted
    pub fn seek_step(&mut self, direction: i8);            // ±5 s via seek(), clamped [0, track end]   (FR-004a)
    pub fn step_master_volume(&mut self, direction: i8);   // ±5 % via set_master_volume(), saturating  (FR-004a)
}
pub const SEEK_STEP: Duration = Duration::from_secs(5);
pub const VOLUME_STEP: u8 = 5;
```

- `PlaybackController::new` seeds the registry from
  `settings_store.load().settings.keybinding_overrides` and raises every
  `LoadOutcome.warnings` entry (including `keybindings-invalid-entries`).
- Each persisting mutator calls the existing `persist_settings` with
  `settings.keybinding_overrides = self.actions.overrides().clone()`;
  a failed write raises `settings-save-failed` and leaves the in-memory
  registry changed (same semantics as `set_nudge_step_ms`).
- `seek_step`/`step_master_volume` are no-ops while
  `transport_enabled()` is `false` (mirrors the disabled buttons); with
  no current track `seek_step` is a no-op.

## 6. Tests pinning this contract (`crates/modplayer-core/tests/actions.rs`, `tests/settings.rs`)

| Requirement | Test |
|---|---|
| FR-001/FR-002 catalog shape | `catalog_has_44_unique_ids_in_spec_order`, `ids_round_trip_through_parse`, `no_continuous_actions_in_this_slice` |
| FR-004 defaults = spec table, conflict-free | `defaults_match_spec_table`, `shipped_defaults_never_conflict` |
| FR-003 chord grammar & display | `chord_parse_encode_round_trip` (proptest over `Mods × KEY_NAMES`), `chord_parse_rejects_bad_grammar`, `chord_display_mac_and_other` |
| FR-008 zero bindings valid | `remove_last_binding_leaves_action_unbound_and_resolvable_by_nothing` |
| FR-009 symmetric, both silent | `conflict_is_symmetric` (proptest over random keymaps), `resolve_returns_none_for_conflicting_chord` |
| FR-009 disabled excluded, re-evaluated on enable | `disabled_action_never_conflicts_or_blocks`, `enabling_action_flags_existing_collision` (US3 AS6) |
| FR-010 scope-aware | `can_coexist_table_is_symmetric_and_all_true_for_nested_chain`, `disjoint_scopes_never_conflict` (table-driven with a synthetic disjoint row) |
| FR-011 reset semantics | `reset_restores_defaults_without_touching_enabled`, `reset_all_clears_every_conflict` |
| FR-012 disabled never resolves | `disabled_action_never_resolves` |
| FR-007 duplicate | `add_duplicate_binding_is_rejected` |
| FR-013 sparse overrides | `overrides_are_sparse_relative_to_defaults` (proptest: set to default ⇒ no entry), `overrides_round_trip_through_raw_settings` |
| FR-013 invalid entry isolation | `unknown_action_id_entry_dropped_with_warning`, `bad_chord_entry_dropped_others_kept`, `non_array_entry_dropped_others_kept`, `unparseable_file_still_defaults_with_settings_unreadable` |
| FR-013 / SC-008 crash mid-write | `settings.rs::crash_mid_write_keeps_previous_keybindings` |
| FR-004a steps | `seek_step_moves_five_seconds_and_clamps`, `volume_step_moves_five_percent_and_saturates` (`controller_actions.rs`) |
| FR-005 sync dispatch | `add_binding_persists_and_applies_in_same_call` |

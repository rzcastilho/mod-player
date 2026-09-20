# Data Model: Named Actions and Keyboard Shortcuts

**Feature**: 007-keyboard-actions-and-shortcuts | **Plan**: [plan.md](plan.md) | **Research**: [research.md](research.md)

All types live in `modplayer_core::actions` unless a section says
otherwise. Nothing here touches the real-time path; nothing here is
audio. Requirement ids in brackets.

## 1. Core value types

### 1.1 `HostAction` (DM-14 identity) — [FR-001, FR-002]

`#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]`
enum with exactly 44 variants, in catalog order:

| Category | Variants | Stable id (`HostAction::id()`) |
|---|---|---|
| Transport | `Play`, `Pause`, `TogglePlayPause`, `Stop`, `NextTrack`, `PreviousTrack`, `SeekForwardStep`, `SeekBackwardStep`, `VolumeUp`, `VolumeDown` | `host.transport.play`, `.pause`, `.toggle`, `.stop`, `.next`, `.previous`, `.seek_forward_step`, `.seek_backward_step`, `.volume_up`, `.volume_down` |
| Markers | `AddPointMarker`, `SetA`, `SetB`, `NudgeEarlier`, `NudgeLater`, `NudgeEarlierX10`, `NudgeLaterX10`, `ClearAllMarkers` | `host.markers.add_point`, `.set_a`, `.set_b`, `.nudge_earlier`, `.nudge_later`, `.nudge_earlier_x10`, `.nudge_later_x10`, `.clear_all` |
| Loop | `ToggleLoop` | `host.loop.toggle` |
| Cues | `SetCue(CueSlot)`, `JumpToCue(CueSlot)` — 8 + 8 | `host.cues.set_1` … `set_8`, `host.cues.jump_1` … `jump_8` |
| Navigation | `NavLibrary`, `NavSearch`, `NavNowPlaying`, `NavPlugins`, `NavSettings`, `ToggleQueue`, `FocusSearch` | `host.nav.library`, `.search`, `.now_playing`, `.plugins`, `.settings`, `.toggle_queue`, `.focus_search` |
| Effects | `TempoStepUp`, `TempoStepDown` | `host.effects.tempo_step_up`, `.tempo_step_down` |

- `HostAction::ALL: [HostAction; 44]` — catalog order (the shortcut
  map's display order within a category).
- `HostAction::parse(id: &str) -> Option<HostAction>` — exact-match
  inverse of `id()`; unknown → `None` (drives FR-013's "unknown action
  id" drop).
- `CueSlot` is 006's `modplayer_core::markers::CueSlot` (1–8).
- Ids are append-only across slices; never renumbered or removed
  (Key Entities "Action Catalog").

### 1.2 `ActionDef` (DM-14 attributes) — [FR-001, FR-018, FR-019]

```rust
pub struct ActionDef {
    pub action: HostAction,
    pub category: ActionCategory,      // Transport | Markers | Loop | Cues | Navigation | Effects
    pub label_key: &'static str,       // Fluent key, e.g. "action-transport-toggle"
    pub kind: ActionKind,              // Trigger | Continuous (no Continuous instance in this slice)
    pub owner: ActionOwner,            // Host (only variant today; reserves DM-14 `owner`)
    pub scope: Scope,                  // App | NowPlaying | MarkerFocused
    pub repeats_while_held: bool,
    pub enabled_by_default: bool,      // false only for the two Effects actions
    pub default_bindings: &'static [&'static str], // chord strings, R2 grammar
}
pub const CATALOG: [ActionDef; 44];
pub fn def(action: HostAction) -> &'static ActionDef;
```

Default bindings, scopes, repeat and enabled flags are exactly the
spec's § Default Action Catalog table, encoded as: `Space`,
`Shift+Space`, `Primary+Right`, `Primary+Left`, `Primary+Shift+Right`,
`Primary+Shift+Left`, `Primary+Up`, `Primary+Down`, `M`, `I`, `O`,
`Left`, `Right`, `Shift+Left`, `Shift+Right`, `L`, `Shift+1`…`Shift+8`,
`1`…`8`, `Primary+1`…`Primary+5`, `Q`, `Primary+F` + `Slash`, `Equals` +
`Plus`, `Minus`; `Play`, `Pause`, `ClearAllMarkers` have `&[]`.

Invariants (tested): 44 entries, one per `HostAction::ALL` in the same
order; ids unique; every default chord parses; no two *enabled* defaults
share a chord (FR-004); `Continuous` count is 0.

### 1.3 `KeyName`, `Mods`, `Chord` (DM-15 keyboard fields) — [FR-003]

```rust
pub struct KeyName(&'static str);          // Copy, Eq, Hash, Ord; from KEY_NAMES only
pub const KEY_NAMES: &[&str];               // egui 0.36 `Key::name()` vocabulary (R2)
impl KeyName { pub fn parse(&str) -> Option<KeyName>; pub fn as_str(&self) -> &'static str; }

#[derive(Default)] pub struct Mods { pub primary: bool, pub shift: bool, pub alt: bool }
impl Mods { pub fn is_none(&self) -> bool; }

pub struct Chord { pub mods: Mods, pub key: KeyName }   // Copy, Eq, Hash, Ord
impl Chord {
    pub fn parse(&str) -> Result<Chord, ChordParseError>; // "[Primary+][Shift+][Alt+]<KeyName>"
    pub fn encode(&self) -> String;                       // inverse of parse (canonical order)
    pub fn display(&self, platform: Platform) -> String;  // "⌘⇧→" | "Ctrl+Shift+Right"
}
pub enum Platform { Mac, Other }
pub enum ChordParseError { Empty, UnknownModifier(String), DuplicateModifier, UnknownKey(String) }
```

Validation rules:
- `parse` accepts modifiers only in the order `Primary`, `Shift`, `Alt`,
  each at most once, followed by exactly one key name; anything else is
  an error (FR-013 "unparseable key combination").
- `encode(parse(s)) == s` for every canonical `s`; `parse(encode(c)) == c`
  for every `c` (proptest).
- `display` never returns an empty string; on `Mac` it uses `⌘ ⇧ ⌥` and
  symbols `← ↑ → ↓ ␣ ⏎ ⇥ ⎋` for arrows/Space/Enter/Tab/Escape, else the
  key name; on `Other` it joins `Ctrl`/`Shift`/`Alt`/name with `+`.

### 1.4 `Scope`, `ScopeState` — [FR-010, FR-018]

```rust
pub enum Scope { App, NowPlaying, MarkerFocused }
pub struct ScopeState { pub now_playing_shown: bool, pub marker_focused: bool }
impl Scope {
    pub fn is_live(self, state: &ScopeState) -> bool;   // App: always; NowPlaying: now_playing_shown; MarkerFocused: marker_focused
    pub fn can_coexist(a: Scope, b: Scope) -> bool;     // table; all true in this slice (nested chain)
}
```

`ScopeState` invariant: `marker_focused ⇒ now_playing_shown` (the
`App` computes it that way; `is_live` does not depend on it).

## 2. `KeymapOverrides` (persisted, sparse) — [FR-013]

```rust
#[derive(Default, Clone, PartialEq, Eq)]
pub struct KeymapOverrides(BTreeMap<HostAction, Vec<Chord>>);
impl KeymapOverrides {
    pub fn get(&self, action) -> Option<&[Chord]>;
    pub fn set(&mut self, action, chords: Vec<Chord>);  // removes the entry when chords == defaults
    pub fn remove(&mut self, action);                    // back to default
    pub fn is_empty(&self) -> bool;
}
```

Rules:
- Only actions whose effective binding list **differs from the
  catalog default** have an entry; `set` compares against
  `def(action).default_bindings` (order-insensitive, duplicates
  removed) and drops the entry when equal. An empty `Vec` is a valid
  entry ("deliberately unbound").
- Lives in `AudioSettings.keybinding_overrides` and round-trips through
  `RawSettings.keybindings: BTreeMap<String, toml::Value>` (see
  [contracts/keymap-settings.md](contracts/keymap-settings.md)).
- Load drops, per entry: unknown id, a value that is not an array of
  strings, any chord string failing `Chord::parse`; dropped ids surface
  as `SettingsWarning::InvalidKeybindings(Vec<String>)`.

## 3. `ActionRegistry` (runtime, `PlaybackController`-owned shadow state) — [FR-005, FR-008, FR-009, FR-011, FR-012]

```rust
pub struct ActionRegistry {
    overrides: KeymapOverrides,
    enabled: [bool; 44],                       // from enabled_by_default, mutable at runtime
    index: HashMap<Chord, Vec<HostAction>>,    // enabled actions only; rebuilt on every mutation
    conflicts: BTreeSet<(HostAction, Chord)>,  // transient (Key Entities "Binding Conflict")
}
impl ActionRegistry {
    pub fn new(overrides: KeymapOverrides) -> Self;
    pub fn overrides(&self) -> &KeymapOverrides;
    pub fn bindings(&self, action) -> &[Chord];              // effective: override or default
    pub fn is_enabled(&self, action) -> bool;
    pub fn set_enabled(&mut self, action, bool);            // not persisted; rebuilds index
    pub fn add_binding(&mut self, action, chord) -> Result<(), BindingError>; // Duplicate if already held
    pub fn remove_binding(&mut self, action, chord);
    pub fn reset(&mut self, action);
    pub fn reset_all(&mut self);
    pub fn is_conflicting(&self, action, chord) -> bool;
    pub fn conflict_partner(&self, action, chord) -> Option<HostAction>;
    pub fn resolve(&self, chord: Chord, state: &ScopeState) -> Option<HostAction>;
    pub fn rows(&self) -> impl Iterator<Item = ActionRow<'_>>;  // catalog order, for the shortcut map
}
pub enum BindingError { Duplicate }
pub struct ActionRow<'a> { pub def: &'static ActionDef, pub enabled: bool, pub bindings: &'a [Chord], pub conflicts: Vec<Chord> }
```

State rules:
- **Effective bindings** = `overrides.get(action)` if present else
  `def(action).default_bindings` parsed. Never `None`; may be empty.
- **Index** contains `(chord → actions)` for every enabled action's
  every effective chord.
- **Conflict set** = every `(action, chord)` such that
  `index[chord]` contains another enabled action `b ≠ action` with
  `can_coexist(def(action).scope, def(b).scope)`. Symmetric by
  construction. Recomputed by `rebuild_index` after `new`, every
  `add_binding`/`remove_binding`/`reset`/`reset_all`/`set_enabled`.
- **`resolve`** returns `Some(a)` iff exactly one enabled `a` in
  `index[chord]` has a live scope and `(a, chord) ∉ conflicts`; with two
  live but non-conflicting candidates (impossible while all scopes are
  comparable; possible only after a disjoint scope is added) the first
  in catalog order wins.
- `add_binding` on a chord the same action already holds →
  `Err(Duplicate)`, no change; on a chord another action holds →
  `Ok(())` and the pair enters `conflicts` (US3).
- `reset`/`reset_all` touch bindings only, never `enabled`.

### 3.1 Controller façade (`crates/modplayer-core/src/controller.rs`)

`actions(&self) -> &ActionRegistry`; mutators `add_binding`,
`remove_binding`, `reset_binding`, `reset_all_bindings`, each applying
to the registry then `persist_settings(|s| s.keybinding_overrides = registry.overrides().clone())`;
`set_action_enabled(action, bool)` (no persistence). New transport
helpers `seek_step(direction: i8)` and `step_master_volume(direction: i8)`
with constants `SEEK_STEP = 5 s`, `VOLUME_STEP = 5 %` (FR-004a).

## 4. UI-side state (`crates/modplayer-ui`) — [FR-006, FR-007, FR-019]

### 4.1 `actions::FocusClaims` (per frame, `App`-owned)

```rust
pub enum Claim { TextLike, Keys(&'static [ChordPattern]) }   // ChordPattern = (Mods, KeyName)
pub struct FocusClaims { entries: Vec<(egui::Id, Claim)> }
impl FocusClaims {
    pub fn clear(&mut self);                                  // start of each frame, after dispatch
    pub fn register(&mut self, id: egui::Id, claim: Claim);   // widgets call while drawing
    pub fn claim_for(&self, id: egui::Id) -> Option<&Claim>;
}
pub const TOOLKIT_DEFAULT_CLAIMS: &[ChordPattern];   // Space, Enter, Tab, Shift+Tab, Escape, Left, Right, Up, Down
pub const WAVEFORM_CLAIMS, MARKER_CLAIMS, ROW_CLAIMS, VOLUME_SLIDER_CLAIMS: &[ChordPattern];
```

Rule: the dispatcher reads **last frame's** claims (the `Id` in
`memory.focused()` is last frame's too), then `clear()`s them; 006's
`WaveformState::text_field_ids` is replaced by `TextLike` registrations.

### 4.2 `actions::Invocation` and dispatch output

```rust
pub struct Invocation { pub action: HostAction, pub repeat: bool }
pub fn dispatch(ctx: &egui::Context, claims: &FocusClaims, registry: &ActionRegistry, scope: &ScopeState) -> Vec<Invocation>;
pub fn invoke<B, H>(inv: Invocation, controller: &mut PlaybackController<B, H>, shell: &mut Shell, waveform: &mut WaveformState, ctx: &egui::Context);
```

`dispatch` mutates `ctx.input_mut().events` (consumes matched keys,
drops ignored repeats). `invoke` is total over `HostAction` (44 arms;
`TempoStep*` are no-ops; marker/cue refusals write
`waveform.marker_status` exactly as 006's handler did).

### 4.3 `settings::controls::ControlsScreen`

```rust
pub struct ControlsScreen {
    pub filter: String,
    pub capture: Option<HostAction>,            // Some while capture mode is active
    pub capture_error: Option<&'static str>,    // Fluent key of the last inline rejection
    pub reset_all_confirm: bool,                // two-step inline confirm state
}
```

Transitions (FR-007/FR-011):

| From | Event | To |
|---|---|---|
| `capture = None` | "Add binding" on row *a* | `capture = Some(a)`, `capture_error = None` |
| `capture = Some(a)` | `Esc`, or capture control lost focus | `capture = None`, no binding change |
| `capture = Some(a)` | key event rejected by `CaptureRule` | `capture` unchanged, `capture_error = Some(reason)` |
| `capture = Some(a)` | key event accepted | `controller.add_binding(a, chord)`; `capture = None` (conflict, if any, shown on the row by the registry) |
| `reset_all_confirm = false` | "Reset all to defaults" | `true` |
| `reset_all_confirm = true` | Confirm | `controller.reset_all_bindings()`; `false` |
| `reset_all_confirm = true` | Cancel / `Esc` / focus leaves the confirm pair | `false`, no change |

`CaptureRule::check(event_chord: Option<Chord>, raw: &egui::Event, is_mac, existing: &[Chord]) -> Result<Chord, CaptureReject>`
with `CaptureReject { LoneModifier, TabReserved, MacControl, Duplicate }`
→ Fluent keys `controls-reject-modifier-only`, `controls-reject-tab`,
`controls-reject-mac-control`, `controls-reject-duplicate`.

## 5. Settings file delta — [FR-013]

See [contracts/keymap-settings.md](contracts/keymap-settings.md).
`AudioSettings` gains `keybinding_overrides: KeymapOverrides`
(default empty); `RawSettings` gains `keybindings: BTreeMap<String, toml::Value>`;
`LoadOutcome.warning` becomes `warnings: Vec<SettingsWarning>`;
`SettingsWarning` gains `InvalidKeybindings(Vec<String>)` →
`keybindings-invalid-entries`. Schema version stays 1.

## 6. Notifications — [FR-013, FR-015]

| Key (`modplayer_core::notifications`) | Severity | When |
|---|---|---|
| `KEY_KEYBINDINGS_INVALID_ENTRIES = "keybindings-invalid-entries"` | Warning | one per load with ≥ 1 dropped `[keybindings]` entry; message lists the dropped ids |
| `settings-save-failed` (existing) | Warning | a binding change could not be written |

## 7. Relationships

```
AudioSettings 1 ── 1 KeymapOverrides ── * (HostAction → Vec<Chord>)
ActionRegistry 1 ── 1 KeymapOverrides
ActionRegistry 1 ── 44 ActionDef (static CATALOG)      ActionDef 1 ── 1 Scope
ActionRegistry ── * (HostAction, Chord) conflicts       (transient, derived)
App 1 ── 1 FocusClaims (per frame)   App 1 ── 1 ScopeState (per frame)
SettingsScreen 1 ── 1 ControlsScreen
```

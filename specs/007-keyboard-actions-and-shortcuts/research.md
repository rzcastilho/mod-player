# Research: Named Actions and Keyboard Shortcuts

**Feature**: 007-keyboard-actions-and-shortcuts | **Plan**: [plan.md](plan.md) | **Spec**: [spec.md](spec.md)

Every decision below was taken headlessly against the repository as it
stands after 006 (`crates/modplayer-core`, `crates/modplayer-ui`, egui
0.36.2 / egui-winit 0.36 in `~/.cargo/registry`). Each entry records the
decision, the rationale, the alternatives rejected, and — where the
choice is an assumption rather than a derivation — the evidence it rests
on. No `NEEDS CLARIFICATION` marker survives this file.

## R1. Where the Action & Binding service lives

**Decision**: a new `modplayer_core::actions` module (`crates/modplayer-core/src/actions/{mod,catalog,chord,keymap,registry}.rs`):
the action catalog (`HostAction` enum, 44 variants, plus its
`ActionDef` table), the toolkit-agnostic `Chord`/`Mods`/`KeyName` types,
the sparse `KeymapOverrides` persisted record, and the `ActionRegistry`
(catalog + effective bindings + enabled flags + conflict index +
resolution). The module has **no egui dependency**; the egui adapter
(event → chord, chord → egui `Key`, dispatch, invocation) is
`crates/modplayer-ui/src/actions.rs`. `PlaybackController` owns the
registry as shadow state exactly as it owns `nudge_step_ms`, persisting
changes through its existing `persist_settings`.

**Rationale**: AR-8 names the Action & Binding Service a *core service*;
DM-14/DM-15 are data-model entities. Keeping the registry free of egui
lets every rule the spec makes testable (conflicts, scopes, capture
validation, persistence) run as pure `modplayer-core` unit and property
tests (Constitution VIII), and leaves the door open for the MIDI slice
to add a second binding kind without touching the UI crate. The
controller already is the single settings gateway (`persist_settings`,
`settings-save-failed`), so binding persistence rides on it unchanged.

**Alternatives rejected**: a new `modplayer-actions` crate — fails
Constitution X's "why is the existing crate insufficient" test until
plugins are a second consumer (exactly the reasoning 003–006 used for
`queue`, `library`, `analysis`, `markers`). Registry owned by `App` in
the UI crate — would put persistence and conflict rules where only
egui-driven tests can reach them, and would duplicate the controller's
settings-write path.

## R2. Key identity and chord encoding (FR-003)

**Decision**: `KeyName` is a `Copy` newtype over a `&'static str` drawn
from a closed table `KEY_NAMES` in core, whose vocabulary is exactly
egui 0.36's `Key::name()` output (`"Space"`, `"Left"`, `"Plus"`,
`"Equals"`, `"Slash"`, `"1"`…`"9"`, `"A"`…`"Z"`, `"F1"`…`"F35"`, …).
`Mods { primary, shift, alt }` is a plain three-`bool` struct. A chord's
**persisted and displayed-in-tests encoding** is
`[Primary+][Shift+][Alt+]<KeyName>` in that fixed order, e.g.
`Primary+Shift+Right`, `Shift+1`, `Space`, `Plus`. `Chord::parse` is
total over that grammar and rejects anything else (unknown key name,
repeated/unknown modifier, empty key). The UI crate maps
`egui::Key::name()` → `KeyName::parse` and `egui::Key::from_name(name)`
back; a UI test pins that the core table equals `egui::Key::ALL`
mapped through `name()` (bijection, no drift).

**Rationale**: the spec fixes *logical* keys (what 001–006 already match
on) and a platform-neutral persisted form. egui's `name()` vocabulary is
already stable, layout-resolved and round-trips through `from_name`;
reusing it avoids inventing a second key vocabulary while keeping core
free of the egui type. A closed table (not a free string) lets core
reject an unparseable combination on load (FR-013) without asking the
UI. "Primary" (not "Ctrl"/"Cmd") in the encoding is what makes one file
mean the same thing on all three platforms (NFR-9.2).

**Alternatives rejected**: mirroring egui's ~100-variant `Key` enum in
core — 200 lines of match arms for no behavioural gain. Storing egui's
`KeyboardShortcut` directly — pulls egui into core and persists
`Modifiers { ctrl, mac_cmd, command, … }`, which is platform-specific by
construction. Physical scancodes — contradicts 001–006 and FR-003.

## R3. Matching an egui key event against bindings

**Finding (verified in `egui-winit-0.36/src/lib.rs`, `on_keyboard_input`)**:
egui-winit reports the *logical* key (`Key::from_name` of winit's
`Character`), and falls back to the *physical* key only when the logical
character is not in egui's table. On a US layout, `Shift+1` arrives as
logical `Exclamationmark` (in the table) with `shift = true`, while
`Shift+3` arrives as `Num3` + `shift` because `"#"` is not in the table.
006's manual scenario M13 passed with `Shift+3`/`Shift+5` — and its
`Shift+1` binding is dead on real hardware today. `Shift+=` arrives as
logical `Plus` with `shift = true`; `consume_key(NONE, Key::Plus)` only
matches it because egui's `matches_logically` ignores extra `Shift`.

**Decision**: the dispatcher resolves each `Event::Key { key, physical_key, modifiers, repeat, pressed: true }`
in two passes, stopping at the first that finds any binding:

1. **Logical chord**: `key` with the event's modifiers, except that
   `Shift` is dropped when `physical_key` is `Some` and differs from
   `key` (the layout consumed Shift to produce the character):
   `Shift+=` → `Plus`; `Shift+1` → `Exclamationmark`; `Shift+I` → `Shift+I`;
   `Shift+Space` → `Shift+Space`.
2. **Physical chord**: `physical_key` with the event's full modifiers:
   `Shift+1` → `Shift+1`; `Shift+=` → `Shift+Equals`.

Modifiers are compared **exactly** (`Mods` equality), not with egui's
`matches_logically`, so `Shift+Space` (stop) and `Space` (toggle) are
distinct bindings and `Shift+I` no longer fires set A (it never was a
documented binding; not in the FR-017 regression set).

**Capture** canonicalises the same event to **one** stored chord: the
physical form when `Shift` is held and `physical_key` differs from the
logical key (`Shift+1`, `Shift+Equals` — matching the spec's own catalog
notation and what the user physically pressed), otherwise the logical
form (`Plus` from a dedicated `+` key, `Space`, `Primary+Right`).

**Rationale**: pass 1 alone breaks `Shift+1`–`Shift+8` (in the FR-017
regression set) on US hardware; pass 2 alone breaks the spec's "`Shift+=`
arrives as `+`" and any layout-specific punctuation binding. Two passes
make both work, are deterministic, and cost two `HashMap` lookups per
key event.

**Known residual (documented in spec Assumptions, unchanged)**: a
captured `Shift+1` (stored physical) and a captured `!` on another
layout are different stored chords and are not flagged against each
other; likewise `Shift+Equals` vs `Plus`. Pass ordering makes the
outcome deterministic (the logical binding wins).

**Alternatives rejected**: `consume_key`/`matches_logically` — cannot
distinguish `Space` from `Shift+Space`. Physical-only — breaks layout
punctuation and FR-003. Logical-only — breaks `Shift+1` today.

## R4. Where dispatch runs and how FR-019's precedence is enforced

**Finding (egui 0.36.2 `context.rs:1466`, `memory/mod.rs:576-600`)**:
any *focused* widget that senses click is activated by `Space`/`Enter`
via `key_pressed` (non-consuming); egui's own focus traversal consumes
plain arrows/`Tab`/`Esc` for the focused widget in `Memory::begin_pass`
(before `App::ui` runs) unless the widget set an `EventFilter`;
`TextEdit` locks all of those; `Slider` locks its orientation's arrows;
`Context::text_edit_focused()` reports a focused `TextEdit` (including a
`DragValue` in edit mode). Widgets in 004/005/006 read keys with
non-consuming `key_pressed` (rows: `Enter`, `Shift+F10`; volume slider:
`PgUp`/`PgDn`; waveform: arrows, `Alt`/`Shift` arrows, `Home`, `End`,
`+`/`=`/`-`/`0`, `Esc`) or with `consume_key` (marker keys, view-level
marker shortcuts, `Cmd+Q`).

**Decision**: one dispatcher call per frame, in `App::ui` immediately
after `controller.tick()` and **before any widget is drawn** (where
`Shell::handle_shortcuts` runs today). It consumes matching events out
of `input.events` so no later widget sees them, and returns the
invocations, which `App` applies synchronously in the same frame
(FR-005). Precedence is made mechanical by a per-frame **focus-claims
registry** (`modplayer_ui::actions::FocusClaims`, `App`-owned, rebuilt
every frame like 006's `WaveformState::text_field_ids`):

1. **Text field focused** — `ctx.text_edit_focused()` *or* the focused
   id is registered as `Claim::TextLike` (006's `DragValue`s, the
   settings search box, the Controls filter box, the capture control) →
   the dispatcher does nothing this frame.
2. **Focused widget's own keys** — the focused id's registered claim
   set, or, for a widget that registered nothing, the toolkit default
   `{Space, Enter, Tab, Shift+Tab, Escape, plain ←↑→↓}` (what egui does
   for every focusable widget) → events matching a claimed chord are
   left alone. Registered sets: volume slider `{←, →, PgUp, PgDn}`;
   waveform overview/detail `{←, →, Shift+←/→, Alt+←/→, Alt+Shift+←/→, Home, End, Plus, Equals, Minus, 0, Escape, Tab}`
   (Space deliberately absent — a focused waveform still lets `Space`
   toggle play/pause); marker glyph/row `{Delete, Backspace, F2, Enter, C, Escape, Tab}`
   (arrows absent — the four nudge actions own them, scope
   `marker_focused`); library/search/queue rows `{Enter, Shift+F10, Space, Tab, Escape, ←↑→↓}`.
3. **Registered action** — the first enabled, non-conflicting binding
   whose scope is live (R6) matching pass 1 then pass 2 of R3.
4. Otherwise untouched.

`Event::Key.repeat` is honoured per FR-019: an event with
`repeat == true` is consumed and *dropped* for an action whose
`repeats_while_held` is `false`, and re-fires the action otherwise.

**Rationale**: running before widgets is the only position from which
the dispatcher can guarantee exactly one consumer — widgets read keys
non-consumingly, so a dispatcher running *after* them could not tell
whether the key was already acted on. The claims registry lets each
existing widget keep its own key table untouched (Constitution X, FR-017
regression safety) and makes rule (2) a data table the FR-017 proptest
can enumerate. The toolkit-default claim reproduces egui's own
behaviour, so a focused "Stop" button on `Space` stops and nothing
else (US1 AS9).

**Consequence for 006**: `now_playing::handle_marker_shortcuts`, the
`Cmd+Q` branch, `Shell::handle_shortcuts` and the arrow arms of
`waveform::focused_marker_key` are deleted; their behaviour is the
catalog. The marker glyph/row additionally sets
`EventFilter { horizontal_arrows: true }` while focused so egui's focus
traversal no longer also moves focus on a nudge arrow (a latent 006
quirk the claims model surfaces; the nudge result is unchanged).

**Alternatives rejected**: dispatcher after widgets — cannot detect a
widget's non-consuming read (double fire). Wrapping every widget's key
handling in the registry — rewrites 004/005/006 for no user-visible
gain and inflates the regression risk. Using `egui_wants_keyboard_input`
— true for *any* focused widget (006 R17 already rejected it).

## R5. Scopes and where they are computed

**Decision**: `Scope { App, NowPlaying, MarkerFocused }` in core with
`Scope::contains(self, live: &ScopeState)` and
`Scope::can_coexist(a, b) -> bool` (a lookup table; all `true` for the
nested chain this slice ships, the table exists so a later disjoint
scope changes one row, FR-010). `ScopeState { now_playing_shown, marker_focused }`
is computed by `App` each frame: `now_playing_shown = launch_step == Main && device_check.is_none() && shell.section == NowPlaying`;
`marker_focused = now_playing_shown && waveform.focused_marker.is_some()`.
Dispatch runs only while `launch_step == Main` and no Device Check
overlay is open.

**Rationale**: FR-018's three scopes are exactly view + focus
predicates `App` already owns. Restricting dispatch to `Main` stops
`Space` from reaching the transport from the sign-in screen; 004's
section shortcuts used to fire there too, but with no visible effect
(the gate keeps drawing itself), so nothing observable changes.

## R6. Conflict detection and resolution (FR-009/FR-010)

**Decision**: `ActionRegistry::rebuild_index()` recomputes, after every
load, binding change and `set_enabled`, a `HashMap<Chord, Vec<HostAction>>`
over **enabled** actions only, plus `conflicts: BTreeSet<(HostAction, Chord)>`
holding every (action, chord) pair whose chord maps to ≥ 2 enabled
actions with `can_coexist` scopes. `resolve(chord, &ScopeState)` returns
the single enabled action whose scope is live and whose (action, chord)
is not in `conflicts`, else `None`. `conflict_partner(action, chord) -> Option<HostAction>`
feeds the inline message. Disabled actions never enter the index (so
never flag, never block). Shipped defaults are asserted conflict-free by
a test and by construction (44 actions, 43 default chords — 8 transport,
7 marker, 1 loop, 16 cue, 8 navigation, 3 effects — all distinct within
the enabled set).

**Rationale**: the spec's rule is a pure function of (bindings, enabled
flags, scopes); recomputing eagerly on each mutation keeps the per-key
hot path to a hash lookup and makes "recomputed on every enable
transition" trivially true. Symmetry, scope-awareness, disabled
exclusion and default conflict-freedom become proptests over random
keymaps (FR-017).

**Alternatives rejected**: lazy per-key conflict scanning — 44 × bindings
per key press for nothing. Persisting conflict state — the spec forbids
it (transient, computed).

## R7. Persisted shape of customisations (FR-013)

**Decision**: `settings.toml` gains an optional `[keybindings]` table:
one entry per *customised* action, key = action id, value = array of
chord strings (R2), empty array = deliberately unbound:

```toml
[keybindings]
"host.loop.toggle" = ["K"]
"host.transport.stop" = []
"host.nav.toggle_queue" = ["Primary+Q", "Q"]
```

`RawSettings.keybindings: BTreeMap<String, toml::Value>` (`#[serde(default)]`)
so a malformed *value shape* of one entry never fails the whole file;
`RawSettings::into_settings` converts each entry through
`HostAction::parse(id)` + `Chord::parse` and collects the ids of dropped
entries into `KeymapOverrides` + `Vec<String>` invalid. `LoadOutcome.warning: Option<SettingsWarning>`
becomes `warnings: Vec<SettingsWarning>` (two callers:
`PlaybackController::new`, `settings.rs` tests) with a new variant
`SettingsWarning::InvalidKeybindings(Vec<String>)` →
`keybindings-invalid-entries`; an unparseable file stays
`settings-unreadable` (defaults for everything). Duplicated chords
inside one entry are deduplicated silently; a chord that fails R2's
grammar drops the whole entry (one id, one warning line). The table is
rewritten clean (only surviving overrides) on the next successful
change because `persist_settings` always serialises the full struct.
`AudioSettings` gains `keybinding_overrides: KeymapOverrides`
(`BTreeMap<HostAction, Vec<Chord>>`, defaults empty; compared in
`PartialEq` like every other field).

**Rationale**: reuses 001's file, atomic-write path, crash-mid-write
guarantee and warning plumbing verbatim, as the spec directs; the
sparse diff keeps the file readable and lets the shipped defaults evolve
without invalidating user files. `toml::Value` per entry is the only way
to isolate a single bad entry with serde's derive.

**Note on debounce**: 001's contract states a 250 ms debounce but the
shipped `persist_settings` is a synchronous load-mutate-save; volume
steps (`Primary+↑` held, ~30 events/s) therefore write ~30 times a
second — exactly what the slider drag already does. Unchanged here;
a later slice may debounce for all settings at once.

**Alternatives rejected**: a separate `keybindings.toml` — a second
atomic-write implementation and a second warning family for one table.
Full (non-sparse) dump — every future default change would be masked by
old files.

## R8. Runtime `enabled` flag and the two tempo actions (FR-012)

**Decision**: `enabled` is a runtime field of `ActionRegistry`
initialised from the catalog default (`false` only for
`effects.tempo_step_up/down`); `ActionRegistry::set_enabled(action, bool)`
is public (used by tests and by the future effects slice), not
persisted. Invoking a disabled action is impossible by construction
(never indexed); the UI's `invoke` still has a `TempoStepUp/Down => {}`
arm so the match is exhaustive today. The Controls page renders disabled
rows with `ui.visuals().weak_text_color()` and an externalised
"(inactive)" suffix, keeping every control on the row enabled.

## R9. Transport step actions (FR-004a)

**Decision**: two new `PlaybackController` methods — `seek_step(direction: i8)`
(`position() ± 5 s`, clamped to `[0, track_len_ms]`, then the existing
`seek(Duration)` so 003's T6–T8 and "seek past end" rules apply) and
`step_master_volume(direction: i8)` (`master_volume().value() ± 5`,
saturating into `0..=100`, then the existing `set_master_volume`, so the
engine's smooth gain ramp, the persisted value and the Connect volume
mirror are all reused). Constants `SEEK_STEP: Duration = 5 s`,
`VOLUME_STEP: u8 = 5` in `controller.rs`. `transport.play`/`pause`/`toggle`/`stop`/`next`/`previous`
map 1:1 onto `play`/`pause`/`stop`/`skip_forward`/`skip_back`; `toggle`
reads `transport_state().intent == Playing` exactly as the Now Playing
button does. Transport actions are ignored (no-op, no refusal) while
`transport_enabled()` is `false`, mirroring the disabled buttons.

## R10. Navigation, queue toggle and search focus as actions

**Decision**: `App::invoke` handles `Nav*` by setting `shell.section`,
`FocusSearch` by setting `shell.section = Search` and
`shell.focus_search_requested = true`, `ToggleQueue` through a new
`now_playing::toggle_queue_panel(ctx)` helper flipping the existing egui
temp-memory flag. `Shell::handle_shortcuts` is removed. The Settings
screen's own `Ctrl/Cmd+F` handler (`settings/mod.rs`) is removed too: it
has been unreachable since 004 made `Ctrl/Cmd+F` an app-wide jump to
Search (the shell switched sections before Settings could see the key),
so nothing observable changes.

## R11. Settings › Controls page and capture mode (FR-006/FR-007)

**Decision**: `crates/modplayer-ui/src/settings/controls.rs`, state
`ControlsScreen { filter: String, capture: Option<HostAction>, capture_error: Option<&'static str>, reset_all_confirm: bool }`
in `SettingsScreen`. Layout: filter `TextEdit` (registered `TextLike`),
page-level "Reset all to defaults" (two-step inline confirm as 006's
clear-all: `controls-reset-all` → `controls-reset-all-confirm` +
Confirm/Cancel, `Esc`/focus loss cancels), then per category a header
and one row per action: label, kind, "(inactive)" when disabled, one
chip per binding (`Button` labelled with the platform display string,
accessible name `controls-binding-chip { $binding }`, and a `×` remove
button `controls-remove-binding { $binding }`), a `⚠` + `controls-conflict-with { $other }`
label when conflicting, "Add binding", "Reset to default". Filter is a
case-insensitive substring over `tr(label)` and `tr(category)`; empty
result shows `controls-no-match`.

Capture: activating "Add binding" sets `capture = Some(action)`, draws a
focused placeholder button ("Press a key…", `controls-capture-prompt`)
with `EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true }`
so `Tab`/arrows/`Esc` reach it instead of egui's focus traversal, then
reads the first `Event::Key { pressed: true }` this frame:
`Esc` → cancel; a chord failing `CaptureRule` (lone modifier, `Tab`,
`Shift+Tab`, macOS physical `Control`, exact duplicate on the same
action) → `capture_error = Some(key)`, capture stays; otherwise
`controller.add_binding(action, chord)` and capture ends, and if the
chord now conflicts the row shows the partner's name. `!response.has_focus()`
on any later frame (click elsewhere, window blur) cancels. The capture
control is registered `TextLike` so rule R4-1 keeps every action silent
while capturing. Chords display with `Chord::display(Platform)` —
`⌘⇧→`-style glyphs on macOS (`ctx.os().is_mac()`), `Ctrl+Shift+Right`
elsewhere; the settings registry gains one descriptor
`controls.keybindings` ("Keyboard shortcuts") so the page is reachable
from the settings search box.

**Rationale**: reuses 006's proven inline-confirm and 001's settings
screen conventions; a focus-locked capture widget is the only way in
egui to receive `Tab`/arrows without the toolkit moving focus first.

## R12. Test strategy and the FR-017 regression suite

**Decision**:
- `modplayer-core` unit + proptests (`tests/actions.rs`): catalog
  invariants (44 actions, unique ids, defaults conflict-free, every
  `Chord` round-trips `display`/`parse`), conflict symmetry/scope/disabled
  properties, resolution determinism, `KeymapOverrides` sparse
  round-trip through `RawSettings`, single-entry drop with warning,
  crash-mid-write (existing store test extended with a `[keybindings]`
  table), capture-rule table.
- `modplayer-ui` tests (`tests/actions.rs`): `actions::dispatch_and_invoke`
  is a public function taking `(&Context, &FocusClaims, &ScopeState, &mut controller, &mut shell, &mut waveform)`
  so tests drive it exactly as `App::ui` does without an
  `eframe::CreationContext`. The **inherited-binding regression table**
  injects each chord of the FR-017 set as an `Event::Key` (logical and,
  for `Shift+1..8`, the physical-fallback shape R3 documents) and
  asserts the same controller/shell observable as the pre-slice tests in
  `tests/markers.rs`, `tests/now_playing.rs`, `shell.rs` and
  `tests/search_view.rs` did — those existing tests are re-pointed at
  the dispatcher rather than at `now_playing::show` alone, and the `/`
  -in-text-field case flips to "does not fire". US1 transport keys, the
  FR-019 one-consumer/repeat rules, and the Controls page (filter,
  capture accept/reject/duplicate/conflict, chip removal, reset, greyed
  rows, accessibility enumeration, no unused Fluent keys) are new tests.
- Manual scenarios M1–M14 in [quickstart.md](quickstart.md), executed by
  the implementing agent per Governance › Manual Scenario Sign-Off.

## R13. Fluent keys

**Decision**: new `locales/en-US/controls.ftl` (picked up automatically
by `static_loader!` over `locales/`): 44 `action-*` labels, 6
`action-cat-*` category labels, ~14 page/capture/conflict strings and
the `keybindings-invalid-entries` warning; `settings.ftl` gains
`setting-keybindings`/`-desc` for the registry descriptor. `fluent_keys.rs`'s
unused-key audit is extended to `controls.ftl`.

## R14. Platform modifier handling

**Decision**: `Mods::from_egui(modifiers, is_mac) -> Option<Mods>`:
`primary = modifiers.command` (egui sets it from Ctrl on Windows/Linux
and from ⌘ on macOS), `shift`, `alt`; returns `None` on macOS when
`modifiers.ctrl` is set (physical Control held) so neither capture nor
dispatch ever sees a chord the spec declares unbindable. Display:
`Platform::Mac` → `⌘`, `⇧`, `⌥` glyphs with arrow/`Space`/`Return`
symbols and no separator (`⌘⇧→`); `Platform::Other` → `Ctrl+Shift+Right`.

## R15. Dependency and crate impact

**Decision**: no new runtime or dev dependency (`proptest` is already a
dev-dependency of both `modplayer-core` and `modplayer-ui`; `toml::Value`
comes with the existing `toml` dependency). No new crate. The engine,
audio-source, receiver, account and secure-store crates are untouched.

## R16. Things deliberately left as they are

- 005's waveform keys and 006's marker rename/delete/recolour/drag stay
  widget-local (spec Clarifications); they only *register claims*.
- `settings.toml` schema version stays `1` (additive optional table,
  unknown-key-tolerant readers on every platform).
- No global/media-key/MIDI/plugin surface (FR-016); `ActionKind::Continuous`
  exists in the model with zero instances, as the spec requires.

# Implementation Plan: Named Actions and Keyboard Shortcuts

**Branch**: `feature/007-keyboard-actions-and-shortcuts` | **Date**: 2026-09-17 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/007-keyboard-actions-and-shortcuts/spec.md`

## Summary

Let a musician with both hands on their instrument drive the whole
player from the keyboard: every transport, marker, loop, cue,
navigation and (forward-declared) effect-chain operation becomes one of
**44 named host actions** with a stable id, label, kind, activation
scope, repeat flag and `enabled` flag; the host ships a **default
shortcut set** that makes every category keyboard-operable (new:
`Space`/`Shift+Space`, `Ctrl/Cmd+←/→`, `Ctrl/Cmd+Shift+←/→`,
`Ctrl/Cmd+↑/↓`, `Q`; inherited unchanged from 001/004/006: everything
else); **Settings › Controls** shows the whole map grouped by category
with a filter, lets the user add, remove and reset bindings through a
capture mode, flags **conflicts** on both rows and silences both until
resolved, greys **disabled** actions while keeping them rebindable; and
**customisations survive restart** through a sparse `[keybindings]`
table in the existing `settings.toml`.

Technical approach (details in [research.md](research.md)): a
toolkit-agnostic `modplayer_core::actions` module (R1) — a static
44-entry catalog, a `Chord` type whose key vocabulary is egui's logical
key names encoded platform-neutrally as `Primary+Shift+Right` (R2), a
sparse `KeymapOverrides` persisted as `[keybindings]` with per-entry
drop-and-warn (R7), and an `ActionRegistry` that keeps an eager
chord→action index and a transient conflict set recomputed on every
mutation and enable transition (R6). The egui adapter in
`modplayer-ui::actions` runs **once per frame before any widget is
drawn** (R4): it applies FR-019's precedence through a per-frame
**focus-claims registry** (text field → focused widget's declared keys
→ registered action), resolves each key event in two passes (logical
key with layout-consumed Shift dropped, then physical key — R3, which
also fixes 006's `Shift+1` being dead on US hardware), consumes the
event and invokes the owning component synchronously. The 004/006
ad-hoc key handlers (`Shell::handle_shortcuts`,
`handle_marker_shortcuts`, `Cmd+Q`, nudge arrows) are deleted in favour
of the catalog; 005/006 widget-local keys stay put and merely register
their claims. `Settings › Controls` is a new `settings/controls.rs`
screen reusing 006's inline two-step confirm and 001's settings
conventions (R11).

Assumptions taken headlessly (each with its rejected alternative in
Complexity Tracking): registry in core, owned by the controller; key
names from egui's vocabulary rather than a mirrored enum; two-pass
logical/physical matching with exact modifiers; dispatcher before
widgets with a claims registry; dispatch only on the `Main` launch
step; synchronous (non-debounced) settings writes as today.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–006

**Primary Dependencies**: existing eframe/egui 0.36 (+accesskit), fluent-templates 0.15, serde 1, toml 0.9 (`toml::Value` for per-entry tolerant parsing), directories 6, proptest 1 (dev, both crates). **No new runtime or dev dependency, no new crate** (research R15)

**Storage**: the existing `settings.toml` (001 `contracts/settings-file.md`; override `MODPLAYER_CONFIG_DIR`) gains one optional `[keybindings]` table holding only customisations (action id → array of chord strings, schema version stays 1), written by the existing atomic `.tmp` + `sync_all` + `rename` path; conflicts and `enabled` are never persisted; no secure-store entries; no new files

**Testing**: `cargo test --workspace` with `FakeBackend` (001), `ScriptedHost`/`SyntheticSource` (003/006), offscreen `egui::Context` with injected `Event::Key` (004–006 pattern); proptest for chord grammar round-trip, conflict symmetry/scope/disabled properties and sparse-override serialisation; a table-driven FR-017 regression suite over every inherited chord; accessibility-tree enumeration and Fluent unused-key audit extended; manual scenarios M1–M14 in [quickstart.md](quickstart.md); CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers) on ubuntu / macos / windows

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical bindings and behaviour (three platform-uniform modifiers Primary/Shift/Alt; `⌘` glyph display on macOS only; the physical macOS Control key unbindable); platform differences confined to `Mods::from_egui(is_mac)` and `Chord::display(Platform)`

**Project Type**: Desktop application — Cargo workspace stays at 10 crates (no new crate)

**Performance Goals**: key → owning-component call in the same UI frame with no queue/thread hop (FR-005, SC-011; NFR-1.10's 100 ms and NFR-1.1's control-to-audio budget unchanged because the dispatcher adds two `HashMap` lookups per key event, ~µs); conflict/index rebuild O(total bindings ≈ 50) per mutation, never per frame; Controls page renders 44 rows + chips well within a 16 ms frame; settings write per binding change ≈ the existing `persist_settings` cost

**Constraints**: nothing on the real-time path is touched (no engine/source change at all — Constitution I trivially satisfied); `#![forbid(unsafe_code)]` everywhere; no `unwrap`/`expect` outside tests; every new control keyboard-operable with an accessible name/role/state; strings externalised, en-US only; exactly one consumer per key press (FR-019); shipped defaults conflict-free by construction (FR-004); no MIDI, global shortcuts, media keys or plugin actions (FR-016); every inherited 001/004/006 binding behaves identically except `/` inside a focused text field (FR-017)

**Scale/Scope**: core `actions` module (~700 LOC incl. unit tests: catalog table 44 rows, chord grammar, keymap, registry) + settings model/store delta (~120 LOC) + controller façade (~120 LOC); UI `actions.rs` dispatcher/invoker/claims (~400 LOC) + `settings/controls.rs` (~450 LOC) + claim registrations and handler removals across 8 files (~150 LOC net); ~75 new automated tests + ~15 re-pointed; 1 new `.ftl` with 77 keys; single user / 44 actions / ≤ ~60 bindings

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No | N/A | No file under `crates/modplayer-engine`, `crates/modplayer-audio-source*` or `crates/modplayer-audio-io` changes. Every new call reaches the engine only through the controller's existing non-RT command path (`seek`, `set_master_volume`, marker/cue calls) exactly as the buttons/sliders already do; the dispatcher and registry run on the UI thread only. No real-time safety note is required because the engine crate is not modified. |
| II. Plugins Are Guests | No | N/A | No plugin runtime exists; `ActionOwner::Host` is the only variant and no plugin-facing API is added (FR-016). The namespaced id, `owner`, `kind` and `enabled` fields reserve DM-14's extension points without a breaking change. |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | Actions are a host concept (`modplayer-core::actions`, FR-9.1); the registry, conflict rules and bindings are host Rust; the owning components (transport, markers, cues, nav) stay where 003–006 put them and are only *invoked* through the dispatcher. No DSP involved. |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No `AudioSource` change; every automated test runs on `FakeBackend` + the synthetic/scripted source; the receiver crate is untouched and `single_dependent.rs` stays green. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | Nothing here sees samples; the persisted table holds action ids and key names only. `decoded_store_boundary.rs` is unaffected. |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | No credential path touched; `[keybindings]` holds no personal data and is never logged; no network, no telemetry. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate or dependency; `#![forbid(unsafe_code)]` kept; `thiserror`-style error enums (`ChordParseError`, `BindingError`); no `unwrap`/`expect` outside tests (`CueSlot::new(n)` sites use `unwrap_or_else(|| unreachable!())` as 006 does, or the catalog is built from `CueSlot` constants); doc examples on new public items (`Chord::parse`, `ActionRegistry`, `HostAction::id`); `cargo deny` unchanged; CI matrix unchanged. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Test-first for public behaviour: proptests for the state serialisation of `KeymapOverrides` and the chord grammar (constitution: "state serialization"), conflict properties (symmetric, scope-aware, disabled-excluded, defaults conflict-free), capture validation and the FR-019 precedence/repeat rules; a table-driven regression suite over 100 % of the inherited bindings (SC-007) with the sole `/`-in-text-field deviation asserted; crash-mid-write test extended to the new table (SC-008); same-frame dispatch asserted (SC-011); accessibility and Fluent audits extended; manual scenarios M1–M14 executed by the implementing agent (Governance › Manual Scenario Sign-Off). Criterion/soak remain the engine's obligations and are unaffected. |
| IX. One Plugin API Definition | No | N/A | No plugin API in this slice. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No new trait, feature flag, crate or dependency; `actions` is a core module like `queue`/`markers` until plugins are a second consumer; bindings are one platform-neutral encoding with three platform-uniform modifiers (NFR-9.2), the only platform branches being display glyphs and the macOS Control rejection; every host function keyboard-operable and every new element accessibly named (NFR-6.1/6.2); all strings externalised (NFR-7.1, en-US as 001–006). User override N/A (no plugins), but the user can always take any binding back via reset. |
| Governance: engine/gateway/runtime sign-off | No | N/A | No engine, Capability Gateway or Plugin Runtime change; ordinary code review. Requirement ids (FR-, SC-, NFR-, DM-, AR-) are referenced throughout spec, plan, contracts and tests. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no `unsafe`, no feature flag, no trait, no crate, no dependency; the real-time path and the raw-sample boundary are untouched; the headless assumptions (Complexity Tracking) all sit outside the non-negotiable principles.

## Project Structure

### Documentation (this feature)

```text
specs/007-keyboard-actions-and-shortcuts/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R16 with evidence from egui/egui-winit sources
├── data-model.md        # Phase 1: HostAction/ActionDef catalog, Chord grammar, KeymapOverrides, ActionRegistry, UI state, settings delta, notifications
├── quickstart.md        # Phase 1: automated gates (named tests), manual scenarios M1–M14
├── contracts/
│   ├── action-registry.md    # modplayer_core::actions API, registry rules G1–G9, controller façade, tests
│   ├── keymap-settings.md    # [keybindings] table: schema, read/write rules, types, tests
│   └── ui-actions.md         # frame order, dispatch precedence & claim sets, invoke mapping, Controls page, capture mode, Fluent keys, tests
├── checklists/requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

Existing layout (001–006) is kept; `+` marks new files, `~` modified files.

```text
Cargo.toml                                   (unchanged — no new workspace dependency)
locales/en-US/
├── controls.ftl                             + 44 action labels, 6 category labels, page/capture/conflict strings, keybindings-invalid-entries
└── settings.ftl                             ~ + setting-keybindings, setting-keybindings-desc
crates/
├── modplayer-core/
│   ├── src/lib.rs                           ~ pub mod actions; re-exports (HostAction, Chord, ActionRegistry, ScopeState, …)
│   ├── src/actions/mod.rs                   + module root, ActionCategory, ActionKind, ActionOwner, Scope, ScopeState, errors
│   ├── src/actions/catalog.rs               + HostAction (44), ActionDef, CATALOG, def(), ids, label keys
│   ├── src/actions/chord.rs                 + KeyName + KEY_NAMES, Mods, Chord::{parse, encode, display}, Platform
│   ├── src/actions/keymap.rs                + KeymapOverrides (sparse)
│   ├── src/actions/registry.rs              + ActionRegistry: index, conflicts, resolve, rows, mutators
│   ├── src/controller.rs                    ~ registry shadow state, add/remove/reset bindings (persisting), set_action_enabled, seek_step, step_master_volume, warnings loop
│   ├── src/settings/model.rs                ~ AudioSettings.keybinding_overrides, RawSettings.keybindings (BTreeMap<String, toml::Value>), into_settings drops+reports
│   ├── src/settings/store.rs                ~ SettingsWarning::InvalidKeybindings, LoadOutcome.warnings: Vec<_>
│   ├── src/settings_registry.rs             ~ controls.keybindings descriptor; test list updated
│   ├── src/notifications.rs                 ~ KEY_KEYBINDINGS_INVALID_ENTRIES
│   └── tests/{actions.rs +, controller_actions.rs +, settings.rs ~}
├── modplayer-ui/
│   ├── src/lib.rs                           ~ pub mod actions
│   ├── src/actions.rs                       + FocusClaims + claim constants, event→chord (two-pass), dispatch, Invocation, invoke, CaptureRule
│   ├── src/app.rs                           ~ claims + scope per frame; dispatch/invoke before widgets; remove handle_shortcuts call
│   ├── src/shell.rs                         ~ remove handle_shortcuts (tests re-pointed at dispatcher)
│   ├── src/now_playing.rs                   ~ remove Cmd+Q branch and handle_marker_shortcuts; toggle_queue_panel(ctx) helper; DragValue TextLike claims
│   ├── src/markers.rs                       ~ glyph/row MARKER_CLAIMS registration + horizontal_arrows focus filter; nudge arms move to invoke
│   ├── src/waveform/input.rs                ~ focused_marker_key drops the four arrow arms; WAVEFORM_CLAIMS registration
│   ├── src/waveform/state.rs                ~ text_field_ids replaced by FocusClaims registrations (field removed)
│   ├── src/widgets/volume.rs                ~ VOLUME_SLIDER_CLAIMS registration
│   ├── src/rows.rs                          ~ ROW_CLAIMS registration
│   ├── src/search_view.rs                   ~ search box TextLike claim (Esc handling unchanged)
│   ├── src/settings/mod.rs                  ~ Controls category → controls::show; search box TextLike claim; drop the dead Ctrl+F handler
│   ├── src/settings/controls.rs             + ControlsScreen: filter, grouped rows, chips, capture mode, per-row/all reset, conflict + disabled rendering
│   └── tests/{actions.rs +, controls.rs +, markers.rs ~, now_playing.rs ~, accessibility.rs ~, fluent_keys.rs ~, search_view.rs ~}
└── modplayer/
    └── tests/{decoded_store_boundary.rs (unchanged, must stay green), single_dependent.rs (unchanged)}
```

**Structure Decision**: keep the single Cargo workspace under `crates/`
(one crate per architectural component, Constitution VII) and add **no
crate**. The Action & Binding service is a module of
`crates/modplayer-core` (`src/actions/`) beside `queue`, `library`,
`analysis` and `markers`, because AR-8 names it a core service and a
`modplayer-actions` crate would fail Constitution X's "why is the
existing crate insufficient" test until plugins become a second
consumer; it depends on nothing egui-specific so every rule is testable
in core. The egui adapter (dispatch, claims, invocation, Controls page)
lives in `crates/modplayer-ui/src/actions.rs` and
`crates/modplayer-ui/src/settings/controls.rs` beside the screens it
drives. Persistence rides on `crates/modplayer-core/src/settings/` and
the controller's existing `persist_settings`. The dependency graph is
unchanged from 006: `modplayer → {ui, core, audio-io, account, secure-store, audio-source-connect}`;
`ui → {core, audio-io, engine, account, secure-store, audio-source}`;
`core → {engine, audio-io, audio-source, audio-source-synthetic}`; the
engine, source and receiver crates are not modified. Locale files stay
under `locales/en-US/`.

## Design notes that tasks must respect

1. **Dispatch before widgets, once per frame** (R4): `App::ui` calls
   `actions::dispatch` right after `controller.tick()`, then clears the
   claims, then invokes; no widget may read a key the dispatcher already
   consumed, and the dispatcher must never consume a key the focused
   widget claims.
2. **Claims are last frame's** (R4): every widget with custom keys
   registers its `Id` while drawing; `TextLike` replaces 006's
   `text_field_ids`; unregistered focused widgets get
   `TOOLKIT_DEFAULT_CLAIMS`.
3. **Two-pass matching with exact modifiers** (R3): logical chord with
   layout-consumed Shift dropped first, then physical chord with full
   modifiers; never `consume_key`/`matches_logically` for registered
   actions.
4. **Capture stores one canonical chord** (R3): physical form when
   Shift is held and physical ≠ logical, else logical.
5. **Registry is the only authority on "does this chord fire"** (R6):
   `resolve` alone decides; the UI never re-implements conflict, scope
   or enabled checks.
6. **Disabled actions never enter the index** (R6/R8); `set_enabled`
   rebuilds and re-flags at that moment.
7. **Overrides are sparse** (R7): `KeymapOverrides::set` drops an entry
   equal to the default; a freshly reset map writes no table.
8. **One bad entry drops only itself** (R7): `BTreeMap<String, toml::Value>`
   at the wire level; `keybindings-invalid-entries` once per load.
9. **Shipped defaults are conflict-free** (FR-004) and asserted by a
   test; a new default that collides fails the build's test gate.
10. **Steps reuse existing paths** (R9): `seek_step` → `seek`;
    `step_master_volume` → `set_master_volume` (ramp, persist, Connect
    mirror all inherited).
11. **Scope from `App`'s own state** (R5): `now_playing_shown` and
    `marker_focused`; dispatch only on `LaunchStep::Main` with no
    Device Check overlay.
12. **Marker glyph/row lock horizontal arrows** while focused so a
    nudge never also moves focus (R4).
13. **Every new string in `controls.ftl`**, resolved through `tr`/
    `tr_args`; platform glyphs come from `Chord::display`, not from
    Fluent.
14. **Existing 004/006 key tests are re-pointed, not deleted** (R12):
    they become the seed of the FR-017 regression suite.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. The headless assumptions and spec-level
decisions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Registry as a `modplayer-core::actions` module owned by `PlaybackController` (R1) | AR-8 calls it a core service; the controller is already the single settings gateway (`persist_settings`), so binding persistence reuses one code path; every rule is testable without egui. | `App`-owned registry in the UI crate — persistence and conflict rules reachable only through egui-driven tests, and a second settings-write path. New crate — fails Constitution X until plugins are a second consumer. |
| Key vocabulary = egui's `Key::name()` strings in a closed core table (R2) | Platform-neutral persisted form (`Primary+Shift+Right`), validated on load without an egui dependency in core; a UI test pins bijection with `egui::Key::ALL`. | Mirrored 100-variant enum — 200 lines of match arms for no behaviour. Persisting egui `KeyboardShortcut` — platform-specific `Modifiers` on disk. |
| Two-pass logical→physical matching with exact modifier comparison (R3) | egui-winit delivers `Shift+1` as `Exclamationmark` on US layouts (verified in source; 006's `Shift+1` is dead on real hardware) while `Shift+3` arrives as `Num3`; exact modifiers are needed so `Space` ≠ `Shift+Space`. | `consume_key`/`matches_logically` — cannot separate `Space` from `Shift+Space`. Logical-only — breaks `Shift+1`. Physical-only — breaks layout punctuation and FR-003. |
| Dispatcher before any widget + per-frame focus-claims registry (R4) | Widgets read keys non-consumingly, so only a pre-widget dispatcher can guarantee one consumer per press; declared claim sets make FR-019's rule (2) a data table the tests enumerate while 004/005/006 widget code stays intact. | Dispatcher after widgets — cannot detect a widget's non-consuming read (double fire). Rewriting every widget's keys into the registry — large regression surface for no user-visible gain. |
| Dispatch only on `LaunchStep::Main` with no Device Check overlay (R5) | `Space` must not reach the transport from the sign-in gate; 004's section shortcuts used to fire there with no visible effect, so nothing observable changes. | Dispatching on every step — transport keys active behind the welcome/sign-in screens. |
| Capture canonicalises to the physical chord when Shift is held and physical ≠ logical (R3) | Chips read `Shift+1` like the spec's catalog and what the user pressed; dispatch pass 2 matches it; consistent with the shipped defaults. | Logical form (`!`) — inconsistent chips across layouts and against the shipped `Shift+1..8` defaults. |
| `LoadOutcome.warning: Option` → `warnings: Vec` (R7) | An invalid enum value and an invalid keybinding entry in the same file must each raise their own warning key. | A second optional field — ad hoc; a combined variant — loses one message. Two call sites change. |
| Synchronous (non-debounced) settings writes kept (R7) | 001's contract mentions a 250 ms debounce but the shipped `persist_settings` is synchronous for every setting; adding a debounce for bindings alone would diverge from the slider's own behaviour. | Debounce for bindings only — inconsistent; a general debounce is a separate change across all settings. |
| `Chord::display` platform glyphs computed in core, not Fluent (R14) | Glyphs are not language, and tests can pin both platforms without a locale; keeps `controls.ftl` to real strings. | Fluent-side formatting — 100+ keys for key names, untestable per platform. |

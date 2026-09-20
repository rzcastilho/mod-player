# Research: Key & Tempo Bundled Plugin and Getting Started Panel

**Feature**: 013-key-and-tempo-plugin | **Date**: 2026-09-20 | **Plan**: [plan.md](plan.md)

Every decision below was checked against the shipped code on this branch
(009's gateway/runtime, 008's `ChainModel`/catalog, 011's widget rules,
012's package and tests), not only against the specs' prose. Items that
correct the spec's own assumptions are marked **(spec deviation)** and
are repeated in plan.md § Complexity Tracking. Headless run: no human
was available; each choice records the alternative rejected.

## R1. Plugin API 1.4 is a schema-first, zero-request bump: `params` + `auto_switched` on `NodeInfo`, `effect_chain_changed` on parameter change

**Decision**: bump `crates/modplayer-capability-gateway/api/v1.toml`
`[api_version] minor = 4`. No new `[[request]]`, no new `[[event]]`, no
new permission. The delta is (1) two additive fields on `NodeInfo`
(`gateway/src/request.rs`): `params: BTreeMap<String, ParamValue>` and
`auto_switched: bool`, with `ParamValue` a new `#[serde(untagged)]` enum
`Number(f64) | Bool(bool) | Name(String)`; (2) the `EffectChainChanged`
event's trigger widening, implemented entirely in `modplayer-core`'s
`ChainModel` by bumping `revision` on parameter/mode changes (R3); (3) a
documentary `[[node_kind]]` table in `v1.toml` (name + ordered param
wire names + value shape) rendered by `tests/api_reference.rs` into a
new "Node parameters" section of `docs/plugin-api/v1.md`, kept honest by
a core test that asserts the catalog's wire names equal the schema's
(R2) — so the reference and the runtime cannot diverge (Constitution
IX). `build.rs`'s `Schema` struct has no `deny_unknown_fields`, so the
new table is parsed only by the reference test, never by the generator.

**Evidence**: `request.rs:110` `NodeInfo { id, kind, owner, bypassed,
auto_bypassed, orphaned, index }` — no values; `model.rs:85` "Bumped on
add/remove/move/bypass/auto-bypass/orphan/readopt", `set_param` at
`model.rs:266` never touches `revision`; `controller.rs:2182` fans out
`effect_chain_changed` from a per-tick `revision` diff and
`controller.rs:2062` republishes the `list_chain()` snapshot from the
same diff — so one bump site gives both the event and the local
`list_chain()` read the new values, already coalesced per tick.

**Rationale**: the smallest additive change that lets the plugin keep
its panel and stored entry equal to what its nodes hold after 008
FR-017's host `+`/`-` or an Effect Chain panel edit (spec Clarifications
clarify-1, J-2 steps 6→8). 012 set the precedent of an additive minor
when a bundled plugin needs a host primitive the API cannot express.

**Alternatives rejected**: a new `effects.get_params(node)` request —
adds an RPC and still leaves host edits unobserved; keeping 1.3 with a
stale panel — contradicts FR-006/FR-007 and SC-009; unbinding the host's
`+`/`-` — contradicts ratified 008 FR-017.

## R2. Parameter wire names live in the effects catalog; `set_param` accepts name **or** id, and boolean/enum values **(spec deviation, additive)**

**Decision**: the spec (FR-020, FR-006, FR-009) writes
`set_param(node, "semitones", …)`; the shipped binding is
`set_param(node_id: u32, param: u8, value: f32)`
(`runtime/src/bindings/effects.rs:57`) and `ChainModel::set_param` takes
a numeric `ParamId` (`catalog.rs:76`). There are no wire names today —
`ParamDef.key` is a Fluent label key (`"effects-param-semitones"`).
API 1.4 therefore adds, in `crates/modplayer-effects/src/catalog.rs`
(host-owned, non-RT data):

- `param_wire_name(kind, ParamId) -> Option<&'static str>` and
  `param_by_wire_name(kind, &str) -> Option<ParamId>`: `pitch_shift`
  = `semitones`, `formant`, `quality_mode`; `time_stretch` = `ratio`,
  `quality_mode`; `gain` = `level`, `mute`; `filter` = `mode`, `cutoff`,
  `resonance`; `stereo_tools` = `width`, `balance`, `mono_sum`,
  `phase_invert`, `channel_swap`; `equalizer` = `band<n>_freq`,
  `band<n>_gain`, `band<n>_q`, `band<n>_type` for `n` in `1..=8`
  (32 `&'static str` literals in a const table keyed by `ParamId::eq_band`).
- `enum_names(kind, ParamId) -> Option<&'static [&'static str]>` for
  `Unit::Combo` discrete params, in the catalog's discrete index order:
  `quality_mode` = `["performance", "quality"]`; filter `mode` =
  `["high_pass", "low_pass"]`; band `type` = `["peak", "low_shelf",
  "high_shelf"]` (the order of the existing `effects-*` Fluent keys and
  the `Discrete { count }` indices — a core test pins each index against
  the engine's own enum).

The value **shape** rule for `params` (and for `set_param`'s value):
`Continuous` → number; `Discrete { count: 2 }` with `Unit::Toggle` →
boolean; `Discrete` with `Unit::Combo` → enum name string.

`set_param`/`schedule_param` 1.4 signature: `(node_id, param, value)`
where `param` is a number (1.0–1.3 form, unchanged) **or** the wire-name
string, and `value` is a number (unchanged), a boolean (toggles) or an
enum name string (combos). The runtime binding takes `mlua::Value` for
both and maps to `Request::SetParam { node, param: ParamRef, value:
ParamValue }` (`ParamRef::Id(u8) | Name(String)`); resolution against
the catalog happens in `core/src/plugins/apply.rs` next to
`parse_node_kind` (core already depends on `modplayer-effects`; the
runtime crate does not, and adding that dependency for a name lookup
would put catalog knowledge on the wrong side of the gateway). An
unknown name/enum name refuses `invalid_state`/`invalid_argument`
("Unknown parameter …"), the existing shape for bad arguments; a
numeric id or value keeps the 1.3 semantics byte for byte.

**Rationale**: the plugin (and every future author) should read
`node.params.semitones` and write `set_param(node, "semitones", v)`
with the same vocabulary; the numeric form stays so 1.0–1.3 scripts
and fixtures load unchanged (SC-008).

**Alternatives rejected**: `params` as an array indexed by `ParamId`
— unreadable for EQ ids (`16 + 4·band + n`) and forces every author to
carry a catalog in their head; a new `set_param_by_name` request —
duplicates a method for the sake of not touching one binding;
resolving names in the runtime binding — needs a runtime→effects
dependency purely for a string table.

## R3. `ChainModel` bumps `revision` on a **changed** parameter target or mode; mode-param writes route through `set_mode`

**Decision**: in `core/src/effects/model.rs`:

- `set_param`: after clamping, if the stored value changes
  (`(clamped - old).abs() > f32::EPSILON`) or the FR-008 rule changes
  `mode_state`, `revision += 1`. A no-op write (same value) neither bumps
  nor emits — no event storm from an idle slider, and a plugin's own
  echo is never "confirmed" twice.
- `set_param` with `param == mode_param_id(kind)` (pitch-shift `ParamId(2)`,
  time-stretch `ParamId(1)`) delegates to `set_mode(id, QualityMode::from(value))`
  so an explicit mode write — from a plugin or anyone — runs 008 FR-008
  rule 3 (`mode_after_user_set`, clears `auto_switched`). Today
  `set_param` on the mode id writes `params[pos]` but leaves
  `mode_state` stale, so a plugin's own `set_param(node, 2, 1.0)` would
  put the model's two views of the mode out of agreement; the UI already
  uses `chain_set_mode` (`effects_view.rs:585`), so host behaviour is
  unchanged.
- `set_mode`: bumps `revision` when `mode_state` changes (mode or the
  `auto_switched` flag).
- `set_source_rate`: bumps once if it emitted any command (008 FR-014
  recomputation is a parameter-target change).

`controller.rs`'s two hand-rolled `GatewayNodeInfo` projections
(`publish_plugin_snapshot_if_changed`, `fan_out_revision_events`) are
folded into one `node_info(index, &NodeModel, ids)` helper that also
fills `params` (via R2's wire names and shape rule, from
`NodeModel.params` — the control-side clamped **target**, never an RT
value: Constitution I) and `auto_switched` (`mode_state.map_or(false,
|m| m.auto_switched)`).

**Evidence**: `model.rs:266-308` (no bump), `:312-333` (`set_mode`, no
bump), `:338-363` (`set_source_rate`), `controller.rs:1089-1113`
(`tempo_step` → `chain_set_param` → the model), `effects_view.rs:272-325`
(host panel → `chain_set_param`/`chain_set_mode`). Every actor FR-020
lists already passes through these three model methods, so bumping
there covers plugin `set_param`/`schedule_param` (`apply.rs:371-396`),
panel edits, `tempo_step`, auto-switch/revert (inside `set_param`) and
rate recomputation, with nothing new in the controller.

**Alternatives rejected**: a separate `param_revision` counter and a
second event — two diffs, two events, for one plugin-visible concept;
bumping on every write including no-ops — needless per-tick fan-out
during a held slider.

## R4. Latent 009 defect: `effect_chain_changed` cannot be delivered for a plugin-owned node — fixed here with a regression test

**Decision**: `runtime/src/scheduler.rs:601-605` builds the event
payload with `lua.to_value(chain)`, but `NodeInfo.owner` is
`OwnerInfo::Plugin(String)`, an internally-tagged newtype variant that
serde cannot serialize — the exact failure `bindings/mod.rs:55-72`
documents and already worked around for `list_chain()`/`markers.list()`
by hand-building tables (`node_info_to_lua`). No shipped test or
fixture subscribes to `effect_chain_changed` (grep: zero hits under
`crates/*/tests` and `plugins/fixtures`), so the error never surfaced.
Key & Tempo's own nodes are plugin-owned, so every one of its
`effect_chain_changed` deliveries would abort the handler
(`AbortCause::Exception`). Fix: make `node_info_to_lua` `pub(crate)`
and use it in the scheduler; add the runtime regression test
`effect_chain_changed_with_plugin_owned_node_reaches_handler`.

**Rationale**: Constitution VIII — every bug fix ships with a regression
test; the fix is a prerequisite for FR-020 to be observable at all.

## R5. The reset/restore state machine mirrors the host (012 A12) and keeps one `settings` entry

**Decision**: the script keeps one state table `S` with three groups:

- **Node handles**: `S.pitch`, `S.stretch` (node ids or `nil`), found by
  `list_chain()` (own `kind`/`owner`) on `ready_ack` and re-found on every
  `effect_chain_changed` (a node the user removed becomes `nil`; never
  recreated while Active — spec Edge Cases).
- **Mirrored view** (rewritten from `params` on every
  `effect_chain_changed`/`list_chain`): `S.key`, `S.cents`, `S.tempo`,
  `S.formant`, `S.quality` (OR of both nodes' `quality_mode`), plus
  `S.sent = { semitones = <last composed value sent>, key, cents }` for
  the FR-006 decomposition tie-break (`|semitones − sent.semitones| ≤ 1e-6`
  ⇒ keep `sent.key/cents`, else `key = round-half-up(semitones)`,
  `cents = round((semitones − key) × 100)`).
- **Session flags**: `S.remember` (≡ current track has a `settings`
  entry), `S.keep_across` (session-wide, never persisted), `S.restored`
  (badge condition), `S.step` (Tempo step slider, default 10),
  `S.last_written` (last `settings` table written, to skip identical writes).

Flow: `track_changed` → `S.restored = false`; read
`api.state.track.get("settings")`; entry ⇒ apply five `set_param` calls
(semitones composed, formant, `quality_mode` on **both** nodes, ratio),
`S.remember = true`, `S.restored = true`, badge text set; no entry and
`not S.keep_across` ⇒ apply defaults (0 / false / performance / 1.0),
`S.remember = false`; no entry and `S.keep_across` ⇒ touch nothing,
`S.remember = false`. `effect_chain_changed` → re-find nodes, mirror
widgets, and if `S.remember` write `settings` when it differs from
`S.last_written`. A refusal from `state.track.set` reverts the toggle
and shows `err.message` in `restored` (012 A3 pattern).

Field-by-field defaulting on restore: a missing/out-of-range field
becomes its default (0 / 0 / 100 / false / "performance") and the full
table is rewritten on the next update (spec clarify-4).

**Rationale**: contract A12 in 012 ("never reinterprets its intent;
rebuilds from the host's next event") is the only rule that also stays
correct under host-side edits; one key keeps the write atomic and the
"remember is on ≡ entry exists" invariant trivially true.

**Alternatives rejected**: applying panel values optimistically —
diverges from the host on a clamp or a refusal; five separate keys —
five writes per change and a partial-write state to define.

## R6. Panel buttons are plain buttons routed in-script (012 R11) **(spec deviation)**

**Decision**: FR-003 declares the six buttons with a target `action`
(011 FR-008) and asserts dispatch "regardless of that action's own
keyboard-binding state". Verified false on this branch:
`controller.rs:2755` gates every panel-button dispatch on
`ActionRegistry::is_invocable`, which (`actions/registry.rs:320-326`)
is `false` while any chord of the action is in the conflict set.
`tempo_up`/`tempo_down` ship bound to `Plus`/`Minus`, which collide with
the host's `TempoStepUp` (`Equals`, `Plus`) / `TempoStepDown` (`Minus`)
(`actions/catalog.rs:581-594`), so on a fresh install the "Tempo up"/
"Tempo down" buttons would be inert — contradicting US2 scenarios 3
and 5. As 012 did for Set A/Set B, all six buttons carry no `action`
field and their `panel_interaction` calls the same script functions the
`action_invoked` handlers call. The eight actions are still registered
(keyboard rebinding works exactly as FR-004 describes).

**Rationale**: same ratified 011 rule, same fix, same living-example
pattern as the first bundled plugin; relaxing 011 D3/G14 is not this
feature's to reopen.

## R7. Default bindings: `tempo_up` → `Plus`, `tempo_down` → `Minus`; the host's fixed 10 % step wins on a fresh install

**Decision**: chord names are 007's canonical key names (`chord.rs:37-39`:
`Minus`, `Plus`; `Plus` renders as `+`). Both bindings conflict with the
host's `TempoStepUp`/`TempoStepDown` and are flagged inactive in the
shortcut map (011 FR-011); the host action steps the first time-stretch
node in chain order — Key & Tempo's own, the only one in this slice —
by `TEMPO_STEP` (0.10) via `chain_set_param`, which under R3 raises
`effect_chain_changed` and the panel follows. The `step_note` text
widget reads the "still steps by the default 10 %" string while
`S.step ≠ 10`. No host action changes.

## R8. Package layout, embedding, licence, README (012 R5, unchanged shape)

**Decision**: `plugins/bundled/org.modplayer.key-tempo/{plugin.toml,
main.luau, README.md, LICENSE-MIT, LICENSE-APACHE}`; embedded by a new
`key_tempo()` in `core/src/plugins/bundled.rs`, `packages()` =
`[section_loop(), key_tempo()]`. Manifest: `api = "1.4"`, `license =
"MIT OR Apache-2.0"`, `source = "bundled"`, `default_locale = "en-US"`,
five `[[permissions.required]]` (`audio.effects`, `ui.panel`,
`ui.shortcuts`, `state.track`, `playback.observe`) each with a one-line
justification, `[strings.en-US]` with every user-facing key (data-model
§3.5). Licence copies are byte-identical to the root files (test).
Existing tests that assert `packages()`/the Plugins list holds exactly
Section Loop (012 R5) now expect both bundled plugins, in declaration
order.

## R9. Strings: every string is `@key`; `update_widget` text and refusal messages

**Decision**: 012 R9 already extended `apply.rs::resolve_string` to
`UpdateWidget` text, so the badge (`@restored_text`), the step note
(`@step_note_text`) and the removed-node message (`@node_removed_text`)
are `@key` references resolved host-side; `""` and a host refusal
`message` (not `@`-prefixed) pass through. The Getting Started card's
strings are host Fluent keys in `locales/en-US/app.ftl`
(`getting-started-*`), checked by `ui/tests/fluent_keys.rs`.

## R10. Getting Started card: host-native, device-scoped flag in the settings file, `open_url` only from `App`

**Decision**: settings model (`core/src/settings/model.rs`) gains
`AudioSettings.getting_started_dismissed: bool` persisted as
`[onboarding] getting_started_dismissed = true` (absent ⇒ `false`),
saved through the store's existing atomic replace; it sits beside
`disclosure` (DM-27), which sign-out and revocation never touch
(`model.rs:71-74`), so it survives both. `PlaybackController` loads it
at construction, exposes `getting_started_dismissed()` and
`dismiss_getting_started()` (→ `persist_settings`, `settings-save-failed`
warning on failure like `set_nudge_step_ms`, `controller.rs:1235`).
`core/src/links.rs` gains `GETTING_STARTED_TUTORIAL_URL`
(`https://github.com/rzcastilho/mod-player/blob/main/docs/plugin-tutorial.md`,
a plain constant — no `option_env!` override, the spec names one
constant). New `ui/src/getting_started.rs` draws the card; `App::
show_library` (`app.rs:555`) calls it above `library_view::show` when
no detail target is open and the flag is unset, and handles the two
outcomes: `OpenTutorial` → `ctx.open_url(OpenUrl::new_tab(...))`
(design note 10: `open_url` only ever happens in `App`), `Dismiss` →
`controller.dismiss_getting_started()`. Non-modal (an ordinary framed
group in the scroll content), keyboard-operable (egui buttons carry
their text as the accessible name; the link is a `Button` with
`Role::Link` via `accesskit_node_builder`, as the tab row does with
`Role::Tab`).

**Rationale**: spec clarify-6 (002 FR-003's device-scoped precedent;
J-1 step 5 placement; 002's browser path).

**Alternatives rejected**: an account record — deleted on sign-out
(002), would re-show after every sign-in; a plugin `ui.panel` — cannot
exist before any plugin is enabled and cannot speak for two plugins.

## R11. Test strategy: host-observable, real package, no probes (012 R10)

**Decision**: `core/tests/controller_key_tempo.rs` drives the real
`org.modplayer.key-tempo` package through `plugin_panel_interaction`/
`invoke_plugin_action`/`tempo_step`/`chain_set_param` on `FakeBackend` +
`ScriptedHost`, asserting on `controller.chain().nodes()[..].params`,
the panel registry's widget values, `PluginStateStore` contents and the
`orphaned` flags — mirroring `controller_section_loop.rs`'s harness
(`fresh_store`, `pump_until`, `wait_panel_registered`).
`core/tests/bundled_key_tempo.rs`: manifest validity, licence bytes,
`packages()` order, and the SC-008 scan (every `api.<ns>.<method>(` in
`main.luau` is a `v1.toml` request — std string scanning, as 012).
Audio-level claims (SC-001/SC-002 "no artefacts", 20 ms) are 008's
already-gated latency/ramp tests plus manual scenarios; no new RT test.

## R12. Nothing on the real-time path changes

**Decision**: no edit to `modplayer-engine`/`modplayer-effects` RT
code; `Command::ChainSetParam` and the ramp/crossfade are untouched.
The catalog additions (R2) are `const`/`static` tables read on the
control thread only. PR real-time note: "N/A — no real-time path
changes."

## R13. Rate/budget headroom

**Decision**: worst case per `track_changed` restore: 1 `state.track.get`
+ 5 `set_param` + ≤ 9 `update_widget` — under the `effects` and `ui`
buckets (100/s). Per `effect_chain_changed` tick: 0 RPC reads (the
event carries the chain), ≤ 9 `update_widget` (only for changed
values), ≤ 1 `state.track.set`. The host side adds one `Vec<NodeInfo>`
projection per tick in which any parameter changed (≤ 16 nodes × ≤ 32
params), on the control thread — negligible.

## R14. Restart-after-suspend and re-enable reuse the orphaned nodes

**Decision**: `ready_ack` → `list_chain()`; own `pitch_shift` and
`time_stretch` present (009 `readopt`, `model.rs:396`) ⇒ adopt, no
`set_param`, mirror from `params`, `S.remember` = entry exists,
badge `""`; exactly one missing ⇒ create only it (`before`/`after` the
survivor, `resolve_position`, `model.rs:440`); none ⇒ create
`pitch_shift` then `time_stretch { after = "pitch_shift" }` (adjacent,
008 FR-007), then run the `track_changed` logic with
`api.playback.state().track` (`nil` ≡ no entry). `keep_across` starts
`false` on every `ready()` (script memory). Matches spec clarify-5.

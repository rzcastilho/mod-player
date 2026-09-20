# Research: Plugin UI Contributions

**Feature**: 011-plugin-ui-contributions | **Date**: 2026-09-20 | **Spec**: [spec.md](spec.md)

Phase 0 output. Every decision below was checked against the code on
this branch (009's gateway/runtime/core/ui, 010's arbiter, 007's Action
& Binding service, 005/006's waveform and markers, 001's theme,
notifications and Settings registry), the constitution (v1.1.1) and the
spec's two Clarifications sessions. Format per decision: Decision /
Rationale / Alternatives considered. No `NEEDS CLARIFICATION` remains in
[plan.md](plan.md)'s Technical Context.

## R1. API delta: 1.1 → 1.2, schema-first in `api/v1.toml`

**Decision**: One edit point, `crates/modplayer-capability-gateway/api/
v1.toml`: `minor = 2`; the five `ui.*` permissions flip to
`operable = true`; nine `[[request]]` entries (`RegisterPanel`,
`UpdateWidget`, `AddOverlays`, `RemoveOverlays`, `ClearOverlays`,
`RegisterAction`, `RegisterSettings`, `GetSettings` in a new `category =
"ui"`; `Notify` in a new `category = "notify"`); three `[[event]]`
entries (`PanelInteraction` requires `ui.panel`, `ActionInvoked` requires
`ui.shortcuts`, `SettingsChanged` requires `ui.settings`). `build.rs`
regenerates `RequestKind`/`EventKind`/`RateCategory` consumers; the
exhaustive matches in `bindings::dispatch`, `HostEvent::kind()` and
`scheduler::event_to_lua` fail to compile until every arm exists;
`tests/api_reference.rs` regenerates `docs/plugin-api/v1.md`.
`HOST_CAPABILITIES` gains the five `ui.*` names. Lua surface:
`api.ui.register_panel / update_widget / add_overlays / remove_overlays /
clear_overlays / register_action / register_settings / get_settings /
notify` (namespace `ui`), matching §6.5's `ui.<call>` naming.

**Rationale**: Constitution IX (one generated definition); 010 already
proved the additive-minor path (1.0 → 1.1) with zero manifest breakage.
Every 1.0/1.1 manifest still loads: `api = "1.0"` means `>= 1.0, < 2.0`
(`ApiRange::supports`).

**Alternatives considered**: (a) a separate `api/ui.toml` — rejected,
Constitution IX says *exactly once*; (b) bumping to 2.0 — rejected, no
removal, no semantic change to any existing call.

## R2. Rate limiting: two new buckets, `Ui` (100/1 s) and `Notify` (6/60 s), inside the existing `RateLimiter`

**Decision**: `RateCategory` gains `Ui` and `Notify`; `RateLimiter`
becomes seven buckets, each with its own `(limit, window)` pair
(`Transport/Markers/Effects/State/Timers/Ui` = 100 per 1 s, `Notify` = 6
per 60 s). `Gateway::admit`'s order (permission → focus → rate) is
unchanged, so the notify window is consumed **before** core validates
`level`/`text` — exactly FR-020's "a call refused at validation still
counts; a `permission_denied`/`rate_limited` call does not".

**Rationale**: FR-020a extends 009 FR-025 by category; FR-020 needs a
different window shape. One limiter, one admission path (Constitution
II, G1/G2); the notification budget PL-8.1 lives where every other
per-plugin budget already lives.

**Alternatives considered**: (a) a separate notify counter in core —
rejected: it would run *after* validation (wrong count semantics) and
duplicate the gateway's role; (b) counting `notify` under `Ui` too —
rejected: FR-020a says `notify` is governed solely by its own window.

## R3. UI DTOs and pure validation live in the gateway crate; the registry (state) lives in core

**Decision**: `crates/modplayer-capability-gateway/src/ui.rs` (+
`ui/limits.rs`) defines the closed vocabularies as plain data —
`WidgetSpec { id, kind: WidgetKind, label, … }`, `OverlayPrimitive`,
`SettingsField`, `ActionSpec`, `NotifyLevel`, `OverlayColor`, the id
grammar `[a-z][a-z0-9_]{0,63}`, and the spec-fixed caps (100 widgets,
16 panels, 500 primitives, 64 actions, 100 fields, 256 items, 64
options, 32 glyphs, text bounds) — plus **pure** validators
(`validate_panel(&[WidgetSpec]) -> Result<(), Refusal>`,
`validate_primitives`, `validate_schema`, `validate_notify`) that
produce the exact `invalid_state` reasons FR-024 lists and the
`widgets[<i>] (<id>)` / `fields[<i>]` path messages. `Request` gains
one variant per new request carrying these DTOs. The **stateful**
registry — which plugin owns which panels/overlays/actions/schema,
current widget values, per-plugin counts — is
`crates/modplayer-core/src/plugins/ui/{panel,overlay,settings,notify}.rs`
behind `PluginHost` (Constitution III), applied from
`plugins::apply::dispatch` like every other RPC.

**Rationale**: the gateway crate is the "runtime call checker"
Constitution IX names and already hosts manifest validation with
proptests (Constitution VIII); dependency-free pure validators are unit-
and property-testable without a plugin thread. Core owns everything that
composes with `PluginRecord`, `TrackMarkers`, `ActionRegistry`, the
notification center and the settings file.

**Alternatives considered**: (a) validate in `bindings::` on the plugin
thread before the RPC — rejected: the caps depend on host-side counts
(a 17th panel) so a second check in core is unavoidable; one place is
simpler; (b) validate only in core — rejected: loses the dependency-free
proptest surface and puts DTO grammar next to controller code.

## R4. Every `ui.*` request except `get_settings` is an RPC to core; `get_settings` is served locally from the plugin thread's store

**Decision**: `register_panel`, `update_widget`, `add_overlays`,
`remove_overlays`, `clear_overlays`, `register_action`,
`register_settings`, `notify` fall through `bindings::dispatch`'s `_ =>
rpc(shared, request)` arm (RT7) and are applied in
`plugins::apply::dispatch` on the controller thread. `get_settings` is a
local read (like `state.plugin.get`): the runtime keeps the last
successfully registered schema in `Shared.settings_schema` and reads
values from the store's new `Scope::Settings` (R5), substituting
`default` for missing/invalid values and logging a console warning.

**Rationale**: registration and updates mutate host-owned state and
must serialise with the controller's `tick` (C1); `get_settings` is a
pure read of data the plugin thread already holds, and keeping it local
avoids a 1-frame RPC round trip for the most frequently polled call.

**Alternatives considered**: `get_settings` as an RPC — rejected: the
values would then live in two places (core + store) with no benefit.

## R5. Settings values: a third store scope, `Scope::Settings`, written only by host-initiated `Control::SettingsWrite`

**Decision**: `PluginStateStore` gains `Scope::Settings` (file
`<root>/<hex(identifier)>/settings.json`, flushed by the same
`StateWriter`, counted in `used_bytes` against the 10 MB cap). The Lua
`api.state` table exposes only `plugin`/`track`, so `Settings` is
unreachable from script (FR-018 "not addressable through `state.plugin`
get/set"). Writes originate solely from the Settings page: core updates
its own view copy, checks `gauges.storage_used_bytes() + delta ≤ cap`
(a new atomic on `PluginGauges` the store updates on every set/remove),
then sends `Control::SettingsWrite { changes }` to the handle; the
scheduler applies `store.set(Scope::Settings, …)` for each change and
**then** dispatches the `settings_changed` handler with `{ changes }`
in the same inbox item, so a plugin never observes the event before the
value. On `register_settings` the runtime first reads the current
`Settings` scope and ships `{ fields, values }` in the RPC so core can
render the page without a second round trip; core stores the values it
was handed and never reads `settings.json` itself. A cap breach at the
core-side check reverts the edit, logs a console warning and shows no
notification (edge case; ≤ 100 fields × ≤ 1 KB makes it unreachable in
practice).

**Rationale**: FR-018 and Constitution III/X — one store, one cap, one
atomic writer, one thread owning the file; the event ordering guarantee
falls out of the single inbox.

**Alternatives considered**: (a) a core-owned `settings.json` written by
the controller — rejected: two writers to one directory, cap accounting
split across threads; (b) settings as ordinary `state.plugin` keys under
a reserved prefix — rejected: FR-018 forbids `state.plugin` reachability
and 009's key grammar has no reserved namespace.

## R6. Action registry generalisation: `ActionId = Host(HostAction) | Plugin(PluginActionId)` with ownership tiers

**Decision**: `modplayer-core::actions` gains `PluginActionId { plugin:
PluginIdentifier, name: String }` (persisted id `"<identifier>.<name>"`,
parsed by `rsplit_once('.')` — the identifier may contain dots, the name
never does), `ActionId`, `OwnerTier { Host, Bundled, Community }`, a
dynamic `PluginActionDef { label, kind, repeats_while_held, tier,
default_binding: Option<Chord> }` table, and `ActionOwner::Plugin { id:
PluginId, tier }`. `ActionRegistry` is re-keyed on `ActionId`
(`BTreeMap` for the dynamic side, the existing 46-slot array for the
host side); `HostAction`-typed convenience methods stay via
`impl From<HostAction> for ActionId` so 007's tests compile unchanged.
New mutators: `register_plugin_action`, `unregister_plugin_actions(id)`,
`set_plugin_enabled(id, bool)` (FR-013). `rebuild()`'s conflict pass
applies FR-011: for a chord with ≥ 2 enabled live candidates, if the
top tier is held by exactly one candidate it stays unflagged and only
the lower tiers are flagged; otherwise (same-tier tie at the top) every
top-tier candidate is flagged, lower tiers too. `resolve()` still fires
only unflagged candidates, so the host's `L` keeps toggling the loop.
`KeymapOverrides` becomes keyed by `String` id with a `dormant` set for
non-`host.` ids whose action is not registered yet (FR-010a): they are
persisted verbatim, never reported as invalid, and adopted the moment
`register_plugin_action` sees a matching id. Scope of every plugin
action is `App`.

**Rationale**: 007 FR-016 reserved exactly this extension; keeping the
host side as the fixed array preserves 007's O(1) hot path and every
existing test; the tier rule is a strict superset of 007 FR-009's
symmetric rule (the two-host case is a same-tier tie).

**Alternatives considered**: (a) a second `PluginActionRegistry` beside
`ActionRegistry` — rejected: conflicts are cross-registry by definition
and `dispatch` would need to consult two indexes; (b) making `HostAction`
carry a `Plugin(String)` variant — rejected: `HostAction: Copy` and the
46-slot array assumptions permeate 007.

## R7. Plugin action dispatch: `Invocation.action: ActionId`; `Plugin(_)` goes to `controller.invoke_plugin_action(id, source)`

**Decision**: `modplayer-ui::actions::dispatch` returns `Invocation {
action: ActionId, repeat }`; `invoke()` keeps its `HostAction` match
and adds one arm for `ActionId::Plugin` that calls
`PlaybackController::invoke_plugin_action(id, ActionSource::Keyboard)`.
A panel `button` with `action` set calls the same façade with
`ActionSource::Ui` (FR-008). The façade gates on
`ActionRegistry::is_invocable(id)`: the action exists, `enabled` is
`true` (FR-013 — owning plugin Active) **and** it is not flagged as
conflicting on any of its chords (FR-012's "inactive action" covers
both cases and FR-008 says the button path includes "the FR-011
conflict/FR-013 enabled gate"). Keyboard invocations reach the façade
only after `resolve()` already excluded flagged chords, so the gate is
redundant there and decisive for the button path. The façade delivers
`HostEvent::ActionInvoked { action, source, value }` **directly to the
handle** (never fanned out), `value = 1.0` for `Continuous`, absent for
`Trigger`. An unknown target action on a button is inert and logged
(spec Edge Cases).

**Rationale**: FR-008/FR-012; the direct-to-handle path is 010's
precedent for per-plugin events (`focus_granted`).

**Alternatives considered**: routing a panel button through a synthetic
key event — rejected: it would be subject to chord conflicts the user
never typed.

## R8. Panels: a core-owned `PanelRegistry` + `PluginPanelsView`; egui rendering in `modplayer-ui/src/plugin_panels.rs`

**Decision**: `plugins/ui/panel.rs` holds, per plugin, an ordered
`Vec<Panel { id, title, widgets: Vec<WidgetState>, registered_seq }>`
(≤ 16) where `WidgetState = (WidgetSpec, Value)`; `register_panel`
replaces atomically; `update_widget` validates per kind (FR-007) and
mutates the value. The view (`PluginPanelsView`) is rebuilt each frame
from records + registry + `[plugin_panels]` state and gives the UI, per
visible panel: attribution (name, icon handle key), placement, `body:
Live(widgets) | Placeholder { cause }`. Rendering: the **dock** is an
`egui::SidePanel::right("plugin-dock")` drawn inside Now Playing's
central area (hidden when it holds no visible panel, `ScrollArea`
inside, fixed width 280 px); **floated** panels are `egui::Window`s
with `.constrain(true)`, `.min_size(200×120)`, `.default_size(docked
size)`, `.default_pos(centre of the Now Playing rect)`, drawn only
while `Section::NowPlaying` is shown, above the central content and
below the notification area (drawn before `notifications::show` in
`shell.rs`'s order). Widget kinds map to egui: `label` → `Label`,
`button` → `Button`, `toggle` → `Checkbox`, `slider` → `Slider`, `knob` →
`Slider` with a rotary painter (custom `widgets/knob.rs`, same
`Response`/`WidgetInfo` as a slider), `list` → a `ScrollArea` of
`selectable_label` rows with arrow-key selection-follows-focus, `marker
list` → 006's marker-row painter filtered by owner, `text` → `Label`
(wrapped, ≤ 1,024 chars), `meter` → `widgets/peak_meter.rs` in a
`[0,1]` single-bar mode. Every widget sets `WidgetInfo::labeled(role,
enabled, label)` so AccessKit announces "Tempo, slider" (NFR-6.3).
Focus claims: each interactive widget registers `Claim::Keys` for
exactly the keys its kind consumes (FR-007a).

**Rationale**: mirrors 008/010's Now Playing-scoped panels; egui's
`SidePanel`/`Window` already give dock/float/clamp/resize; the theme is
applied through `ctx.set_theme` → `ui.visuals()` so a theme switch
re-renders every widget on the next frame with zero plugin involvement
(FR-009).

**Alternatives considered**: (a) a bespoke docking layout — rejected,
Constitution X; (b) `egui::Window` for docked panels too — rejected,
docked order must be deterministic (plugin name, registration order),
which a `SidePanel` column gives for free.

## R9. Panel interaction delivery: one `panel_interaction` per committed change, direct to the handle

**Decision**: the UI pushes `PanelInteraction { plugin, panel, widget,
value }` to the controller (`controller.plugin_panel_interaction(..)`)
which updates the registry's value and delivers
`HostEvent::PanelInteraction` to the handle. Slider/knob pointer drags
deliver on `drag_stopped()` only; each keyboard step delivers
immediately; toggle on flip; button on `clicked()`; list on selection
change (focus/arrow/click). A `button` whose `action` is set never
produces `panel_interaction` (R7).

**Rationale**: FR-002/FR-007; the direct-to-handle delivery avoids the
permission fan-out filter (the plugin necessarily holds `ui.panel`).

**Alternatives considered**: per-pixel drag events — rejected by the
spec (Clarifications).

## R10. Overlays: `OverlaySet` per plugin in core; painted through 006's existing `overlays` callback

**Decision**: `plugins/ui/overlay.rs` holds per plugin a
`BTreeMap<String, OverlayPrimitive>` (≤ 500, count checked after the
prospective merge) plus a registration sequence number for
cross-plugin z-order. `now_playing.rs`'s existing `overlays: &mut dyn
FnMut(&Painter, &TimeSpace)` closure (005/006's attachment point, run
after peaks/markers/loop region and before the playhead) gains a
`plugin_overlays::paint(painter, space, view, kind: Overview|Detail)`
call after the marker/loop pass, which converts track-time ms →
frames via `space`'s sample rate and draws: `line` = 1 px vertical
span, `region` = translucent fill, `glyph` = 12 px texture/host glyph
in a 16 px "plugin lane" above the waveform rect (the lane is added to
both views' reserved height), `label` = detail view only, text in the
lane. Colors come from `theme::overlay_color(token, visuals)`: `accent`
= `visuals.selection.bg_fill`, `secondary` = `visuals.hyperlink_color`,
`positive` = `MARKER_PALETTE[2]`, `warning` = `MARKER_PALETTE[3]`,
`neutral` = `visuals.weak_text_color()` — the only new colour mapping,
in `theme.rs`. Cleared on `TrackChanged` (same fan-out trigger as
010's first-wins reset) and in `PluginHost::stop` for every reason.

**Rationale**: FR-014/FR-015/FR-016; the re-projection is free because
paint reads `TimeSpace` each frame — no plugin code runs on zoom/scroll
(SC-003).

**Alternatives considered**: a separate egui layer over the waveform —
rejected: it would need its own coordinate mapping and break the
"above markers, below playhead" order.

## R11. Icons and glyphs: PNG only, decoded once at discovery, `image` crate gains the `png` feature

**Decision**: `Manifest` gains `icon: Option<String>` and `glyphs:
BTreeMap<String, String>` (package-relative paths, keys
`[a-z][a-z0-9_]{0,31}`, ≤ 32). `BundledPackage` gains `resources:
&'static [(&'static str, &'static [u8])]` (`include_bytes!`).
`PluginHost::discover` resolves each declared asset: present, decodes
as PNG, within size/dimension limits → `PluginAssets { icon:
Option<RgbaImage>, glyphs }` on the record; otherwise a console warning
naming the file and the generic glyph is used (FR-014a). The UI uploads
each `RgbaImage` to an `egui::TextureHandle` lazily on first draw and
caches it per plugin in `Context` memory. Workspace `image` dependency
gains `"png"` beside `"jpeg"` (still no default features).

**Rationale**: NFR-4.7 — PNG cannot carry script; SVG is excluded by
the spec; decoding at discovery keeps the UI thread free of file I/O
and makes a malformed asset visible once, in the console.

**Alternatives considered**: (a) a new `png` crate — rejected, `image`
is already a dependency (Constitution X: no new crate); (b) decoding in
the UI on demand — rejected, repeated warnings and per-frame cost.

## R12. Notifications: attribution field + one Fluent key for plugin text

**Decision**: `Notification` gains `attribution: Option<PluginAttribution
{ id: PluginId, name: String }>`; `notify` raises
`raise_with_args("plugin-notification", [("plugin", name), ("text",
text)])` with the attribution set; the UI renders the plugin's icon (or
generic glyph) and name before the text and never as a modal (FR-019).
Severity maps `critical`/`warning`/`info` 1:1 to 001's `Severity`.

**Rationale**: `message_key` is a Fluent key by design (001 FR-021);
routing the raw plugin text through a `{ $text }` argument keeps that
invariant while the attribution icon needs a typed field the ordinary
`raise*` methods never set.

**Alternatives considered**: a raw-text notification variant — rejected:
breaks 001's "never raw text" rule for host notifications.

## R13. Settings pages: `PluginSettingsView` under Settings › Plugins; search merges a dynamic descriptor list

**Decision**: a new `settings/plugins.rs` takes over the `Plugins` arm
of `settings/mod.rs` (today a shared placeholder arm with Offline and
Privacy) — it keeps 009's placeholder plugin-management content and
adds a sub-page list — one entry per Active plugin with a
registered schema, sorted by name — rendering fields as egui
`Checkbox`/`DragValue`+`Slider`/`TextEdit`/`ComboBox` with accessible
names from the field label and description as tooltip/desc; boolean
and choice apply on change, number/string on commit (`lost_focus() ||
Enter`). `settings_registry::search` keeps its static `DESCRIPTORS`; a
new `search_plugin_settings(query, &[PluginSettingsView]) ->
Vec<PluginSettingHit>` returns matches by field title, and the Settings
search UI concatenates both result lists, rendering plugin hits with
the category path "Plugins › <plugin name>" and deep-linking to the
sub-page (SC-006).

**Rationale**: FR-017; 001's search is static by construction and must
stay `&'static` for host settings; a parallel dynamic list is the
smallest change.

**Alternatives considered**: making `DESCRIPTORS` dynamic — rejected: it
is `const` and referenced by 001's tests.

## R14. Panel placement/visibility persistence: `settings.toml` `[plugin_panels]`

**Decision**: `AudioSettings` gains `plugin_panels: BTreeMap<String,
PanelPlacement { placement: Docked | Floated, x, y, w, h: Option<f32>,
disabled: bool }>` keyed `"<identifier>/<panel-id>"`, read/written by
`settings/store.rs` under 007's atomic-write rule (unknown keys
retained dormant; malformed entries dropped with the existing
`InvalidField` reporting). Session-only "closed" state lives in
`PanelRegistry` (never persisted). The Plugins list row gains per-panel
Show/Hide + Enable/Disable controls (`plugins_view.rs`).

**Rationale**: FR-005/FR-006; 007 FR-013 and 010 R5 precedent (user
customisation → `settings.toml`).

**Alternatives considered**: egui `Context` memory persistence —
rejected: not cross-process-deterministic and not user-inspectable.

## R15. Suspension placeholder vs. removal: registry keeps definitions on `Suspend`, clears on `Ready`/`Disable`/`Shutdown`

**Decision**: `PluginHost::stop(id, reason, ..)` calls `ui.on_stop(id,
reason)`: `Suspend` → keep panels, clear overlays, mark actions disabled
(`set_plugin_enabled(false)`), hide the settings page (view filters on
`Lifecycle::Active`); `Disable`/`Shutdown` → remove panels, overlays,
actions (bindings' user overrides return to the dormant set), schema.
`RuntimeEvent::Ready` → `ui.on_ready(id)`: drop any placeholder panels
(the plugin re-registers), re-enable its actions, conflicts
re-evaluated by `rebuild()`. The view renders a kept panel of a
`Suspended` record as `Placeholder { cause }` with Restart/Disable
buttons wired to 009's existing `plugin_restart`/`plugin_disable`
façades.

**Rationale**: FR-025 and the Clarifications' reading of §5.4 vs PL-3.4.

**Alternatives considered**: keeping widgets visible-but-inert during
suspension — rejected by the spec ("widgets removed").

## R16. `request_focus()` from an interaction handler: an origin flag on the RPC

**Decision**: the scheduler sets `Shared.in_interaction_handler = true`
while it runs a `panel_interaction` or `action_invoked` handler; the
`transport.request_focus` binding reads it and sends `Request::
RequestFocus { interaction: bool }`. `FocusArbiter::request(p, running,
origin: RequestOrigin)` grants immediately under `AutoOnInteraction`
when `origin == UserInteraction` (revoking any plugin holder through the
existing hand-over sequence, C3); `Manual`/`FirstRequestWins` ignore
the flag (010 unchanged). The flag is a plain bool in the plugin's own
`Shared`, reset when the handler returns or aborts.

**Rationale**: FR-026 and 010's deferred clarification; the plugin thread
is the only place that knows which handler is running, and the flag
never crosses threads except inside the request.

**Alternatives considered**: core inferring "recently delivered
interaction" from timestamps — rejected: racy and observable.

## R17. String localisation: `[strings.<locale>]` manifest tables, resolved in core at registration time

**Decision**: `ManifestDto` gains `default_locale: Option<String>`
(default `"en-US"`) and `strings: BTreeMap<String, BTreeMap<String,
String>>` (`[strings.en-US] key = "…"`), recorded on `Manifest`.
`plugins/ui/strings.rs::resolve(record, s) -> String` renders a literal
as-is and a `@key` through the active host locale (`i18n::active_locale()`,
en-US only) then the plugin's default locale; an unresolved key renders
literally with a console warning; an empty resolved widget label is an
unlabeled widget (FR-001 via the validator, which runs on resolved
labels). Applied to every user-facing string the spec lists.

**Rationale**: FR-021/PL-7.3 mechanism now, no host locale switch yet
(unchanged since 001).

**Alternatives considered**: Fluent files inside the package — rejected:
the host's `fluent-templates` loader is compile-time and would let a
plugin ship Fluent function calls the host must not evaluate.

## R18. Fixtures: six new `plugins/fixtures/ui-*` packages, one per surface plus one refusal driver

**Decision**: `ui-panel` (one of every widget kind; `debug_probe`
`register_unlabeled`, `register_17th`, `update_out_of_range`,
`update_label`; requests focus from a "Take over" button's
`panel_interaction` handler), `ui-shortcuts` (registers `take_over`
bound to `L`, `nudge` continuous bound to `Shift+K`, `tab_bound` with
default `Tab`; logs `action_invoked`), `ui-overlay` (adds one primitive
of each kind on `track_changed`; probes `add_501st`, `bad_region`,
`bad_icon`), `ui-settings` (boolean + number + string + choice; logs
`settings_changed`; probe `get`), `ui-notify` (probe `post` ×N with
level/text arguments), `ui-icons` (manifest `icon` + two glyphs, one
deliberately over-limit). Each is a real package (`plugin.toml`,
`main.luau`, `README.md`, PNGs where needed), embedded via
`bundled.rs`, visible only under `MODPLAYER_PLUGIN_FIXTURES=1` (16
fixtures total).

**Rationale**: 009 R18/010 R9 precedent — every manual scenario and
every controller test drives a real Luau plugin, so the Lua payload
shapes and the gateway path are exercised end to end.

**Alternatives considered**: one mega-fixture — rejected: permissions
differ per surface and refusal tests need a plugin *without* a grant.

## R19. Test strategy per NFR

**Decision**: gateway — `tests/ui_validation.rs` (every refusal reason,
proptest over the id grammar and widget lists, caps at the boundary),
`tests/gateway.rs` (+ `notify_window_6_per_60s`, `ui_category_101st`),
`tests/api_reference.rs` regenerated for 1.2; runtime —
`tests/bindings.rs` (ui namespace → RPC, `get_settings` local,
`Scope::Settings` unreachable from Lua), `tests/scheduler.rs` (three
payload renderers, `SettingsWrite` ordering, interaction flag); core —
`tests/actions.rs` (+ tier rule table, dormant overrides, plugin id
parsing), `tests/plugin_ui_registry.rs` (panels/overlays/settings/notify
state machine), `tests/controller_plugin_ui.rs` (US1–US5 end to end on
`FakeBackend` + fixtures via `debug_probe`), `tests/settings.rs`
(`[plugin_panels]` round trip proptest); ui — `tests/plugin_panels.rs`
(offscreen `egui::Context`: roles/names per kind, key precedence,
one-event-per-drag, theme switch repaint with zero events),
`tests/plugin_overlays.rs` (re-projection under zoom/scroll),
`tests/settings_plugins.rs`, `tests/notifications.rs` (attribution, no
modal), `accessibility.rs`/`fluent_keys.rs` extended. Manual M1–M10 in
[quickstart.md](quickstart.md).

**Rationale**: Constitution VIII names permission enforcement and plugin
isolation explicitly; SC-001/SC-004 need AccessKit-level assertions the
offscreen context provides.

## R20. Things deliberately not built (Constitution X)

Detach to a second display, user reordering of docked panels, container
widgets, plugin-settable widget disabled/hidden, MIDI sources,
notification keys/replace/dismiss for plugins, community source, pt-BR
resources (en-US only, additive later), per-plugin scope other than
`App`, accessible exposure of individual overlays (panel `list`/`marker
list` is the accessible channel).

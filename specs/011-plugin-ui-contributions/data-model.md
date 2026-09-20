# Data Model: Plugin UI Contributions

**Feature**: 011-plugin-ui-contributions | **Date**: 2026-09-20 | **Spec**: [spec.md](spec.md) | **Research**: [research.md](research.md)

Phase 1 output. `+` = new type, `~` = changed type. Rust signatures are
indicative; visibility follows 009/010's conventions (private with
accessors unless the UI reads them as a view). Every numeric bound is
spec-fixed (spec Assumptions) and lives once, in
`modplayer_capability_gateway::ui::limits`.

## 1. `modplayer-capability-gateway` — DTOs, limits, validators

### 1.1 `api/v1.toml` (~) — see [contracts/plugin-api-v1.2.md](contracts/plugin-api-v1.2.md)

`minor = 2`; `ui.panel`/`ui.overlay`/`ui.shortcuts`/`ui.settings`/
`ui.notify` → `operable = true`; 9 requests; 3 events. `HOST_CAPABILITIES`
+= the five `ui.*` names. `RateCategory` (~) += `Ui`, `Notify`.

### 1.2 `src/ui/limits.rs` (+)

| Constant | Value | Spec |
|---|---|---|
| `MAX_PANELS_PER_PLUGIN` | 16 | FR-003 |
| `MAX_WIDGETS_PER_PANEL` | 100 | FR-003 |
| `MAX_LIST_ITEMS` | 256 | FR-002 |
| `MAX_LABEL_CHARS` | 256 | FR-002 |
| `MAX_TEXT_CHARS` | 1,024 | FR-002 / FR-007 |
| `MAX_PANEL_TITLE_CHARS` | 64 | FR-003 |
| `MAX_OVERLAY_PRIMITIVES` | 500 | FR-015 |
| `MAX_OVERLAY_LABEL_CHARS` | 64 | FR-014 |
| `MAX_ACTIONS_PER_PLUGIN` | 64 | FR-010 |
| `MAX_SETTINGS_FIELDS` | 100 | FR-017 |
| `MAX_CHOICE_OPTIONS` | 64 | FR-017 |
| `MAX_STRING_FIELD_CHARS` | 1,024 | FR-017 |
| `MAX_NOTIFY_TEXT_CHARS` | 200 | FR-019 |
| `NOTIFY_LIMIT` / `NOTIFY_WINDOW` | 6 / 60 s | FR-020 |
| `UI_LIMIT` / `UI_WINDOW` | 100 / 1 s | FR-020a |
| `MAX_GLYPHS` | 32 | FR-014a |
| `ICON_MAX_PX` / `ICON_MAX_BYTES` | 128 / 256 KiB | FR-014a |
| `GLYPH_MAX_PX` / `GLYPH_MAX_BYTES` | 32 / 64 KiB | FR-014a |
| `ID_GRAMMAR` | `[a-z][a-z0-9_]{0,63}` | FR-002a |
| `GLYPH_KEY_GRAMMAR` | `[a-z][a-z0-9_]{0,31}` | FR-014a |

### 1.3 `src/ui.rs` (+) — closed vocabularies (Key Entities "Widget", "Overlay Primitive", "Settings Field")

```rust
+ pub struct UiId(String);                       // parse() enforces ID_GRAMMAR
+ pub enum WidgetKind { Label, Button, Toggle, Slider, Knob, List, Text, MarkerList, Meter }
+ pub struct ListItem { pub id: UiId, pub label: String }
+ pub struct WidgetSpec {
+     pub id: UiId,
+     pub kind: WidgetKind,
+     pub label: String,                          // resolved (R17) before validation; empty ⇒ unlabeled_widget
+     pub min: Option<f64>, pub max: Option<f64>, pub step: Option<f64>, pub value: Option<f64>, // slider/knob
+     pub items: Vec<ListItem>, pub selected: Option<UiId>,                                        // list
+     pub action: Option<String>,                 // button → "<identifier>.<name>" target (FR-008)
+     pub text: Option<String>,                   // label/text initial content
+ }
+ pub enum WidgetValue { Bool(bool), Number(f64), Text(String), Item(UiId), Items { items: Vec<ListItem>, selected: Option<UiId> } }
+ pub enum OverlayColor { Accent /*default*/, Secondary, Positive, Warning, Neutral }
+ pub enum OverlayPrimitive {
+     Line   { id: UiId, at_ms: u64, color: OverlayColor },
+     Region { id: UiId, from_ms: u64, to_ms: u64, color: OverlayColor },   // from < to
+     Label  { id: UiId, at_ms: u64, text: String, color: OverlayColor },   // ≤ 64 chars, detail view only
+     Glyph  { id: UiId, at_ms: u64, icon: GlyphRef, color: OverlayColor },
+ }
+ pub enum GlyphRef { Host(HostGlyph), Package(String) }   // Package key must exist in manifest.glyphs
+ pub enum HostGlyph { Dot, Flag, Note, Chord, Star, Warning }
+ pub enum ActionKindSpec { Trigger, Continuous }
+ pub struct ActionSpec { pub id: UiId, pub label: String, pub kind: ActionKindSpec,
+                         pub default_binding: Option<String>, pub repeats_while_held: bool }
+ pub enum FieldKind { Boolean, Number { min: f64, max: f64, step: f64 }, String, Choice { options: Vec<ListItem> } }
+ pub struct SettingsField { pub id: UiId, pub kind: FieldKind, pub label: String,
+                            pub description: Option<String>, pub default: serde_json::Value }
+ pub enum NotifyLevel { Critical, Warning, Info }
```

Validators (pure, `Result<(), Refusal>`; first failure wins, ordered by
widget/field index; messages name `widgets[<i>]` / `fields[<i>]` and
append ` (<id>)` when an id parsed):

| Function | Checks (in order) | Refusal `reason` |
|---|---|---|
| `validate_panel(title, &[WidgetSpec])` | title ≤ 64; count ≤ 100; each: id parses & unique; label non-empty & ≤ 256; slider/knob `min < max`, `step > 0`, `value ∈ [min,max]`; list ≤ 256 items, item ids unique, `selected` ∈ items; text ≤ 1,024 | `panel_widget_limit`, `invalid_widget_id`, `unlabeled_widget`, `invalid_value` |
| `validate_update(spec, &WidgetValue)` | kind accepts value; slider/knob range; list item known / items ≤ 256; text ≤ 1,024; meter clamped (never refused); label/button/marker list refused | `not_updatable`, `invalid_value`, `not_found` (list item) |
| `validate_primitives(&[OverlayPrimitive], glyph_keys)` | id parses; `from < to`; positions finite ≥ 0 (u64 already); label ≤ 64; glyph ref resolvable | `invalid_value` |
| `validate_action(&ActionSpec)` | id parses; label non-empty; binding string is *not* validated here (parsed later, unparseable ⇒ unbound + warning) | `invalid_value` |
| `validate_schema(&[SettingsField])` | count ≤ 100; ids parse & unique; label non-empty; number `step > 0`, `min ≤ default ≤ max`; choice 1–64 options, `default` ∈ options; string default ≤ 1,024; default type matches kind | `settings_field_limit`, `invalid_schema` |
| `validate_notify(level_str, text)` | level ∈ set; text ≤ 200 | `invalid_value` |
| `validate_stored(field, &Value) -> bool` | stored value valid for kind/range (for `get_settings` default substitution) | — |

### 1.4 `src/request.rs` (~)

```rust
~ pub enum Request {
+     RegisterPanel { panel: UiId, title: String, widgets: Vec<WidgetSpec> },
+     UpdateWidget { panel: UiId, widget: UiId, value: WidgetValue },
+     AddOverlays { primitives: Vec<OverlayPrimitive> },
+     RemoveOverlays { ids: Vec<UiId> },
+     ClearOverlays,
+     RegisterAction { action: ActionSpec },
+     RegisterSettings { fields: Vec<SettingsField>, stored: BTreeMap<String, serde_json::Value> }, // stored = runtime's Settings scope snapshot (R5)
+     GetSettings,                                   // local only; never sent as RPC
+     Notify { level: NotifyLevel, text: String },
~     RequestFocus { interaction: bool },            // R16 (was unit variant)
  }
~ pub enum Response { … , + Settings(BTreeMap<String, serde_json::Value>) }
```

### 1.5 `src/event.rs` (~)

```rust
+ pub enum ActionSource { Keyboard, Ui }             // "midi" reserved (later slice)
~ pub enum HostEvent {
+     PanelInteraction { panel: String, widget: String, value: WidgetValue },
+     ActionInvoked { action: String, source: ActionSource, value: Option<f64> },
+     SettingsChanged { changes: BTreeMap<String, serde_json::Value> },
  }
```

### 1.6 `src/manifest.rs` (~)

```rust
~ pub struct Manifest {
+     pub icon: Option<String>,                           // package-relative PNG path
+     pub glyphs: BTreeMap<String, String>,               // key (GLYPH_KEY_GRAMMAR) → path; ≤ 32
+     pub default_locale: String,                         // default "en-US"
+     pub strings: BTreeMap<String, BTreeMap<String, String>>, // locale → key → text
  }
```

Rule 3 additions: a `glyphs` key failing the grammar or > 32 entries is
`MalformedField { field: "glyphs" }` (a manifest error — the *map* is
schema, the *files* are assets). Asset presence/format/size is **not**
checked here (FR-014a: never a manifest error).

### 1.7 `src/state/store.rs` (~)

```rust
~ pub enum Scope { Plugin, Track, + Settings }          // Settings: host-written, never exposed to Lua
~ PluginStateStore { + settings: BTreeMap<String, Value>, + settings_dirty: bool }
+ pub fn load_settings(&mut self, bytes) / encode(Scope::Settings)
+ pub fn settings_snapshot(&self) -> BTreeMap<String, Value>
+ pub fn used_bytes(&self) -> usize                     // feeds PluginGauges::storage_used
```

`PluginStatePaths` (+) `settings_file(identifier) → <root>/<hex>/settings.json`.
`WriteJob` gains the settings scope target. Sign-out never touches it
(device-scoped like `plugin.json`).

### 1.8 `src/limiter.rs` (~)

```rust
~ pub enum RateCategory { Transport, Markers, Effects, State, Timers, + Ui, + Notify }
~ impl RateCategory { const fn limit(self) -> usize; const fn window(self) -> Duration }  // Notify: 6 / 60 s
~ pub struct RateLimiter { windows: [VecDeque<Instant>; 7] }
```

### 1.9 `src/budgets.rs` — unchanged (`storage` 10 MB already covers `Scope::Settings`).

## 2. `modplayer-plugin-runtime` (~)

```rust
~ pub enum Control {
+     SettingsWrite { changes: BTreeMap<String, serde_json::Value> },  // R5: store.set(Settings, …) then dispatch settings_changed
  }
~ pub struct Shared {
+     pub settings_schema: Option<Vec<SettingsField>>,   // set after a successful RegisterSettings RPC
+     pub in_interaction_handler: bool,                  // R16
  }
~ PluginGauges { + storage_used: AtomicUsize }           // updated by the store on set/remove/load
```

`bindings/ui.rs` (+): installs `api.ui.*`; every method except
`get_settings` builds a `Request` and calls `call(..)`; `get_settings`
is served locally (schema + `Scope::Settings` + `validate_stored`).
Lua table shapes → [contracts/plugin-api-v1.2.md](contracts/plugin-api-v1.2.md) §3.

## 3. `modplayer-core` — actions (~)

### 3.1 `actions/mod.rs`

```rust
+ #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
+ pub struct PluginActionId { pub plugin: PluginIdentifier, pub name: String }   // id(): "<identifier>.<name>"; parse(): rsplit_once('.')
+ pub enum ActionId { Host(HostAction), Plugin(PluginActionId) }                  // From<HostAction>
+ pub enum OwnerTier { Host, Bundled, Community }                                 // Ord: Host > Bundled > Community
~ pub enum ActionOwner { Host, + Plugin { id: PluginId, tier: OwnerTier } }
+ pub enum ActionSource { Keyboard, Ui }                                          // re-export of the gateway type
+ pub struct PluginActionDef { pub id: PluginActionId, pub owner: PluginId, pub tier: OwnerTier,
+     pub label: String, pub kind: ActionKind, pub repeats_while_held: bool,
+     pub default_binding: Option<Chord> }                                        // None = ships unbound (rejected/unparseable default → None + console warning)
```

Scope of every plugin action: `Scope::App` (FR-010).

### 3.2 `actions/registry.rs` (~) — [contracts/action-registry-plugins.md](contracts/action-registry-plugins.md)

```rust
~ pub struct ActionRegistry {
      overrides: KeymapOverrides,
      enabled: [bool; 46],
+     plugin_defs: BTreeMap<PluginActionId, PluginActionDef>,
+     plugin_enabled: BTreeMap<PluginActionId, bool>,       // FR-013 (owning plugin Active?)
+     plugin_effective: BTreeMap<PluginActionId, Vec<Chord>>,
~     index: HashMap<Chord, Vec<ActionId>>,                 // host entries first (catalog order), then plugin ids sorted
~     conflicts: BTreeSet<(ActionId, Chord)>,
  }
+ pub fn register_plugin_action(&mut self, def: PluginActionDef) -> Result<(), ActionLimit>   // replaces label/kind/flag/default; keeps user override; 65th → Err
+ pub fn unregister_plugin_actions(&mut self, owner: PluginId)                                // overrides → dormant
+ pub fn set_plugin_enabled(&mut self, owner: PluginId, enabled: bool)
+ pub fn is_invocable(&self, id: &ActionId) -> bool                                           // enabled ∧ no flagged chord
~ pub fn resolve(&self, chord, state) -> Option<ActionId>
~ pub fn rows(&self) -> impl Iterator<Item = ActionRow>   // host rows in catalog order, then one group per plugin (name-sorted)
~ pub struct ActionRow<'a> { pub id: ActionId, pub label: RowLabel /* Fluent key | resolved plugin string */,
      pub category: RowCategory /* Host(ActionCategory) | Plugin { name } */, pub kind, pub owner_tier,
      pub enabled: bool, pub bindings: &'a [Chord], pub conflicts: Vec<Chord> }
```

Conflict rule (`rebuild`, FR-011): for each chord with ≥ 2 enabled
candidates whose scopes can coexist, let `top` = highest tier present.
If exactly one candidate holds `top`, flag every *other* candidate; if
≥ 2 hold `top`, flag every candidate. `resolve` fires only unflagged
candidates; ties among unflagged go host-catalog-order then plugin id.

### 3.3 `actions/keymap.rs` (~)

```rust
~ pub struct KeymapOverrides { host: BTreeMap<HostAction, Vec<Chord>>, plugin: BTreeMap<PluginActionId, Vec<Chord>>,
+     dormant: BTreeMap<String, Vec<Chord>> }              // non-`host.` ids not currently registered (FR-010a); persisted verbatim
+ pub fn adopt_dormant(&mut self, id: &PluginActionId) -> Option<Vec<Chord>>
+ pub fn park(&mut self, id: PluginActionId)               // registered → dormant on unregister
```

`settings/model.rs::into_settings` (~): an id that is not `host.`-namespaced
and decodes to a valid chord list goes to `dormant` instead of
`dropped_keybindings`; only `host.*` unknowns and unparseable values
raise `keybindings-invalid-entries`.

## 4. `modplayer-core` — `plugins/ui/` (+) — Key Entities "Panel", "Overlay Primitive", "Settings Schema", "Plugin Notification"

### 4.1 `plugins/ui/mod.rs`

```rust
+ pub struct PluginUi {                          // owned by PluginHost
+     panels: PanelRegistry, overlays: OverlayRegistry, settings: SettingsRegistry,
+     notify: (), /* window lives in the gateway limiter */
+ }
+ impl PluginUi {
+     pub fn on_ready(&mut self, id: PluginId)                       // clear placeholder panels (R15)
+     pub fn on_stop(&mut self, id: PluginId, reason: UnloadReason)  // Suspend: keep panels, clear overlays; else remove all
+     pub fn on_track_changed(&mut self)                             // clear every plugin's overlays (FR-016)
+ }
```

### 4.2 `plugins/ui/panel.rs`

```rust
+ pub struct Panel { pub id: UiId, pub title: String, pub widgets: Vec<WidgetState>, pub seq: u64 /* per-plugin registration order */ }
+ pub struct WidgetState { pub spec: WidgetSpec, pub value: WidgetValue }
+ pub struct PanelRegistry {
+     panels: BTreeMap<PluginId, Vec<Panel>>,          // ≤ 16 per plugin, insertion order = registration order
+     closed: BTreeSet<PanelKey>,                      // session-only "close" (FR-006)
+     next_seq: u64,
+ }
+ pub struct PanelKey { pub plugin: PluginIdentifier, pub panel: UiId }   // settings.toml key "<identifier>/<panel>"
+ pub fn register(&mut self, plugin, title, widgets) -> Result<(), Refusal>   // panel_limit; atomic replace
+ pub fn update(&mut self, plugin, panel, widget, value) -> Result<(), Refusal> // not_found / not_updatable / invalid_value; meter clamped
+ pub fn interact(&mut self, plugin, panel, widget, value) -> Option<WidgetValue>  // UI-driven; returns value to deliver
+ pub fn close / show / is_closed
```

Initial values: toggle `false`; slider/knob `value.unwrap_or(min)`;
list `selected.unwrap_or(first)`; text `text.unwrap_or("")`; meter
`0.0`.

### 4.3 `plugins/ui/overlay.rs`

```rust
+ pub struct OverlayRegistry { sets: BTreeMap<PluginId, OverlaySet> }
+ pub struct OverlaySet { primitives: BTreeMap<UiId, OverlayPrimitive>, seq: u64 }
+ pub fn add(&mut self, plugin, Vec<OverlayPrimitive>) -> Result<(), Refusal>  // overlay_limit on post-merge count; whole call
+ pub fn remove(&mut self, plugin, &[UiId]) -> Result<(), Refusal>              // not_found (whole call, nothing removed)
+ pub fn clear(&mut self, plugin)
+ pub fn view(&self, records) -> Vec<OverlayLayer { plugin_seq, plugin: PluginId, primitives: Vec<OverlayPrimitive>, glyph_assets }>  // Active plugins only, seq order
```

### 4.4 `plugins/ui/settings.rs`

```rust
+ pub struct SettingsRegistry { pages: BTreeMap<PluginId, SettingsPage> }
+ pub struct SettingsPage { pub fields: Vec<SettingsField>, pub values: BTreeMap<String, serde_json::Value> /* current, defaults substituted */ }
+ pub fn register(&mut self, plugin, fields, stored) -> Result<(), Refusal>   // settings_field_limit / invalid_schema; replaces page; values for kept fields retained
+ pub fn edit(&mut self, plugin, field, value) -> Option<BTreeMap<String, Value>>  // validates against kind; returns `changes` for Control::SettingsWrite
```

### 4.5 `plugins/ui/strings.rs` — `resolve(manifest, locale, &str) -> (String, Option<Warning>)` (R17).

### 4.6 `plugins/ui/assets.rs`

```rust
+ pub struct PluginAssets { pub icon: Option<DecodedPng>, pub glyphs: BTreeMap<String, DecodedPng> }
+ pub struct DecodedPng { pub width: u32, pub height: u32, pub rgba: Arc<[u8]> }
+ pub fn load(package: &BundledPackage, manifest: &Manifest, log: &mut PluginLog) -> PluginAssets   // every failure → warning + omitted
```

`PluginRecord` (~) `+ pub assets: PluginAssets`.

### 4.7 Views (`plugins/view.rs` ~)

```rust
+ pub struct PluginPanelsView { pub docked: Vec<PanelView>, pub floated: Vec<PanelView> }   // docked sorted by (plugin name, seq)
+ pub struct PanelView { pub key: PanelKey, pub plugin: PluginId, pub plugin_name: String, pub has_icon: bool,
+     pub title: String, pub placement: PanelPlacement, pub body: PanelBody }
+ pub enum PanelBody { Live(Vec<WidgetState>), Placeholder { cause: SuspendCause } }
+ pub struct PluginSettingsView { pub plugin: PluginId, pub name: String, pub page: SettingsPage }
+ pub struct MarkerListRow { pub id: MarkerId, pub name: String, pub position_ms: u64, pub color: PaletteIndex }  // for `marker list` widgets
~ PluginRow { + pub panels: Vec<PanelRowControl { key, title, closed: bool, disabled: bool }> }
```

Visibility (view filter): a panel appears iff the record is
`Active` (Live) or `Suspended` (Placeholder), not `closed`, not
`disabled` in `[plugin_panels]`.

## 5. `modplayer-core` — settings, notifications, focus, controller (~)

### 5.1 `settings/model.rs`

```rust
+ pub enum PanelPlacement { #[default] Docked, Floated }
+ pub struct PanelPersisted { pub placement: PanelPlacement, pub x: Option<f32>, pub y: Option<f32>,
+                             pub w: Option<f32>, pub h: Option<f32>, pub disabled: bool }
~ AudioSettings { + pub plugin_panels: BTreeMap<String, PanelPersisted> }     // key "<identifier>/<panel-id>"
~ RawSettings { + plugin_panels: BTreeMap<String, RawPanel> }                 // [plugin_panels."org.x.y/main"] placement = "floated" …
~ InvalidField { + PluginPanel(String) }                                      // bad placement string → entry dropped, warned
```

### 5.2 `notifications.rs`

```rust
+ pub struct PluginAttribution { pub id: PluginId, pub name: String }
~ Notification { + pub attribution: Option<PluginAttribution> }
+ pub fn raise_attributed(&mut self, severity, message_key: &'static str, args, attribution) -> u64
```

Fluent: `plugin-notification = { $plugin }: { $text }` (`plugins.ftl`).

### 5.3 `plugins/focus.rs`

```rust
+ pub enum RequestOrigin { Api, UserInteraction }
~ pub fn request(&mut self, p: PluginId, running: bool, origin: RequestOrigin) -> Vec<FocusChange>
```

Under `AutoOnInteraction` + `UserInteraction`: grant now (revoke current
plugin holder via the existing hand-over). All other combinations:
unchanged 010 behaviour.

### 5.4 Controller façade (+)

```rust
+ pub fn plugin_panels_view(&self) -> PluginPanelsView
+ pub fn plugin_overlays(&self) -> Vec<OverlayLayer>
+ pub fn plugin_settings_views(&self) -> Vec<PluginSettingsView>
+ pub fn plugin_marker_list(&self, plugin: PluginId) -> Vec<MarkerListRow>
+ pub fn plugin_panel_interaction(&mut self, key: &PanelKey, widget: &UiId, value: WidgetValue)   // → registry + HostEvent::PanelInteraction to handle
+ pub fn invoke_plugin_action(&mut self, id: &PluginActionId, source: ActionSource)             // gate → HostEvent::ActionInvoked
+ pub fn plugin_settings_edit(&mut self, plugin: PluginId, field: &str, value: Value)            // → Control::SettingsWrite
+ pub fn plugin_panel_close / show / set_disabled / set_placement(key, PanelPersisted)            // settings.toml write for persisted fields
+ pub fn plugin_assets(&self, id: PluginId) -> &PluginAssets
```

`apply.rs::dispatch` (~) gains one arm per new `RequestKind`;
`RequestFocus { interaction }` passes the origin to the arbiter.
`tick()` order unchanged; `on_track_changed` also calls
`ui.on_track_changed()`.

## 6. `modplayer-ui` (~/+)

| Module | Role |
|---|---|
| `plugin_panels.rs` (+) | dock `SidePanel` + floated `Window`s; per-kind widget renderers; a11y `WidgetInfo`; focus claims; interaction → controller; placement changes → `set_placement` |
| `widgets/knob.rs` (+) | rotary painter over a slider `Response` |
| `plugin_overlays.rs` (+) | `paint(painter, space, layers, view_kind, glyph textures)`; plugin lane |
| `plugin_assets.rs` (+) | `TextureHandle` cache keyed by `(PluginId, asset)` in `Context` memory; generic glyph |
| `theme.rs` (~) | `overlay_color(OverlayColor, &Visuals) -> Color32`; `HOST_GLYPHS` painters |
| `settings/plugins.rs` (+) | Plugins category screen (replaces the placeholder arm in `settings/mod.rs`): per-plugin sub-pages; field renderers |
| `settings/controls.rs` (~) | rows via `ActionRow` (plugin groups, tier-aware conflict text) |
| `settings/mod.rs` (~) | search merges `search_plugin_settings` hits |
| `plugins_view.rs` (~) | per-panel Show/Hide + Enable/Disable controls |
| `notifications.rs` (~) | attribution icon + name |
| `actions.rs` (~) | `Invocation.action: ActionId`; plugin arm in `invoke` |
| `now_playing.rs` (~) | dock + windows; overlay paint hook in both waveforms |

## 7. Fixtures (R18)

| Identifier | Grants | Behaviour |
|---|---|---|
| `org.modplayer.fixture.ui-panel` | `ui.panel`, `ui.shortcuts`, `ui.notify`, `transport.control`, `markers.write` | panel "Controls" with every widget kind; "Take over" button → `request_focus()` in handler; action `focus_me` (`Shift+K`, collides with `ui-shortcuts.nudge` for M6); buttons "Register bad" / "Hang" / "Notify ×7" driving probes; probes for every refusal |
| `org.modplayer.fixture.ui-shortcuts` | `ui.shortcuts`, `ui.panel` | actions `take_over` (`L`), `nudge` (continuous, `Shift+K`), `tab_bound` (`Tab`); button targeting `take_over` |
| `org.modplayer.fixture.ui-overlay` | `ui.overlay`, `playback.observe` | one primitive of each kind on every `track_changed`; probes `add_501st`, `bad_region`, `bad_icon` |
| `org.modplayer.fixture.ui-settings` | `ui.settings` | 4 fields; logs `settings_changed`; probe `get` |
| `org.modplayer.fixture.ui-notify` | `ui.notify` | probe `post {level, text, n}` |
| `org.modplayer.fixture.ui-icons` | `ui.overlay` | manifest `icon` + glyphs `ok` (valid) and `big` (over-limit) |

## 8. State transitions

```text
Panel:        (none) --register_panel--> Registered(visible) --close--> Closed(session) --show/relaunch--> Registered
              Registered --disable(persisted)--> Disabled --enable--> Registered
              Registered --plugin Suspend--> Placeholder --Ready--> (none, until re-registered) ; --Disable/Unload--> (none)
Overlays:     Set --track change | stop(any)--> ∅
Plugin action:Registered(enabled) --plugin not Active--> Registered(disabled) --Active--> Registered(enabled) ; --Disable/Unload--> (none, override → dormant)
Settings:     Page --plugin not Active--> hidden (values intact) ; --re-register--> Page' (kept-field values retained)
Notify window:per plugin, rolling 60 s; admitted calls append; refused (permission/rate) never append
```

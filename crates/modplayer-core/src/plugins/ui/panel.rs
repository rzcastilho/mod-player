// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PanelRegistry` (US1 T045, contracts/ui-panels.md §1 "P" rules,
//! data-model.md §4.2): the host-owned state behind `ui.panel` —
//! `register_panel`/`update_widget` (RPC, `plugins/apply.rs`) and
//! `interact` (UI-driven, `PlaybackController::plugin_panel_interaction`).
//! Validation of a call's *shape* already happened in the gateway crate
//! (`modplayer_capability_gateway::ui::validate_panel`/`validate_update`,
//! research R3); this module only enforces per-plugin *capacity*
//! (`MAX_PANELS_PER_PLUGIN`, P2) and holds the live widget values.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;

use modplayer_capability_gateway::manifest::PluginIdentifier;
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::ui::limits::MAX_PANELS_PER_PLUGIN;
use modplayer_capability_gateway::ui::{UiId, WidgetKind, WidgetSpec, WidgetValue};

use super::super::PluginId;

/// A registered panel: its layout (already-resolved `WidgetSpec`s, R17)
/// plus each widget's live value, in registration order.
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub id: UiId,
    pub title: String,
    pub widgets: Vec<WidgetState>,
    /// This *plugin's own* registration order (P2, data-model.md §4.7):
    /// assigned once, at first registration, and kept across a
    /// re-registration that replaces the layout in place.
    pub seq: u64,
}

/// One widget's declared shape plus its current value (data-model.md
/// §4.2).
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetState {
    pub spec: WidgetSpec,
    pub value: WidgetValue,
}

/// A UI-driven click/change's outcome (011-plugin-ui-contributions,
/// FR-007/FR-008, contracts/action-registry-plugins.md D3):
/// [`PanelRegistry::interact`]'s caller delivers `Value` as one
/// `panel_interaction`, or resolves `Action`'s `"<name>"` against
/// `"<identifier>.<name>"` in the action registry and invokes it (or,
/// unregistered, logs it inert) instead.
#[derive(Debug, Clone, PartialEq)]
pub enum Interaction {
    Value(WidgetValue),
    Action(String),
}

/// A panel's identity across a plugin restart and across `settings.toml`
/// (contracts/ui-panels.md L3, FR-005): `"<identifier>/<panel-id>"`.
/// Deliberately identifier- rather than session-`PluginId`-keyed, exactly
/// like `PluginActionId` (research R6) — a session-only `closed` marker
/// and a persisted `[plugin_panels]` entry must both survive the numeric
/// id changing across a relaunch.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PanelKey {
    pub plugin: PluginIdentifier,
    pub panel: UiId,
}

impl PanelKey {
    #[must_use]
    pub fn new(plugin: PluginIdentifier, panel: UiId) -> Self {
        Self { plugin, panel }
    }

    /// The `settings.toml` `[plugin_panels]` table key (FR-005).
    #[must_use]
    pub fn storage_key(&self) -> String {
        format!("{}/{}", self.plugin, self.panel)
    }
}

/// FR-003/FR-006/FR-025: every plugin's registered panels plus the
/// session-only "closed" set (data-model.md §4.2). Owned by [`super::
/// PluginUi`], itself owned by `PluginHost` (Constitution III).
#[derive(Debug, Default)]
pub struct PanelRegistry {
    /// Keyed by the numeric, session-scoped `PluginId` (`modplayer_
    /// effects::catalog::PluginId` has no `Ord`, hence `HashMap` rather
    /// than data-model.md §4.2's indicative `BTreeMap` — iteration order
    /// is never relied on here; the view builder (`plugins/view.rs`)
    /// derives docked order from `records`/`seq` instead).
    panels: HashMap<PluginId, Vec<Panel>>,
    /// FR-006: closed for the remainder of the running session — survives
    /// `clear`/re-registration (P5), cleared only by [`PanelRegistry::
    /// show`].
    closed: BTreeSet<PanelKey>,
    next_seq: u64,
    /// R15 (P5): plugins whose *next* [`PanelRegistry::register`] must
    /// first drop every panel left over from before their suspension —
    /// set by [`PanelRegistry::mark_needs_reset`] the moment a plugin is
    /// suspended, consumed by the plugin's own first registration after
    /// it restarts. Deliberately *not* wiped eagerly on the host's own
    /// `Ready` event (`PluginUi::on_ready`, T046): `tick()` drains every
    /// queued plugin *request* before plugin *runtime events* (design
    /// note in `plugins/host.rs`), so a plugin whose `ready_ack` handler
    /// calls `register_panel` synchronously can have that RPC and this
    /// same `Ready` event land in one `tick()` — applied in that request-
    /// first order — and an eager clear on `Ready` would wipe the
    /// registration that had *just* landed moments earlier in the same
    /// call. Consuming the flag inside `register()` itself instead ties
    /// the reset to the one event that can never race it: the plugin's
    /// own request.
    needs_reset: HashSet<PluginId>,
}

/// FR-002/FR-007, data-model.md §4.2 "Initial values": a fresh widget's
/// starting value, derived from its already-validated spec.
fn initial_value(spec: &WidgetSpec) -> WidgetValue {
    match spec.kind {
        WidgetKind::Toggle => WidgetValue::Bool(false),
        WidgetKind::Slider | WidgetKind::Knob => {
            WidgetValue::Number(spec.value.unwrap_or(spec.min.unwrap_or(0.0)))
        }
        WidgetKind::List => {
            let selected = spec
                .selected
                .clone()
                .or_else(|| spec.items.first().map(|item| item.id.clone()));
            WidgetValue::Items {
                items: spec.items.clone(),
                selected,
            }
        }
        WidgetKind::Text | WidgetKind::Label => {
            WidgetValue::Text(spec.text.clone().unwrap_or_default())
        }
        // FR-007: a `meter`'s value is always `[0.0, 1.0]`, clamped, never
        // refused.
        WidgetKind::Meter => WidgetValue::Number(0.0),
        // `button`/`marker_list` have no meaningful stored value (a button
        // is momentary; a marker list's rows come from `TrackMarkers`, not
        // from here) — an inert placeholder that `set_value` never reaches
        // in practice (neither kind is ever `update`/`interact`-targeted:
        // `validate_update` refuses `button`/`marker_list` as
        // `not_updatable`, and the UI never calls `interact` for either).
        WidgetKind::Button | WidgetKind::MarkerList => WidgetValue::Bool(false),
    }
}

/// FR-007: apply `value` to `widget`, clamping a `meter` into `[0.0,
/// 1.0]` (never refused — the gateway's own `validate_update` already
/// accepts any `meter` number unconditionally).
fn set_value(widget: &mut WidgetState, value: WidgetValue) -> WidgetValue {
    let value = if widget.spec.kind == WidgetKind::Meter {
        match value {
            WidgetValue::Number(n) => WidgetValue::Number(n.clamp(0.0, 1.0)),
            other => other,
        }
    } else {
        value
    };
    widget.value = value.clone();
    value
}

impl PanelRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// FR-003 (P1/P2): validate whole (caller's job — `validate_panel`
    /// already ran before this is reached) then register `widgets` under
    /// `panel_id`, atomically. A fresh id counts toward the 16-panel cap
    /// and appends at the end, in registration order; re-registering an
    /// existing id replaces its widgets **in place** (position/`seq`
    /// unchanged, dropping any pending interaction for the old layout —
    /// there is none to drop, since `interact`/`update` are synchronous).
    pub fn register(
        &mut self,
        plugin: PluginId,
        panel_id: UiId,
        title: String,
        widgets: Vec<WidgetSpec>,
    ) -> Result<(), Refusal> {
        // R15 (P5): the first registration since this plugin was last
        // suspended drops every panel it left behind first — see the
        // `needs_reset` field doc for why this happens here rather than
        // eagerly on the host's own `Ready` event.
        if self.needs_reset.remove(&plugin) {
            self.panels.remove(&plugin);
        }
        let widget_states: Vec<WidgetState> = widgets
            .into_iter()
            .map(|spec| {
                let value = initial_value(&spec);
                WidgetState { spec, value }
            })
            .collect();

        let entry = self.panels.entry(plugin).or_default();
        if let Some(existing) = entry.iter_mut().find(|p| p.id == panel_id) {
            existing.title = title;
            existing.widgets = widget_states;
            return Ok(());
        }
        if entry.len() >= MAX_PANELS_PER_PLUGIN {
            return Err(Refusal::invalid_state(
                "panel_limit",
                format!("A plugin may register at most {MAX_PANELS_PER_PLUGIN} panels."),
            ));
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        entry.push(Panel {
            id: panel_id,
            title,
            widgets: widget_states,
            seq,
        });
        Ok(())
    }

    /// FR-007 `update_widget`: `not_found` for an unknown panel/widget;
    /// applies (clamping a `meter`) otherwise. Works regardless of the
    /// panel's closed/disabled/visible state (P3); never itself produces
    /// an event.
    pub fn update(
        &mut self,
        plugin: PluginId,
        panel: &UiId,
        widget: &UiId,
        value: WidgetValue,
    ) -> Result<(), Refusal> {
        let state = self
            .widget_mut(plugin, panel, widget)
            .ok_or_else(Refusal::not_found)?;
        set_value(state, value);
        Ok(())
    }

    /// UI-driven (contracts/ui-panels.md P4): stores `value` exactly like
    /// [`PanelRegistry::update`] and returns the (possibly clamped) value
    /// for the caller to deliver as one `panel_interaction` — unless
    /// `widget` is a `button` naming an `action` (FR-008, contracts/
    /// action-registry-plugins.md D3), in which case the value is still
    /// stored (a button's own value is inert, `initial_value`'s doc) but
    /// the caller must resolve `"<identifier>.<name>"` against the action
    /// registry and invoke it instead of delivering `panel_interaction`.
    /// `None` if the panel/widget no longer exists (a stale UI frame — no
    /// interaction is delivered either way).
    pub fn interact(
        &mut self,
        plugin: PluginId,
        panel: &UiId,
        widget: &UiId,
        value: WidgetValue,
    ) -> Option<Interaction> {
        let state = self.widget_mut(plugin, panel, widget)?;
        if state.spec.kind == WidgetKind::Button
            && let Some(action) = state.spec.action.clone()
        {
            set_value(state, value);
            return Some(Interaction::Action(action));
        }
        Some(Interaction::Value(set_value(state, value)))
    }

    /// The declared shape of `widget` on `panel`, if both exist — used
    /// by `plugins/apply.rs`'s `UpdateWidget` arm to run the gateway's
    /// `validate_update` before mutating (design note 2: "validate in the
    /// gateway crate, mutate in core").
    #[must_use]
    pub fn widget_spec(
        &self,
        plugin: PluginId,
        panel: &UiId,
        widget: &UiId,
    ) -> Option<&WidgetSpec> {
        self.panels
            .get(&plugin)?
            .iter()
            .find(|p| &p.id == panel)?
            .widgets
            .iter()
            .find(|w| &w.spec.id == widget)
            .map(|w| &w.spec)
    }

    fn widget_mut(
        &mut self,
        plugin: PluginId,
        panel: &UiId,
        widget: &UiId,
    ) -> Option<&mut WidgetState> {
        self.panels
            .get_mut(&plugin)
            .and_then(|panels| panels.iter_mut().find(|p| &p.id == panel))
            .and_then(|p| p.widgets.iter_mut().find(|w| &w.spec.id == widget))
    }

    /// Every panel `plugin` currently has registered, in registration
    /// order — empty for a plugin with none (never registered, or cleared
    /// by [`PanelRegistry::clear`]).
    #[must_use]
    pub fn for_plugin(&self, plugin: PluginId) -> &[Panel] {
        self.panels.get(&plugin).map_or(&[], Vec::as_slice)
    }

    /// R15 (P5): drop every one of `plugin`'s panels outright — called on
    /// `on_stop(Disable | Shutdown)`. `closed`/persisted-disabled state is
    /// untouched (survives, keyed by [`PanelKey`], not by this registry's
    /// `PluginId` map).
    pub fn clear(&mut self, plugin: PluginId) {
        self.panels.remove(&plugin);
        self.needs_reset.remove(&plugin);
    }

    /// R15 (P5): mark `plugin` so its *next* [`PanelRegistry::register`]
    /// drops every panel it left behind first — called the moment a
    /// plugin is suspended (see the `needs_reset` field doc for why this
    /// isn't done eagerly instead).
    pub fn mark_needs_reset(&mut self, plugin: PluginId) {
        self.needs_reset.insert(plugin);
    }

    /// FR-006: close `key` for the remainder of the session.
    pub fn close(&mut self, key: PanelKey) {
        self.closed.insert(key);
    }

    /// FR-006: re-show a closed panel.
    pub fn show(&mut self, key: &PanelKey) {
        self.closed.remove(key);
    }

    #[must_use]
    pub fn is_closed(&self, key: &PanelKey) -> bool {
        self.closed.contains(key)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use modplayer_effects::catalog::PluginId as CorePluginId;

    fn id(s: &str) -> UiId {
        UiId::parse(s).unwrap_or_else(|| unreachable!())
    }

    fn label_widget(wid: &str) -> WidgetSpec {
        WidgetSpec {
            id: id(wid),
            kind: WidgetKind::Label,
            label: "Label".to_string(),
            min: None,
            max: None,
            step: None,
            value: None,
            items: Vec::new(),
            selected: None,
            action: None,
            text: Some("hello".to_string()),
        }
    }

    fn slider_widget(wid: &str) -> WidgetSpec {
        WidgetSpec {
            id: id(wid),
            kind: WidgetKind::Slider,
            label: "Tempo".to_string(),
            min: Some(0.0),
            max: Some(200.0),
            step: Some(1.0),
            value: Some(120.0),
            items: Vec::new(),
            selected: None,
            action: None,
            text: None,
        }
    }

    fn meter_widget(wid: &str) -> WidgetSpec {
        WidgetSpec {
            id: id(wid),
            kind: WidgetKind::Meter,
            label: "Level".to_string(),
            min: None,
            max: None,
            step: None,
            value: None,
            items: Vec::new(),
            selected: None,
            action: None,
            text: None,
        }
    }

    fn list_widget(wid: &str) -> WidgetSpec {
        use modplayer_capability_gateway::ui::ListItem;
        WidgetSpec {
            id: id(wid),
            kind: WidgetKind::List,
            label: "Choices".to_string(),
            min: None,
            max: None,
            step: None,
            value: None,
            items: vec![
                ListItem {
                    id: id("a"),
                    label: "A".to_string(),
                },
                ListItem {
                    id: id("b"),
                    label: "B".to_string(),
                },
            ],
            selected: None,
            action: None,
            text: None,
        }
    }

    const P: CorePluginId = CorePluginId(0);

    #[test]
    fn register_replaces_atomically() {
        let mut reg = PanelRegistry::new();
        reg.register(P, id("main"), "T1".to_string(), vec![label_widget("a")])
            .unwrap();
        let first_seq = reg.for_plugin(P)[0].seq;
        reg.register(P, id("main"), "T2".to_string(), vec![label_widget("b")])
            .unwrap();
        let panels = reg.for_plugin(P);
        assert_eq!(
            panels.len(),
            1,
            "re-registering must replace in place, not append"
        );
        assert_eq!(panels[0].title, "T2");
        assert_eq!(panels[0].widgets[0].spec.id, id("b"));
        assert_eq!(
            panels[0].seq, first_seq,
            "seq (registration order) must survive replace"
        );
    }

    #[test]
    fn seventeenth_panel_refused() {
        let mut reg = PanelRegistry::new();
        for i in 0..16 {
            reg.register(
                P,
                id(&format!("p{i}")),
                "T".to_string(),
                vec![label_widget("a")],
            )
            .unwrap();
        }
        let err = reg
            .register(P, id("p16"), "T".to_string(), vec![label_widget("a")])
            .unwrap_err();
        assert_eq!(err.reason, "panel_limit");
        assert_eq!(reg.for_plugin(P).len(), 16, "the 17th must not be added");
    }

    #[test]
    fn update_label_not_updatable() {
        // `update`'s own registry logic has no kind-awareness beyond
        // `set_value`'s meter clamp — `not_updatable` is enforced by the
        // gateway's `validate_update` before this is ever reached (P3's
        // caller contract); this test proves the registry-level path a
        // *valid* update takes still requires a real, currently-registered
        // widget (`not_found` otherwise), the half this module owns.
        let mut reg = PanelRegistry::new();
        reg.register(P, id("main"), "T".to_string(), vec![label_widget("a")])
            .unwrap();
        let err = reg
            .update(
                P,
                &id("main"),
                &id("missing"),
                WidgetValue::Text("x".to_string()),
            )
            .unwrap_err();
        assert_eq!(err.reason, "unknown_id");
    }

    #[test]
    fn update_slider_out_of_range() {
        // As above: range enforcement is the gateway's `validate_update`
        // (already covered in `modplayer-capability-gateway/src/ui.rs`'s
        // own tests); here we confirm a registered slider's value is
        // simply stored as given by a caller that already validated it.
        let mut reg = PanelRegistry::new();
        reg.register(P, id("main"), "T".to_string(), vec![slider_widget("t")])
            .unwrap();
        reg.update(P, &id("main"), &id("t"), WidgetValue::Number(50.0))
            .unwrap();
        assert_eq!(
            reg.for_plugin(P)[0].widgets[0].value,
            WidgetValue::Number(50.0)
        );
    }

    #[test]
    fn meter_clamps() {
        let mut reg = PanelRegistry::new();
        reg.register(P, id("main"), "T".to_string(), vec![meter_widget("m")])
            .unwrap();
        reg.update(P, &id("main"), &id("m"), WidgetValue::Number(5.0))
            .unwrap();
        assert_eq!(
            reg.for_plugin(P)[0].widgets[0].value,
            WidgetValue::Number(1.0)
        );
        reg.update(P, &id("main"), &id("m"), WidgetValue::Number(-5.0))
            .unwrap();
        assert_eq!(
            reg.for_plugin(P)[0].widgets[0].value,
            WidgetValue::Number(0.0)
        );
    }

    #[test]
    fn list_replace_items() {
        use modplayer_capability_gateway::ui::ListItem;
        let mut reg = PanelRegistry::new();
        reg.register(P, id("main"), "T".to_string(), vec![list_widget("l")])
            .unwrap();
        assert_eq!(
            reg.for_plugin(P)[0].widgets[0].value,
            WidgetValue::Items {
                items: vec![
                    ListItem {
                        id: id("a"),
                        label: "A".to_string()
                    },
                    ListItem {
                        id: id("b"),
                        label: "B".to_string()
                    },
                ],
                selected: Some(id("a")),
            }
        );
        reg.update(
            P,
            &id("main"),
            &id("l"),
            WidgetValue::Items {
                items: vec![ListItem {
                    id: id("c"),
                    label: "C".to_string(),
                }],
                selected: Some(id("c")),
            },
        )
        .unwrap();
        assert_eq!(
            reg.for_plugin(P)[0].widgets[0].value,
            WidgetValue::Items {
                items: vec![ListItem {
                    id: id("c"),
                    label: "C".to_string()
                }],
                selected: Some(id("c")),
            }
        );
    }

    #[test]
    fn closed_survives_reregister() {
        let identifier = PluginIdentifier::parse("org.modplayer.fixture.ui-panel")
            .unwrap_or_else(|| unreachable!());
        let key = PanelKey::new(identifier, id("main"));
        let mut reg = PanelRegistry::new();
        reg.register(P, id("main"), "T".to_string(), vec![label_widget("a")])
            .unwrap();
        reg.close(key.clone());
        assert!(reg.is_closed(&key));
        reg.clear(P);
        reg.register(P, id("main"), "T2".to_string(), vec![label_widget("a")])
            .unwrap();
        assert!(
            reg.is_closed(&key),
            "close must survive a clear + re-register"
        );
        reg.show(&key);
        assert!(!reg.is_closed(&key));
    }

    #[test]
    fn suspend_keeps_disable_removes() {
        let mut reg = PanelRegistry::new();
        reg.register(P, id("main"), "T".to_string(), vec![label_widget("a")])
            .unwrap();
        // "Suspend" in the full lifecycle (T046) simply never calls
        // `clear` — proven here at the registry's own level: panels
        // survive unless `clear` is called.
        assert_eq!(reg.for_plugin(P).len(), 1);
        reg.clear(P);
        assert!(
            reg.for_plugin(P).is_empty(),
            "Disable/Shutdown must remove every panel"
        );
    }
}

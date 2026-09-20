// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SettingsRegistry` (US4 T098, contracts/overlays-settings-notify.md §2
//! "S" rules, data-model.md §4.4): the host-owned state behind
//! `ui.settings` — `register_settings` (RPC, `plugins/apply.rs`) and
//! `edit` (UI-driven, `PlaybackController::plugin_settings_edit`).
//! Validation of a call's *shape* already happened in the gateway crate
//! (`modplayer_capability_gateway::ui::validate_schema`/`validate_stored`,
//! research R3); this module only holds each plugin's live field values
//! and reconciles them across a re-registration (S1) and a UI-driven edit
//! (S4).
//!
//! Unlike `PanelRegistry`/`OverlayRegistry`, nothing here ever clears a
//! plugin's page outright: data-model.md §8's "Settings" transition is
//! "Page --plugin not Active--> hidden (values intact)" — a plugin going
//! inactive (Suspended, Disabled, even Unloaded) never loses its stored
//! values, only its visibility in Settings › Plugins, which is the view
//! layer's job (`plugins/view.rs`'s `PluginSettingsView`, exactly mirroring
//! `PanelRegistry`/`OverlayRegistry`'s own registry-vs-view split). So
//! `PluginUi::on_stop`/`on_ready` have no half to call into here at all.

use std::collections::{BTreeMap, HashMap};

use modplayer_capability_gateway::ui::{SettingsField, validate_stored};

use super::super::PluginId;

/// One plugin's registered settings page: its declared fields (already
/// validated whole by the gateway's `validate_schema` before this is ever
/// reached) plus each field's current value (data-model.md §4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsPage {
    pub fields: Vec<SettingsField>,
    pub values: BTreeMap<String, serde_json::Value>,
}

/// FR-017/FR-018: every plugin's registered settings page, at most one per
/// plugin. Owned by [`super::PluginUi`], itself owned by `PluginHost`
/// (Constitution III).
#[derive(Debug, Default)]
pub struct SettingsRegistry {
    /// `HashMap`, not `BTreeMap` (mirrors `PanelRegistry::panels`'s own
    /// doc note): `modplayer_effects::catalog::PluginId` has no `Ord`.
    pages: HashMap<PluginId, SettingsPage>,
}

impl SettingsRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// S1: replace `plugin`'s page with `fields` (the caller already ran
    /// `validate_schema` over the whole batch's *shape*). A field that
    /// already had a value on this plugin's *current* page keeps it —
    /// re-registering (e.g. the plugin re-declaring an identical schema
    /// after a restart, or adding a field) must never reset a value the
    /// host has since applied via [`SettingsRegistry::edit`]. A field new
    /// to this plugin's page takes its value from `stored` (the runtime's
    /// own `Scope::Settings` snapshot at registration time, R5) when still
    /// valid for that field's kind/range, else the field's own `default`
    /// (S1's "default-substituted when invalid"). A field dropped from the
    /// new schema simply has no entry in the new page — its value in
    /// `settings.json` is untouched (S1: "retained ... in storage
    /// untouched"), just no longer reachable from this page.
    pub fn register(
        &mut self,
        plugin: PluginId,
        fields: Vec<SettingsField>,
        stored: BTreeMap<String, serde_json::Value>,
    ) {
        let previous_values = self
            .pages
            .get(&plugin)
            .map(|page| page.values.clone())
            .unwrap_or_default();
        let values = fields
            .iter()
            .map(|field| {
                let key = field.id.as_str().to_string();
                let value =
                    previous_values
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| match stored.get(&key) {
                            Some(v) if validate_stored(field, v) => v.clone(),
                            _ => field.default.clone(),
                        });
                (key, value)
            })
            .collect();
        self.pages.insert(plugin, SettingsPage { fields, values });
    }

    /// S4: apply a UI-driven edit. `None` if `plugin` has no page, `field`
    /// is not one of its declared field ids, or `value` fails
    /// `validate_stored` for that field's kind/range (a widget only ever
    /// offers an in-range value, so this is a defensive no-op, not a user-
    /// facing refusal path — S4 names none). `Some(changes)` (exactly
    /// `{field: value}`, the shape `Control::SettingsWrite` and
    /// `settings_changed` both want) otherwise; the page's own `values`
    /// copy is updated immediately so a read the same frame (`plugin_
    /// settings_views()`) already reflects it.
    pub fn edit(
        &mut self,
        plugin: PluginId,
        field: &str,
        value: serde_json::Value,
    ) -> Option<BTreeMap<String, serde_json::Value>> {
        let page = self.pages.get_mut(&plugin)?;
        let field_def = page.fields.iter().find(|f| f.id.as_str() == field)?;
        if !validate_stored(field_def, &value) {
            return None;
        }
        page.values.insert(field.to_string(), value.clone());
        let mut changes = BTreeMap::new();
        changes.insert(field.to_string(), value);
        Some(changes)
    }

    /// This plugin's current page, if it has ever registered one — used by
    /// `plugins/view.rs`'s `PluginSettingsView::from_records` (lifecycle-
    /// aware visibility lives there, exactly like `PanelRegistry`/
    /// `OverlayRegistry`'s own split: the registry itself never reads a
    /// `PluginRecord`).
    #[must_use]
    pub fn page_for(&self, plugin: PluginId) -> Option<&SettingsPage> {
        self.pages.get(&plugin)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use modplayer_capability_gateway::ui::{FieldKind, UiId};
    use modplayer_effects::catalog::PluginId as CorePluginId;
    use serde_json::json;

    const P: CorePluginId = CorePluginId(0);

    fn fid(s: &str) -> UiId {
        UiId::parse(s).unwrap_or_else(|| unreachable!())
    }

    fn boolean_field(id: &str) -> SettingsField {
        SettingsField {
            id: fid(id),
            kind: FieldKind::Boolean,
            label: "Snap".to_string(),
            description: None,
            default: json!(false),
        }
    }

    fn number_field(id: &str) -> SettingsField {
        SettingsField {
            id: fid(id),
            kind: FieldKind::Number {
                min: 0.0,
                max: 10.0,
                step: 1.0,
            },
            label: "Shift".to_string(),
            description: None,
            default: json!(0.0),
        }
    }

    /// S1: a stored value that is invalid for the field's kind/range
    /// substitutes the field's own `default` instead of being carried
    /// over verbatim.
    #[test]
    fn settings_invalid_stored_uses_default() {
        let mut reg = SettingsRegistry::new();
        let mut stored = BTreeMap::new();
        stored.insert("shift".to_string(), json!(999.0)); // out of [0,10]
        reg.register(P, vec![number_field("shift")], stored);
        assert_eq!(
            reg.page_for(P).unwrap().values.get("shift"),
            Some(&json!(0.0)),
            "an out-of-range stored value must fall back to the field's default"
        );
    }

    /// S1/data-model.md §8: re-registering the same field id keeps its
    /// *current* value (possibly edited since the first registration),
    /// never resetting it from a (potentially stale) `stored` snapshot.
    #[test]
    fn settings_reregister_keeps_values() {
        let mut reg = SettingsRegistry::new();
        let mut stored = BTreeMap::new();
        stored.insert("snap".to_string(), json!(false));
        reg.register(P, vec![boolean_field("snap")], stored.clone());
        reg.edit(P, "snap", json!(true))
            .unwrap_or_else(|| unreachable!());
        assert_eq!(
            reg.page_for(P).unwrap().values.get("snap"),
            Some(&json!(true))
        );

        // Re-register with the *same* stale `stored` snapshot (as if the
        // plugin's own runtime thread hadn't yet observed the edit) — the
        // edited value must survive.
        reg.register(P, vec![boolean_field("snap")], stored);
        assert_eq!(
            reg.page_for(P).unwrap().values.get("snap"),
            Some(&json!(true)),
            "re-registering an existing field must keep its current value"
        );
    }

    /// S4: an edit naming an unknown field is a no-op.
    #[test]
    fn edit_unknown_field_is_none() {
        let mut reg = SettingsRegistry::new();
        reg.register(P, vec![boolean_field("snap")], BTreeMap::new());
        assert_eq!(reg.edit(P, "missing", json!(true)), None);
    }

    /// S4: an edit whose value doesn't fit the field's kind/range is a
    /// no-op, and the stored value is unchanged.
    #[test]
    fn edit_out_of_range_is_none() {
        let mut reg = SettingsRegistry::new();
        reg.register(P, vec![number_field("shift")], BTreeMap::new());
        assert_eq!(reg.edit(P, "shift", json!(999.0)), None);
        assert_eq!(
            reg.page_for(P).unwrap().values.get("shift"),
            Some(&json!(0.0))
        );
    }

    /// S4: a valid edit returns exactly `{field: value}` and updates the
    /// page's own live value.
    #[test]
    fn edit_valid_returns_changes() {
        let mut reg = SettingsRegistry::new();
        reg.register(P, vec![number_field("shift")], BTreeMap::new());
        let changes = reg
            .edit(P, "shift", json!(3.0))
            .unwrap_or_else(|| unreachable!());
        assert_eq!(changes.len(), 1);
        assert_eq!(changes.get("shift"), Some(&json!(3.0)));
        assert_eq!(
            reg.page_for(P).unwrap().values.get("shift"),
            Some(&json!(3.0))
        );
    }
}

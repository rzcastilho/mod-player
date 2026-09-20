// SPDX-License-Identifier: MIT OR Apache-2.0

//! `KeymapOverrides`: the sparse, diff-against-default per-user binding
//! store (007, data-model.md §2, contracts/action-registry.md §3),
//! extended (011-plugin-ui-contributions, data-model.md §3.3,
//! contracts/action-registry-plugins.md §4) with a plugin-action override
//! map and a `dormant` bucket for a persisted override whose action isn't
//! currently registered.

use std::collections::{BTreeMap, BTreeSet};

use super::PluginActionId;
use super::catalog::{HostAction, def};
use super::chord::Chord;

/// The persisted, sparse record of every action whose effective binding
/// list differs from its default (FR-013/FR-010a). Only actions with an
/// entry in `host`/`plugin` have been customized; `dormant` holds a
/// persisted `[keybindings]` entry for a non-`host.`-namespaced id that
/// is not currently registered by any running plugin (FR-010a) — moved
/// into `plugin` the moment a matching id registers
/// ([`KeymapOverrides::adopt_dormant`]), and back on unregister
/// ([`KeymapOverrides::park`]). `KeymapOverrides::default()` means "every
/// host action at its shipped default, nothing dormant."
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KeymapOverrides {
    host: BTreeMap<HostAction, Vec<Chord>>,
    plugin: BTreeMap<PluginActionId, Vec<Chord>>,
    dormant: BTreeMap<String, Vec<Chord>>,
}

impl KeymapOverrides {
    /// The override for `action`, if any (`None` means "use the catalog
    /// default").
    pub fn get(&self, action: HostAction) -> Option<&[Chord]> {
        self.host.get(&action).map(Vec::as_slice)
    }

    /// Set `action`'s effective bindings to `chords` (deduplicated,
    /// first occurrence kept). Removes the entry instead when the
    /// resulting set equals the catalog default set — an empty `Vec` is
    /// still a valid entry ("deliberately unbound") whenever the
    /// default is non-empty.
    pub fn set(&mut self, action: HostAction, chords: Vec<Chord>) {
        let deduped = dedup(chords);
        if chord_sets_equal(&deduped, &default_chords(action)) {
            self.host.remove(&action);
        } else {
            self.host.insert(action, deduped);
        }
    }

    /// Remove `action`'s override, restoring the catalog default.
    pub fn remove(&mut self, action: HostAction) {
        self.host.remove(&action);
    }

    /// Every customized host action and its overriding binding list.
    pub fn iter(&self) -> impl Iterator<Item = (HostAction, &[Chord])> {
        self.host
            .iter()
            .map(|(&action, chords)| (action, chords.as_slice()))
    }

    /// Whether no host or plugin action has been customized (`dormant`
    /// entries — not currently owned by any registered action — don't
    /// count).
    pub fn is_empty(&self) -> bool {
        self.host.is_empty() && self.plugin.is_empty()
    }

    /// The override for plugin action `id`, if any (data-model.md §3.3).
    pub fn get_plugin(&self, id: &PluginActionId) -> Option<&[Chord]> {
        self.plugin.get(id).map(Vec::as_slice)
    }

    /// As [`KeymapOverrides::set`], for a plugin action: `default` is its
    /// current [`super::PluginActionDef::default_binding`] (`None` when it
    /// ships/registers unbound), the plugin-action equivalent of a host
    /// action's static catalog default.
    pub fn set_plugin(&mut self, id: PluginActionId, chords: Vec<Chord>, default: Option<Chord>) {
        let deduped = dedup(chords);
        let default_set: Vec<Chord> = default.into_iter().collect();
        if chord_sets_equal(&deduped, &default_set) {
            self.plugin.remove(&id);
        } else {
            self.plugin.insert(id, deduped);
        }
    }

    /// Remove `id`'s override, restoring its (possibly-unbound) default.
    pub fn remove_plugin(&mut self, id: &PluginActionId) {
        self.plugin.remove(id);
    }

    /// Every customized plugin action and its overriding binding list.
    pub fn plugin_iter(&self) -> impl Iterator<Item = (&PluginActionId, &[Chord])> {
        self.plugin
            .iter()
            .map(|(id, chords)| (id, chords.as_slice()))
    }

    /// Every dormant entry, keyed by its raw, persisted (non-`host.`-
    /// namespaced) id string (FR-010a) — re-serialised verbatim
    /// (contracts/action-registry-plugins.md K1).
    pub fn dormant_iter(&self) -> impl Iterator<Item = (&String, &[Chord])> {
        self.dormant
            .iter()
            .map(|(id, chords)| (id, chords.as_slice()))
    }

    /// A raw `[keybindings]` entry outside the `host.` namespace, loaded
    /// straight into `dormant` (FR-010a, `settings/model.rs::into_settings`)
    /// — `chords` is already decoded/deduplicated by the caller.
    pub fn set_dormant(&mut self, id: String, chords: Vec<Chord>) {
        self.dormant.insert(id, chords);
    }

    /// K3 (contracts/action-registry-plugins.md §4): move a dormant entry
    /// matching `id` into the live plugin override map, if one exists —
    /// called by `ActionRegistry::register_plugin_action` before its own
    /// `rebuild()`. A pure map move: nothing is written to disk until the
    /// user next changes a binding.
    pub fn adopt_dormant(&mut self, id: &PluginActionId) -> Option<Vec<Chord>> {
        let chords = self.dormant.remove(&id.id())?;
        self.plugin.insert(id.clone(), chords.clone());
        Some(chords)
    }

    /// K3: the inverse of [`KeymapOverrides::adopt_dormant`] — move `id`'s
    /// live override (if any) back to `dormant` on unregister. A no-op if
    /// `id` held no override (it was at its default, nothing to preserve).
    pub fn park(&mut self, id: PluginActionId) {
        if let Some(chords) = self.plugin.remove(&id) {
            self.dormant.insert(id.id(), chords);
        }
    }
}

fn dedup(chords: Vec<Chord>) -> Vec<Chord> {
    let mut deduped: Vec<Chord> = Vec::with_capacity(chords.len());
    for chord in chords {
        if !deduped.contains(&chord) {
            deduped.push(chord);
        }
    }
    deduped
}

/// `action`'s catalog default bindings, parsed. The catalog's own
/// invariant (every default chord parses, pinned by
/// `defaults_match_spec_table`) means this never silently drops a
/// binding in practice; a defensive `filter_map` avoids a `expect` here
/// regardless (Constitution VII: no `unwrap`/`expect` outside tests).
pub(super) fn default_chords(action: HostAction) -> Vec<Chord> {
    def(action)
        .default_bindings
        .iter()
        .filter_map(|s| Chord::parse(s).ok())
        .collect()
}

/// Order-insensitive, duplicate-insensitive equality (data-model.md §2
/// "compares against the default... order-insensitive, duplicates
/// removed").
fn chord_sets_equal(a: &[Chord], b: &[Chord]) -> bool {
    let a: BTreeSet<Chord> = a.iter().copied().collect();
    let b: BTreeSet<Chord> = b.iter().copied().collect();
    a == b
}

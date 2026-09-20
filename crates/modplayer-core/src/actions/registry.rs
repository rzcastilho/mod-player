// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ActionRegistry`: the eager chord→action index, conflict set and
//! binding mutators (007, data-model.md §3, contracts/action-registry.md
//! §4), extended (011-plugin-ui-contributions, data-model.md §3.2,
//! contracts/action-registry-plugins.md) with a plugin-action side keyed
//! by [`ActionId`] uniformly alongside the host catalog.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::catalog::{HostAction, def};
use super::chord::Chord;
use super::keymap::{KeymapOverrides, default_chords};
use super::{
    ActionCategory, ActionId, ActionKind, BindingError, OwnerTier, PluginActionDef, PluginActionId,
    Scope, ScopeState,
};
use crate::plugins::PluginId;

fn action_index(action: HostAction) -> usize {
    HostAction::ALL
        .iter()
        .position(|&a| a == action)
        .unwrap_or(0)
}

/// `ActionRegistry::register_plugin_action` rejected (contracts/
/// action-registry-plugins.md rule G10): the 65th distinct id for one
/// plugin owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionLimit;

/// The shortcut map's own label for a row (contracts/action-registry-
/// plugins.md G15): a host action's Fluent key, or a plugin action's
/// already-resolved (R17) label string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowLabel {
    Fluent(&'static str),
    Literal(String),
}

/// A row's category grouping (contracts/action-registry-plugins.md G15):
/// a host action's [`ActionCategory`], or the owning plugin's resolved
/// display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowCategory {
    Host(ActionCategory),
    Plugin { name: String },
}

/// The runtime, conflict-aware view over a [`KeymapOverrides`]: which
/// chord fires which action right now (contracts/action-registry.md
/// §4, contracts/action-registry-plugins.md). Owned as shadow state by
/// `PlaybackController` (data-model.md §3.1).
pub struct ActionRegistry {
    overrides: KeymapOverrides,
    enabled: [bool; 46],
    /// Every registered plugin action's fixed definition, keyed by its
    /// namespaced id (011-plugin-ui-contributions, data-model.md §3.2).
    plugin_defs: BTreeMap<PluginActionId, PluginActionDef>,
    /// FR-013: tracks whether the owning plugin is currently `Active`.
    /// Absent (or `false`) excludes the action from `index`/dispatch
    /// exactly like a disabled host action.
    plugin_enabled: BTreeMap<PluginActionId, bool>,
    /// Cached effective bindings per plugin action (override, or the
    /// def's own default), recomputed by `rebuild`.
    plugin_effective: BTreeMap<PluginActionId, Vec<Chord>>,
    /// Cached effective bindings per host action (override, or the parsed
    /// catalog default), indexed like `HostAction::ALL` — recomputed by
    /// `rebuild` so [`ActionRegistry::bindings`] can return a borrowed
    /// slice.
    effective: Vec<Vec<Chord>>,
    /// `chord -> actions`, enabled actions only: host entries first (in
    /// catalog order), then plugin ids in `plugin_defs`'s own sorted
    /// order (contracts/action-registry-plugins.md G13's tie-break).
    index: HashMap<Chord, Vec<ActionId>>,
    /// Every `(action, chord)` pair currently flagged as conflicting
    /// (transient; never persisted).
    conflicts: BTreeSet<(ActionId, Chord)>,
}

/// One row of the shortcut map (contracts/action-registry-plugins.md
/// rule G15).
pub struct ActionRow<'a> {
    pub id: ActionId,
    pub label: RowLabel,
    pub category: RowCategory,
    pub kind: ActionKind,
    pub owner_tier: OwnerTier,
    pub enabled: bool,
    pub bindings: &'a [Chord],
    pub conflicts: Vec<Chord>,
}

impl ActionRegistry {
    /// Build a registry from `overrides`, seeding `enabled` from the
    /// catalog's `enabled_by_default` and computing the index/conflict
    /// set once.
    pub fn new(overrides: KeymapOverrides) -> Self {
        let enabled = HostAction::ALL.map(|action| def(action).enabled_by_default);
        let mut registry = Self {
            overrides,
            enabled,
            plugin_defs: BTreeMap::new(),
            plugin_enabled: BTreeMap::new(),
            plugin_effective: BTreeMap::new(),
            effective: Vec::new(),
            index: HashMap::new(),
            conflicts: BTreeSet::new(),
        };
        registry.rebuild();
        registry
    }

    /// The overrides this registry was built from / has accumulated
    /// (for persistence, contracts/action-registry.md §5).
    pub fn overrides(&self) -> &KeymapOverrides {
        &self.overrides
    }

    /// `id`'s effective bindings: its override if one exists, else its
    /// (possibly unbound) default. May be empty.
    pub fn bindings(&self, id: impl Into<ActionId>) -> &[Chord] {
        self.effective_of(&id.into())
    }

    fn effective_of(&self, id: &ActionId) -> &[Chord] {
        match id {
            ActionId::Host(a) => &self.effective[action_index(*a)],
            ActionId::Plugin(p) => self
                .plugin_effective
                .get(p)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        }
    }

    fn enabled_of(&self, id: &ActionId) -> bool {
        match id {
            ActionId::Host(a) => self.enabled[action_index(*a)],
            ActionId::Plugin(p) => self.plugin_enabled.get(p).copied().unwrap_or(false),
        }
    }

    fn tier_of(&self, id: &ActionId) -> OwnerTier {
        match id {
            ActionId::Host(_) => OwnerTier::Host,
            ActionId::Plugin(p) => self
                .plugin_defs
                .get(p)
                .map_or(OwnerTier::Community, |d| d.tier),
        }
    }

    fn scope_of(id: &ActionId) -> Scope {
        match id {
            ActionId::Host(a) => def(*a).scope,
            // FR-010: every plugin action's activation scope is `App`.
            ActionId::Plugin(_) => Scope::App,
        }
    }

    /// Whether `action` currently fires from any of its bindings
    /// (host actions only — a plugin action's `enabled` tracks its
    /// owning plugin's health instead, [`ActionRegistry::
    /// set_plugin_enabled`]).
    pub fn is_enabled(&self, action: HostAction) -> bool {
        self.enabled[action_index(action)]
    }

    /// Enable/disable `action` (not persisted, FR-012). Rebuilds the
    /// index and re-evaluates conflicts immediately (rule G2).
    pub fn set_enabled(&mut self, action: HostAction, enabled: bool) {
        self.enabled[action_index(action)] = enabled;
        self.rebuild();
    }

    fn set_effective(&mut self, id: ActionId, chords: Vec<Chord>) {
        match id {
            ActionId::Host(a) => self.overrides.set(a, chords),
            ActionId::Plugin(p) => {
                let default = self.plugin_defs.get(&p).and_then(|d| d.default_binding);
                self.overrides.set_plugin(p, chords, default);
            }
        }
    }

    /// Add `chord` to `id`'s bindings. `Err(Duplicate)` (no change) if
    /// `id` already holds it; otherwise appended even if another enabled
    /// action already holds it too — that pair becomes conflicting (rule
    /// G5, US3 AS1; contracts/action-registry-plugins.md FR-010 "fully
    /// rebindable ... exactly like a host action").
    pub fn add_binding(
        &mut self,
        id: impl Into<ActionId>,
        chord: Chord,
    ) -> Result<(), BindingError> {
        let id = id.into();
        let current = self.effective_of(&id);
        if current.contains(&chord) {
            return Err(BindingError::Duplicate);
        }
        let mut chords = current.to_vec();
        chords.push(chord);
        self.set_effective(id, chords);
        self.rebuild();
        Ok(())
    }

    /// Remove `chord` from `id`'s bindings; a no-op if it wasn't held
    /// (rule G6).
    pub fn remove_binding(&mut self, id: impl Into<ActionId>, chord: Chord) {
        let id = id.into();
        let current = self.effective_of(&id);
        if !current.contains(&chord) {
            return;
        }
        let mut chords = current.to_vec();
        chords.retain(|&c| c != chord);
        self.set_effective(id, chords);
        self.rebuild();
    }

    /// Restore `id`'s default bindings, leaving `enabled` untouched (rule
    /// G7, FR-011).
    pub fn reset(&mut self, id: impl Into<ActionId>) {
        match id.into() {
            ActionId::Host(a) => self.overrides.remove(a),
            ActionId::Plugin(p) => self.overrides.remove_plugin(&p),
        }
        self.rebuild();
    }

    /// Restore every host action's catalog default bindings, leaving
    /// `enabled` untouched; the resulting host `conflicts` set is empty
    /// (shipped defaults are conflict-free by construction, rule G7).
    /// Plugin action overrides are untouched (no "reset all" affordance
    /// exists for them this slice).
    pub fn reset_all(&mut self) {
        let plugin = std::mem::take(&mut self.overrides);
        self.overrides = KeymapOverrides::default();
        // Re-apply every plugin override the wholesale reset above would
        // otherwise have discarded too (`KeymapOverrides` has no
        // host-only clear).
        for (id, chords) in plugin.plugin_iter() {
            let default = self.plugin_defs.get(id).and_then(|d| d.default_binding);
            self.overrides
                .set_plugin(id.clone(), chords.to_vec(), default);
        }
        self.rebuild();
    }

    /// Whether `(id, chord)` is currently flagged as conflicting.
    pub fn is_conflicting(&self, id: impl Into<ActionId>, chord: Chord) -> bool {
        self.conflicts.contains(&(id.into(), chord))
    }

    /// The other action `(id, chord)` conflicts with, if any (for the
    /// inline "conflicts with …" message, US3/FR-011).
    pub fn conflict_partner(&self, id: impl Into<ActionId>, chord: Chord) -> Option<ActionId> {
        let id = id.into();
        if !self.is_conflicting(id.clone(), chord) {
            return None;
        }
        self.index
            .get(&chord)
            .and_then(|ids| ids.iter().find(|other| **other != id).cloned())
    }

    /// The single enabled, non-conflicting, live-scoped action `chord`
    /// fires, if any (rule G4/G13). Ties among non-conflicting candidates
    /// go host-catalog-order, then plugin id (insertion order in
    /// `index`, contracts/action-registry-plugins.md G13).
    pub fn resolve(&self, chord: Chord, state: &ScopeState) -> Option<ActionId> {
        let candidates = self.index.get(&chord)?;
        candidates
            .iter()
            .find(|&id| {
                Self::scope_of(id).is_live(state) && !self.conflicts.contains(&(id.clone(), chord))
            })
            .cloned()
    }

    /// `id` is registered, its owning context is enabled/Active, and no
    /// chord of it is currently flagged (contracts/action-registry-
    /// plugins.md G14) — the gate `invoke_plugin_action`/a panel button
    /// applies before dispatching (FR-012/FR-013).
    /// Whether `id` is currently a registered plugin action (`false` for
    /// an id that was never registered, or has since been unregistered —
    /// its override, if any, lives on in `dormant`).
    #[must_use]
    pub fn plugin_action_registered(&self, id: &PluginActionId) -> bool {
        self.plugin_defs.contains_key(id)
    }

    /// `id`'s registered definition, if any (`PlaybackController::
    /// invoke_plugin_action`'s own lookup for the owning plugin/kind).
    #[must_use]
    pub fn plugin_action_def(&self, id: &PluginActionId) -> Option<&PluginActionDef> {
        self.plugin_defs.get(id)
    }

    /// Whether `id` repeats while held (contracts/action-registry-plugins.md
    /// D1: `push_invocation` reads this "from the catalog or the plugin
    /// def"): a host action's catalog flag, or a plugin action's own
    /// `PluginActionDef::repeats_while_held` (`false` for an id that is no
    /// longer registered — a stale `ActionId` from a dropped frame).
    #[must_use]
    pub fn repeats_while_held(&self, id: &ActionId) -> bool {
        match id {
            ActionId::Host(a) => def(*a).repeats_while_held,
            ActionId::Plugin(p) => self
                .plugin_defs
                .get(p)
                .is_some_and(|d| d.repeats_while_held),
        }
    }

    #[must_use]
    pub fn is_invocable(&self, id: &ActionId) -> bool {
        self.enabled_of(id)
            && !self
                .effective_of(id)
                .iter()
                .any(|&c| self.conflicts.contains(&(id.clone(), c)))
    }

    /// Register (or re-register) a plugin action (contracts/action-
    /// registry-plugins.md G10): the 65th distinct id for `def.owner`
    /// refuses with [`ActionLimit`]; re-registering an existing id
    /// replaces `label`/`kind`/`repeats_while_held`/`default_binding`
    /// (and this owner's own `name`), keeping any live user override
    /// untouched. A brand-new id first adopts a matching dormant override
    /// (K3), if any, and starts enabled (registration only ever reaches
    /// the registry from a plugin's own running thread, so its owner is
    /// already Active).
    pub fn register_plugin_action(&mut self, def: PluginActionDef) -> Result<(), ActionLimit> {
        use modplayer_capability_gateway::ui::limits::MAX_ACTIONS_PER_PLUGIN;

        let is_new = !self.plugin_defs.contains_key(&def.id);
        if is_new {
            let count = self
                .plugin_defs
                .values()
                .filter(|d| d.owner == def.owner)
                .count();
            if count >= MAX_ACTIONS_PER_PLUGIN {
                return Err(ActionLimit);
            }
            self.overrides.adopt_dormant(&def.id);
            self.plugin_enabled.insert(def.id.clone(), true);
        }
        self.plugin_defs.insert(def.id.clone(), def);
        self.rebuild();
        Ok(())
    }

    /// Unregister every action `owner` currently holds (contracts/
    /// action-registry-plugins.md G12): on `Disable`/`Shutdown`/uninstall.
    /// Each id's live override (if any) moves to `dormant` (K3), never
    /// dropped.
    pub fn unregister_plugin_actions(&mut self, owner: PluginId) {
        let ids: Vec<PluginActionId> = self
            .plugin_defs
            .iter()
            .filter(|(_, d)| d.owner == owner)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.plugin_defs.remove(&id);
            self.plugin_enabled.remove(&id);
            self.overrides.park(id);
        }
        self.rebuild();
    }

    /// FR-013: `set_plugin_enabled(owner, false)` on `Suspend`/`Disable`;
    /// `true` on `Ready` (contracts/action-registry-plugins.md G11).
    /// Rebuilds and re-evaluates conflicts immediately. A no-op if
    /// `owner` currently holds no actions.
    pub fn set_plugin_enabled(&mut self, owner: PluginId, enabled: bool) {
        let ids: Vec<PluginActionId> = self
            .plugin_defs
            .iter()
            .filter(|(_, d)| d.owner == owner)
            .map(|(id, _)| id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        for id in ids {
            self.plugin_enabled.insert(id, enabled);
        }
        self.rebuild();
    }

    /// Every action, host rows first in catalog order, then one group per
    /// owning plugin (sorted by that plugin's resolved display name),
    /// each action sorted by label (contracts/action-registry-plugins.md
    /// rule G15).
    pub fn rows(&self) -> impl Iterator<Item = ActionRow<'_>> + '_ {
        let host_rows = HostAction::ALL.iter().enumerate().map(move |(i, &action)| {
            let id = ActionId::Host(action);
            let bindings = self.effective[i].as_slice();
            let conflicts = bindings
                .iter()
                .copied()
                .filter(|&c| self.conflicts.contains(&(id.clone(), c)))
                .collect();
            ActionRow {
                id,
                label: RowLabel::Fluent(def(action).label_key),
                category: RowCategory::Host(def(action).category),
                kind: def(action).kind,
                owner_tier: OwnerTier::Host,
                enabled: self.enabled[i],
                bindings,
                conflicts,
            }
        });

        // One entry per distinct owning plugin, in display-name order
        // (`PluginId` itself has no `Ord`, hence sorting the name column
        // directly rather than collecting into a `BTreeMap<PluginId, _>`).
        let mut owners: Vec<(PluginId, &str)> = Vec::new();
        for d in self.plugin_defs.values() {
            if !owners.iter().any(|&(id, _)| id == d.owner) {
                owners.push((d.owner, d.name.as_str()));
            }
        }
        owners.sort_by(|a, b| a.1.cmp(b.1));

        let mut plugin_rows: Vec<ActionRow<'_>> = Vec::new();
        for (owner, _) in owners {
            let mut defs: Vec<&PluginActionDef> = self
                .plugin_defs
                .values()
                .filter(|d| d.owner == owner)
                .collect();
            defs.sort_by(|a, b| a.label.cmp(&b.label));
            for d in defs {
                let id = ActionId::Plugin(d.id.clone());
                let bindings = self
                    .plugin_effective
                    .get(&d.id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let conflicts = bindings
                    .iter()
                    .copied()
                    .filter(|&c| self.conflicts.contains(&(id.clone(), c)))
                    .collect();
                plugin_rows.push(ActionRow {
                    id: id.clone(),
                    label: RowLabel::Literal(d.label.clone()),
                    category: RowCategory::Plugin {
                        name: d.name.clone(),
                    },
                    kind: d.kind,
                    owner_tier: d.tier,
                    enabled: self.plugin_enabled.get(&d.id).copied().unwrap_or(false),
                    bindings,
                    conflicts,
                });
            }
        }

        host_rows.chain(plugin_rows)
    }

    /// Recompute `effective`, `plugin_effective`, `index` and `conflicts`
    /// from scratch (`overrides`/`plugin_defs` + `enabled`/
    /// `plugin_enabled`). O(total bindings); called after every mutation
    /// (rule G8) — never per key event.
    fn rebuild(&mut self) {
        self.effective = HostAction::ALL
            .iter()
            .map(|&action| match self.overrides.get(action) {
                Some(chords) => chords.to_vec(),
                None => default_chords(action),
            })
            .collect();

        self.plugin_effective = self
            .plugin_defs
            .iter()
            .map(|(id, def)| {
                let chords = match self.overrides.get_plugin(id) {
                    Some(chords) => chords.to_vec(),
                    None => def.default_binding.into_iter().collect(),
                };
                (id.clone(), chords)
            })
            .collect();

        self.index.clear();
        for (i, &action) in HostAction::ALL.iter().enumerate() {
            if !self.enabled[i] {
                continue;
            }
            for &chord in &self.effective[i] {
                self.index
                    .entry(chord)
                    .or_default()
                    .push(ActionId::Host(action));
            }
        }
        // `plugin_defs` is a `BTreeMap<PluginActionId, _>`, so this walk is
        // already in the "plugin ids sorted" order G13's tie-break wants.
        for (id, chords) in &self.plugin_effective {
            if !self.plugin_enabled.get(id).copied().unwrap_or(false) {
                continue;
            }
            for &chord in chords {
                self.index
                    .entry(chord)
                    .or_default()
                    .push(ActionId::Plugin(id.clone()));
            }
        }

        self.conflicts.clear();
        for (&chord, ids) in &self.index {
            if ids.len() < 2 {
                continue;
            }
            // Only ids that coexist with at least one other candidate on
            // this chord are ever in contention — a truly scope-disjoint
            // pair (none exist among this slice's real scopes, all of
            // which nest; `Scope::App` for every plugin action coexists
            // with all three too) would simply never conflict.
            let contenders: Vec<&ActionId> = ids
                .iter()
                .filter(|id| {
                    ids.iter().any(|other| {
                        *other != **id
                            && Scope::can_coexist(Self::scope_of(id), Self::scope_of(other))
                    })
                })
                .collect();
            if contenders.len() < 2 {
                continue;
            }
            // G13: `top` = the highest tier represented; exactly one
            // candidate at `top` fires (everyone else flagged); ≥ 2 at
            // `top` flags every contender, top tier included.
            let top = contenders
                .iter()
                .map(|id| self.tier_of(id))
                .max()
                .unwrap_or(OwnerTier::Host);
            let top_count = contenders
                .iter()
                .filter(|id| self.tier_of(id) == top)
                .count();
            for id in contenders {
                let flag = top_count >= 2 || self.tier_of(id) != top;
                if flag {
                    self.conflicts.insert((id.clone(), chord));
                }
            }
        }
    }
}

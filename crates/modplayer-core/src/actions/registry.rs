// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ActionRegistry`: the eager chord→action index, conflict set and
//! binding mutators (007, data-model.md §3, contracts/action-registry.md
//! §4).

use std::collections::{BTreeSet, HashMap};

use super::catalog::{ActionDef, HostAction, def};
use super::chord::Chord;
use super::keymap::{KeymapOverrides, default_chords};
use super::{BindingError, Scope, ScopeState};

fn action_index(action: HostAction) -> usize {
    HostAction::ALL
        .iter()
        .position(|&a| a == action)
        .unwrap_or(0)
}

/// The runtime, conflict-aware view over a [`KeymapOverrides`]: which
/// chord fires which action right now (contracts/action-registry.md
/// §4). Owned as shadow state by `PlaybackController` (data-model.md
/// §3.1).
pub struct ActionRegistry {
    overrides: KeymapOverrides,
    enabled: [bool; 46],
    /// Cached effective bindings per action (override, or the parsed
    /// catalog default), indexed like `HostAction::ALL` — recomputed by
    /// `rebuild` so [`ActionRegistry::bindings`] can return a borrowed
    /// slice.
    effective: Vec<Vec<Chord>>,
    /// `chord -> actions`, enabled actions only, in catalog order
    /// (`resolve`'s tie-break, contracts/action-registry.md §4 rule
    /// G4).
    index: HashMap<Chord, Vec<HostAction>>,
    /// Every `(action, chord)` pair currently flagged as conflicting
    /// (transient; never persisted).
    conflicts: BTreeSet<(HostAction, Chord)>,
}

/// One row of the shortcut map (contracts/action-registry.md §4 rule
/// G9).
pub struct ActionRow<'a> {
    pub def: &'static ActionDef,
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

    /// `action`'s effective bindings: its override if one exists, else
    /// the parsed catalog default (rule G1). May be empty.
    pub fn bindings(&self, action: HostAction) -> &[Chord] {
        &self.effective[action_index(action)]
    }

    /// Whether `action` currently fires from any of its bindings.
    pub fn is_enabled(&self, action: HostAction) -> bool {
        self.enabled[action_index(action)]
    }

    /// Enable/disable `action` (not persisted, FR-012). Rebuilds the
    /// index and re-evaluates conflicts immediately (rule G2).
    pub fn set_enabled(&mut self, action: HostAction, enabled: bool) {
        self.enabled[action_index(action)] = enabled;
        self.rebuild();
    }

    /// Add `chord` to `action`'s bindings. `Err(Duplicate)` (no change)
    /// if `action` already holds it; otherwise appended even if another
    /// enabled action already holds it too — that pair becomes
    /// conflicting (rule G5, US3 AS1).
    pub fn add_binding(&mut self, action: HostAction, chord: Chord) -> Result<(), BindingError> {
        let idx = action_index(action);
        if self.effective[idx].contains(&chord) {
            return Err(BindingError::Duplicate);
        }
        let mut chords = self.effective[idx].clone();
        chords.push(chord);
        self.overrides.set(action, chords);
        self.rebuild();
        Ok(())
    }

    /// Remove `chord` from `action`'s bindings; a no-op if it wasn't
    /// held (rule G6).
    pub fn remove_binding(&mut self, action: HostAction, chord: Chord) {
        let idx = action_index(action);
        if !self.effective[idx].contains(&chord) {
            return;
        }
        let mut chords = self.effective[idx].clone();
        chords.retain(|&c| c != chord);
        self.overrides.set(action, chords);
        self.rebuild();
    }

    /// Restore `action`'s catalog default bindings, leaving `enabled`
    /// untouched (rule G7, FR-011).
    pub fn reset(&mut self, action: HostAction) {
        self.overrides.remove(action);
        self.rebuild();
    }

    /// Restore every action's catalog default bindings, leaving
    /// `enabled` untouched; the resulting `conflicts` set is empty
    /// (shipped defaults are conflict-free by construction, rule G7).
    pub fn reset_all(&mut self) {
        self.overrides = KeymapOverrides::default();
        self.rebuild();
    }

    /// Whether `(action, chord)` is currently flagged as conflicting.
    pub fn is_conflicting(&self, action: HostAction, chord: Chord) -> bool {
        self.conflicts.contains(&(action, chord))
    }

    /// The other action `(action, chord)` conflicts with, if any
    /// (for the inline "conflicts with …" message, US3).
    pub fn conflict_partner(&self, action: HostAction, chord: Chord) -> Option<HostAction> {
        if !self.is_conflicting(action, chord) {
            return None;
        }
        self.index
            .get(&chord)
            .and_then(|actions| actions.iter().copied().find(|&other| other != action))
    }

    /// The single enabled, non-conflicting, live-scoped action `chord`
    /// fires, if any (rule G4). Ties among non-conflicting candidates go
    /// to catalog order (never happens under this slice's nested
    /// scopes).
    pub fn resolve(&self, chord: Chord, state: &ScopeState) -> Option<HostAction> {
        let candidates = self.index.get(&chord)?;
        candidates
            .iter()
            .copied()
            .find(|&action| def(action).scope.is_live(state) && !self.is_conflicting(action, chord))
    }

    /// Every action, in catalog order, for the shortcut map (rule G9).
    pub fn rows(&self) -> impl Iterator<Item = ActionRow<'_>> + '_ {
        HostAction::ALL.iter().enumerate().map(move |(i, &action)| {
            let bindings = self.effective[i].as_slice();
            let conflicts = bindings
                .iter()
                .copied()
                .filter(|&chord| self.is_conflicting(action, chord))
                .collect();
            ActionRow {
                def: def(action),
                enabled: self.enabled[i],
                bindings,
                conflicts,
            }
        })
    }

    /// Recompute `effective`, `index` and `conflicts` from scratch
    /// (`overrides` + `enabled`). O(total bindings); called after every
    /// mutation (rule G8) — never per key event.
    fn rebuild(&mut self) {
        self.effective = HostAction::ALL
            .iter()
            .map(|&action| match self.overrides.get(action) {
                Some(chords) => chords.to_vec(),
                None => default_chords(action),
            })
            .collect();

        self.index.clear();
        for (i, &action) in HostAction::ALL.iter().enumerate() {
            if !self.enabled[i] {
                continue;
            }
            for &chord in &self.effective[i] {
                self.index.entry(chord).or_default().push(action);
            }
        }

        self.conflicts.clear();
        for (&chord, actions) in &self.index {
            if actions.len() < 2 {
                continue;
            }
            for &a in actions {
                let conflicts_with_another = actions
                    .iter()
                    .any(|&b| b != a && Scope::can_coexist(def(a).scope, def(b).scope));
                if conflicts_with_another {
                    self.conflicts.insert((a, chord));
                }
            }
        }
    }
}

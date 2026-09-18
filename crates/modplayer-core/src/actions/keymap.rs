// SPDX-License-Identifier: MIT OR Apache-2.0

//! `KeymapOverrides`: the sparse, diff-against-default per-user binding
//! store (007, data-model.md §2, contracts/action-registry.md §3).

use std::collections::{BTreeMap, BTreeSet};

use super::catalog::{HostAction, def};
use super::chord::Chord;

/// The persisted, sparse record of every action whose effective binding
/// list differs from the catalog default (FR-013). Only actions with an
/// entry here have been customized; `KeymapOverrides::default()` means
/// "every action at its shipped default."
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KeymapOverrides(BTreeMap<HostAction, Vec<Chord>>);

impl KeymapOverrides {
    /// The override for `action`, if any (`None` means "use the catalog
    /// default").
    pub fn get(&self, action: HostAction) -> Option<&[Chord]> {
        self.0.get(&action).map(Vec::as_slice)
    }

    /// Set `action`'s effective bindings to `chords` (deduplicated,
    /// first occurrence kept). Removes the entry instead when the
    /// resulting set equals the catalog default set — an empty `Vec` is
    /// still a valid entry ("deliberately unbound") whenever the
    /// default is non-empty.
    pub fn set(&mut self, action: HostAction, chords: Vec<Chord>) {
        let mut deduped: Vec<Chord> = Vec::with_capacity(chords.len());
        for chord in chords {
            if !deduped.contains(&chord) {
                deduped.push(chord);
            }
        }
        if chord_sets_equal(&deduped, &default_chords(action)) {
            self.0.remove(&action);
        } else {
            self.0.insert(action, deduped);
        }
    }

    /// Remove `action`'s override, restoring the catalog default.
    pub fn remove(&mut self, action: HostAction) {
        self.0.remove(&action);
    }

    /// Every customized action and its overriding binding list.
    pub fn iter(&self) -> impl Iterator<Item = (HostAction, &[Chord])> {
        self.0
            .iter()
            .map(|(&action, chords)| (action, chords.as_slice()))
    }

    /// Whether no action has been customized.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
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

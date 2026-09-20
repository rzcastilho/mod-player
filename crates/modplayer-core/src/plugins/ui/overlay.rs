// SPDX-License-Identifier: MIT OR Apache-2.0

//! `OverlayRegistry` (US3 T084, contracts/overlays-settings-notify.md §1.1
//! "O" rules, data-model.md §4.3): the host-owned state behind
//! `ui.overlay` — `add_overlays`/`remove_overlays`/`clear_overlays` (RPC,
//! `plugins/apply.rs`). Validation of a call's *shape* already happened in
//! the gateway crate (`modplayer_capability_gateway::ui::
//! validate_primitives`, research R3); this module only enforces
//! per-plugin *capacity* (`MAX_OVERLAY_PRIMITIVES`, O1) and holds the live
//! primitive set, keyed by id so a re-added id replaces it in place (O1).
//! Ordering the result into per-plugin, `Active`-only layers for
//! `modplayer-ui` (O4) is [`super::super::view::OverlayLayer`]'s job,
//! exactly like `PanelRegistry`'s own split with `PluginPanelsView`
//! (`view.rs` owns lifecycle-aware view assembly; the registry itself
//! never reads a `PluginRecord`).

use std::collections::{BTreeMap, HashMap};

use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::ui::limits::MAX_OVERLAY_PRIMITIVES;
use modplayer_capability_gateway::ui::{OverlayPrimitive, UiId};

use super::super::PluginId;

/// One plugin's overlay primitives, keyed by id (O1: re-adding an id
/// replaces it in place) plus this plugin's own first-registration
/// sequence (O4: cross-plugin z-order, stable for the session — assigned
/// once, on this plugin's first ever `add`, kept across every later
/// `add`/`remove`/`clear` for the life of the session, exactly like
/// `PanelRegistry`'s own `Panel::seq`).
#[derive(Debug, Default)]
struct OverlaySet {
    primitives: BTreeMap<UiId, OverlayPrimitive>,
    seq: u64,
}

/// FR-014/FR-015/FR-016: every plugin's registered overlay primitives.
/// Owned by [`super::PluginUi`], itself owned by `PluginHost` (Constitution
/// III).
#[derive(Debug, Default)]
pub struct OverlayRegistry {
    /// `HashMap`, not `BTreeMap` (mirrors `PanelRegistry::panels`'s own
    /// doc note): `modplayer_effects::catalog::PluginId` has no `Ord`.
    sets: HashMap<PluginId, OverlaySet>,
    next_seq: u64,
}

impl OverlayRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// O1: the caller (`plugins/apply.rs`) already ran
    /// `validate_primitives` over the whole batch's *shape*; this only
    /// enforces the post-merge *count* — nothing changes on refusal.
    pub fn add(
        &mut self,
        plugin: PluginId,
        primitives: Vec<OverlayPrimitive>,
    ) -> Result<(), Refusal> {
        if !self.sets.contains_key(&plugin) {
            let seq = self.next_seq;
            self.next_seq += 1;
            self.sets.insert(
                plugin,
                OverlaySet {
                    primitives: BTreeMap::new(),
                    seq,
                },
            );
        }
        let set = self
            .sets
            .get_mut(&plugin)
            .unwrap_or_else(|| unreachable!("just inserted above"));
        let net_new = primitives
            .iter()
            .filter(|p| !set.primitives.contains_key(p.id()))
            .count();
        if set.primitives.len() + net_new > MAX_OVERLAY_PRIMITIVES {
            return Err(Refusal::invalid_state(
                "overlay_limit",
                format!("A plugin may have at most {MAX_OVERLAY_PRIMITIVES} overlay primitives."),
            ));
        }
        for primitive in primitives {
            set.primitives.insert(primitive.id().clone(), primitive);
        }
        Ok(())
    }

    /// O2: `not_found` if any `id` is unknown (no set at all counts as
    /// every id being unknown) — removes nothing in that case.
    pub fn remove(&mut self, plugin: PluginId, ids: &[UiId]) -> Result<(), Refusal> {
        let Some(set) = self.sets.get_mut(&plugin) else {
            return Err(Refusal::not_found());
        };
        if ids.iter().any(|id| !set.primitives.contains_key(id)) {
            return Err(Refusal::not_found());
        }
        for id in ids {
            set.primitives.remove(id);
        }
        Ok(())
    }

    /// O3 (explicit `clear_overlays`, `TrackChanged`, `on_stop` any
    /// reason): empty `plugin`'s primitives — its registration `seq` is
    /// deliberately kept (O4: "stable for the session"), so a plugin that
    /// re-adds overlays for a new track (the `ui-overlay` fixture's own
    /// per-track behaviour) never jumps behind a plugin that registered
    /// later. A no-op for a plugin with no set yet.
    pub fn clear(&mut self, plugin: PluginId) {
        if let Some(set) = self.sets.get_mut(&plugin) {
            set.primitives.clear();
        }
    }

    /// FR-016: clear every plugin's overlays at once (`TrackChanged`, any
    /// direction — including sign-out's `None`).
    pub fn clear_all(&mut self) {
        for set in self.sets.values_mut() {
            set.primitives.clear();
        }
    }

    /// This plugin's own first-registration sequence (O4), if it has ever
    /// called `add` — used by [`super::super::view::OverlayLayer::
    /// from_records`] for the cross-plugin z-order key.
    #[must_use]
    pub fn seq_of(&self, plugin: PluginId) -> Option<u64> {
        self.sets.get(&plugin).map(|set| set.seq)
    }

    /// Every primitive `plugin` currently has registered (order not
    /// meaningful — only kind/position matter for painting) — empty for a
    /// plugin with none (never registered, or cleared).
    #[must_use]
    pub fn primitives_for(&self, plugin: PluginId) -> Vec<OverlayPrimitive> {
        self.sets
            .get(&plugin)
            .map(|set| set.primitives.values().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use modplayer_capability_gateway::ui::OverlayColor;
    use modplayer_effects::catalog::PluginId as CorePluginId;

    const P: CorePluginId = CorePluginId(0);
    const Q: CorePluginId = CorePluginId(1);

    fn id(s: &str) -> UiId {
        UiId::parse(s).unwrap_or_else(|| unreachable!())
    }

    fn line(s: &str, at_ms: u64) -> OverlayPrimitive {
        OverlayPrimitive::Line {
            id: id(s),
            at_ms,
            color: OverlayColor::Accent,
        }
    }

    /// O1: a batch over the 500 cap is refused whole; anything already
    /// registered is untouched.
    #[test]
    fn overlay_501st_refused_prior_unchanged() {
        let mut reg = OverlayRegistry::new();
        let first_batch: Vec<_> = (0..500).map(|i| line(&format!("p{i}"), i)).collect();
        reg.add(P, first_batch)
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        let err = reg.add(P, vec![line("one_more", 999)]).unwrap_err();
        assert_eq!(err.reason, "overlay_limit");
        assert_eq!(
            reg.primitives_for(P).len(),
            500,
            "the 501st must not be added"
        );
    }

    /// O1: re-adding an existing id replaces it in place, never
    /// double-counting toward the cap.
    #[test]
    fn overlay_readd_replaces_in_place() {
        let mut reg = OverlayRegistry::new();
        reg.add(P, vec![line("a", 100)])
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        reg.add(P, vec![line("a", 200)])
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        let primitives = reg.primitives_for(P);
        assert_eq!(
            primitives.len(),
            1,
            "re-adding 'a' must replace, not append"
        );
        match &primitives[0] {
            OverlayPrimitive::Line { at_ms, .. } => assert_eq!(*at_ms, 200),
            other => unreachable!("{other:?}"),
        }
    }

    /// O2: removing an unknown id refuses the whole call, atomically.
    #[test]
    fn remove_unknown_not_found_atomic() {
        let mut reg = OverlayRegistry::new();
        reg.add(P, vec![line("a", 100), line("b", 200)])
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        let err = reg.remove(P, &[id("a"), id("missing")]).unwrap_err();
        assert_eq!(err.reason, "unknown_id");
        assert_eq!(
            reg.primitives_for(P).len(),
            2,
            "a partial removal must not apply"
        );
    }

    /// FR-016: `clear_all` empties every plugin, but a plugin's `seq`
    /// survives (proven by re-adding and checking `seq_of`'s cross-plugin
    /// order is unchanged).
    #[test]
    fn overlays_cleared_on_track_change() {
        let mut reg = OverlayRegistry::new();
        reg.add(P, vec![line("a", 100)])
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        reg.add(Q, vec![line("b", 200)])
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        let (p_seq, q_seq) = (reg.seq_of(P).unwrap(), reg.seq_of(Q).unwrap());
        reg.clear_all();
        assert!(reg.primitives_for(P).is_empty());
        assert!(reg.primitives_for(Q).is_empty());

        // Re-add after the clear: `seq_of` must stay stable (P registered
        // first, so it must keep sorting before Q).
        reg.add(Q, vec![line("b2", 250)])
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        reg.add(P, vec![line("a2", 150)])
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        assert_eq!(reg.seq_of(P), Some(p_seq));
        assert_eq!(reg.seq_of(Q), Some(q_seq));
        assert!(p_seq < q_seq);
    }

    /// O3: `clear` (this module's half of `on_stop(any reason)` —
    /// `PluginUi::on_stop` calls it unconditionally) empties one plugin
    /// without touching another's.
    #[test]
    fn overlays_cleared_on_stop() {
        let mut reg = OverlayRegistry::new();
        reg.add(P, vec![line("a", 100)])
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        reg.add(Q, vec![line("b", 200)])
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        reg.clear(P);
        assert!(reg.primitives_for(P).is_empty());
        assert_eq!(reg.primitives_for(Q).len(), 1);
    }
}

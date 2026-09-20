// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ChainView`/`NodeRow` and `MeterSnapshot`/`LevelPair`: the UI's
//! per-frame projections of the effect chain (008, data-model.md §2.4,
//! §2.5). Built from [`super::model::ChainModel`] plus live `RtShared`
//! cost/meter/spectrum reads. `project` itself stays pure (a `node_cost`
//! closure, no direct `RtShared` dependency, so it is unit-testable
//! without one) and always returns `over_budget: false`/`overload_count:
//! 0`; `PlaybackController::chain_view` (contracts/effects-service.md §2
//! rule C9) is the one that overwrites those two fields from
//! `RtShared::over_budget()`/`overload_count()` after calling `project`.
//! `PlaybackController::chain_meters` reads `MeterSnapshot`'s fields
//! straight from `RtShared`'s meter/spectrum atomics (data-model.md §4).

use modplayer_effects::catalog::{NodeKind, NodeOwner};

use super::model::{ChainModel, NodeId};

/// One row of the Effect Chain panel (data-model.md §2.4).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRow {
    pub id: NodeId,
    pub index: usize,
    pub kind: NodeKind,
    pub owner: NodeOwner,
    pub params: Vec<f32>,
    pub bypassed: bool,
    pub auto_bypassed: bool,
    /// "quality mode auto-switched" note (FR-008, US2 T061).
    pub mode_note: bool,
    /// This node's rolling-mean cost, as a percentage of the callback
    /// period; `0.0` while not playing (C9) or before any render.
    pub cost_pct: f32,
    /// FR-007: this node is the *second* member of an adjacent combined
    /// pitch/stretch pair this render (tooltip only). Always `false`
    /// until Phase 3's combined-stage detection lands.
    pub combined_with_neighbour: bool,
}

/// The whole panel's projection of the chain (data-model.md §2.4).
#[derive(Debug, Clone, PartialEq)]
pub struct ChainView {
    pub nodes: Vec<NodeRow>,
    pub total_cost_pct: f32,
    /// `RtShared::over_budget()`, overwritten by `PlaybackController::
    /// chain_view` after `project` (this module stays pure).
    pub over_budget: bool,
    /// `RtShared::overload_count()`, overwritten the same way.
    pub overload_count: u32,
    pub capacity: usize,
}

/// Peak + RMS, one channel pair, linear amplitude (data-model.md §2.5).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LevelPair {
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
}

/// The Chain Meter Snapshot entity (data-model.md §2.5): pre-/post-chain
/// levels, the 64-band spectrum, and the chain's current advance rate.
/// Read fresh every frame the panel is open; never stored.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterSnapshot {
    pub pre: LevelPair,
    pub post: LevelPair,
    pub spectrum: [f32; 64],
    pub spectrum_generation: u32,
    pub advance_rate: f32,
}

impl Default for MeterSnapshot {
    fn default() -> Self {
        Self {
            pre: LevelPair::default(),
            post: LevelPair::default(),
            spectrum: [0.0; 64],
            spectrum_generation: 0,
            advance_rate: 1.0,
        }
    }
}

/// Project `model` into a [`ChainView`]. `node_cost` supplies each row's
/// live cost (`RtShared::node_cost(slot)` while playing, `0.0`
/// otherwise, per C9) — a closure rather than a direct `RtShared`
/// dependency so this stays pure and unit-testable without one.
pub fn project(model: &ChainModel, mut node_cost: impl FnMut(u8) -> f32) -> ChainView {
    let mut total_cost_pct = 0.0;
    let nodes = model
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let cost_pct = node_cost(node.slot);
            total_cost_pct += cost_pct;
            NodeRow {
                id: node.id,
                index,
                kind: node.kind,
                owner: node.owner,
                params: node.params.clone(),
                bypassed: node.bypassed,
                auto_bypassed: node.auto_bypassed,
                mode_note: node.mode_state.is_some_and(|s| s.auto_switched),
                cost_pct,
                combined_with_neighbour: false,
            }
        })
        .collect();
    ChainView {
        nodes,
        total_cost_pct,
        over_budget: false,
        overload_count: 0,
        capacity: model.capacity(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn project_reports_zero_cost_and_mode_note_from_model() {
        let mut model = ChainModel::new(44_100);
        let (id, _) = model
            .add(NodeKind::PitchShift, NodeOwner::Host)
            .expect("add");
        model
            .set_param(id, modplayer_effects::catalog::ParamId(0), 7.0)
            .expect("set_param");

        let view = project(&model, |_slot| 0.0);
        assert_eq!(view.nodes.len(), 1);
        assert_eq!(view.total_cost_pct, 0.0);
        assert!(
            view.nodes[0].mode_note,
            "auto-switched must set the note flag"
        );
        assert_eq!(view.capacity, 16);
    }
}

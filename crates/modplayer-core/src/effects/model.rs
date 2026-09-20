// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ChainModel`: the controller's effect-chain shadow state (008,
//! data-model.md §2, contracts/effects-service.md §1) — the sole
//! authority on node ids, order and parameter values. Every operation is
//! pure and returns the `Command`s the controller must push, so the
//! controller stays a thin adapter and this is fully unit-testable
//! without a running `Processor`.

use modplayer_capability_gateway::manifest::SuggestedPosition;
use modplayer_effects::catalog::{
    self, ModeState, NodeKind, NodeOwner, ParamId, ParamShape, PluginId, QualityMode,
    mode_after_user_set, mode_after_value_change,
};
use modplayer_effects::consts::MAX_NODES;
use modplayer_engine::Command;

/// A node's identity: monotonic per `ChainModel`, never reused — a
/// removed node's id is dead forever, so a stale UI reference can never
/// hit a re-used slot (data-model.md §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Crate-internal: reconstructs a `NodeId` from its cross-boundary
    /// `u32` form (009 `plugins::apply`'s `SetParam`/`ScheduleParam`/
    /// `SetBypass`/`RemoveNode`, US2 T086) — never exposed outside the
    /// crate, since a fresh `NodeId` is otherwise only ever handed out by
    /// [`ChainModel::add_at`].
    pub(crate) const fn from_raw(id: u32) -> Self {
        Self(id)
    }
}

/// One node's shadow state (data-model.md §2.2). Position in the chain is
/// this node's index in [`ChainModel::nodes`].
#[derive(Debug, Clone, PartialEq)]
pub struct NodeModel {
    pub id: NodeId,
    /// The RT slot this node occupies (`0..MAX_NODES`); freed on remove.
    pub slot: u8,
    pub kind: NodeKind,
    pub owner: NodeOwner,
    /// Indexed positionally into `catalog::params(kind)` (not by raw
    /// `ParamId`) — always the value clamped at the *current*
    /// `source_rate`.
    pub params: Vec<f32>,
    pub bypassed: bool,
    /// Set only by `mark_auto_bypassed` (from `Event::AutoBypassed`);
    /// cleared by a user un-bypass.
    pub auto_bypassed: bool,
    /// `Some` for `PitchShift`/`TimeStretch` only (FR-008).
    pub mode_state: Option<ModeState>,
    /// Reserved (DM-17): always `false` in this slice (no plugin runtime
    /// yet to orphan a node's owner).
    pub orphaned: bool,
}

/// `ChainModel`'s refusals (`thiserror`, never a panic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ChainError {
    #[error("effect chain is full")]
    Full,
    #[error("unknown effect node")]
    UnknownNode,
    #[error("parameter does not exist for this node kind")]
    WrongKind,
}

/// The controller's effect-chain shadow state (data-model.md §2.3).
#[derive(Debug, Clone)]
pub struct ChainModel {
    /// In processing order — this *is* the order, unlike the RT's
    /// separate slot pool + `order` permutation.
    nodes: Vec<NodeModel>,
    /// Bit `i` set ⇒ RT slot `i` is free.
    free_slots: u16,
    next_id: u32,
    source_rate: u32,
    /// Bumped on add/remove/move/bypass/auto-bypass/orphan/readopt (009
    /// C3) — the controller compares this against its own last-seen
    /// value once per `tick()` to decide whether to fan out
    /// `effect_chain_changed`.
    revision: u64,
}

/// `kind`'s catalog position of `id`, or `None` if `kind` has no such
/// parameter.
fn param_index(kind: NodeKind, id: ParamId) -> Option<usize> {
    catalog::params(kind).iter().position(|p| p.id == id)
}

/// The mode-carrying `ParamId` for a `PitchShift`/`TimeStretch` kind
/// (data-model.md §1.3), or `None` for any other kind.
const fn mode_param_id(kind: NodeKind) -> Option<ParamId> {
    match kind {
        NodeKind::PitchShift => Some(ParamId(2)),
        NodeKind::TimeStretch => Some(ParamId(1)),
        _ => None,
    }
}

/// 013-key-and-tempo-plugin (research R3): a numeric write to a mode
/// `ParamId` (from a plugin's `set_param`, numeric or wire-name form, or
/// any other future caller) resolves to the nearest valid `QualityMode`
/// index — `NaN`/negative defensively becomes `Performance`, mirroring
/// `catalog::clamp`'s own discrete-rounding rule.
fn quality_mode_from_value(value: f32) -> QualityMode {
    if value.round().max(0.0) as u8 >= 1 {
        QualityMode::Quality
    } else {
        QualityMode::Performance
    }
}

impl ChainModel {
    /// An empty chain, every slot free (FR-002: session-scoped, empty at
    /// launch).
    #[must_use]
    pub const fn new(source_rate: u32) -> Self {
        Self {
            nodes: Vec::new(),
            free_slots: u16::MAX,
            next_id: 0,
            source_rate,
            revision: 0,
        }
    }

    /// Monotonic, bumped on every structural/ownership change (009 C3).
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    fn allocate_slot(&mut self) -> Option<u8> {
        if self.free_slots == 0 {
            return None;
        }
        let slot = self.free_slots.trailing_zeros() as u8;
        self.free_slots &= !(1u16 << slot);
        Some(slot)
    }

    fn release_slot(&mut self, slot: u8) {
        self.free_slots |= 1u16 << slot;
    }

    fn find(&self, id: NodeId) -> Result<usize, ChainError> {
        self.nodes
            .iter()
            .position(|n| n.id == id)
            .ok_or(ChainError::UnknownNode)
    }

    /// G1: `Err(Full)` beyond `MAX_NODES`, model and slot set unchanged.
    /// Appends at the end — equivalent to `add_at(kind, owner,
    /// self.nodes().len())`.
    pub fn add(
        &mut self,
        kind: NodeKind,
        owner: NodeOwner,
    ) -> Result<(NodeId, Vec<Command>), ChainError> {
        self.add_at(kind, owner, self.nodes.len())
    }

    /// As [`Self::add`], but inserts at `index` (clamped to the current
    /// length) instead of always appending — the plugin `create_node`
    /// path (009 contracts/plugin-api-v1.md §3), via
    /// [`Self::resolve_position`].
    pub fn add_at(
        &mut self,
        kind: NodeKind,
        owner: NodeOwner,
        index: usize,
    ) -> Result<(NodeId, Vec<Command>), ChainError> {
        if self.nodes.len() >= MAX_NODES {
            return Err(ChainError::Full);
        }
        let Some(slot) = self.allocate_slot() else {
            return Err(ChainError::Full);
        };
        // G2: ids are monotonic and never reused; slots are (the lowest
        // free one, per `allocate_slot`).
        let id = NodeId(self.next_id);
        self.next_id += 1;
        let params: Vec<f32> = catalog::params(kind).iter().map(|p| p.default).collect();
        let mode_state =
            matches!(kind, NodeKind::PitchShift | NodeKind::TimeStretch).then_some(ModeState {
                mode: QualityMode::Performance,
                auto_switched: false,
            });
        let position = index.min(self.nodes.len());
        self.nodes.insert(
            position,
            NodeModel {
                id,
                slot,
                kind,
                owner,
                params,
                bypassed: false,
                auto_bypassed: false,
                mode_state,
                orphaned: false,
            },
        );
        self.revision += 1;
        Ok((
            id,
            vec![Command::ChainInsert {
                slot,
                position: position as u8,
                kind,
                owner,
            }],
        ))
    }

    pub fn remove(&mut self, id: NodeId) -> Result<Vec<Command>, ChainError> {
        let idx = self.find(id)?;
        let node = self.nodes.remove(idx);
        self.release_slot(node.slot);
        self.revision += 1;
        Ok(vec![Command::ChainRemove { slot: node.slot }])
    }

    pub fn move_to(&mut self, id: NodeId, index: usize) -> Result<Vec<Command>, ChainError> {
        let cur = self.find(id)?;
        let target = index.min(self.nodes.len().saturating_sub(1));
        if target == cur {
            return Ok(Vec::new());
        }
        let node = self.nodes.remove(cur);
        let slot = node.slot;
        self.nodes.insert(target, node);
        self.revision += 1;
        Ok(vec![Command::ChainMove {
            slot,
            position: target as u8,
        }])
    }

    /// G6: a no-op (`Ok`, no commands) at either end.
    pub fn move_by(&mut self, id: NodeId, delta: i8) -> Result<Vec<Command>, ChainError> {
        let cur = self.find(id)?;
        let len = self.nodes.len();
        let target = if delta.is_negative() {
            cur.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            (cur + delta as usize).min(len.saturating_sub(1))
        };
        self.move_to(id, target)
    }

    /// G5: un-bypassing clears `auto_bypassed`.
    pub fn set_bypass(&mut self, id: NodeId, bypassed: bool) -> Result<Vec<Command>, ChainError> {
        let idx = self.find(id)?;
        let node = &mut self.nodes[idx];
        node.bypassed = bypassed;
        if !bypassed {
            node.auto_bypassed = false;
        }
        self.revision += 1;
        Ok(vec![Command::ChainSetBypass {
            slot: node.slot,
            bypassed,
        }])
    }

    /// G3/G4: clamps and stores the clamped value; runs the FR-008 rule
    /// on a `semitones`/`ratio` change, possibly emitting a second
    /// command for `mode`. Returns the clamped value (SC-005).
    ///
    /// 013-key-and-tempo-plugin (research R3): a write to `kind`'s own
    /// mode `ParamId` is not stored here at all — it delegates entirely
    /// to [`Self::set_mode`], so an explicit mode write (numeric or
    /// wire-name, from a plugin or any other caller) runs FR-008 rule 3
    /// and keeps `mode_state` and `params` in agreement (previously a
    /// numeric write to the mode id left `mode_state` stale). `revision`
    /// bumps iff the stored value or `mode_state` actually changed — a
    /// no-op write (same value) neither bumps nor, via the controller's
    /// per-tick diff, fans out `effect_chain_changed`.
    pub fn set_param(
        &mut self,
        id: NodeId,
        param: ParamId,
        requested: f32,
    ) -> Result<(f32, Vec<Command>), ChainError> {
        let idx = self.find(id)?;
        let kind = self.nodes[idx].kind;
        if mode_param_id(kind) == Some(param) {
            let mode = quality_mode_from_value(requested);
            let commands = self.set_mode(id, mode)?;
            return Ok((mode as u8 as f32, commands));
        }
        let Some(pos) = param_index(kind, param) else {
            return Err(ChainError::WrongKind);
        };
        let clamped = catalog::clamp(kind, param, requested, self.source_rate);
        let old = self.nodes[idx].params[pos];
        let mut changed = (clamped - old).abs() > f32::EPSILON;
        self.nodes[idx].params[pos] = clamped;
        let slot = self.nodes[idx].slot;
        let mut commands = vec![Command::ChainSetParam {
            slot,
            param,
            value: clamped,
        }];

        // G4: only `semitones` (PitchShift, id 0) / `ratio` (TimeStretch,
        // id 0) drive the auto-switch rule.
        let is_stage_param =
            param == ParamId(0) && matches!(kind, NodeKind::PitchShift | NodeKind::TimeStretch);
        if is_stage_param && let Some(state) = self.nodes[idx].mode_state {
            let new_state = mode_after_value_change(kind, clamped, state);
            if new_state != state {
                self.nodes[idx].mode_state = Some(new_state);
                changed = true;
                if let Some(mode_param) = mode_param_id(kind)
                    && let Some(mode_pos) = param_index(kind, mode_param)
                {
                    let value = new_state.mode as u8 as f32;
                    self.nodes[idx].params[mode_pos] = value;
                    commands.push(Command::ChainSetParam {
                        slot,
                        param: mode_param,
                        value,
                    });
                }
            }
        }
        if changed {
            self.revision += 1;
        }
        Ok((clamped, commands))
    }

    /// Explicit user mode choice (FR-008 rule 3); `Err(WrongKind)` for a
    /// kind without a mode (only `PitchShift`/`TimeStretch` have one).
    /// `revision` bumps iff `mode_state` actually changed (mode or
    /// `auto_switched`) — research R3.
    pub fn set_mode(&mut self, id: NodeId, mode: QualityMode) -> Result<Vec<Command>, ChainError> {
        let idx = self.find(id)?;
        let kind = self.nodes[idx].kind;
        let Some(old_state) = self.nodes[idx].mode_state else {
            return Err(ChainError::WrongKind);
        };
        let Some(mode_param) = mode_param_id(kind) else {
            return Err(ChainError::WrongKind);
        };
        let Some(pos) = param_index(kind, mode_param) else {
            return Err(ChainError::WrongKind);
        };
        let new_state = mode_after_user_set(mode);
        self.nodes[idx].mode_state = Some(new_state);
        let value = mode as u8 as f32;
        self.nodes[idx].params[pos] = value;
        let slot = self.nodes[idx].slot;
        if new_state != old_state {
            self.revision += 1;
        }
        Ok(vec![Command::ChainSetParam {
            slot,
            param: mode_param,
            value,
        }])
    }

    /// G7 (FR-014): re-clamps every `nyquist_clamped` parameter of every
    /// node at the new rate, emitting a command only for a value that
    /// actually changed. `revision` bumps once iff any command was
    /// emitted (research R3: an 008 FR-014 recomputation is a
    /// parameter-target change).
    pub fn set_source_rate(&mut self, rate: u32) -> Vec<Command> {
        self.source_rate = rate;
        let mut commands = Vec::new();
        for node in &mut self.nodes {
            for (pos, def) in catalog::params(node.kind).iter().enumerate() {
                let ParamShape::Continuous {
                    nyquist_clamped: true,
                    ..
                } = def.shape
                else {
                    continue;
                };
                let old = node.params[pos];
                let clamped = catalog::clamp(node.kind, def.id, old, rate);
                if (clamped - old).abs() > f32::EPSILON {
                    node.params[pos] = clamped;
                    commands.push(Command::ChainSetParam {
                        slot: node.slot,
                        param: def.id,
                        value: clamped,
                    });
                }
            }
        }
        if !commands.is_empty() {
            self.revision += 1;
        }
        commands
    }

    /// From `Event::AutoBypassed { slot }` (the RT's overload state
    /// machine); a no-op if `slot` no longer maps to a node.
    pub fn mark_auto_bypassed(&mut self, slot: u8) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.slot == slot) {
            node.bypassed = true;
            node.auto_bypassed = true;
            self.revision += 1;
        }
    }

    /// L7e (009 FR-012 order): mark every node `owner` owns as orphaned —
    /// it keeps running with its last parameters, the RT is untouched.
    /// Returns the affected ids for the caller's own bookkeeping (e.g.
    /// deciding whether anything changed).
    pub fn orphan_owned_by(&mut self, owner: PluginId) -> Vec<NodeId> {
        let mut affected = Vec::new();
        for node in &mut self.nodes {
            if node.owner == NodeOwner::Plugin(owner) && !node.orphaned {
                node.orphaned = true;
                affected.push(node.id);
            }
        }
        if !affected.is_empty() {
            self.revision += 1;
        }
        affected
    }

    /// L4: when `owner` calls `ready()` again, re-adopt every node it
    /// still owns (ids and owner unchanged — only the `orphaned` flag
    /// clears; contracts/plugin-api-v1.md §6).
    pub fn readopt(&mut self, owner: PluginId) -> Vec<NodeId> {
        let mut affected = Vec::new();
        for node in &mut self.nodes {
            if node.owner == NodeOwner::Plugin(owner) && node.orphaned {
                node.orphaned = false;
                affected.push(node.id);
            }
        }
        if !affected.is_empty() {
            self.revision += 1;
        }
        affected
    }

    /// C2: who owns `id`, or `None` if it does not exist.
    #[must_use]
    pub fn owned_by(&self, id: NodeId) -> Option<NodeOwner> {
        self.nodes.iter().find(|n| n.id == id).map(|n| n.owner)
    }

    /// This kind's wire name, exactly as `plugin.toml`'s `[[effect_nodes]]`
    /// and `create_node`'s `kind` argument spell it (contracts/
    /// manifest.md §2, plugin-api-v1.md §3). `pub(crate)`: also
    /// `PlaybackController::fan_out_revision_events`'s own `NodeInfo.kind`
    /// projection (US2 T087).
    #[must_use]
    pub(crate) const fn wire_name(kind: NodeKind) -> &'static str {
        match kind {
            NodeKind::PitchShift => "pitch_shift",
            NodeKind::TimeStretch => "time_stretch",
            NodeKind::Gain => "gain",
            NodeKind::Equalizer => "equalizer",
            NodeKind::Filter => "filter",
            NodeKind::StereoTools => "stereo_tools",
        }
    }

    /// Resolves a manifest/`create_node` suggested position against the
    /// *current* chain (data-model.md §1.2): `Index(n)` clamps to
    /// `0..=len()`; `Before`/`After` match the first node whose kind's
    /// wire name equals the given string, falling back to the end of the
    /// chain when no such node exists (the position is only ever a
    /// suggestion).
    #[must_use]
    pub fn resolve_position(&self, suggested: &SuggestedPosition) -> usize {
        let len = self.nodes.len();
        match suggested {
            SuggestedPosition::Index(index) => (*index).min(len),
            SuggestedPosition::Before(name) => self
                .nodes
                .iter()
                .position(|n| Self::wire_name(n.kind) == name)
                .unwrap_or(len),
            SuggestedPosition::After(name) => self
                .nodes
                .iter()
                .position(|n| Self::wire_name(n.kind) == name)
                .map_or(len, |i| i + 1),
        }
    }

    /// G9: the earliest `TimeStretch` node in processing order, bypass
    /// ignored (FR-017's `tempo_step` target).
    #[must_use]
    pub fn first_time_stretch(&self) -> Option<NodeId> {
        self.nodes
            .iter()
            .find(|n| n.kind == NodeKind::TimeStretch)
            .map(|n| n.id)
    }

    #[must_use]
    pub fn node_by_slot(&self, slot: u8) -> Option<&NodeModel> {
        self.nodes.iter().find(|n| n.slot == slot)
    }

    /// G8: every command needed to rebuild the RT copy from scratch, in
    /// processing order (research R11) — an insert per node, a
    /// `ChainSetParam` for every value that differs from its default, and
    /// a `ChainSetBypass` for a bypassed node.
    #[must_use]
    pub fn replay(&self) -> Vec<Command> {
        let mut commands = Vec::new();
        for (position, node) in self.nodes.iter().enumerate() {
            commands.push(Command::ChainInsert {
                slot: node.slot,
                position: position as u8,
                kind: node.kind,
                owner: node.owner,
            });
            for (pos, def) in catalog::params(node.kind).iter().enumerate() {
                let value = node.params[pos];
                if (value - def.default).abs() > f32::EPSILON {
                    commands.push(Command::ChainSetParam {
                        slot: node.slot,
                        param: def.id,
                        value,
                    });
                }
            }
            if node.bypassed {
                commands.push(Command::ChainSetBypass {
                    slot: node.slot,
                    bypassed: true,
                });
            }
        }
        commands
    }

    #[must_use]
    pub fn nodes(&self) -> &[NodeModel] {
        &self.nodes
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        MAX_NODES
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn add_appends_at_catalog_defaults() {
        let mut model = ChainModel::new(44_100);
        let (id, commands) = model.add(NodeKind::Gain, NodeOwner::Host).expect("add");
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands[0],
            Command::ChainInsert { position: 0, .. }
        ));
        let node = model.nodes().iter().find(|n| n.id == id).expect("node");
        assert_eq!(node.params, vec![0.0, 0.0]);
    }

    // `add_17th_is_refused_without_corruption` (G1),
    // `ids_never_reused_slots_are` (G2), proptest
    // `set_param_returns_and_stores_clamped` (G3), `move_by_at_ends_is_noop`
    // (G6), `rate_change_reclamps_only_frequencies` (G7),
    // `replay_is_complete_and_ordered` (G8), `first_time_stretch_ignores_bypass`
    // (G9) live in `tests/controller_effects.rs`.

    #[test]
    fn auto_switch_emits_mode_command() {
        let mut model = ChainModel::new(44_100);
        let (id, _) = model
            .add(NodeKind::PitchShift, NodeOwner::Host)
            .expect("add");
        let (_, commands) = model.set_param(id, ParamId(0), 7.0).expect("set_param");
        assert_eq!(commands.len(), 2, "value + mode commands");
        let node = model.nodes().iter().find(|n| n.id == id).expect("node");
        assert!(node.mode_state.expect("mode_state").auto_switched);
    }
}

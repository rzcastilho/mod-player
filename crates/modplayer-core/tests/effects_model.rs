// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `ChainModel` owner/position tests (009, data-model.md §2, FR-012,
//! contracts/plugin-api-v1.md §3 `create_node`'s suggested position,
//! contracts/plugin-host-service.md §7 orphan/re-adopt). The rest of
//! `ChainModel`'s behaviour (G1-G9) is covered by `controller_effects.rs`
//! from 008; this file only adds the 009 deltas.

use modplayer_capability_gateway::manifest::SuggestedPosition;
use modplayer_core::effects::ChainModel;
use modplayer_effects::catalog::{NodeKind, NodeOwner, PluginId};

/// `resolve_position` (data-model.md §1.2): `Index(n)` clamps to
/// `0..=len()`; `Before`/`After` match the first node whose kind's wire
/// name equals the given string, falling back to the end of the chain
/// when no such node exists; and `add_at` actually inserts at whatever
/// index it resolved to, shifting the nodes that were already there.
#[test]
fn add_at_resolves_before_after_index_or_appends() {
    let mut model = ChainModel::new(44_100);
    let (gain_id, _) = model
        .add_at(NodeKind::Gain, NodeOwner::Host, 0)
        .unwrap_or_else(|e| unreachable!("{e}"));

    // Index beyond the current length clamps to append.
    assert_eq!(
        model.resolve_position(&SuggestedPosition::Index(99)),
        1,
        "Index clamps to len()"
    );
    // Before/After an existing kind resolve to its index / index + 1.
    assert_eq!(
        model.resolve_position(&SuggestedPosition::Before("gain".to_string())),
        0
    );
    assert_eq!(
        model.resolve_position(&SuggestedPosition::After("gain".to_string())),
        1
    );
    // Before/After a kind that isn't in the chain fall back to the end.
    assert_eq!(
        model.resolve_position(&SuggestedPosition::Before("time_stretch".to_string())),
        1
    );
    assert_eq!(
        model.resolve_position(&SuggestedPosition::After("time_stretch".to_string())),
        1
    );

    // `Before("gain")` resolves to 0 — inserting there puts the new node
    // ahead of `gain`, not merely "a suggestion" that gets ignored.
    let before_gain = model.resolve_position(&SuggestedPosition::Before("gain".to_string()));
    let (pitch_id, commands) = model
        .add_at(NodeKind::PitchShift, NodeOwner::Host, before_gain)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert!(matches!(
        commands[0],
        modplayer_engine::Command::ChainInsert { position: 0, .. }
    ));
    assert_eq!(model.nodes()[0].id, pitch_id, "inserted before gain");
    assert_eq!(model.nodes()[1].id, gain_id, "gain shifted right");

    // `After("pitch_shift")` now resolves to 1 (right before gain), not
    // to the end of the (now two-node) chain.
    let after_pitch = model.resolve_position(&SuggestedPosition::After("pitch_shift".to_string()));
    assert_eq!(after_pitch, 1);
    let (ts_id, _) = model
        .add_at(NodeKind::TimeStretch, NodeOwner::Host, after_pitch)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(model.nodes()[0].id, pitch_id);
    assert_eq!(
        model.nodes()[1].id,
        ts_id,
        "inserted between pitch and gain"
    );
    assert_eq!(model.nodes()[2].id, gain_id);
}

/// L7e/L4 (009 FR-012, contracts/plugin-host-service.md §7):
/// `orphan_owned_by` flags only the calling owner's own, not-yet-orphaned
/// nodes (idempotent, no revision bump on a repeat call with nothing
/// left to flag); `readopt` clears the flag again for the same owner's
/// still-owned nodes. A host node and another plugin's node are never
/// touched by either call.
#[test]
fn orphan_and_readopt() {
    let mut model = ChainModel::new(44_100);
    let plugin = PluginId(7);
    let other = PluginId(9);

    let (owned_id, _) = model
        .add(NodeKind::Gain, NodeOwner::Plugin(plugin))
        .unwrap_or_else(|e| unreachable!("{e}"));
    let (other_id, _) = model
        .add(NodeKind::Filter, NodeOwner::Plugin(other))
        .unwrap_or_else(|e| unreachable!("{e}"));
    let (host_id, _) = model
        .add(NodeKind::Equalizer, NodeOwner::Host)
        .unwrap_or_else(|e| unreachable!("{e}"));

    assert_eq!(model.owned_by(owned_id), Some(NodeOwner::Plugin(plugin)));
    assert_eq!(model.owned_by(host_id), Some(NodeOwner::Host));

    let rev_before_orphan = model.revision();
    let affected = model.orphan_owned_by(plugin);
    assert_eq!(affected, vec![owned_id], "only the plugin's own node");
    assert!(
        model
            .nodes()
            .iter()
            .find(|n| n.id == owned_id)
            .unwrap_or_else(|| unreachable!())
            .orphaned
    );
    assert!(
        !model
            .nodes()
            .iter()
            .find(|n| n.id == other_id)
            .unwrap_or_else(|| unreachable!())
            .orphaned,
        "a different plugin's node is untouched"
    );
    assert!(
        !model
            .nodes()
            .iter()
            .find(|n| n.id == host_id)
            .unwrap_or_else(|| unreachable!())
            .orphaned,
        "the host's own node is never orphaned"
    );
    assert!(
        model.revision() > rev_before_orphan,
        "orphaning bumps the revision (009 C3)"
    );

    // Idempotent: nothing left of the plugin's to orphan, so no bump.
    let rev_after_first = model.revision();
    assert!(model.orphan_owned_by(plugin).is_empty());
    assert_eq!(model.revision(), rev_after_first);

    // `ready()` again: readopt returns the same node id, clears the flag,
    // owner and id both unchanged (contracts/plugin-api-v1.md §6).
    let rev_before_readopt = model.revision();
    let readopted = model.readopt(plugin);
    assert_eq!(readopted, vec![owned_id]);
    assert!(
        !model
            .nodes()
            .iter()
            .find(|n| n.id == owned_id)
            .unwrap_or_else(|| unreachable!())
            .orphaned
    );
    assert_eq!(model.owned_by(owned_id), Some(NodeOwner::Plugin(plugin)));
    assert!(model.revision() > rev_before_readopt);

    // Idempotent the other way: nothing orphaned left to re-adopt.
    let rev_after_readopt = model.revision();
    assert!(model.readopt(plugin).is_empty());
    assert_eq!(model.revision(), rev_after_readopt);

    // `owned_by` on a removed node is `None` — existence, not ownership.
    let _ = model
        .remove(owned_id)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(model.owned_by(owned_id), None);
}

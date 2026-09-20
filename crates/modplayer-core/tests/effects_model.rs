// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `ChainModel` owner/position tests (009, data-model.md §2, FR-012,
//! contracts/plugin-api-v1.md §3 `create_node`'s suggested position,
//! contracts/plugin-host-service.md §7 orphan/re-adopt). The rest of
//! `ChainModel`'s behaviour (G1-G9) is covered by `controller_effects.rs`
//! from 008; this file only adds the 009 deltas.

use std::path::PathBuf;

use modplayer_capability_gateway::manifest::SuggestedPosition;
use modplayer_core::effects::ChainModel;
use modplayer_effects::catalog::{self, NodeKind, NodeOwner, ParamId, PluginId, QualityMode};

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

// -- 013-key-and-tempo-plugin (API 1.4, research R1-R3) ---------------------

#[derive(Debug, serde::Deserialize)]
struct NodeKindDto {
    name: String,
    params: Vec<NodeKindParamDto>,
}

#[derive(Debug, serde::Deserialize)]
struct NodeKindParamDto {
    name: String,
}

#[derive(Debug, serde::Deserialize)]
struct SchemaDto {
    node_kind: Vec<NodeKindDto>,
}

fn kind_from_wire(name: &str) -> NodeKind {
    match name {
        "pitch_shift" => NodeKind::PitchShift,
        "time_stretch" => NodeKind::TimeStretch,
        "gain" => NodeKind::Gain,
        "filter" => NodeKind::Filter,
        "stereo_tools" => NodeKind::StereoTools,
        "equalizer" => NodeKind::Equalizer,
        other => unreachable!("unknown [[node_kind]] name '{other}' in v1.toml"),
    }
}

/// A `[[node_kind]]` param name, expanded for the equalizer's generic
/// `band<n>_*` template (`n` = 1..=8, contracts/plugin-api-v1.4.md §5)
/// into its 8 concrete names; every other kind's name is used as-is.
fn expand_schema_param_name(name: &str) -> Vec<String> {
    if name.contains("<n>") {
        (1..=8)
            .map(|n| name.replace("<n>", &n.to_string()))
            .collect()
    } else {
        vec![name.to_string()]
    }
}

/// research R1/R2 (contract plugin-api-v1.4.md §5, §8): the wire names
/// `crates/modplayer-effects/src/catalog.rs`'s `param_wire_name` produces
/// for every `NodeKind` equal exactly the documentary `[[node_kind]]`
/// table in `api/v1.toml` — the reference and the runtime cannot diverge
/// (Constitution IX).
#[test]
fn wire_names_match_api_schema() {
    let schema_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../modplayer-capability-gateway/api/v1.toml");
    let text =
        std::fs::read_to_string(&schema_path).unwrap_or_else(|e| unreachable!("read v1.toml: {e}"));
    let schema: SchemaDto =
        toml::from_str(&text).unwrap_or_else(|e| unreachable!("parse v1.toml: {e}"));
    assert_eq!(schema.node_kind.len(), 6, "six built-in node kinds");

    for entry in &schema.node_kind {
        let kind = kind_from_wire(&entry.name);
        let mut expected: Vec<String> = entry
            .params
            .iter()
            .flat_map(|p| expand_schema_param_name(&p.name))
            .collect();
        expected.sort();

        let mut actual: Vec<String> = catalog::params(kind)
            .iter()
            .filter_map(|def| catalog::param_wire_name(kind, def.id))
            .map(str::to_string)
            .collect();
        actual.sort();

        assert_eq!(
            expected, actual,
            "{} wire names disagree between v1.toml and the catalog",
            entry.name
        );
    }
}

/// research R2: `param_by_wire_name(kind, param_wire_name(kind, id))` is
/// `Some(id)` for every parameter of every kind — the name lookup exactly
/// inverts.
#[test]
fn wire_name_round_trip_every_kind() {
    for kind in NodeKind::ALL {
        for def in catalog::params(kind) {
            let name = catalog::param_wire_name(kind, def.id)
                .unwrap_or_else(|| unreachable!("{kind:?} {:?} has no wire name", def.id));
            assert_eq!(
                catalog::param_by_wire_name(kind, name),
                Some(def.id),
                "{kind:?}/{name} does not round-trip"
            );
        }
    }
}

/// research R3: `revision` bumps by exactly one on a changed parameter
/// target, is untouched by a same-value write, and bumps again when the
/// FR-008 auto-switch rule flips `mode_state`.
#[test]
fn set_param_bumps_revision_only_on_change() {
    let mut model = ChainModel::new(44_100);
    let (id, _) = model
        .add(NodeKind::Gain, NodeOwner::Host)
        .unwrap_or_else(|e| unreachable!("{e}"));

    let rev0 = model.revision();
    let _ = model
        .set_param(id, ParamId(0), -6.0)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(model.revision(), rev0 + 1, "a changed value bumps once");

    let rev1 = model.revision();
    let _ = model
        .set_param(id, ParamId(0), -6.0)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(model.revision(), rev1, "the same value again does not bump");

    // A stage-use excursion that also flips `mode_state` (FR-008 rule 1)
    // still bumps exactly once — not twice for "value changed" and "mode
    // changed" separately.
    let (pid, _) = model
        .add(NodeKind::PitchShift, NodeOwner::Host)
        .unwrap_or_else(|e| unreachable!("{e}"));
    let rev2 = model.revision();
    let _ = model
        .set_param(pid, ParamId(0), 7.0)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(
        model.revision(),
        rev2 + 1,
        "value change + auto-switch is one bump, not two"
    );
}

/// research R3: a numeric `set_param` write to the mode-carrying
/// `ParamId` (`quality_mode`) delegates entirely to `set_mode` — it runs
/// FR-008 rule 3 (clears `auto_switched`) and keeps `mode_state`/`params`
/// in agreement, rather than leaving `mode_state` stale as a direct
/// `params[pos]` write would.
#[test]
fn set_param_on_mode_id_delegates_to_set_mode() {
    let mut model = ChainModel::new(44_100);
    let (id, _) = model
        .add(NodeKind::PitchShift, NodeOwner::Host)
        .unwrap_or_else(|e| unreachable!("{e}"));

    // A stage-use excursion auto-switches to Quality.
    let _ = model
        .set_param(id, ParamId(0), 7.0)
        .unwrap_or_else(|e| unreachable!("{e}"));
    let auto = model
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.mode_state)
        .unwrap_or_else(|| unreachable!());
    assert!(auto.auto_switched, "value excursion auto-switches");

    // An explicit numeric write to the mode `ParamId` (2, per data-model.md
    // §1.3) clears `auto_switched` and sets the mode, both in
    // `mode_state` and in `params`.
    let (clamped, _) = model
        .set_param(id, ParamId(2), 0.0)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(clamped, QualityMode::Performance as u8 as f32);
    let node = model
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| unreachable!());
    let state = node.mode_state.unwrap_or_else(|| unreachable!());
    assert_eq!(state.mode, QualityMode::Performance);
    assert!(
        !state.auto_switched,
        "an explicit mode write always clears auto_switched"
    );
    let mode_pos = catalog::params(NodeKind::PitchShift)
        .iter()
        .position(|p| p.id == ParamId(2))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(node.params[mode_pos], QualityMode::Performance as u8 as f32);
}

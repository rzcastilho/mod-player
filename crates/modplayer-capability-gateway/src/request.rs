// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Request`/`Response`: what crosses the boundary from a plugin's Lua
//! call to the host, and what comes back (data-model.md §1.4, contracts/
//! plugin-api-v1.md §3).
//!
//! The read-model types (`MarkerInfo`, `NodeInfo`, `QueueItemInfo`,
//! `OwnerInfo` and the id newtypes) derive `Serialize` so the runtime
//! crate can hand them to `mlua`'s `LuaSerdeExt::to_value` and get a Lua
//! table for free, rather than hand-writing a table builder per DTO.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::ui::{
    ActionSpec, NotifyLevel, OverlayPrimitive, SettingsField, UiId, WidgetSpec, WidgetValue,
};

/// A marker's cross-boundary identity (mirrors `modplayer_core::markers::
/// MarkerId`; the core crate converts at the boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct MarkerId(pub u32);

/// A loop region's cross-boundary identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct RegionId(pub u32);

/// An effect node's cross-boundary identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct NodeId(pub u32);

/// A queue item's cross-boundary identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct QueueItemId(pub u32);

/// Where a suggested effect-node insertion point resolves against the
/// current chain (manifest.md §2, data-model.md §1.2); re-exported here
/// since both the manifest's `[[effect_nodes]]` and a live `create_node`
/// call share the exact same shape.
pub use crate::manifest::SuggestedPosition;

/// Who owns a marker, region, cue or effect node, as seen by a plugin
/// (data-model.md §1.4). `Me` is reserved for a future "this plugin"
/// shorthand and is never produced by this slice's host code (always
/// `Plugin(identifier)` for the caller's own items).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OwnerInfo {
    Host,
    Plugin(String),
    Me,
}

/// One marker or loop-region endpoint, as reported to a plugin (contract
/// §3 `markers.list`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarkerInfo {
    pub id: MarkerId,
    pub kind: String,
    pub position_ms: u64,
    pub name: String,
    pub color: u8,
    pub owner: OwnerInfo,
    pub transient: bool,
    pub region: Option<RegionId>,
    pub slot: Option<u8>,
}

/// One effect node, as reported to a plugin (contract §3 `effects.
/// list_chain`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeInfo {
    pub id: NodeId,
    pub kind: String,
    pub owner: OwnerInfo,
    pub bypassed: bool,
    pub auto_bypassed: bool,
    pub orphaned: bool,
    pub index: usize,
}

/// One queue row, as reported to a plugin (contract §3 `queue.list`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueItemInfo {
    pub id: QueueItemId,
    pub track: String,
    pub title: String,
    pub index: usize,
    pub is_current: bool,
}

/// Every request a plugin may make, exactly the [`crate::api::
/// RequestKind`] set with arguments attached (data-model.md §1.4).
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    // -- playback.observe (local; no RPC) --------------------------------
    SubscribePosition {
        rate_hz: u8,
    },
    PlaybackState,

    // -- transport.control (needs_focus unless noted) ---------------------
    Play,
    Pause,
    Toggle,
    Seek {
        position_ms: u64,
    },
    SkipNext,
    SkipPrevious,
    /// 011-plugin-ui-contributions (R16, FR-026): `interaction` is `true`
    /// when this call was made synchronously inside a `panel_interaction`/
    /// `action_invoked` handler — the plugin-thread-only flag `transport.
    /// request_focus`'s binding reads (was a unit variant before this
    /// slice).
    RequestFocus {
        interaction: bool,
    },
    ReleaseFocus,
    ArmLoop {
        region: RegionId,
    },
    DisarmLoop,

    // -- queue.write (never focus-gated) -----------------------------------
    QueueList,
    QueueMove {
        item: QueueItemId,
        to: usize,
    },
    QueueRemove {
        item: QueueItemId,
    },
    QueuePlayNext {
        item: QueueItemId,
    },
    QueueAdd {
        track: String,
    },

    // -- markers.read / markers.write --------------------------------------
    ListMarkers,
    CreateMarker {
        position_ms: u64,
        name: Option<String>,
        transient: bool,
    },
    MoveMarker {
        id: MarkerId,
        position_ms: u64,
    },
    RenameMarker {
        id: MarkerId,
        name: String,
    },
    RecolorMarker {
        id: MarkerId,
        color: u8,
    },
    DeleteMarker {
        id: MarkerId,
    },
    CreateLoopRegion {
        a_ms: u64,
        b_ms: u64,
        transient: bool,
    },
    SetCue {
        slot: u8,
        position_ms: u64,
    },

    // -- audio.effects ------------------------------------------------------
    ListChain,
    CreateNode {
        kind: String,
        suggested: Option<SuggestedPosition>,
    },
    SetParam {
        node: NodeId,
        param: u8,
        value: f32,
    },
    ScheduleParam {
        node: NodeId,
        param: u8,
        value: f32,
        at_ms: u64,
    },
    SetBypass {
        node: NodeId,
        bypassed: bool,
    },
    RemoveNode {
        node: NodeId,
    },

    // -- state.plugin / state.track (local to the plugin thread, R9) --------
    StateGet {
        scope: crate::state::store::Scope,
        key: String,
    },
    StateSet {
        scope: crate::state::store::Scope,
        key: String,
        value: serde_json::Value,
    },
    StateRemove {
        scope: crate::state::store::Scope,
        key: String,
    },

    // -- timers (thread-local, RT9) ------------------------------------------
    SetTimeout {
        ms: u64,
    },
    SetInterval {
        ms: u64,
    },
    ScheduleAtPosition {
        position_ms: u64,
    },
    ClearTimer {
        handle: u32,
    },

    // -- fixture-only ---------------------------------------------------------
    DebugProbe {
        name: String,
    },

    // -- ui.panel / ui.overlay / ui.shortcuts / ui.settings / ui.notify -------
    // (011-plugin-ui-contributions, contracts/plugin-api-v1.2.md §3): every
    // one of these is an RPC except `GetSettings`, which the runtime crate
    // serves locally and never sends across (research R4).
    RegisterPanel {
        panel: UiId,
        title: String,
        widgets: Vec<WidgetSpec>,
    },
    UpdateWidget {
        panel: UiId,
        widget: UiId,
        value: WidgetValue,
    },
    AddOverlays {
        primitives: Vec<OverlayPrimitive>,
    },
    RemoveOverlays {
        ids: Vec<UiId>,
    },
    ClearOverlays,
    RegisterAction {
        action: ActionSpec,
    },
    /// `stored` is the runtime's `Scope::Settings` snapshot at
    /// registration time (R5), so core can render the page without a
    /// second round trip.
    RegisterSettings {
        fields: Vec<SettingsField>,
        stored: BTreeMap<String, serde_json::Value>,
    },
    /// Never sent as an RPC (R4) — kept in [`Request`] only so
    /// [`crate::api::RequestKind::GetSettings`] has a matching variant for
    /// the exhaustive dispatch tables that key off it.
    GetSettings,
    Notify {
        level: NotifyLevel,
        text: String,
    },
}

/// What a [`Request`] returns on success (data-model.md §1.4).
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Ok,
    MarkerId(MarkerId),
    RegionId(RegionId),
    NodeId(NodeId),
    Markers {
        markers: Vec<MarkerInfo>,
        armed: Option<RegionId>,
    },
    Chain(Vec<NodeInfo>),
    Queue(Vec<QueueItemInfo>),
    /// `state.plugin.get`/`state.track.get`'s result: the value, or
    /// `None` for an unset key.
    StoreValue(Option<serde_json::Value>),
    /// A newly allocated (or referenced) timer handle.
    TimerHandle(u32),
    Probe(serde_json::Value),
    /// `get_settings()`'s result (contract §3.5): field id -> current (or
    /// default-substituted) value.
    Settings(BTreeMap<String, serde_json::Value>),
}

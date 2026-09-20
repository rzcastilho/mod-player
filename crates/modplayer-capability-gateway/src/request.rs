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

/// Which endpoint of a loop region `markers.set_loop_endpoint` targets
/// (contract plugin-api-v1.3.md §3.1). Wire: `"a"` | `"b"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopEndpoint {
    A,
    B,
}

/// A region's repeat count (contract plugin-api-v1.3.md §3.2). Wire:
/// an integer `1..=1000`, or the string `"infinite"`. `#[serde(untagged)]`
/// so `Times(n)` serialises as a bare number and `Infinite` as the
/// literal string, matching `RepeatCount`'s existing host-side domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum RepeatArg {
    Times(u16),
    Infinite,
}

/// One loop region (both endpoints, if present, plus its repeat count
/// and arm state), as reported to a plugin (contract plugin-api-v1.3.md
/// §3.3 `markers.list().regions`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegionInfo {
    pub id: RegionId,
    /// Owner of `a`, else of `b` (both endpoints always share an owner).
    pub owner: OwnerInfo,
    pub a: Option<MarkerId>,
    pub b: Option<MarkerId>,
    pub repeat: RepeatArg,
    pub armed: bool,
}

/// A parameter's current target value as a plugin sees it (API 1.4,
/// data-model.md §1.2). `#[serde(untagged)]` so it serialises as a bare
/// Lua-facing scalar (number / boolean / string), never a tagged table.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ParamValue {
    Number(f64),
    Bool(bool),
    Name(String),
}

/// One effect node, as reported to a plugin (contract §3 `effects.
/// list_chain`). `NodeInfo` derives `PartialEq` only (not `Eq`, since
/// `params` carries an `f64`) — no caller relies on `Eq` (grep: only
/// `PartialEq` comparisons in tests).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeInfo {
    pub id: NodeId,
    pub kind: String,
    pub owner: OwnerInfo,
    pub bypassed: bool,
    pub auto_bypassed: bool,
    pub orphaned: bool,
    pub index: usize,
    /// API 1.4: wire name -> current clamped *target* value (never
    /// mid-ramp; data-model.md §1.2).
    pub params: BTreeMap<String, ParamValue>,
    /// API 1.4: 008 FR-008's flag for `pitch_shift`/`time_stretch`;
    /// `false` for every other kind.
    pub auto_switched: bool,
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

/// `set_param`/`schedule_param`'s `param` argument (API 1.4, data-model.md
/// §1.3): the 1.0-1.3 numeric id, or the wire name a plugin may now spell
/// it with (research R2). Resolved against the catalog in
/// `modplayer-core`'s `plugins::apply` (never in this crate or the
/// runtime — the gateway has no dependency on the effects catalog by
/// design).
#[derive(Debug, Clone, PartialEq)]
pub enum ParamRef {
    Id(u8),
    Name(String),
}

/// `set_param`/`schedule_param`'s `value` argument (API 1.4, data-model.md
/// §1.3): the 1.0-1.3 numeric form, or a boolean/enum-name value for a
/// `boolean`/`enum`-shaped parameter (research R2).
#[derive(Debug, Clone, PartialEq)]
pub enum ParamArg {
    Number(f32),
    Bool(bool),
    Name(String),
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
    /// `region = None` creates a new caller-owned region holding only
    /// `which` (data-model.md §1.2, contract plugin-api-v1.3.md §3.1).
    SetLoopEndpoint {
        region: Option<RegionId>,
        which: LoopEndpoint,
        position_ms: u64,
    },
    SetLoopRepeat {
        region: RegionId,
        repeat: RepeatArg,
    },

    // -- audio.effects ------------------------------------------------------
    ListChain,
    CreateNode {
        kind: String,
        suggested: Option<SuggestedPosition>,
    },
    SetParam {
        node: NodeId,
        param: ParamRef,
        value: ParamArg,
    },
    ScheduleParam {
        node: NodeId,
        param: ParamRef,
        value: ParamArg,
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
        regions: Vec<RegionInfo>,
    },
    /// `set_loop_endpoint`'s `region, marker` result (data-model.md §1.3).
    LoopEndpoint {
        region: RegionId,
        marker: MarkerId,
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

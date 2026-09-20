// SPDX-License-Identifier: MIT OR Apache-2.0

//! `drain_plugin_requests()` (C1): the one dispatch table every admitted
//! plugin `Request` runs through before its reply crosses back to the
//! plugin thread — always replies, in arrival order. Exhaustive over
//! `Request` (which mirrors `RequestKind` 1:1) so a schema addition
//! without a matching arm fails to compile. Every branch this slice's
//! Foundational phase doesn't implement yet returns a placeholder
//! `invalid_state` refusal; its own user-story task (contracts/
//! plugin-host-service.md §3) fills the arm in.

use std::time::Duration;

use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{SourceHost, TrackId};
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{
    MarkerId as GatewayMarkerId, NodeId as GatewayNodeId, QueueItemId as GatewayQueueItemId,
    RegionId as GatewayRegionId, Request, Response,
};
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_plugin_runtime::handle::Control;

use crate::PlaybackController;
use crate::effects::{ChainError, ChainModel, NodeId as CoreNodeId};
use crate::markers::{
    CueSlot, MarkerError, MarkerId as CoreMarkerId, Owner, PaletteIndex, RegionId as CoreRegionId,
};
use crate::queue::QueueItemId as CoreQueueItemId;
use crate::transport::Intent;

use super::{FocusHolder, PluginId, TransportActor};

/// `CreateNode`'s `kind` argument (contracts/plugin-api-v1.md §3): the
/// wire name back to a `NodeKind`, `None` for anything else
/// (`unsupported_kind`). The inverse of `ChainModel::wire_name`.
fn parse_node_kind(kind: &str) -> Option<NodeKind> {
    NodeKind::ALL
        .into_iter()
        .find(|k| ChainModel::wire_name(*k) == kind)
}

/// `Queue*` (US3 T092, R21): resolve a gateway `QueueItemId` to the
/// current effective order's matching item, if any — the queue's own
/// `QueueItemId` is a private newtype outside `crate::queue`, so this is
/// the boundary conversion (mirrors `CoreMarkerId::from_raw`'s role for
/// markers).
fn find_queue_item<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    id: GatewayQueueItemId,
) -> Option<CoreQueueItemId> {
    controller
        .queue()
        .effective_order()
        .into_iter()
        .find(|item| item.uid.get() as u32 == id.0)
        .map(|item| item.uid)
}

/// `QueueAdd`'s own lookup (R21): a track already visible in the queue's
/// effective order (re-used as-is, richer metadata and all), else the
/// library index — `not_found` otherwise.
fn find_track_ref<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    id: &TrackId,
) -> Option<modplayer_audio_source::TrackRef> {
    controller
        .queue()
        .effective_order()
        .into_iter()
        .find(|item| &item.track.id == id)
        .map(|item| item.track.clone())
        .or_else(|| controller.library().track(id).cloned())
}

/// Every request kind US3's own tasks still need to implement
/// (contracts/plugin-host-service.md §3) refuses with this until then —
/// never reaches Lua as an error (contract §2: a request never throws).
fn not_yet_implemented() -> Refusal {
    Refusal::invalid_state(
        "invalid_state",
        "This request is not implemented in this build yet.",
    )
}

fn require_track<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
) -> Result<(), Refusal> {
    if controller.transport_enabled() {
        Ok(())
    } else {
        Err(Refusal::no_track())
    }
}

/// C1/R12: the host-side re-check ahead of every focus-gated request —
/// closes the admission→application race (a revoke landing in the one UI
/// frame between the plugin thread's own `Gateway::admit` check and this
/// request actually being drained) so FR-001's "no side effect" holds
/// exactly, not just usually.
fn require_focus<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
) -> Result<(), Refusal> {
    if controller.plugins_mut().arbiter().holder() == FocusHolder::Plugin(plugin) {
        Ok(())
    } else {
        Err(Refusal::no_focus())
    }
}

/// C2 (FR-024): maps a `MarkerError` from an owned mutation to its wire
/// `Refusal` — `NotOwner`/`NotFound` cover every T085 handler;
/// `RegionIncomplete`/`RegionTooShort` are `ArmLoop`'s own model errors
/// (contracts/plugin-api-v1.md §3).
fn marker_refusal(err: MarkerError) -> Refusal {
    match err {
        MarkerError::NotFound => Refusal::not_found(),
        MarkerError::NotOwner => Refusal::not_owner(),
        MarkerError::NoTrack => Refusal::no_track(),
        MarkerError::LimitReached => Refusal::invalid_state(
            "marker_limit",
            "The track already has the maximum number of markers.",
        ),
        MarkerError::RegionIncomplete => Refusal::invalid_state(
            "region_incomplete",
            "This loop region has no A/B endpoints yet.",
        ),
        MarkerError::RegionTooShort => {
            Refusal::invalid_state("region_too_short", "This loop region is too short to arm.")
        }
    }
}

/// As [`marker_refusal`], for a `ChainError` from an owned effect-node
/// mutation (US2 T086).
fn chain_refusal(err: ChainError) -> Refusal {
    match err {
        ChainError::UnknownNode => Refusal::not_found(),
        ChainError::Full => Refusal::invalid_state("chain_full", "The effect chain is full."),
        ChainError::WrongKind => Refusal::invalid_state(
            "invalid_argument",
            "That parameter does not exist for this node kind.",
        ),
    }
}

/// C2: who owns marker/region-endpoint `id`, or `None` if it does not
/// exist — existence and ownership are deliberately separate checks
/// (FR-008), left to [`require_owner`] to turn into the right refusal.
fn marker_owner<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    id: CoreMarkerId,
) -> Option<Owner> {
    controller.markers().and_then(|m| m.owner_of(id))
}

/// As [`marker_owner`], for a loop region: the owner of either endpoint
/// (both share the same owner — `new_loop_region_owned` creates them
/// together).
fn region_owner<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    id: CoreRegionId,
) -> Option<Owner> {
    let markers = controller.markers()?;
    let region = markers.region(id)?;
    region
        .a
        .and_then(|m| markers.owner_of(m))
        .or_else(|| region.b.and_then(|m| markers.owner_of(m)))
}

/// As [`marker_owner`], for an effect node (C2).
fn node_owner<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    id: CoreNodeId,
) -> Option<NodeOwner> {
    controller.chain().owned_by(id)
}

/// `not_found` when `owner` is `None` (the id does not exist); `not_owner`
/// when it exists but belongs to someone else; `Ok(())` when `plugin`
/// itself owns it (C2).
fn require_owner(owner: Option<Owner>, plugin: PluginId) -> Result<(), Refusal> {
    match owner {
        None => Err(Refusal::not_found()),
        Some(o) if o == Owner::Plugin(plugin) => Ok(()),
        Some(_) => Err(Refusal::not_owner()),
    }
}

/// As [`require_owner`], for an effect node's `NodeOwner`.
fn require_node_owner(owner: Option<NodeOwner>, plugin: PluginId) -> Result<(), Refusal> {
    match owner {
        None => Err(Refusal::not_found()),
        Some(NodeOwner::Plugin(p)) if p == plugin => Ok(()),
        Some(_) => Err(Refusal::not_owner()),
    }
}

/// C1: drain every queued `RpcEnvelope` in arrival order, always
/// replying — a dropped reply is a bug (`rpc_always_replied`).
pub fn drain_plugin_requests<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
) {
    loop {
        let Some(envelope) = controller.plugins_mut().try_recv_request() else {
            break;
        };
        let plugin = super::from_gateway_id(envelope.plugin);
        // 010-transport-focus (design note 5, C1): every admitted request
        // this plugin's own thread sent runs under `Plugin(id)` — the
        // auto-policy revoke hook only ever fires for a genuine local
        // user action, never for a plugin's own transport RPC.
        let response = controller.with_transport_actor(TransportActor::Plugin(plugin), |ctrl| {
            dispatch(ctrl, plugin, envelope.request)
        });
        let _ = envelope.reply.send(response);
    }
}

/// The dispatch table itself (C1). `Play`/`Pause`/`Toggle`/`Seek`/
/// `SkipNext`/`SkipPrevious` are implemented here (Phase 2: Foundational);
/// every other request kind is a later user story's task.
fn dispatch<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
    request: Request,
) -> Result<Response, Refusal> {
    match request {
        Request::Play => {
            require_track(controller)?;
            require_focus(controller, plugin)?;
            controller.play();
            Ok(Response::Ok)
        }
        Request::Pause => {
            require_track(controller)?;
            require_focus(controller, plugin)?;
            controller.pause();
            Ok(Response::Ok)
        }
        Request::Toggle => {
            require_track(controller)?;
            require_focus(controller, plugin)?;
            if controller.transport_state().intent == Intent::Playing {
                controller.pause();
            } else {
                controller.play();
            }
            Ok(Response::Ok)
        }
        Request::Seek { position_ms } => {
            require_track(controller)?;
            require_focus(controller, plugin)?;
            controller.seek(Duration::from_millis(position_ms));
            Ok(Response::Ok)
        }
        Request::SkipNext => {
            require_track(controller)?;
            require_focus(controller, plugin)?;
            controller.skip_forward();
            Ok(Response::Ok)
        }
        Request::SkipPrevious => {
            require_track(controller)?;
            require_focus(controller, plugin)?;
            controller.skip_back();
            Ok(Response::Ok)
        }

        // -- transport focus (010-transport-focus, C1): a plain
        // `request_focus`/`release_focus` RPC to core — always recorded,
        // never refused (FR-003/FR-004/FR-011); the arbiter decides
        // whether it grants immediately.
        Request::RequestFocus => {
            controller.focus_request(plugin);
            Ok(Response::Ok)
        }
        Request::ReleaseFocus => {
            controller.focus_release(plugin);
            Ok(Response::Ok)
        }

        // -- loop arm/disarm (C1: focus-gated) ----------------------------
        Request::ArmLoop { region } => {
            require_focus(controller, plugin)?;
            let region_id = CoreRegionId::from_raw(region.0);
            require_owner(region_owner(controller, region_id), plugin)?;
            controller.arm_loop(region_id).map_err(marker_refusal)?;
            controller.set_last_marker_actor(Owner::Plugin(plugin));
            Ok(Response::Ok)
        }
        Request::DisarmLoop => {
            require_focus(controller, plugin)?;
            let owned = controller.markers().and_then(|m| m.armed_region_owner())
                == Some(Owner::Plugin(plugin));
            if !owned {
                return Err(Refusal::invalid_state(
                    "nothing_armed",
                    "This plugin has no armed loop region to disarm.",
                ));
            }
            controller.disarm_loop().map_err(marker_refusal)?;
            controller.set_last_marker_actor(Owner::Plugin(plugin));
            Ok(Response::Ok)
        }

        // -- owned marker mutations (US2 T085, C2) ------------------------
        Request::MoveMarker { id, position_ms } => {
            let mid = CoreMarkerId::from_raw(id.0);
            require_owner(marker_owner(controller, mid), plugin)?;
            let frames = controller.ms_to_frames(u32::try_from(position_ms).unwrap_or(u32::MAX));
            controller
                .move_marker(mid, frames)
                .map_err(marker_refusal)?;
            controller.set_last_marker_actor(Owner::Plugin(plugin));
            Ok(Response::Ok)
        }
        Request::RenameMarker { id, name } => {
            let mid = CoreMarkerId::from_raw(id.0);
            require_owner(marker_owner(controller, mid), plugin)?;
            controller
                .rename_marker(mid, &name)
                .map_err(marker_refusal)?;
            controller.set_last_marker_actor(Owner::Plugin(plugin));
            Ok(Response::Ok)
        }
        Request::RecolorMarker { id, color } => {
            let mid = CoreMarkerId::from_raw(id.0);
            require_owner(marker_owner(controller, mid), plugin)?;
            controller
                .recolor_marker(mid, PaletteIndex::new(color))
                .map_err(marker_refusal)?;
            controller.set_last_marker_actor(Owner::Plugin(plugin));
            Ok(Response::Ok)
        }
        Request::DeleteMarker { id } => {
            let mid = CoreMarkerId::from_raw(id.0);
            require_owner(marker_owner(controller, mid), plugin)?;
            controller.delete_marker(mid).map_err(marker_refusal)?;
            controller.set_last_marker_actor(Owner::Plugin(plugin));
            Ok(Response::Ok)
        }
        Request::SetCue { slot, position_ms } => {
            let Some(cue_slot) = CueSlot::new(slot) else {
                return Err(Refusal::invalid_state(
                    "invalid_argument",
                    "Cue slots range 1..=8.",
                ));
            };
            let id = controller
                .plugin_set_cue(cue_slot, position_ms, Owner::Plugin(plugin))
                .map_err(marker_refusal)?;
            Ok(Response::MarkerId(GatewayMarkerId(id.raw())))
        }

        // -- owned effect-node mutations (US2 T086, C2) -------------------
        Request::SetParam { node, param, value } => {
            let nid = CoreNodeId::from_raw(node.0);
            require_node_owner(node_owner(controller, nid), plugin)?;
            controller
                .chain_set_param(nid, ParamId(param), value)
                .map_err(chain_refusal)?;
            Ok(Response::Ok)
        }
        // 009 contracts/plugin-api-v1.md §3: this slice applies
        // `schedule_param` immediately, through the same next-buffer ramp
        // as `set_param` — no absolute-time scheduling exists yet, so
        // `at_ms` is accepted and ignored (the wire shape won't need to
        // change once a later slice adds real scheduling).
        Request::ScheduleParam {
            node,
            param,
            value,
            at_ms: _,
        } => {
            let nid = CoreNodeId::from_raw(node.0);
            require_node_owner(node_owner(controller, nid), plugin)?;
            controller
                .chain_set_param(nid, ParamId(param), value)
                .map_err(chain_refusal)?;
            Ok(Response::Ok)
        }
        Request::SetBypass { node, bypassed } => {
            let nid = CoreNodeId::from_raw(node.0);
            require_node_owner(node_owner(controller, nid), plugin)?;
            controller
                .chain_set_bypass(nid, bypassed)
                .map_err(chain_refusal)?;
            Ok(Response::Ok)
        }
        Request::RemoveNode { node } => {
            let nid = CoreNodeId::from_raw(node.0);
            require_node_owner(node_owner(controller, nid), plugin)?;
            controller.chain_remove_node(nid).map_err(chain_refusal)?;
            Ok(Response::Ok)
        }
        Request::CreateNode { kind, suggested } => {
            let Some(kind) = parse_node_kind(&kind) else {
                return Err(Refusal::invalid_state(
                    "unsupported_kind",
                    "That effect node kind does not exist.",
                ));
            };
            let index = suggested
                .as_ref()
                .map_or(usize::MAX, |s| controller.chain().resolve_position(s));
            let id = controller
                .chain_add_node_owned(kind, NodeOwner::Plugin(plugin), index)
                .map_err(chain_refusal)?;
            Ok(Response::NodeId(GatewayNodeId(id.as_u32())))
        }

        // -- queue (US3 T092, R21; never focus-gated) ---------------------
        Request::QueueMove { item, to } => {
            let Some(uid) = find_queue_item(controller, item) else {
                return Err(Refusal::not_found());
            };
            controller.queue_reorder(uid, to);
            Ok(Response::Ok)
        }
        Request::QueueRemove { item } => {
            let Some(uid) = find_queue_item(controller, item) else {
                return Err(Refusal::not_found());
            };
            controller.queue_remove(uid);
            Ok(Response::Ok)
        }
        Request::QueuePlayNext { item } => {
            let Some(uid) = find_queue_item(controller, item) else {
                return Err(Refusal::not_found());
            };
            controller.queue_play_next(uid);
            Ok(Response::Ok)
        }
        Request::QueueAdd { track } => {
            let Ok(track_id) = TrackId::new(track) else {
                return Err(Refusal::not_found());
            };
            let Some(track_ref) = find_track_ref(controller, &track_id) else {
                return Err(Refusal::not_found());
            };
            controller.queue_play_next_track(track_ref);
            Ok(Response::Ok)
        }

        // -- owned marker/region creation (US3 T093, C2) -------------------
        Request::CreateMarker {
            position_ms,
            name,
            transient,
        } => {
            let id = controller
                .plugin_add_marker(
                    position_ms,
                    name.as_deref(),
                    Owner::Plugin(plugin),
                    transient,
                )
                .map_err(marker_refusal)?;
            Ok(Response::MarkerId(GatewayMarkerId(id.raw())))
        }
        Request::CreateLoopRegion {
            a_ms,
            b_ms,
            transient,
        } => {
            let id = controller
                .plugin_add_loop_region(a_ms, b_ms, Owner::Plugin(plugin), transient)
                .map_err(marker_refusal)?;
            Ok(Response::RegionId(GatewayRegionId(id.raw())))
        }

        // -- fixture-only debug probe (L10), forwarded to the plugin's own
        // thread as `Control::Probe` — used by the host-driven manual/test
        // path only: a plugin's own `api.debug_probe(name)` is answered
        // entirely locally by the runtime crate (bindings::install_debug_
        // probe) and never constructs this `Request` at all.
        Request::DebugProbe { name } => {
            if !controller.plugins_mut().fixtures_enabled() {
                return Err(Refusal::invalid_state(
                    "invalid_argument",
                    "Fixtures are not enabled.",
                ));
            }
            let Some(handle) = controller
                .plugins_mut()
                .record(plugin)
                .and_then(|r| r.handle.as_ref())
            else {
                return Err(Refusal::invalid_state(
                    "invalid_argument",
                    "This plugin has no running thread to probe.",
                ));
            };
            let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
            if !handle.send_control(Control::Probe {
                name,
                reply: reply_tx,
            }) {
                return Err(Refusal::invalid_state(
                    "invalid_argument",
                    "This plugin's inbox is full.",
                ));
            }
            match reply_rx.recv_timeout(Duration::from_millis(250)) {
                Ok(value) => Ok(Response::Probe(value)),
                Err(_) => Err(Refusal::host_busy()),
            }
        }

        // -- read-model requests answered entirely on the plugin's own
        // thread from `PluginSnapshot` (US3 T094, contracts/plugin-host-
        // service.md §3 "never reach core (snapshot)") — an admitted
        // `RpcEnvelope` for one of these is unreachable from any real
        // plugin call; kept as its own arm (rather than falling into the
        // wildcard below) purely for exhaustiveness/documentation.
        Request::ListMarkers | Request::ListChain | Request::QueueList => {
            Err(not_yet_implemented())
        }

        _ => Err(not_yet_implemented()),
    }
}

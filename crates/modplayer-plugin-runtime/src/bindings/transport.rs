// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.transport.*` (contract §3): play/pause/toggle/seek/skip, focus,
//! arm/disarm loop.
//!
//! `request_focus`/`release_focus` are ordinary RPCs to core like every
//! other call here (010-transport-focus research R2, contracts/
//! plugin-api-v1.1.md RT-F1): `bindings::dispatch` has no local arm for
//! them, so they fall through to `_ => rpc(..)` and always return
//! `Ok(Response::Ok)`; whether/when focus is actually granted is reported
//! only by the `focus_granted`/`focus_revoked` events.

use mlua::{Lua, Table};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::request::{RegionId, Request};

use super::{SharedHandle, call, lock};

macro_rules! simple_call {
    ($lua:expr, $ns:expr, $shared:expr, $name:literal, $kind:expr, $request:expr) => {{
        let shared = $shared.clone();
        let f = $lua.create_function(move |lua, ()| call(lua, &shared, $kind, $request))?;
        $ns.set($name, f)?;
    }};
}

pub fn install(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let ns = lua.create_table()?;

    simple_call!(
        lua,
        ns,
        shared,
        "play",
        RequestKind::TransportPlay,
        Request::Play
    );
    simple_call!(
        lua,
        ns,
        shared,
        "pause",
        RequestKind::TransportPause,
        Request::Pause
    );
    simple_call!(
        lua,
        ns,
        shared,
        "toggle",
        RequestKind::TransportToggle,
        Request::Toggle
    );
    simple_call!(
        lua,
        ns,
        shared,
        "skip_next",
        RequestKind::TransportSkipNext,
        Request::SkipNext
    );
    simple_call!(
        lua,
        ns,
        shared,
        "skip_previous",
        RequestKind::TransportSkipPrevious,
        Request::SkipPrevious
    );
    // 011-plugin-ui-contributions (R16, FR-026): unlike every other
    // `simple_call!` here, this reads `Shared.in_interaction_handler` at
    // call time rather than sending a fixed `Request` value — the flag is
    // only ever `true` while this call happens synchronously inside a
    // `panel_interaction`/`action_invoked` handler.
    let shared_focus = shared.clone();
    let request_focus_fn = lua.create_function(move |lua, ()| {
        let interaction = lock(&shared_focus).in_interaction_handler;
        call(
            lua,
            &shared_focus,
            RequestKind::TransportRequestFocus,
            Request::RequestFocus { interaction },
        )
    })?;
    ns.set("request_focus", request_focus_fn)?;
    simple_call!(
        lua,
        ns,
        shared,
        "release_focus",
        RequestKind::TransportReleaseFocus,
        Request::ReleaseFocus
    );
    simple_call!(
        lua,
        ns,
        shared,
        "disarm_loop",
        RequestKind::TransportDisarmLoop,
        Request::DisarmLoop
    );

    let shared_seek = shared.clone();
    let seek = lua.create_function(move |lua, position_ms: u64| {
        call(
            lua,
            &shared_seek,
            RequestKind::TransportSeek,
            Request::Seek { position_ms },
        )
    })?;
    ns.set("seek", seek)?;

    let shared_arm = shared.clone();
    let arm_loop = lua.create_function(move |lua, region_id: u32| {
        call(
            lua,
            &shared_arm,
            RequestKind::TransportArmLoop,
            Request::ArmLoop {
                region: RegionId(region_id),
            },
        )
    })?;
    ns.set("arm_loop", arm_loop)?;

    api.set("transport", ns)?;
    Ok(())
}

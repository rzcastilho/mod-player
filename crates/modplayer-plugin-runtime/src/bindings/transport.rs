// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.transport.*` (contract §3): play/pause/toggle/seek/skip, focus,
//! arm/disarm loop.

use mlua::{Lua, Table};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::request::{RegionId, Request};

use super::{SharedHandle, call};

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
    simple_call!(
        lua,
        ns,
        shared,
        "request_focus",
        RequestKind::TransportRequestFocus,
        Request::RequestFocus
    );
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

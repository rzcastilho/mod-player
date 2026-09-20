// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.playback.*`: `subscribe_position`/`state` — both local, no RPC
//! (contract §3).

use mlua::{Lua, Table};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::request::Request;

use super::{SharedHandle, call};

pub fn install(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let ns = lua.create_table()?;

    let shared1 = shared.clone();
    let subscribe = lua.create_function(move |lua, rate_hz: Option<u8>| {
        let rate = rate_hz.unwrap_or(10);
        call(
            lua,
            &shared1,
            RequestKind::PlaybackSubscribePosition,
            Request::SubscribePosition { rate_hz: rate },
        )
    })?;
    ns.set("subscribe_position", subscribe)?;

    let shared2 = shared.clone();
    let state = lua.create_function(move |lua, ()| {
        call(
            lua,
            &shared2,
            RequestKind::PlaybackState,
            Request::PlaybackState,
        )
    })?;
    ns.set("state", state)?;

    api.set("playback", ns)?;
    Ok(())
}

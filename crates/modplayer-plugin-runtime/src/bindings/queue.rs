// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.queue.*` (contract §3): list/move/remove/play_next/add.

use mlua::{Lua, Table};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::request::{QueueItemId, Request};

use super::{SharedHandle, call};

pub fn install(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let ns = lua.create_table()?;

    let shared_list = shared.clone();
    let list = lua.create_function(move |lua, ()| {
        call(
            lua,
            &shared_list,
            RequestKind::QueueList,
            Request::QueueList,
        )
    })?;
    ns.set("list", list)?;

    let shared_move = shared.clone();
    let move_fn = lua.create_function(move |lua, (item_id, to_index): (u32, usize)| {
        call(
            lua,
            &shared_move,
            RequestKind::QueueMove,
            Request::QueueMove {
                item: QueueItemId(item_id),
                to: to_index,
            },
        )
    })?;
    ns.set("move", move_fn)?;

    let shared_remove = shared.clone();
    let remove = lua.create_function(move |lua, item_id: u32| {
        call(
            lua,
            &shared_remove,
            RequestKind::QueueRemove,
            Request::QueueRemove {
                item: QueueItemId(item_id),
            },
        )
    })?;
    ns.set("remove", remove)?;

    let shared_play_next = shared.clone();
    let play_next = lua.create_function(move |lua, item_id: u32| {
        call(
            lua,
            &shared_play_next,
            RequestKind::QueuePlayNext,
            Request::QueuePlayNext {
                item: QueueItemId(item_id),
            },
        )
    })?;
    ns.set("play_next", play_next)?;

    let shared_add = shared.clone();
    let add = lua.create_function(move |lua, track_id: String| {
        call(
            lua,
            &shared_add,
            RequestKind::QueueAdd,
            Request::QueueAdd { track: track_id },
        )
    })?;
    ns.set("add", add)?;

    api.set("queue", ns)?;
    Ok(())
}

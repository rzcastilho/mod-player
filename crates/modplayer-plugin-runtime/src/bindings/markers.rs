// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.markers.*` (contract §3): list/create/move/rename/recolor/delete,
//! create_loop, set_cue.

use mlua::{Lua, Table};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::request::{MarkerId, Request};

use super::{SharedHandle, call};

fn opt_bool(table: &Option<Table>, key: &str) -> mlua::Result<bool> {
    match table {
        Some(t) => Ok(t.get::<Option<bool>>(key)?.unwrap_or(false)),
        None => Ok(false),
    }
}

fn opt_string(table: &Option<Table>, key: &str) -> mlua::Result<Option<String>> {
    match table {
        Some(t) => t.get::<Option<String>>(key),
        None => Ok(None),
    }
}

pub fn install(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let ns = lua.create_table()?;

    let shared_list = shared.clone();
    let list = lua.create_function(move |lua, ()| {
        call(
            lua,
            &shared_list,
            RequestKind::ListMarkers,
            Request::ListMarkers,
        )
    })?;
    ns.set("list", list)?;

    let shared_create = shared.clone();
    let create = lua.create_function(move |lua, (position_ms, opts): (u64, Option<Table>)| {
        let name = opt_string(&opts, "name")?;
        let transient = opt_bool(&opts, "transient")?;
        call(
            lua,
            &shared_create,
            RequestKind::CreateMarker,
            Request::CreateMarker {
                position_ms,
                name,
                transient,
            },
        )
    })?;
    ns.set("create", create)?;

    let shared_move = shared.clone();
    let move_fn = lua.create_function(move |lua, (id, position_ms): (u32, u64)| {
        call(
            lua,
            &shared_move,
            RequestKind::MoveMarker,
            Request::MoveMarker {
                id: MarkerId(id),
                position_ms,
            },
        )
    })?;
    ns.set("move", move_fn)?;

    let shared_rename = shared.clone();
    let rename = lua.create_function(move |lua, (id, name): (u32, String)| {
        call(
            lua,
            &shared_rename,
            RequestKind::RenameMarker,
            Request::RenameMarker {
                id: MarkerId(id),
                name,
            },
        )
    })?;
    ns.set("rename", rename)?;

    let shared_recolor = shared.clone();
    let recolor = lua.create_function(move |lua, (id, color): (u32, u8)| {
        call(
            lua,
            &shared_recolor,
            RequestKind::RecolorMarker,
            Request::RecolorMarker {
                id: MarkerId(id),
                color,
            },
        )
    })?;
    ns.set("recolor", recolor)?;

    let shared_delete = shared.clone();
    let delete = lua.create_function(move |lua, id: u32| {
        call(
            lua,
            &shared_delete,
            RequestKind::DeleteMarker,
            Request::DeleteMarker { id: MarkerId(id) },
        )
    })?;
    ns.set("delete", delete)?;

    let shared_loop = shared.clone();
    let create_loop =
        lua.create_function(move |lua, (a_ms, b_ms, opts): (u64, u64, Option<Table>)| {
            let transient = opt_bool(&opts, "transient")?;
            call(
                lua,
                &shared_loop,
                RequestKind::CreateLoopRegion,
                Request::CreateLoopRegion {
                    a_ms,
                    b_ms,
                    transient,
                },
            )
        })?;
    ns.set("create_loop", create_loop)?;

    let shared_cue = shared.clone();
    let set_cue = lua.create_function(move |lua, (slot, position_ms): (u8, u64)| {
        call(
            lua,
            &shared_cue,
            RequestKind::SetCue,
            Request::SetCue { slot, position_ms },
        )
    })?;
    ns.set("set_cue", set_cue)?;

    api.set("markers", ns)?;
    Ok(())
}

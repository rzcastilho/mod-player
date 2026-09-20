// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.effects.*` (contract §3): list_chain, create_node, set_param,
//! schedule_param, bypass, remove_node.

use mlua::{Lua, Table};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::manifest::SuggestedPosition;
use modplayer_capability_gateway::request::{NodeId, Request};

use super::{SharedHandle, call};

fn parse_suggested(opts: &Option<Table>) -> mlua::Result<Option<SuggestedPosition>> {
    let Some(opts) = opts else {
        return Ok(None);
    };
    if let Some(index) = opts.get::<Option<usize>>("index")? {
        return Ok(Some(SuggestedPosition::Index(index)));
    }
    if let Some(before) = opts.get::<Option<String>>("before")? {
        return Ok(Some(SuggestedPosition::Before(before)));
    }
    if let Some(after) = opts.get::<Option<String>>("after")? {
        return Ok(Some(SuggestedPosition::After(after)));
    }
    Ok(None)
}

pub fn install(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let ns = lua.create_table()?;

    let shared_list = shared.clone();
    let list_chain = lua.create_function(move |lua, ()| {
        call(
            lua,
            &shared_list,
            RequestKind::ListChain,
            Request::ListChain,
        )
    })?;
    ns.set("list_chain", list_chain)?;

    let shared_create = shared.clone();
    let create_node = lua.create_function(move |lua, (kind, opts): (String, Option<Table>)| {
        let suggested = parse_suggested(&opts)?;
        call(
            lua,
            &shared_create,
            RequestKind::CreateNode,
            Request::CreateNode { kind, suggested },
        )
    })?;
    ns.set("create_node", create_node)?;

    let shared_set = shared.clone();
    let set_param = lua.create_function(move |lua, (node_id, param, value): (u32, u8, f32)| {
        call(
            lua,
            &shared_set,
            RequestKind::SetParam,
            Request::SetParam {
                node: NodeId(node_id),
                param,
                value,
            },
        )
    })?;
    ns.set("set_param", set_param)?;

    let shared_schedule = shared.clone();
    let schedule_param = lua.create_function(
        move |lua, (node_id, param, value, at_ms): (u32, u8, f32, u64)| {
            call(
                lua,
                &shared_schedule,
                RequestKind::ScheduleParam,
                Request::ScheduleParam {
                    node: NodeId(node_id),
                    param,
                    value,
                    at_ms,
                },
            )
        },
    )?;
    ns.set("schedule_param", schedule_param)?;

    let shared_bypass = shared.clone();
    let bypass = lua.create_function(move |lua, (node_id, bypassed): (u32, bool)| {
        call(
            lua,
            &shared_bypass,
            RequestKind::SetBypass,
            Request::SetBypass {
                node: NodeId(node_id),
                bypassed,
            },
        )
    })?;
    ns.set("bypass", bypass)?;

    let shared_remove = shared.clone();
    let remove_node = lua.create_function(move |lua, node_id: u32| {
        call(
            lua,
            &shared_remove,
            RequestKind::RemoveNode,
            Request::RemoveNode {
                node: NodeId(node_id),
            },
        )
    })?;
    ns.set("remove_node", remove_node)?;

    api.set("effects", ns)?;
    Ok(())
}

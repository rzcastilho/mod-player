// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.effects.*` (contract §3): list_chain, create_node, set_param,
//! schedule_param, bypass, remove_node.

use mlua::{Lua, Table, Value};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::manifest::SuggestedPosition;
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{NodeId, ParamArg, ParamRef, Request};

use super::{SharedHandle, call, lua_err};

fn bad_argument(detail: impl Into<String>) -> Refusal {
    Refusal::invalid_state("invalid_argument", detail)
}

/// `set_param`/`schedule_param`'s `param` argument (API 1.4, contract §5):
/// the 1.0-1.3 numeric id (`Value::Integer`/`Value::Number` with no
/// fraction, `0..=255`) or the parameter's wire name (`Value::String`).
/// Any other Lua type is refused, never a Lua error (contract §2).
fn parse_param_ref(value: &Value) -> Result<ParamRef, Refusal> {
    match value {
        Value::Integer(n) => u8::try_from(*n)
            .map(ParamRef::Id)
            .map_err(|_| bad_argument("param id must be in 0..=255.")),
        Value::Number(n) if n.fract() == 0.0 && (0.0..=255.0).contains(n) =>
        {
            #[allow(clippy::cast_possible_truncation)]
            Ok(ParamRef::Id(*n as u8))
        }
        Value::String(s) => Ok(ParamRef::Name(s.to_string_lossy())),
        _ => Err(bad_argument(
            "param must be an integer id or a parameter wire name string.",
        )),
    }
}

/// `set_param`/`schedule_param`'s `value` argument (API 1.4, contract §5):
/// the 1.0-1.3 numeric form, a boolean for a `boolean`-shaped parameter,
/// or an enum name string for an `enum`-shaped parameter. Any other Lua
/// type is refused, never a Lua error (contract §2).
fn parse_param_arg(value: &Value) -> Result<ParamArg, Refusal> {
    match value {
        Value::Integer(n) =>
        {
            #[allow(clippy::cast_precision_loss)]
            Ok(ParamArg::Number(*n as f32))
        }
        Value::Number(n) =>
        {
            #[allow(clippy::cast_possible_truncation)]
            Ok(ParamArg::Number(*n as f32))
        }
        Value::Boolean(b) => Ok(ParamArg::Bool(*b)),
        Value::String(s) => Ok(ParamArg::Name(s.to_string_lossy())),
        _ => Err(bad_argument(
            "value must be a number, boolean or enum name string.",
        )),
    }
}

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
    let set_param =
        lua.create_function(move |lua, (node_id, param, value): (u32, Value, Value)| {
            let param = match parse_param_ref(&param) {
                Ok(p) => p,
                Err(refusal) => return lua_err(lua, &refusal),
            };
            let value = match parse_param_arg(&value) {
                Ok(v) => v,
                Err(refusal) => return lua_err(lua, &refusal),
            };
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
        move |lua, (node_id, param, value, at_ms): (u32, Value, Value, u64)| {
            let param = match parse_param_ref(&param) {
                Ok(p) => p,
                Err(refusal) => return lua_err(lua, &refusal),
            };
            let value = match parse_param_arg(&value) {
                Ok(v) => v,
                Err(refusal) => return lua_err(lua, &refusal),
            };
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

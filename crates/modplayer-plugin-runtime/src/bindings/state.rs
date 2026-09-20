// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.state.plugin.*` / `api.state.track.*` (contract §3): local
//! key-value reads/writes on the plugin's own `PluginStateStore` — never
//! an RPC (research R9, R3).

use mlua::{Lua, LuaSerdeExt, Table, Value};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{Request, Response};
use modplayer_capability_gateway::state::Scope;

use super::{SharedHandle, call, lock};

/// `dispatch`'s local handler for `StatePluginGet`/`StateTrackGet`
/// (contract §3): `invalid_state`/`no_track` for a `Track` read with no
/// current track loaded, else the stored value or `nil` for an unset key.
pub fn get(
    shared: &SharedHandle,
    _kind: RequestKind,
    request: &Request,
) -> Result<Response, Refusal> {
    let Request::StateGet { scope, key } = request else {
        return Err(Refusal::invalid_state(
            "invalid_argument",
            "Malformed state.get request.",
        ));
    };
    let guard = lock(shared);
    if !guard.store.scope_available(*scope) {
        return Err(Refusal::no_track());
    }
    Ok(Response::StoreValue(guard.store.get(*scope, key).cloned()))
}

/// `dispatch`'s local handler for `StatePluginSet`/`StateTrackSet` (G6 via
/// `PluginStateStore::set`).
pub fn set(
    shared: &SharedHandle,
    _kind: RequestKind,
    request: &Request,
) -> Result<Response, Refusal> {
    let Request::StateSet { scope, key, value } = request else {
        return Err(Refusal::invalid_state(
            "invalid_argument",
            "Malformed state.set request.",
        ));
    };
    let mut guard = lock(shared);
    if !guard.store.scope_available(*scope) {
        return Err(Refusal::no_track());
    }
    guard.store.set(*scope, key, value.clone())?;
    Ok(Response::Ok)
}

/// `dispatch`'s local handler for `StatePluginRemove`/`StateTrackRemove`
/// (a no-op if the key was never set).
pub fn remove(
    shared: &SharedHandle,
    _kind: RequestKind,
    request: &Request,
) -> Result<Response, Refusal> {
    let Request::StateRemove { scope, key } = request else {
        return Err(Refusal::invalid_state(
            "invalid_argument",
            "Malformed state.remove request.",
        ));
    };
    let mut guard = lock(shared);
    if !guard.store.scope_available(*scope) {
        return Err(Refusal::no_track());
    }
    guard.store.remove(*scope, key);
    Ok(Response::Ok)
}

fn install_scope(
    lua: &Lua,
    state: &Table,
    name: &str,
    scope: Scope,
    shared: SharedHandle,
) -> mlua::Result<()> {
    let ns = lua.create_table()?;

    let (get_kind, set_kind, remove_kind) = match scope {
        Scope::Plugin => (
            RequestKind::StatePluginGet,
            RequestKind::StatePluginSet,
            RequestKind::StatePluginRemove,
        ),
        Scope::Track => (
            RequestKind::StateTrackGet,
            RequestKind::StateTrackSet,
            RequestKind::StateTrackRemove,
        ),
    };

    let shared_get = shared.clone();
    let get_fn = lua.create_function(move |lua, key: String| {
        call(lua, &shared_get, get_kind, Request::StateGet { scope, key })
    })?;
    ns.set("get", get_fn)?;

    let shared_set = shared.clone();
    let set_fn = lua.create_function(move |lua, (key, value): (String, Value)| {
        let json: serde_json::Value = lua.from_value(value)?;
        call(
            lua,
            &shared_set,
            set_kind,
            Request::StateSet {
                scope,
                key,
                value: json,
            },
        )
    })?;
    ns.set("set", set_fn)?;

    let remove_fn = lua.create_function(move |lua, key: String| {
        call(
            lua,
            &shared,
            remove_kind,
            Request::StateRemove { scope, key },
        )
    })?;
    ns.set("remove", remove_fn)?;

    state.set(name, ns)?;
    Ok(())
}

/// `api.state.plugin` / `api.state.track` (contract §3).
pub fn install(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let state = lua.create_table()?;
    install_scope(lua, &state, "plugin", Scope::Plugin, shared.clone())?;
    install_scope(lua, &state, "track", Scope::Track, shared)?;
    api.set("state", state)?;
    Ok(())
}

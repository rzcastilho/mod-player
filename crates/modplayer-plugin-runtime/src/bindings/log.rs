// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.log.*` (contract §3): `info`/`warn`/`error` — ungated, local,
//! never through the gateway (no permission, no rate category). Tags the
//! `log` facade with `target: "plugin:<identifier>"` (research R19) and
//! reports a [`RuntimeEvent::Log`] for the plugin's own console (T055's
//! `PluginLog` ring).

use log::Level;
use mlua::{Lua, Table};

use crate::events::RuntimeEvent;

use super::{SharedHandle, lock, lua_ok};

fn emit(shared: &SharedHandle, level: Level, message: String) {
    let (identifier, plugin, events) = {
        let guard = lock(shared);
        (
            guard.identifier.clone(),
            guard.gateway.plugin(),
            guard.deps.events.clone(),
        )
    };
    let target = format!("plugin:{identifier}");
    log::log!(target: &target, level, "{message}");
    let _ = events.send((plugin, RuntimeEvent::Log { level, message }));
}

pub fn install(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let ns = lua.create_table()?;

    let shared_info = shared.clone();
    let info = lua.create_function(move |_lua, message: String| {
        emit(&shared_info, Level::Info, message);
        lua_ok(mlua::Value::Boolean(true))
    })?;
    ns.set("info", info)?;

    let shared_warn = shared.clone();
    let warn = lua.create_function(move |_lua, message: String| {
        emit(&shared_warn, Level::Warn, message);
        lua_ok(mlua::Value::Boolean(true))
    })?;
    ns.set("warn", warn)?;

    let error = lua.create_function(move |_lua, message: String| {
        emit(&shared, Level::Error, message);
        lua_ok(mlua::Value::Boolean(true))
    })?;
    ns.set("error", error)?;

    api.set("log", ns)?;
    Ok(())
}

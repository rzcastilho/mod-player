// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Lua `api` object: one exhaustive dispatcher over `RequestKind::ALL`
//! (design note 1) plus the per-namespace binding modules that register
//! the actual Lua-callable functions (contracts/plugin-api-v1.md).

pub mod effects;
pub mod log;
pub mod markers;
pub mod playback;
pub mod queue;
pub mod state;
pub mod timers;
pub mod transport;
pub mod ui;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use mlua::{Lua, LuaSerdeExt, Table, Value};

use modplayer_capability_gateway::api::{API_VERSION, HOST_CAPABILITIES, RequestKind};
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{
    MarkerInfo, NodeInfo, OwnerInfo, ParamValue, RegionInfo, Request, Response,
};

use crate::context::Shared;
use crate::handle::RpcEnvelope;

/// Every namespace's functions lock this once per call — never a real
/// contention point (RT1: one thread per plugin).
pub type SharedHandle = Arc<Mutex<Shared>>;

/// Convert a `Refusal` into the `{ code, reason, message }` table the
/// contract's result convention returns as the second value (§2).
pub fn refusal_table(lua: &Lua, refusal: &Refusal) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("code", refusal.code.as_str())?;
    table.set("reason", refusal.reason)?;
    table.set("message", refusal.message.clone())?;
    Ok(table)
}

/// `Ok(value), nil`.
pub fn lua_ok(value: Value) -> mlua::Result<(Value, Value)> {
    Ok((value, Value::Nil))
}

/// `nil, refusal` (contract §2: a request never throws).
pub fn lua_err(lua: &Lua, refusal: &Refusal) -> mlua::Result<(Value, Value)> {
    Ok((Value::Nil, Value::Table(refusal_table(lua, refusal)?)))
}

/// `OwnerInfo` on the Lua side (contracts/plugin-api-v1.md §4's
/// `marker_changed { actor = … }` shape, which `MarkerInfo.owner`/
/// `NodeInfo.owner` share): the plain string `"host"` | `"<identifier>"`
/// | `"me"`.
///
/// Hand-written rather than left to `OwnerInfo`'s own `Serialize` impl
/// plus `LuaSerdeExt::to_value`: `Plugin(String)` is a newtype variant
/// wrapping a bare scalar, which serde's (and so `mlua`'s serde bridge's)
/// internally-tagged-enum representation cannot express — `to_value`
/// returns `Err(SerializeError("cannot serialize tagged newtype variant
/// OwnerInfo::Plugin containing a string"))` for it, which previously
/// turned every `markers.list()`/`effects.list_chain()` call naming a
/// plugin-owned item into an uncaught Lua error instead of the
/// contract's `nil, refusal` (violating contract §2 "an admitted call
/// never raises a Lua error") — silently, since the raised error just
/// aborted the calling handler (`AbortCause::Exception`) with no
/// `plugin_log` entry of its own. `MarkerInfo`/`NodeInfo` are therefore
/// built field-by-field below instead of handed whole to `to_value`.
pub(crate) fn owner_to_string(owner: &OwnerInfo) -> String {
    match owner {
        OwnerInfo::Host => "host".to_string(),
        OwnerInfo::Plugin(identifier) => identifier.clone(),
        OwnerInfo::Me => "me".to_string(),
    }
}

fn marker_info_to_lua(lua: &Lua, info: &MarkerInfo) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", info.id.0)?;
    table.set("kind", info.kind.as_str())?;
    table.set("position_ms", info.position_ms)?;
    table.set("name", info.name.as_str())?;
    table.set("color", info.color)?;
    table.set("owner", owner_to_string(&info.owner))?;
    table.set("transient", info.transient)?;
    table.set("region", info.region.map(|r| r.0))?;
    table.set("slot", info.slot)?;
    Ok(table)
}

/// 012-section-loop-plugin (data-model.md §1.3, contract
/// plugin-api-v1.3.md §3.3): `a`/`b` absent when `nil` (an incomplete
/// region), `repeat` a Lua number or the literal string `"infinite"`.
fn region_info_to_lua(lua: &Lua, info: &RegionInfo) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", info.id.0)?;
    table.set("owner", owner_to_string(&info.owner))?;
    table.set("a", info.a.map(|m| m.0))?;
    table.set("b", info.b.map(|m| m.0))?;
    table.set(
        "repeat",
        match info.repeat {
            modplayer_capability_gateway::request::RepeatArg::Times(n) => Value::Integer(n.into()),
            modplayer_capability_gateway::request::RepeatArg::Infinite => {
                Value::String(lua.create_string("infinite")?)
            }
        },
    )?;
    table.set("armed", info.armed)?;
    Ok(table)
}

/// API 1.4: `Number` -> a Lua number, `Bool` -> a Lua boolean, `Name` -> a
/// Lua string (contract plugin-api-v1.4.md §3.3).
fn param_value_to_lua(lua: &Lua, value: &ParamValue) -> mlua::Result<Value> {
    Ok(match value {
        ParamValue::Number(n) => Value::Number(*n),
        ParamValue::Bool(b) => Value::Boolean(*b),
        ParamValue::Name(s) => Value::String(lua.create_string(s)?),
    })
}

/// Hand-built like [`marker_info_to_lua`]/[`region_info_to_lua`] (the same
/// `OwnerInfo::Plugin` newtype-variant `to_value` failure applies here —
/// see this module's doc comment) — `pub(crate)` so the scheduler's
/// `effect_chain_changed` payload (research R4) and this module's own
/// `list_chain` response share one projection.
pub(crate) fn node_info_to_lua(lua: &Lua, info: &NodeInfo) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", info.id.0)?;
    table.set("kind", info.kind.as_str())?;
    table.set("owner", owner_to_string(&info.owner))?;
    table.set("bypassed", info.bypassed)?;
    table.set("auto_bypassed", info.auto_bypassed)?;
    table.set("orphaned", info.orphaned)?;
    table.set("index", info.index)?;
    let params = lua.create_table()?;
    for (name, value) in &info.params {
        params.set(name.as_str(), param_value_to_lua(lua, value)?)?;
    }
    table.set("params", params)?;
    table.set("auto_switched", info.auto_switched)?;
    Ok(table)
}

/// Convert a successful [`Response`] into the Lua-facing return value
/// (the caller wraps it with [`lua_ok`]).
pub fn response_to_lua(lua: &Lua, response: Response) -> mlua::Result<Value> {
    Ok(match response {
        Response::Ok => Value::Boolean(true),
        Response::MarkerId(id) => Value::Integer(i64::from(id.0)),
        Response::RegionId(id) => Value::Integer(i64::from(id.0)),
        Response::NodeId(id) => Value::Integer(i64::from(id.0)),
        Response::Markers {
            markers,
            armed,
            regions,
        } => {
            let table = lua.create_table()?;
            let list = lua.create_table()?;
            for (i, marker) in markers.iter().enumerate() {
                list.set(i + 1, marker_info_to_lua(lua, marker)?)?;
            }
            table.set("markers", list)?;
            table.set("armed", armed.map(|r| i64::from(r.0)))?;
            let region_list = lua.create_table()?;
            for (i, region) in regions.iter().enumerate() {
                region_list.set(i + 1, region_info_to_lua(lua, region)?)?;
            }
            table.set("regions", region_list)?;
            Value::Table(table)
        }
        Response::LoopEndpoint { region, marker } => {
            let table = lua.create_table()?;
            table.set("region", region.0)?;
            table.set("marker", marker.0)?;
            Value::Table(table)
        }
        Response::Chain(nodes) => {
            let table = lua.create_table()?;
            let list = lua.create_table()?;
            for (i, node) in nodes.iter().enumerate() {
                list.set(i + 1, node_info_to_lua(lua, node)?)?;
            }
            table.set("nodes", list)?;
            Value::Table(table)
        }
        Response::Queue(items) => {
            let table = lua.create_table()?;
            table.set("items", lua.to_value(&items)?)?;
            Value::Table(table)
        }
        Response::StoreValue(value) => match value {
            Some(v) => lua.to_value(&v)?,
            None => Value::Nil,
        },
        Response::TimerHandle(handle) => Value::Integer(i64::from(handle)),
        Response::Probe(json) => lua.to_value(&json)?,
        Response::Settings(values) => lua.to_value(&values)?,
    })
}

/// RT7: send an admitted request to core and block for its reply (or the
/// gateway's RPC timeout). Must be called with `shared` **not** locked —
/// nothing else on this thread needs it while blocked, but holding the
/// lock across a blocking wait is needless coupling.
fn rpc(shared: &SharedHandle, request: Request) -> Result<Response, Refusal> {
    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    let (plugin, sender, timeout, budget) = {
        let guard = lock(shared);
        (
            guard.gateway.plugin(),
            guard.deps.requests.clone(),
            guard.budgets.rpc_timeout,
            Arc::clone(&guard.budget),
        )
    };
    if sender
        .send(RpcEnvelope {
            plugin,
            request,
            reply: reply_tx,
        })
        .is_err()
    {
        return Err(Refusal::host_busy());
    }
    let wait_start = Instant::now();
    let outcome = reply_rx.recv_timeout(timeout);
    budget.extend_deadline_for_wait(wait_start.elapsed());
    match outcome {
        Ok(inner) => inner,
        Err(_) => Err(Refusal::host_busy()),
    }
}

pub(crate) fn lock(shared: &SharedHandle) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Every namespace file's Lua-bound closures call through this: dispatch,
/// then convert to the `ok, value` / `nil, refusal` pair (contract §2).
pub fn call(
    lua: &Lua,
    shared: &SharedHandle,
    kind: RequestKind,
    request: Request,
) -> mlua::Result<(Value, Value)> {
    match dispatch(shared, kind, request) {
        Ok(response) => lua_ok(response_to_lua(lua, response)?),
        Err(refusal) => lua_err(lua, &refusal),
    }
}

/// G2/G1: the single entry point every namespace binding calls — admits
/// through the gateway, then routes to a local capability or an RPC
/// (design note 1, T040). Exhaustive over [`RequestKind::ALL`] so a
/// schema addition without a matching arm fails to compile.
pub fn dispatch(
    shared: &SharedHandle,
    kind: RequestKind,
    request: Request,
) -> Result<Response, Refusal> {
    {
        let mut guard = lock(shared);
        guard.gateway.admit(kind, Instant::now())?;
    }
    match kind {
        // -- Local: playback observation (contract §3) ---------------------
        RequestKind::PlaybackSubscribePosition => {
            let mut guard = lock(shared);
            if let Request::SubscribePosition { rate_hz } = request {
                guard.subscriptions.set_position_rate(rate_hz);
            }
            Ok(Response::Ok)
        }
        RequestKind::PlaybackState => {
            let guard = lock(shared);
            let state_str = match guard.deps.playback.intent() {
                crate::events::PlaySnapshotState::Playing => "playing",
                crate::events::PlaySnapshotState::Paused => "paused",
                crate::events::PlaySnapshotState::Stopped => "stopped",
            };
            let position_ms = modplayer_engine::PositionClock::now(
                &guard.deps.shared,
                guard.deps.playback.source_rate().max(1),
            )
            .as_millis() as u64;
            // `track` is left `null` this slice: no per-track metadata is
            // cached on the plugin thread yet (a later slice's job — the
            // `track_changed` event already carries it).
            //
            // Built field-by-field rather than via `serde_json::json!`:
            // the macro's expansion runs afoul of `clippy::disallowed_
            // methods`' `unwrap`/`expect` ban even though nothing here
            // can actually fail.
            let mut probe = serde_json::Map::new();
            probe.insert(
                "state".to_string(),
                serde_json::Value::String(state_str.to_string()),
            );
            probe.insert(
                "position_ms".to_string(),
                serde_json::Value::from(position_ms),
            );
            probe.insert("track".to_string(), serde_json::Value::Null);
            Ok(Response::Probe(serde_json::Value::Object(probe)))
        }

        // -- Local: snapshot reads (research R3) -----------------------------
        RequestKind::ListMarkers => {
            let guard = lock(shared);
            let snapshot = guard
                .deps
                .snapshot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(Response::Markers {
                markers: snapshot.markers.clone(),
                armed: snapshot.armed_region,
                regions: snapshot.regions.clone(),
            })
        }
        RequestKind::ListChain => {
            let guard = lock(shared);
            let snapshot = guard
                .deps
                .snapshot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(Response::Chain(snapshot.chain.clone()))
        }
        RequestKind::QueueList => {
            let guard = lock(shared);
            let snapshot = guard
                .deps
                .snapshot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(Response::Queue(snapshot.queue.clone()))
        }

        // -- Local: per-plugin state store (owned by this thread, R9) -------
        RequestKind::StatePluginGet | RequestKind::StateTrackGet => {
            self::state::get(shared, kind, &request)
        }
        RequestKind::StatePluginSet | RequestKind::StateTrackSet => {
            self::state::set(shared, kind, &request)
        }
        RequestKind::StatePluginRemove | RequestKind::StateTrackRemove => {
            self::state::remove(shared, kind, &request)
        }

        // -- Local: timers (thread-local, RT9) -------------------------------
        RequestKind::TimersSetTimeout
        | RequestKind::TimersSetInterval
        | RequestKind::TimersScheduleAtPosition
        | RequestKind::TimersClear => self::timers::apply(shared, kind, &request),

        // -- Local: settings snapshot (011-plugin-ui-contributions, R4) -----
        RequestKind::GetSettings => self::ui::get_settings_local(shared),

        // -- Fixture-only: handled entirely inside bindings::log's sibling --
        RequestKind::DebugProbe => Err(Refusal::invalid_state(
            "invalid_argument",
            "debug_probe is answered locally, not through the gateway dispatcher.",
        )),

        // -- Everything else is an RPC to core (RT7) -------------------------
        _ => rpc(shared, request),
    }
}

/// `api.granted`, `api.version`, `api.capabilities` (contract §1).
pub fn install_identity(lua: &Lua, api: &Table, shared: &SharedHandle) -> mlua::Result<()> {
    let granted: Vec<&'static str> = {
        let guard = lock(shared);
        guard.gateway.grants().granted().map(|p| p.name()).collect()
    };
    api.set("granted", granted)?;

    let version = lua.create_table()?;
    version.set("major", API_VERSION.major)?;
    version.set("minor", API_VERSION.minor)?;
    api.set("version", version)?;

    api.set("capabilities", HOST_CAPABILITIES.to_vec())?;
    Ok(())
}

/// `api.on(event_name, handler)` (contract §1): registers a handler,
/// replacing any earlier one for the same event. Handlers are stored in a
/// plain Lua table (`api._handlers`) the scheduler reads by event name.
pub fn install_on(lua: &Lua, api: &Table) -> mlua::Result<()> {
    let handlers = lua.create_table()?;
    api.set("_handlers", &handlers)?;
    let on = lua.create_function(move |lua, (name, handler): (String, mlua::Function)| {
        let api: Table = lua.globals().get("api")?;
        let handlers: Table = api.get("_handlers")?;
        handlers.set(name, handler)?;
        Ok(())
    })?;
    api.set("on", on)?;
    Ok(())
}

/// `api.ready()` (contract §1): `true` once; later calls `false,
/// refusal` — harmlessly, per the contract ("repeated `ready()` is
/// harmless"; note `not_ready` is specifically *not* the reason used
/// here, since that code is reserved for "no current track" refusals).
/// The actual `RuntimeEvent::Ready`/`ready_ack` dispatch is the
/// scheduler's job (RT5) — this binding only flips the shared flag.
pub fn install_ready(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let ready_fn = lua.create_function(move |lua, ()| {
        let mut guard = lock(&shared);
        if guard.ready {
            drop(guard);
            return lua_err(
                lua,
                &Refusal::invalid_state("invalid_argument", "api.ready() was already called."),
            );
        }
        guard.ready = true;
        drop(guard);
        Ok((Value::Boolean(true), Value::Nil))
    })?;
    api.set("ready", ready_fn)?;
    Ok(())
}

/// `api.debug_probe(name)` (contract §3, L10): answered locally by
/// calling whatever handler the fixture registered for the synthetic
/// `"debug_probe"` event via `api.on` — never through the gateway, never
/// permission-gated, and only while `fixtures_enabled`. The same lookup
/// is used by [`crate::handle::Control::Probe`] for host-initiated
/// (test-driven) probes.
pub fn install_debug_probe(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let probe_fn = lua.create_function(move |lua, name: String| {
        let fixtures_enabled = lock(&shared).fixtures_enabled;
        if !fixtures_enabled {
            return lua_err(
                lua,
                &Refusal::invalid_state("invalid_argument", "Fixtures are not enabled."),
            );
        }
        match call_probe_handler(lua, &name)? {
            Some(value) => lua_ok(value),
            None => lua_err(
                lua,
                &Refusal::invalid_state(
                    "invalid_argument",
                    "No debug_probe handler is registered.",
                ),
            ),
        }
    })?;
    api.set("debug_probe", probe_fn)?;
    Ok(())
}

/// Shared by `install_debug_probe` and the scheduler's `Control::Probe`
/// handling: look up and call the `"debug_probe"` handler, if any.
pub fn call_probe_handler(lua: &Lua, name: &str) -> mlua::Result<Option<Value>> {
    let api: Table = lua.globals().get("api")?;
    let handlers: Table = api.get("_handlers")?;
    let handler: Option<mlua::Function> = handlers.get("debug_probe")?;
    match handler {
        Some(f) => {
            let payload = lua.create_table()?;
            payload.set("name", name)?;
            let result: Value = f.call(payload)?;
            Ok(Some(result))
        }
        None => Ok(None),
    }
}

/// Install the whole `api` table (T040/T033): identity fields, `on`,
/// `ready`, `debug_probe`, and every namespace's request bindings.
pub fn install(lua: &Lua, shared: SharedHandle) -> mlua::Result<()> {
    let api = lua.create_table()?;

    install_identity(lua, &api, &shared)?;
    install_on(lua, &api)?;
    install_ready(lua, &api, Arc::clone(&shared))?;
    install_debug_probe(lua, &api, Arc::clone(&shared))?;

    self::playback::install(lua, &api, Arc::clone(&shared))?;
    self::transport::install(lua, &api, Arc::clone(&shared))?;
    self::queue::install(lua, &api, Arc::clone(&shared))?;
    self::markers::install(lua, &api, Arc::clone(&shared))?;
    self::effects::install(lua, &api, Arc::clone(&shared))?;
    self::state::install(lua, &api, Arc::clone(&shared))?;
    self::timers::install(lua, &api, Arc::clone(&shared))?;
    self::ui::install(lua, &api, Arc::clone(&shared))?;
    self::log::install(lua, &api, shared)?;

    lua.globals().set("api", api)?;
    Ok(())
}

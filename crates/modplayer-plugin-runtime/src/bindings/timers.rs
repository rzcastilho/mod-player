// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.timers.*` (contract §3): `set_timeout`/`set_interval`/
//! `schedule_at_position`/`clear` — thread-local, never an RPC (RT9).

use std::time::{Duration, Instant};

use mlua::{Lua, Table};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{Request, Response};

use crate::timers::TimerHandle;

use super::{SharedHandle, call, lock};

fn schedule(shared: &SharedHandle, ms: u64, repeat: bool) -> Result<Response, Refusal> {
    if ms < 1 {
        return Err(Refusal::invalid_state(
            "invalid_argument",
            "A timer's delay must be at least 1 ms.",
        ));
    }
    let mut guard = lock(shared);
    let now = Instant::now();
    match guard
        .timers
        .schedule(Duration::from_millis(ms), repeat, now)
    {
        Some(handle) => Ok(Response::TimerHandle(handle.0)),
        None => Err(Refusal::invalid_state(
            "timer_limit",
            "This plugin already has 256 pending timers.",
        )),
    }
}

/// `dispatch`'s local handler for every `Timers*` request kind (RT9).
pub fn apply(
    shared: &SharedHandle,
    kind: RequestKind,
    request: &Request,
) -> Result<Response, Refusal> {
    match (kind, request) {
        (RequestKind::TimersSetTimeout, Request::SetTimeout { ms }) => schedule(shared, *ms, false),
        (RequestKind::TimersSetInterval, Request::SetInterval { ms }) => {
            schedule(shared, *ms, true)
        }
        (RequestKind::TimersScheduleAtPosition, Request::ScheduleAtPosition { position_ms }) => {
            let mut guard = lock(shared);
            match guard.timers.schedule_at_position(*position_ms) {
                Some(handle) => Ok(Response::TimerHandle(handle.0)),
                None => Err(Refusal::invalid_state(
                    "timer_limit",
                    "This plugin already has 256 pending timers.",
                )),
            }
        }
        (RequestKind::TimersClear, Request::ClearTimer { handle }) => {
            let mut guard = lock(shared);
            if guard.timers.clear(TimerHandle(*handle)) {
                Ok(Response::Ok)
            } else {
                Err(Refusal::not_found())
            }
        }
        _ => Err(Refusal::invalid_state(
            "invalid_argument",
            "Malformed timers request.",
        )),
    }
}

pub fn install(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let ns = lua.create_table()?;

    let shared_timeout = shared.clone();
    let set_timeout = lua.create_function(move |lua, ms: u64| {
        call(
            lua,
            &shared_timeout,
            RequestKind::TimersSetTimeout,
            Request::SetTimeout { ms },
        )
    })?;
    ns.set("set_timeout", set_timeout)?;

    let shared_interval = shared.clone();
    let set_interval = lua.create_function(move |lua, ms: u64| {
        call(
            lua,
            &shared_interval,
            RequestKind::TimersSetInterval,
            Request::SetInterval { ms },
        )
    })?;
    ns.set("set_interval", set_interval)?;

    let shared_position = shared.clone();
    let schedule_at_position = lua.create_function(move |lua, position_ms: u64| {
        call(
            lua,
            &shared_position,
            RequestKind::TimersScheduleAtPosition,
            Request::ScheduleAtPosition { position_ms },
        )
    })?;
    ns.set("schedule_at_position", schedule_at_position)?;

    let clear = lua.create_function(move |lua, handle: u32| {
        call(
            lua,
            &shared,
            RequestKind::TimersClear,
            Request::ClearTimer { handle },
        )
    })?;
    ns.set("clear", clear)?;

    api.set("timers", ns)?;
    Ok(())
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The plugin scheduler thread's main loop (RT1-RT13; contracts/gateway-
//! and-runtime.md). One instance runs on each `plugin:<identifier>`
//! thread `PluginHandle::spawn` starts; it owns the `Lua` state for the
//! life of the thread (RT1) and is the only code that ever calls into it.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use mlua::{LuaSerdeExt, Table, Value};

use modplayer_capability_gateway::api::Permission;
use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::event::{
    ActionSource, HostEvent, LevelInfo, PlayState, TrackInfo, UnloadReason,
};
use modplayer_capability_gateway::focus::PluginId;
use modplayer_capability_gateway::gateway::Gateway;
use modplayer_capability_gateway::state::{PluginStateStore, Scope, WriteJob};
use modplayer_capability_gateway::ui::WidgetValue;

use crate::bindings::{self, SharedHandle, lock, node_info_to_lua};
use crate::budget::BudgetState;
use crate::context::{PluginContext, Shared};
use crate::events::{AbortCause, RuntimeEvent, SuspendCause};
use crate::handle::{Control, Inbound, RuntimeDeps, SpawnConfig};
use crate::pump::Subscriptions;
use crate::timers::TimerSet;

/// The most inbound events held while waiting for `api.ready()` (RT5) —
/// bounded independently of the inbox channel's own cap so a plugin that
/// never calls `ready()` cannot grow this without limit either; the
/// oldest is dropped once full.
const MAX_PENDING_EVENTS: usize = 64;

/// How long to wait on the inbox when nothing else is scheduled.
const IDLE_WAIT: Duration = Duration::from_millis(250);

/// RT1: the scheduler thread's entry point. Contains any panic that
/// escapes `run_inner` (RT12) — `mlua`'s `catch_rust_panics` already
/// converts a panic *inside* a Lua call into an ordinary `Err`, so this is
/// the last line of defence for a panic in the scheduler's own bookkeeping.
pub fn run(
    config: SpawnConfig,
    deps: RuntimeDeps,
    inbox: Receiver<Inbound>,
    gauges: Arc<crate::budget::PluginGauges>,
) {
    let plugin = config.id;
    let events = deps.events.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_inner(config, deps, inbox, gauges);
    }));
    if outcome.is_err() {
        let _ = events.send((
            plugin,
            RuntimeEvent::Suspended {
                cause: SuspendCause::Hang,
            },
        ));
        let _ = events.send((plugin, RuntimeEvent::Exited));
    }
}

fn run_inner(
    config: SpawnConfig,
    deps: RuntimeDeps,
    inbox: Receiver<Inbound>,
    gauges: Arc<crate::budget::PluginGauges>,
) {
    let SpawnConfig {
        id,
        identifier,
        entry_source,
        api_range: _,
        grants,
        budgets,
        focus,
        fixtures_enabled,
    } = config;
    let plugin = id;
    let events = deps.events.clone();

    let mut store = PluginStateStore::new();
    if let Some(paths) = &deps.paths
        && let Ok(bytes) = std::fs::read(paths.plugin_file(&identifier))
    {
        let _ = store.load_plugin(&bytes);
    }
    // 011-plugin-ui-contributions (R5): `settings.json` loads the same way
    // as `plugin.json` — a plugin without a settings page yet, or with no
    // file on disk, just starts with an empty `Scope::Settings`.
    if let Some(paths) = &deps.paths
        && let Ok(bytes) = std::fs::read(paths.settings_file(&identifier))
    {
        let _ = store.load_settings(&bytes);
    }

    let gateway = Gateway::new(plugin, grants, focus);
    let observes_playback = gateway.grants().holds(Permission::PlaybackObserve);
    let observes_meter = gateway.grants().holds(Permission::AudioMeter);
    let initial_generation = deps.playback.track_generation();
    let initial_position_epoch = deps.playback.position_epoch();

    let mut subscriptions = Subscriptions::new();
    subscriptions.set_meter_enabled(observes_meter);

    let budget = BudgetState::new(gauges);

    let shared: SharedHandle = Arc::new(Mutex::new(Shared {
        gateway,
        store,
        timers: TimerSet::new(),
        subscriptions,
        deps,
        budgets,
        budget: Arc::clone(&budget),
        identifier: identifier.clone(),
        fixtures_enabled,
        ready: false,
        last_seen_generation: initial_generation,
        last_seen_position_epoch: initial_position_epoch,
        settings_schema: None,
        in_interaction_handler: false,
    }));

    let created_at = Instant::now();
    let ctx = match PluginContext::new(
        &entry_source,
        budgets.memory,
        Arc::clone(&shared),
        Arc::clone(&budget),
    ) {
        Ok(ctx) => ctx,
        Err(err) => {
            // context.rs's `ContextError` doc: "script error -> Exited" —
            // a plugin that fails to even load never reaches `Suspended`.
            log::error!(target: "plugin", "[{identifier}] failed to start: {err}");
            let _ = events.send((plugin, RuntimeEvent::Exited));
            return;
        }
    };

    let is_ready = { lock(&shared).ready };
    let mut pending: VecDeque<HostEvent> = VecDeque::new();

    if is_ready {
        let _ = events.send((plugin, RuntimeEvent::Ready));
        if let Some(cause) = dispatch_ready_ack(&ctx, &shared, &budget, &budgets, &events, plugin) {
            let _ = events.send((plugin, RuntimeEvent::Suspended { cause }));
            run_unloading(
                &ctx,
                &shared,
                &budget,
                &budgets,
                &events,
                plugin,
                UnloadReason::Suspend,
            );
            let _ = events.send((plugin, RuntimeEvent::Exited));
            return;
        }
    }

    'scheduler: loop {
        let now = Instant::now();
        let wait = next_wait(
            &shared,
            is_ready,
            created_at,
            &budgets,
            observes_playback,
            now,
        );

        match inbox.recv_timeout(wait) {
            Ok(Inbound::Control(Control::Stop)) => break 'scheduler,
            Ok(Inbound::Control(Control::Unloading { reason })) => {
                run_unloading(&ctx, &shared, &budget, &budgets, &events, plugin, reason);
                break 'scheduler;
            }
            Ok(Inbound::Control(Control::SettingsWrite { changes })) => {
                // R5: the store write always applies (host-initiated, not
                // gated on readiness); `settings_changed` only fires once
                // the plugin is ready to run a handler at all — the same
                // rule every other `Inbound::Event` already follows.
                {
                    let mut guard = lock(&shared);
                    for (key, value) in &changes {
                        let _ = guard.store.set(Scope::Settings, key, value.clone());
                    }
                    guard
                        .budget
                        .gauges
                        .set_storage_used_bytes(guard.store.used_bytes());
                }
                if is_ready
                    && let Some(cause) = handle_event(
                        &ctx,
                        &shared,
                        &budget,
                        &budgets,
                        &events,
                        plugin,
                        HostEvent::SettingsChanged { changes },
                    )
                {
                    let _ = events.send((plugin, RuntimeEvent::Suspended { cause }));
                    run_unloading(
                        &ctx,
                        &shared,
                        &budget,
                        &budgets,
                        &events,
                        plugin,
                        UnloadReason::Suspend,
                    );
                    break 'scheduler;
                }
            }
            Ok(Inbound::Control(Control::Probe { name, reply })) => {
                let value = bindings::call_probe_handler(&ctx.lua, &name)
                    .ok()
                    .flatten()
                    .and_then(|v| ctx.lua.from_value::<serde_json::Value>(v).ok())
                    .unwrap_or(serde_json::Value::Null);
                let _ = reply.send(value);
            }
            Ok(Inbound::Event(event)) => {
                if !is_ready {
                    if pending.len() >= MAX_PENDING_EVENTS {
                        pending.pop_front();
                    }
                    pending.push_back(event);
                    continue 'scheduler;
                }
                if let Some(cause) =
                    handle_event(&ctx, &shared, &budget, &budgets, &events, plugin, event)
                {
                    let _ = events.send((plugin, RuntimeEvent::Suspended { cause }));
                    run_unloading(
                        &ctx,
                        &shared,
                        &budget,
                        &budgets,
                        &events,
                        plugin,
                        UnloadReason::Suspend,
                    );
                    break 'scheduler;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if !is_ready {
                    if now.saturating_duration_since(created_at) >= budgets.ready_timeout {
                        let _ = events.send((
                            plugin,
                            RuntimeEvent::Suspended {
                                cause: SuspendCause::DidNotStart,
                            },
                        ));
                        // RT5: exits without running `unloading`.
                        break 'scheduler;
                    }
                    continue 'scheduler;
                }
                if let Some(cause) = tick_scheduled_work(
                    &ctx,
                    &shared,
                    &budget,
                    &budgets,
                    &events,
                    plugin,
                    observes_playback,
                    now,
                ) {
                    let _ = events.send((plugin, RuntimeEvent::Suspended { cause }));
                    run_unloading(
                        &ctx,
                        &shared,
                        &budget,
                        &budgets,
                        &events,
                        plugin,
                        UnloadReason::Suspend,
                    );
                    break 'scheduler;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break 'scheduler,
        }
    }

    let _ = events.send((plugin, RuntimeEvent::Exited));
}

/// RT6: how long the scheduler may safely `recv_timeout` for — the
/// earliest of the ready deadline, the next timer, the next due position
/// sample, and the next due meter sample.
fn next_wait(
    shared: &SharedHandle,
    is_ready: bool,
    created_at: Instant,
    budgets: &Budgets,
    observes_playback: bool,
    now: Instant,
) -> Duration {
    if !is_ready {
        let deadline = created_at + budgets.ready_timeout;
        return deadline
            .saturating_duration_since(now)
            .max(Duration::from_millis(1));
    }
    let guard = lock(shared);
    let mut candidates: Vec<Instant> = Vec::new();
    if let Some(t) = guard.timers.next_deadline() {
        candidates.push(t);
    }
    if observes_playback && let Some(t) = guard.subscriptions.next_position_wake(now) {
        candidates.push(t);
    }
    if let Some(t) = guard.subscriptions.next_meter_wake(now) {
        candidates.push(t);
    }
    drop(guard);
    match candidates.into_iter().min() {
        Some(t) => t
            .saturating_duration_since(now)
            .max(Duration::from_micros(100)),
        None => IDLE_WAIT,
    }
}

/// RT3/RT4: run one handler under its budget, record the sample, and
/// report whether the plugin must now be suspended. Always called with
/// `shared` unlocked (bindings called from inside the handler lock it
/// themselves; this thread must never hold it re-entrantly).
#[allow(clippy::too_many_arguments)]
fn run_and_account(
    ctx: &PluginContext,
    budget: &BudgetState,
    handler_budget: Duration,
    share: Duration,
    window: Duration,
    events: &SyncSender<(PluginId, RuntimeEvent)>,
    plugin: PluginId,
    handler_name: &str,
    payload: Value,
) -> Option<SuspendCause> {
    let handlers: Table = ctx
        .lua
        .globals()
        .get::<Table>("api")
        .ok()
        .and_then(|api| api.get::<Table>("_handlers").ok())?;
    let handler: mlua::Function = handlers.get(handler_name).ok().flatten()?;

    let start = Instant::now();
    budget.start_handler(start, handler_budget);
    let result: mlua::Result<()> = handler.call(payload);
    budget.clear_deadline();
    let elapsed = start.elapsed();
    let rpc_wait = budget.take_rpc_wait();
    let cpu = elapsed.saturating_sub(rpc_wait);
    let now = Instant::now();

    match result {
        Ok(()) => {
            let sum = budget.record_sample(cpu, now, window);
            budget.publish_share_gauge(sum, share);
            if sum > share {
                Some(SuspendCause::CpuShare)
            } else {
                None
            }
        }
        Err(err) if is_memory_error(&err) => {
            log::error!(target: "plugin", "handler '{handler_name}' hit the memory limit");
            Some(SuspendCause::Memory)
        }
        Err(err) => {
            let cause = if crate::budget::is_deadline_error(&err) {
                AbortCause::Deadline
            } else {
                AbortCause::Exception(err.to_string())
            };
            log::error!(target: "plugin", "handler '{handler_name}' aborted: {err}");
            let _ = events.send((
                plugin,
                RuntimeEvent::HandlerAborted {
                    cause: cause.clone(),
                },
            ));
            let sum = budget.record_sample(cpu, now, window);
            budget.publish_share_gauge(sum, share);
            if sum > share {
                Some(if matches!(cause, AbortCause::Deadline) {
                    SuspendCause::Hang
                } else {
                    SuspendCause::CpuShare
                })
            } else {
                None
            }
        }
    }
}

fn is_memory_error(err: &mlua::Error) -> bool {
    match err {
        mlua::Error::MemoryError(_) => true,
        mlua::Error::CallbackError { cause, .. } => is_memory_error(cause),
        _ => false,
    }
}

/// RT5's first dispatch: `ready_ack`.
fn dispatch_ready_ack(
    ctx: &PluginContext,
    shared: &SharedHandle,
    budget: &BudgetState,
    budgets: &Budgets,
    events: &SyncSender<(PluginId, RuntimeEvent)>,
    plugin: PluginId,
) -> Option<SuspendCause> {
    let payload = ready_ack_payload(&ctx.lua, shared).ok()?;
    run_and_account(
        ctx,
        budget,
        budgets.handler,
        budgets.share,
        budgets.window,
        events,
        plugin,
        "ready_ack",
        payload,
    )
}

fn ready_ack_payload(lua: &mlua::Lua, shared: &SharedHandle) -> mlua::Result<Value> {
    let granted: Vec<&'static str> = {
        let guard = lock(shared);
        guard.gateway.grants().granted().map(|p| p.name()).collect()
    };
    let table = lua.create_table()?;
    let api_version = lua.create_table()?;
    api_version.set(
        "major",
        modplayer_capability_gateway::api::API_VERSION.major,
    )?;
    api_version.set(
        "minor",
        modplayer_capability_gateway::api::API_VERSION.minor,
    )?;
    table.set("api", api_version)?;
    table.set("granted", granted)?;
    table.set(
        "capabilities",
        modplayer_capability_gateway::api::HOST_CAPABILITIES.to_vec(),
    )?;
    Ok(Value::Table(table))
}

fn level_to_lua(lua: &mlua::Lua, level: &LevelInfo) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    table.set("peak_l", level.peak_l)?;
    table.set("peak_r", level.peak_r)?;
    table.set("rms_l", level.rms_l)?;
    table.set("rms_r", level.rms_r)?;
    Ok(Value::Table(table))
}

fn track_info_to_lua(lua: &mlua::Lua, track: &Option<TrackInfo>) -> mlua::Result<Value> {
    match track {
        None => Ok(Value::Nil),
        Some(info) => {
            let table = lua.create_table()?;
            table.set("id", info.id.clone())?;
            table.set("title", info.title.clone())?;
            table.set("artists", info.artists.clone())?;
            table.set("duration_ms", info.duration_ms)?;
            Ok(Value::Table(table))
        }
    }
}

/// A [`WidgetValue`] as the Lua-facing `panel_interaction`/`update_widget`
/// value shape (contracts/plugin-api-v1.2.md §3.1/§4): a plain
/// bool/number/string for the scalar kinds, a `{items, selected}` table
/// for a bulk list replace.
fn widget_value_to_lua(lua: &mlua::Lua, value: &WidgetValue) -> mlua::Result<Value> {
    Ok(match value {
        WidgetValue::Bool(b) => Value::Boolean(*b),
        WidgetValue::Number(n) => Value::Number(*n),
        WidgetValue::Text(s) => Value::String(lua.create_string(s)?),
        WidgetValue::Item(id) => Value::String(lua.create_string(id.as_str())?),
        WidgetValue::Items { items, selected } => {
            let table = lua.create_table()?;
            let list = lua.create_table()?;
            for (i, item) in items.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("id", item.id.as_str())?;
                row.set("label", item.label.as_str())?;
                list.set(i + 1, row)?;
            }
            table.set("items", list)?;
            table.set(
                "selected",
                selected
                    .as_ref()
                    .map(modplayer_capability_gateway::ui::UiId::as_str),
            )?;
            Value::Table(table)
        }
    })
}

fn action_source_str(source: ActionSource) -> &'static str {
    match source {
        ActionSource::Keyboard => "keyboard",
        ActionSource::Ui => "ui",
    }
}

fn unloading_reason_str(reason: UnloadReason) -> &'static str {
    match reason {
        UnloadReason::Disable => "disable",
        UnloadReason::Suspend => "suspend",
        UnloadReason::Shutdown => "shutdown",
    }
}

/// Builds the Lua event name and payload table for every [`HostEvent`]
/// except `ReadyAck` (built separately by [`ready_ack_payload`] since its
/// shape needs a fresh grants read rather than the event's own fields).
fn event_to_lua(lua: &mlua::Lua, event: &HostEvent) -> mlua::Result<(&'static str, Value)> {
    let name = event.kind().name();
    let payload = match event {
        HostEvent::ReadyAck { .. } => {
            debug_assert!(false, "ready_ack is built by ready_ack_payload");
            Value::Nil
        }
        HostEvent::Unloading { reason } => {
            let table = lua.create_table()?;
            table.set("reason", unloading_reason_str(*reason))?;
            Value::Table(table)
        }
        HostEvent::TrackChanged { track } => {
            let table = lua.create_table()?;
            table.set("track", track_info_to_lua(lua, track)?)?;
            Value::Table(table)
        }
        HostEvent::Position { position_ms } => {
            let table = lua.create_table()?;
            table.set("position_ms", *position_ms)?;
            Value::Table(table)
        }
        HostEvent::PlayStateChanged { state } => {
            let table = lua.create_table()?;
            table.set(
                "state",
                match state {
                    PlayState::Playing => "playing",
                    PlayState::Paused => "paused",
                    PlayState::Stopped => "stopped",
                },
            )?;
            Value::Table(table)
        }
        HostEvent::QueueChanged { items } => {
            let table = lua.create_table()?;
            table.set("items", lua.to_value(items)?)?;
            Value::Table(table)
        }
        HostEvent::MarkerChanged { actor, revision } => {
            let table = lua.create_table()?;
            table.set("actor", bindings::owner_to_string(actor))?;
            table.set("revision", *revision)?;
            Value::Table(table)
        }
        HostEvent::LoopArmed { region, by } => {
            let table = lua.create_table()?;
            table.set("region", region.0)?;
            table.set("by", bindings::owner_to_string(by))?;
            Value::Table(table)
        }
        HostEvent::LoopDisarmed { by } => {
            let table = lua.create_table()?;
            table.set("by", bindings::owner_to_string(by))?;
            Value::Table(table)
        }
        HostEvent::LoopWrapped { region, wraps } => {
            let table = lua.create_table()?;
            table.set("region", region.0)?;
            table.set("wraps", *wraps)?;
            Value::Table(table)
        }
        HostEvent::EffectChainChanged { chain } => {
            // R4 (013-key-and-tempo-plugin): `lua.to_value(chain)` cannot
            // serialize `NodeInfo.owner`'s `OwnerInfo::Plugin(String)`
            // newtype variant — this made the event undeliverable for any
            // chain containing a plugin-owned node (`AbortCause::
            // Exception`, no `plugin_log` entry). Built field-by-field via
            // `node_info_to_lua` instead, exactly like `list_chain()`.
            let table = lua.create_table()?;
            let nodes = lua.create_table()?;
            for (i, node) in chain.iter().enumerate() {
                nodes.set(i + 1, node_info_to_lua(lua, node)?)?;
            }
            table.set("nodes", nodes)?;
            Value::Table(table)
        }
        HostEvent::Meter {
            pre,
            post,
            spectrum,
        } => {
            let table = lua.create_table()?;
            table.set("pre", level_to_lua(lua, pre)?)?;
            table.set("post", level_to_lua(lua, post)?)?;
            table.set("spectrum", spectrum.to_vec())?;
            Value::Table(table)
        }
        HostEvent::Timer { handle } => {
            let table = lua.create_table()?;
            table.set("handle", handle.0)?;
            Value::Table(table)
        }
        HostEvent::PositionReached {
            handle,
            position_ms,
        } => {
            let table = lua.create_table()?;
            table.set("handle", handle.0)?;
            table.set("position_ms", *position_ms)?;
            Value::Table(table)
        }
        HostEvent::FocusGranted { holder } | HostEvent::FocusRevoked { holder } => {
            // 010-transport-focus (contracts/plugin-api-v1.1.md §3, RT-F3):
            // delivered straight to a handle (never fanned out), so
            // `owner_to_string` renders "host" or the identifier — never
            // "me".
            let table = lua.create_table()?;
            table.set("holder", bindings::owner_to_string(holder))?;
            Value::Table(table)
        }
        HostEvent::PanelInteraction {
            panel,
            widget,
            value,
        } => {
            let table = lua.create_table()?;
            table.set("panel", panel.as_str())?;
            table.set("widget", widget.as_str())?;
            table.set("value", widget_value_to_lua(lua, value)?)?;
            Value::Table(table)
        }
        HostEvent::ActionInvoked {
            action,
            source,
            value,
        } => {
            let table = lua.create_table()?;
            table.set("action", action.as_str())?;
            table.set("source", action_source_str(*source))?;
            table.set("value", *value)?;
            Value::Table(table)
        }
        HostEvent::SettingsChanged { changes } => {
            let table = lua.create_table()?;
            table.set("changes", lua.to_value(changes)?)?;
            Value::Table(table)
        }
    };
    Ok((name, payload))
}

/// RT11: flush the previous track scope if dirty, then load (or clear)
/// the new one before the `track_changed` handler runs. The load itself
/// (disk read + decode) is bounded to `budgets.handler` (4 ms): an
/// overrun discards whatever was read, leaves the new track's scope
/// empty rather than populated, and reports `HandlerAborted{
/// RestoreTimeout}` — `track_changed` is still delivered right after this
/// returns, regardless.
#[allow(clippy::too_many_arguments)]
fn restore_track_state(
    shared: &SharedHandle,
    track: &Option<TrackInfo>,
    budgets: &Budgets,
    events: &SyncSender<(PluginId, RuntimeEvent)>,
    plugin: PluginId,
) {
    let mut guard = lock(shared);
    if guard.store.is_dirty(Scope::Track) {
        flush_scope(&mut guard, Scope::Track, None);
    }
    match track {
        Some(info) => {
            let start = Instant::now();
            let bytes = guard.deps.paths.as_ref().and_then(|paths| {
                std::fs::read(paths.track_file(&guard.identifier, &info.id)).ok()
            });
            let overrun = start.elapsed() > budgets.handler;
            let _ = guard.store.load_track(
                &info.id,
                if overrun {
                    &[]
                } else {
                    bytes.as_deref().unwrap_or(&[])
                },
            );
            drop(guard);
            if overrun {
                let _ = events.send((
                    plugin,
                    RuntimeEvent::HandlerAborted {
                        cause: AbortCause::RestoreTimeout,
                    },
                ));
            }
        }
        None => guard.store.clear_track_scope(),
    }
}

/// G7: queue a `WriteJob` for `scope` if it is dirty, else ack
/// immediately (a no-op flush still resolves any waiter).
fn flush_scope(guard: &mut MutexGuard<'_, Shared>, scope: Scope, ack: Option<SyncSender<()>>) {
    if !guard.store.is_dirty(scope) {
        if let Some(ack) = ack {
            let _ = ack.send(());
        }
        return;
    }
    let bytes = guard.store.encode(scope);
    guard.store.mark_clean(scope);
    let (Some(writer), Some(paths)) = (&guard.deps.writer, &guard.deps.paths) else {
        if let Some(ack) = ack {
            let _ = ack.send(());
        }
        return;
    };
    let path = match scope {
        Scope::Plugin => paths.plugin_file(&guard.identifier),
        Scope::Settings => paths.settings_file(&guard.identifier),
        Scope::Track => match guard.store.current_track() {
            Some(track_id) => paths.track_file(&guard.identifier, track_id),
            None => {
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
                return;
            }
        },
    };
    let _ = writer.send(WriteJob::Save { path, bytes, ack });
}

/// Dispatch one `HostEvent` to its registered handler, if any (RT3/RT4).
#[allow(clippy::too_many_arguments)]
fn handle_event(
    ctx: &PluginContext,
    shared: &SharedHandle,
    budget: &BudgetState,
    budgets: &Budgets,
    events: &SyncSender<(PluginId, RuntimeEvent)>,
    plugin: PluginId,
    event: HostEvent,
) -> Option<SuspendCause> {
    if let HostEvent::TrackChanged { track } = &event {
        restore_track_state(shared, track, budgets, events, plugin);
        lock(shared).subscriptions.reset_position_edge();
    }
    // R16/FR-026: `request_focus()` called synchronously from inside one
    // of these two handlers counts as a user interaction — the flag is a
    // plain bool on this plugin's own thread, set only for the duration
    // of the handler call and reset whether it returns or aborts.
    let is_interaction = matches!(
        event,
        HostEvent::PanelInteraction { .. } | HostEvent::ActionInvoked { .. }
    );
    let (name, payload) = event_to_lua(&ctx.lua, &event).ok()?;
    if is_interaction {
        lock(shared).in_interaction_handler = true;
    }
    let result = run_and_account(
        ctx,
        budget,
        budgets.handler,
        budgets.share,
        budgets.window,
        events,
        plugin,
        name,
        payload,
    );
    if is_interaction {
        lock(shared).in_interaction_handler = false;
    }
    result
}

/// RT8: run the `unloading` handler (fire-and-forget — the plugin is
/// going away regardless of its outcome), then flush every dirty scope
/// and wait up to `budgets.unload_window` in total for their acks.
#[allow(clippy::too_many_arguments)]
fn run_unloading(
    ctx: &PluginContext,
    shared: &SharedHandle,
    budget: &BudgetState,
    budgets: &Budgets,
    events: &SyncSender<(PluginId, RuntimeEvent)>,
    plugin: PluginId,
    reason: UnloadReason,
) {
    if let Ok((name, payload)) = event_to_lua(&ctx.lua, &HostEvent::Unloading { reason }) {
        let _ = run_and_account(
            ctx,
            budget,
            budgets.handler,
            budgets.share,
            budgets.window,
            events,
            plugin,
            name,
            payload,
        );
    }

    let deadline = Instant::now() + budgets.unload_window;
    let mut waiters = Vec::new();
    {
        let mut guard = lock(shared);
        for scope in [Scope::Plugin, Scope::Track, Scope::Settings] {
            if guard.store.is_dirty(scope) {
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                flush_scope(&mut guard, scope, Some(tx));
                waiters.push(rx);
            }
        }
    }
    for rx in waiters {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let _ = rx.recv_timeout(remaining);
    }
}

/// RT9/RT10: drain due timers, sample position/meter, and deliver the
/// resulting events (structure only — see [`restore_track_state`]'s note
/// on RT11; the cadence refinements RT10 promises land with US3).
#[allow(clippy::too_many_arguments)]
fn tick_scheduled_work(
    ctx: &PluginContext,
    shared: &SharedHandle,
    budget: &BudgetState,
    budgets: &Budgets,
    events: &SyncSender<(PluginId, RuntimeEvent)>,
    plugin: PluginId,
    observes_playback: bool,
    now: Instant,
) -> Option<SuspendCause> {
    let (due_timers, position, position_timers, meter) = {
        let mut guard = lock(shared);

        let generation = guard.deps.playback.track_generation();
        if generation != guard.last_seen_generation {
            guard.last_seen_generation = generation;
            guard.timers.cancel_position_timers();
            guard.subscriptions.reset_position_edge();
        }

        // US3 T095: a seek or loop wrap (independent of a track change)
        // still resets the position "changed" edge so the next sample is
        // delivered even if unchanged from the last one — but leaves
        // position timers running (only a track change cancels those).
        let position_epoch = guard.deps.playback.position_epoch();
        if position_epoch != guard.last_seen_position_epoch {
            guard.last_seen_position_epoch = position_epoch;
            guard.subscriptions.reset_position_edge();
        }

        let due_timers = guard.timers.drain_due(now);

        let position = if observes_playback {
            let source_rate = guard.deps.playback.source_rate().max(1);
            let rt_shared = Arc::clone(&guard.deps.shared);
            guard
                .subscriptions
                .sample_position(&rt_shared, source_rate, now)
        } else {
            None
        };

        let position_timers = position
            .map(|ms| guard.timers.check_position(ms))
            .unwrap_or_default();

        let meter = if guard.subscriptions.meter_enabled() {
            let rt_shared = Arc::clone(&guard.deps.shared);
            guard.subscriptions.sample_meter(&rt_shared, now)
        } else {
            None
        };

        let pending_timers = guard.timers.len();
        guard
            .budget
            .gauges
            .set_pending_timers(pending_timers as u16);

        (due_timers, position, position_timers, meter)
    };

    for handle in due_timers {
        let (name, payload) = event_to_lua(
            &ctx.lua,
            &HostEvent::Timer {
                handle: modplayer_capability_gateway::event::TimerHandle(handle.0),
            },
        )
        .ok()?;
        if let Some(cause) = run_and_account(
            ctx,
            budget,
            budgets.handler,
            budgets.share,
            budgets.window,
            events,
            plugin,
            name,
            payload,
        ) {
            return Some(cause);
        }
    }

    if let Some(ms) = position {
        let (name, payload) =
            event_to_lua(&ctx.lua, &HostEvent::Position { position_ms: ms }).ok()?;
        if let Some(cause) = run_and_account(
            ctx,
            budget,
            budgets.handler,
            budgets.share,
            budgets.window,
            events,
            plugin,
            name,
            payload,
        ) {
            return Some(cause);
        }
        for handle in position_timers {
            let (name, payload) = event_to_lua(
                &ctx.lua,
                &HostEvent::PositionReached {
                    handle: modplayer_capability_gateway::event::TimerHandle(handle.0),
                    position_ms: ms,
                },
            )
            .ok()?;
            if let Some(cause) = run_and_account(
                ctx,
                budget,
                budgets.handler,
                budgets.share,
                budgets.window,
                events,
                plugin,
                name,
                payload,
            ) {
                return Some(cause);
            }
        }
    }

    if let Some((pre, post, spectrum)) = meter {
        let (name, payload) = event_to_lua(
            &ctx.lua,
            &HostEvent::Meter {
                pre,
                post,
                spectrum,
            },
        )
        .ok()?;
        if let Some(cause) = run_and_account(
            ctx,
            budget,
            budgets.handler,
            budgets.share,
            budgets.window,
            events,
            plugin,
            name,
            payload,
        ) {
            return Some(cause);
        }
    }

    None
}

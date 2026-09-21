// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginContext`: one plugin's sandboxed Luau state plus everything its
//! bindings need (RT1, RT2; data-model.md §2).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use mlua::{Lua, LuaOptions, StdLib};

use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::gateway::Gateway;
use modplayer_capability_gateway::state::PluginStateStore;
use modplayer_capability_gateway::ui::SettingsField;

use crate::budget::BudgetState;
use crate::handle::RuntimeDeps;
use crate::pump::Subscriptions;
use crate::timers::TimerSet;

/// Everything a binding closure needs, gathered behind one lock (mlua's
/// `send` feature requires every captured value to be `Send`; a `Mutex`
/// costs nothing here since only this plugin's own scheduler thread ever
/// locks it — never a real contention point).
pub struct Shared {
    pub gateway: Gateway,
    pub store: PluginStateStore,
    pub timers: TimerSet,
    pub subscriptions: Subscriptions,
    pub deps: RuntimeDeps,
    pub budgets: Budgets,
    /// The same `BudgetState` `PluginContext::lua`'s interrupt callback
    /// checks — bindings read it only to push the deadline forward across
    /// an RPC wait (RT7); they never touch the interrupt itself.
    pub budget: Arc<BudgetState>,
    pub identifier: String,
    pub fixtures_enabled: bool,
    /// Set once `api.ready()` succeeds (RT5); no request namespace call
    /// is gated on this directly (the gateway's admission doesn't know
    /// about readiness), but the scheduler consults it before dispatching
    /// any event.
    pub ready: bool,
    /// The `PlaybackSnapshot::track_generation` value the scheduler last
    /// observed (RT9): a change cancels every position timer and resets
    /// the position "changed" edge, independent of whether a
    /// `TrackChanged` event has reached this thread's inbox yet.
    pub last_seen_generation: u64,
    /// The `PlaybackSnapshot::position_epoch` value the scheduler last
    /// observed (US3 T095): a change resets the position "changed" edge
    /// only (position timers are left running, unlike a track change).
    pub last_seen_position_epoch: u64,
    /// 011-plugin-ui-contributions (R4): the schema from this plugin's
    /// last successful `register_settings` RPC, `None` until then —
    /// `get_settings()`'s local binding reads it to substitute each
    /// field's `default` for a missing/invalid stored value.
    pub settings_schema: Option<Vec<SettingsField>>,
    /// R16/FR-026: set only while the scheduler is running a
    /// `panel_interaction`/`action_invoked` handler; `transport.
    /// request_focus`'s binding reads it (and only it) to mark the RPC as
    /// user-interaction-originated.
    pub in_interaction_handler: bool,
    /// The last few track scopes `flush_scope` handed to the writer
    /// thread, newest first, as `(track id, encoded bytes)`. The writer
    /// is asynchronous (fsync + rename), so a `TrackChanged` straight back
    /// to a just-left track can otherwise read the file before the write
    /// has landed and restore an empty scope; `restore_track_state`
    /// consults this before touching disk. Bounded to
    /// [`RECENT_TRACK_FLUSHES`] entries; every entry is already bounded by
    /// `budgets.storage`.
    pub recent_track_flushes: VecDeque<(String, Vec<u8>)>,
}

/// How many recently flushed track scopes [`Shared::recent_track_flushes`]
/// keeps.
pub const RECENT_TRACK_FLUSHES: usize = 8;

/// A context failed to come up (RT5 "script error -> Exited").
#[derive(Debug)]
pub enum ContextError {
    Script(mlua::Error),
    Lua(mlua::Error),
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextError::Script(e) => write!(f, "entry script failed: {e}"),
            ContextError::Lua(e) => write!(f, "failed to set up the Luau state: {e}"),
        }
    }
}

/// One plugin's sandboxed Luau state (RT2): the safe standard library
/// subset only, a 64 MiB allocator-enforced heap cap, a preemptive
/// per-handler CPU interrupt, and the `api` table shaped by its grants —
/// then `sandbox(true)` freezes globals/builtins before the entry script
/// runs.
pub struct PluginContext {
    pub lua: Lua,
    pub budget: Arc<BudgetState>,
    pub shared: Arc<Mutex<Shared>>,
}

impl PluginContext {
    /// RT2: `Lua::new_with(STRING|TABLE|MATH|BIT|UTF8, ...)`,
    /// `set_memory_limit(64 MiB)`, `set_interrupt`, install `api`,
    /// `sandbox(true)`, then load and run `entry_source` once (its
    /// top-level code registers handlers via `api.on(...)`; `api.ready()`
    /// may be called synchronously here or, more commonly, from a
    /// handler once state is set up).
    pub fn new(
        entry_source: &str,
        memory_limit: usize,
        shared: Arc<Mutex<Shared>>,
        budget: Arc<BudgetState>,
    ) -> Result<PluginContext, ContextError> {
        let libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::BIT | StdLib::UTF8;
        let lua = Lua::new_with(libs, LuaOptions::new().catch_rust_panics(true))
            .map_err(ContextError::Lua)?;
        lua.set_memory_limit(memory_limit)
            .map_err(ContextError::Lua)?;

        lua.set_interrupt(BudgetState::interrupt_check(Arc::clone(&budget)));

        crate::bindings::install(&lua, Arc::clone(&shared)).map_err(ContextError::Lua)?;

        lua.sandbox(true).map_err(ContextError::Lua)?;

        lua.load(entry_source)
            .set_name("main.luau")
            .exec()
            .map_err(ContextError::Script)?;

        Ok(PluginContext {
            lua,
            budget,
            shared,
        })
    }
}

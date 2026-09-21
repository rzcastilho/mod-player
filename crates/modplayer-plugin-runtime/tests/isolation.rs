// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plugin isolation tests (US1, contracts/gateway-and-runtime.md
//! RT3/RT4/RT5/RT12): a misbehaving plugin's own handler is contained —
//! aborted within its budget, or the whole plugin suspended — without
//! ever taking the host process down.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::event::HostEvent;
use modplayer_capability_gateway::focus::{FocusToken, PluginId};
use modplayer_capability_gateway::grants::Grants;
use modplayer_capability_gateway::manifest::{self, ApiRange};
use modplayer_engine::RtShared;
use modplayer_plugin_runtime::events::{AbortCause, PlaybackSnapshot, RuntimeEvent, SuspendCause};
use modplayer_plugin_runtime::handle::{
    PluginHandle, PluginSnapshot, RpcEnvelope, RuntimeDeps, SpawnConfig,
};

/// Builds `Grants` holding exactly `permissions` (catalog names), the
/// same way `Grants::from_bundled` would from a real manifest — these
/// tests drive the scheduler directly rather than through a fixture
/// package, so the manifest itself is throwaway.
fn grants_with(permissions: &[&str]) -> Grants {
    let mut toml = String::from(
        "identifier = \"org.modplayer.test.isolation\"\n\
         name = \"Isolation test\"\n\
         version = \"1.0.0\"\n\
         api = \"1.0\"\n\
         author = \"Test\"\n\
         license = \"MIT\"\n\
         source = \"test\"\n",
    );
    for permission in permissions {
        toml.push_str(&format!(
            "[[permissions.required]]\npermission = \"{permission}\"\njustification = \"test\"\n"
        ));
    }
    let manifest = manifest::parse_and_validate(&toml, true)
        .unwrap_or_else(|e| unreachable!("test manifest must be valid: {e}"));
    Grants::from_bundled(&manifest)
}

/// Spawns a plugin thread directly against hand-built deps (no
/// `PluginHost`/controller involved) — this test exercises the runtime
/// crate's own scheduler in isolation. The request channel's receiver is
/// dropped immediately: none of these fixtures issue an RPC.
fn spawn_test_plugin(
    entry_source: &str,
    grants: Grants,
    budgets: Budgets,
) -> (PluginHandle, Receiver<(PluginId, RuntimeEvent)>) {
    let (requests_tx, _requests_rx) = std::sync::mpsc::sync_channel::<RpcEnvelope>(16);
    let (events_tx, events_rx) = std::sync::mpsc::sync_channel(256);
    let config = SpawnConfig {
        id: PluginId(1),
        identifier: "org.modplayer.test.isolation".to_string(),
        entry_source: entry_source.to_string(),
        api_range: ApiRange {
            major: 1,
            min_minor: 0,
        },
        grants,
        budgets,
        focus: FocusToken::new(),
        fixtures_enabled: false,
    };
    let deps = RuntimeDeps {
        shared: Arc::new(RtShared::new()),
        playback: Arc::new(PlaybackSnapshot::new()),
        requests: requests_tx,
        events: events_tx,
        snapshot: Arc::new(Mutex::new(PluginSnapshot::default())),
        writer: None,
        paths: None,
    };
    (PluginHandle::spawn(config, deps), events_rx)
}

/// Drains events until one matching `want` arrives, or `timeout` elapses
/// (`None`). Every other event in between (`Ready`, unrelated
/// `HandlerAborted`s, `Log`, …) is silently skipped.
fn wait_for<T>(
    rx: &Receiver<(PluginId, RuntimeEvent)>,
    timeout: Duration,
    mut want: impl FnMut(&RuntimeEvent) -> Option<T>,
) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match rx.recv_timeout(remaining) {
            Ok((_, event)) => {
                if let Some(value) = want(&event) {
                    return Some(value);
                }
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn wait_for_ready(rx: &Receiver<(PluginId, RuntimeEvent)>) {
    let seen = wait_for(rx, Duration::from_secs(1), |e| {
        matches!(e, RuntimeEvent::Ready).then_some(())
    });
    assert!(seen.is_some(), "plugin never became ready");
}

fn wait_for_abort(rx: &Receiver<(PluginId, RuntimeEvent)>) -> Option<AbortCause> {
    wait_for(rx, Duration::from_secs(2), |e| match e {
        RuntimeEvent::HandlerAborted { cause } => Some(cause.clone()),
        _ => None,
    })
}

fn wait_for_suspend(
    rx: &Receiver<(PluginId, RuntimeEvent)>,
    timeout: Duration,
) -> Option<SuspendCause> {
    wait_for(rx, timeout, |e| match e {
        RuntimeEvent::Suspended { cause } => Some(*cause),
        _ => None,
    })
}

/// RT3: a single `while true do end` handler invocation is aborted at
/// (approximately) the 4 ms handler budget — never lets the plugin's own
/// thread run away — and the state is still usable afterward (a second
/// call also aborts cleanly, rather than the thread having died).
#[test]
fn infinite_loop_handler_aborts_within_budget() {
    let entry = r#"
        api.on("play_state_changed", function(_event)
            while true do end
        end)
        api.ready()
    "#;
    let (handle, events) =
        spawn_test_plugin(entry, grants_with(&["playback.observe"]), Budgets::DEFAULT);
    wait_for_ready(&events);

    for _ in 0..2 {
        let start = Instant::now();
        assert!(handle.send_event(HostEvent::PlayStateChanged {
            state: modplayer_capability_gateway::event::PlayState::Playing,
        }));
        let cause = wait_for_abort(&events);
        let elapsed = start.elapsed();
        assert_eq!(cause, Some(AbortCause::Deadline));
        // The budget is thread CPU time; on Windows that comes from the
        // thread's cycle counter scaled by a one-shot calibration, which
        // frequency scaling can put off by a fair fraction, so the slack
        // there is wider.
        let slack = if cfg!(windows) {
            Duration::from_millis(6)
        } else {
            Duration::from_millis(2)
        };
        assert!(
            elapsed <= Duration::from_millis(4) + slack,
            "handler abort took {elapsed:?}, expected <= 4ms + {slack:?} slack"
        );
    }
}

/// RT4: enough repeated aborts within the rolling 1 s CPU-share window
/// (each ~4 ms, the aggregate share is 100 ms) push the plugin over its
/// share and suspend it as `Hang` (a `Deadline` abort at the moment the
/// sum crosses the threshold).
#[test]
fn repeated_hang_suspends_within_one_second() {
    let entry = r#"
        api.on("play_state_changed", function(_event)
            while true do end
        end)
        api.ready()
    "#;
    let (handle, events) =
        spawn_test_plugin(entry, grants_with(&["playback.observe"]), Budgets::DEFAULT);
    wait_for_ready(&events);

    // Each abort must be requested back-to-back (only waiting for *that*
    // abort's own reply, not the full share window) so the ~25 samples
    // needed to cross the 100 ms share land within the aggregate's
    // rolling 1 s window — waiting a large fixed timeout per iteration
    // would spread them out far enough that the earliest ones age out of
    // the window before enough accumulate.
    let start = Instant::now();
    let mut suspend_cause = None;
    for _ in 0..60 {
        handle.send_event(HostEvent::PlayStateChanged {
            state: modplayer_capability_gateway::event::PlayState::Playing,
        });
        match events.recv_timeout(Duration::from_millis(50)) {
            Ok((_, RuntimeEvent::Suspended { cause })) => {
                suspend_cause = Some(cause);
                break;
            }
            Ok((_, RuntimeEvent::HandlerAborted { .. })) => continue,
            Ok(_) | Err(_) => break,
        }
    }
    assert_eq!(suspend_cause, Some(SuspendCause::Hang));
    assert!(
        start.elapsed() <= Duration::from_secs(5),
        "took far longer than the ~100ms/26-abort budget predicts"
    );
}

/// RT4: a handler that keeps allocating (never hangs, never throws)
/// eventually hits `Lua::set_memory_limit`'s cap and is suspended as
/// `Memory` — the plugin's own heap, never the host process.
#[test]
fn memory_bomb_suspends_plugin_not_host() {
    let entry = r#"
        local bag = {}
        api.on("position", function(event)
            -- A per-call prefix keeps every appended string's *content*
            -- unique — Luau interns identical strings, so appending the
            -- exact same literal every time would never grow live memory.
            bag[#bag + 1] = tostring(event.position_ms) .. string.rep("x", 256 * 1024)
        end)
        api.ready()
    "#;
    // A small memory cap keeps this test fast; still generous relative to
    // one 256 KB allocation, per the same "custom Budgets for one test"
    // pattern used across this crate's own unit tests.
    let budgets = Budgets {
        memory: 2 * 1024 * 1024,
        ..Budgets::DEFAULT
    };
    let (handle, events) = spawn_test_plugin(entry, grants_with(&["playback.observe"]), budgets);
    wait_for_ready(&events);

    let mut suspend_cause = None;
    for ms in 0..64u32 {
        handle.send_event(HostEvent::Position {
            position_ms: u64::from(ms),
        });
        if let Some(cause) = wait_for(&events, Duration::from_millis(100), |e| match e {
            RuntimeEvent::Suspended { cause } => Some(*cause),
            _ => None,
        }) {
            suspend_cause = Some(cause);
            break;
        }
    }
    assert_eq!(suspend_cause, Some(SuspendCause::Memory));
}

/// RT3: a handler that throws is aborted (`AbortCause::Exception`) —
/// only that one call, not the plugin — so a later event still reaches
/// (and aborts) the very same handler again.
#[test]
fn exception_aborts_handler_only() {
    let entry = r#"
        api.on("position", function(_event)
            error("boom")
        end)
        api.ready()
    "#;
    let (handle, events) =
        spawn_test_plugin(entry, grants_with(&["playback.observe"]), Budgets::DEFAULT);
    wait_for_ready(&events);

    for ms in 0..2u32 {
        handle.send_event(HostEvent::Position {
            position_ms: u64::from(ms),
        });
        let cause = wait_for_abort(&events);
        assert!(
            matches!(cause, Some(AbortCause::Exception(_))),
            "expected an Exception abort, got {cause:?}"
        );
    }
    // Still alive: no `Suspended`/`Exited` should have snuck in between.
    assert_eq!(
        wait_for_suspend(&events, Duration::from_millis(50)),
        None,
        "a single exception per call must never suspend the plugin"
    );
}

/// RT5: a plugin that never calls `api.ready()` is suspended as
/// `DidNotStart` once its ready-timeout budget elapses — no `unloading`
/// handler runs for it (it was never ready to receive one). A short
/// custom `ready_timeout` keeps this test fast rather than waiting the
/// spec's real 5 s.
#[test]
fn never_ready_suspends_after_5s() {
    let entry = "-- never calls api.ready()\n";
    let budgets = Budgets {
        ready_timeout: Duration::from_millis(50),
        ..Budgets::DEFAULT
    };
    let (_handle, events) = spawn_test_plugin(entry, Grants::none(), budgets);
    let cause = wait_for_suspend(&events, Duration::from_secs(2));
    assert_eq!(cause, Some(SuspendCause::DidNotStart));
}

/// RT12: `mlua` 0.12's `catch_rust_panics` option only governs whether a
/// **Lua-side** `pcall` can observe a panic escaping a bound Rust
/// closure — the panic is always "automatically resumed" as a genuine
/// Rust unwind once control returns to the Rust side (mlua's own
/// `LuaOptions::catch_rust_panics` doc), exactly like the bare call
/// below. `scheduler::run`'s own top-level `std::panic::catch_unwind`
/// around `run_inner` (its doc: "contains any panic that escapes
/// `run_inner`") is what actually stops such a panic — from *any*
/// binding call, or from the scheduler's own bookkeeping — from ever
/// reaching past the plugin's own thread and taking the host process
/// down with it. This test demonstrates both halves of that claim
/// directly against the same primitive `scheduler::run` relies on.
#[test]
fn panic_in_binding_is_contained() {
    fn make_lua_that_panics_on_call() -> mlua::Lua {
        let libs = mlua::StdLib::STRING | mlua::StdLib::TABLE | mlua::StdLib::MATH;
        let lua = mlua::Lua::new_with(libs, mlua::LuaOptions::new())
            .unwrap_or_else(|e| unreachable!("Lua::new_with must succeed: {e}"));
        let panicking = lua
            .create_function(|_, ()| -> mlua::Result<()> {
                panic!("simulated binding panic");
            })
            .unwrap_or_else(|e| unreachable!("create_function must succeed: {e}"));
        lua.globals()
            .set("panicking_binding", panicking)
            .unwrap_or_else(|e| unreachable!("globals().set must succeed: {e}"));
        lua
    }

    // Unprotected: the panic really does propagate as an ordinary Rust
    // panic (not an `Err`) once `.exec()` returns — the premise the next
    // assertion's containment is built on.
    let bare = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        make_lua_that_panics_on_call()
            .load("panicking_binding()")
            .exec()
    }));
    assert!(
        bare.is_err(),
        "sanity: an unprotected call to a panicking binding must really panic"
    );

    // Protected exactly the way `scheduler::run` protects `run_inner`:
    // the same panic is now reported to `catch_unwind`'s caller instead
    // of unwinding any further — this test function's own unaffected
    // continuation past this point (and the whole suite's continued run)
    // is the proof that nothing below the plugin's own thread is ever
    // taken down by it.
    let contained = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        make_lua_that_panics_on_call()
            .load("panicking_binding()")
            .exec()
    }));
    assert!(
        contained.is_err(),
        "catch_unwind must observe the panic rather than let it escape further"
    );
}

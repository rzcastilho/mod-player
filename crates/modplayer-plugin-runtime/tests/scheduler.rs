// SPDX-License-Identifier: MIT OR Apache-2.0

//! Scheduler tests — US1 subset (contracts/gateway-and-runtime.md RT6-RT8):
//! events are delivered in order on a plugin's own thread, an RPC wait
//! never counts against its CPU budget (but a host that never replies is
//! still bounded), and `unloading` commits dirty state within its 200 ms
//! window. The US3 subset (below `mod us3`) covers position/meter
//! cadence, timers (RT9) and track-state restore (RT11).

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::event::{ActionSource, HostEvent, TrackInfo, UnloadReason};
use modplayer_capability_gateway::focus::{FocusToken, PluginId};
use modplayer_capability_gateway::grants::Grants;
use modplayer_capability_gateway::manifest::{self, ApiRange};
use modplayer_capability_gateway::request::{OwnerInfo, Request, Response};
use modplayer_capability_gateway::state::PluginStatePaths;
use modplayer_capability_gateway::ui::WidgetValue;
use modplayer_engine::RtShared;
use modplayer_plugin_runtime::events::{AbortCause, PlaybackSnapshot, RuntimeEvent};
use modplayer_plugin_runtime::handle::{
    Control, PluginHandle, PluginSnapshot, RpcEnvelope, RuntimeDeps, SpawnConfig,
};

fn grants_with(permissions: &[&str]) -> Grants {
    let mut toml = String::from(
        "identifier = \"org.modplayer.test.scheduler\"\n\
         name = \"Scheduler test\"\n\
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

/// As `isolation.rs`'s own harness, but keeps the request-channel
/// receiver (rather than dropping it) so a test can answer — or
/// deliberately never answer — an RPC itself, and takes an explicit
/// `writer`/`paths` pair so `unloading_writes_committed_within_200ms` can
/// inspect the real file a `Control::Unloading` flush produces.
fn spawn_test_plugin(
    entry_source: &str,
    grants: Grants,
    budgets: Budgets,
    writer: Option<std::sync::mpsc::Sender<modplayer_capability_gateway::state::WriteJob>>,
    paths: Option<PluginStatePaths>,
) -> (
    PluginHandle,
    Receiver<(PluginId, RuntimeEvent)>,
    Receiver<RpcEnvelope>,
) {
    let (handle, events, requests, _shared, _playback, _focus) =
        spawn_test_plugin_ex(entry_source, grants, budgets, writer, paths);
    (handle, events, requests)
}

/// As [`spawn_test_plugin`], but also hands back the `Arc<RtShared>`/
/// `Arc<PlaybackSnapshot>` the US3 subset's own tests drive directly
/// (position sampling, track-generation/position-epoch bumps).
#[allow(clippy::type_complexity)]
fn spawn_test_plugin_ex(
    entry_source: &str,
    grants: Grants,
    budgets: Budgets,
    writer: Option<std::sync::mpsc::Sender<modplayer_capability_gateway::state::WriteJob>>,
    paths: Option<PluginStatePaths>,
) -> (
    PluginHandle,
    Receiver<(PluginId, RuntimeEvent)>,
    Receiver<RpcEnvelope>,
    Arc<RtShared>,
    Arc<PlaybackSnapshot>,
    FocusToken,
) {
    let (requests_tx, requests_rx) = std::sync::mpsc::sync_channel::<RpcEnvelope>(16);
    let (events_tx, events_rx) = std::sync::mpsc::sync_channel(256);
    let shared = Arc::new(RtShared::new());
    let playback = Arc::new(PlaybackSnapshot::new());
    let focus = FocusToken::new();
    let config = SpawnConfig {
        id: PluginId(1),
        identifier: "org.modplayer.test.scheduler".to_string(),
        entry_source: entry_source.to_string(),
        api_range: ApiRange {
            major: 1,
            min_minor: 0,
        },
        grants,
        budgets,
        focus: focus.clone(),
        fixtures_enabled: false,
    };
    let deps = RuntimeDeps {
        shared: Arc::clone(&shared),
        playback: Arc::clone(&playback),
        requests: requests_tx,
        events: events_tx,
        snapshot: Arc::new(Mutex::new(PluginSnapshot::default())),
        writer,
        paths,
    };
    (
        PluginHandle::spawn(config, deps),
        events_rx,
        requests_rx,
        shared,
        playback,
        focus,
    )
}

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

fn wait_for_log(rx: &Receiver<(PluginId, RuntimeEvent)>, timeout: Duration) -> Option<String> {
    wait_for(rx, timeout, |e| match e {
        RuntimeEvent::Log { message, .. } => Some(message.clone()),
        _ => None,
    })
}

/// RT6/contract §4 "events in order": handlers run on the plugin's own
/// thread strictly in the order their events were sent — three `position`
/// events, each logging its own `position_ms`, come back as three `Log`s
/// in the same order.
#[test]
fn events_in_order() {
    let entry = r#"
        api.on("position", function(event)
            api.log.info(tostring(event.position_ms))
        end)
        api.ready()
    "#;
    let (handle, events, _requests) =
        spawn_test_plugin(entry, Grants::none(), Budgets::DEFAULT, None, None);
    wait_for_ready(&events);

    for ms in [10u64, 20, 30] {
        handle.send_event(HostEvent::Position { position_ms: ms });
    }
    let mut seen = Vec::new();
    for _ in 0..3 {
        seen.push(wait_for_log(&events, Duration::from_secs(1)).unwrap_or_default());
    }
    assert_eq!(seen, vec!["10", "20", "30"]);
}

/// 010-transport-focus (contracts/plugin-api-v1.1.md RT-F3): `holder` on
/// both new events renders with `owner_to_string` — `"host"` or the
/// identifier, never `"me"` (these are delivered straight to a handle,
/// bypassing the fan-out's self-substitution).
#[test]
fn focus_events_render_holder() {
    let entry = r#"
        api.on("focus_granted", function(event)
            api.log.info("granted:" .. event.holder)
        end)
        api.on("focus_revoked", function(event)
            api.log.info("revoked:" .. event.holder)
        end)
        api.ready()
    "#;
    let (handle, events, _requests) =
        spawn_test_plugin(entry, Grants::none(), Budgets::DEFAULT, None, None);
    wait_for_ready(&events);

    handle.send_event(HostEvent::FocusGranted {
        holder: OwnerInfo::Plugin("org.modplayer.test.scheduler".to_string()),
    });
    handle.send_event(HostEvent::FocusRevoked {
        holder: OwnerInfo::Host,
    });

    let first = wait_for_log(&events, Duration::from_secs(1)).unwrap_or_default();
    let second = wait_for_log(&events, Duration::from_secs(1)).unwrap_or_default();
    assert_eq!(first, "granted:org.modplayer.test.scheduler");
    assert_eq!(second, "revoked:host");
}

/// Blocks on `requests_rx` for the next `RpcEnvelope`, waits `delay`, then
/// replies `Ok(Response::Ok)` — simulates a slow (or, with a long delay
/// against a short `rpc_timeout` budget, unresponsive) host answering the
/// plugin's own `transport.play()` RPC.
fn answer_next_rpc_after(requests: &Receiver<RpcEnvelope>, delay: Duration) -> Instant {
    let envelope = requests
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_else(|e| unreachable!("expected one RpcEnvelope: {e}"));
    std::thread::sleep(delay);
    let replied_at = Instant::now();
    let _ = envelope.reply.send(Ok(Response::Ok));
    replied_at
}

/// RT7: the time a handler spends blocked on an admitted RPC's reply is
/// excluded from its CPU sample — a host that takes 50 ms to answer
/// `transport.play()` (well past the 4 ms handler budget if it were
/// counted as CPU) neither aborts the handler nor moves its share gauge
/// meaningfully.
#[test]
fn rpc_wait_excluded_from_cpu() {
    let entry = r#"
        api.on("play_state_changed", function(_event)
            api.transport.play()
        end)
        api.ready()
    "#;
    let (handle, events, requests, _shared, _playback, focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["transport.control"]),
        Budgets::DEFAULT,
        None,
        None,
    );
    // Grant this plugin focus directly (010-transport-focus, research R2):
    // `request_focus()` is now an RPC to core whose grant depends on the
    // user's policy, so a bare binding-level harness with no controller
    // sets the token itself rather than exercising that RPC.
    focus.set_holder(Some(PluginId(1)));
    wait_for_ready(&events);

    handle.send_event(HostEvent::PlayStateChanged {
        state: modplayer_capability_gateway::event::PlayState::Playing,
    });
    answer_next_rpc_after(&requests, Duration::from_millis(50));

    let aborted = wait_for(&events, Duration::from_millis(200), |e| {
        matches!(
            e,
            RuntimeEvent::HandlerAborted { .. } | RuntimeEvent::Suspended { .. }
        )
        .then_some(())
    });
    assert!(
        aborted.is_none(),
        "a 50ms RPC wait must never abort/suspend the handler that issued it"
    );
    // permille of the 100ms share; a wait-excluded handler's real CPU
    // cost here is microseconds, nowhere near even 10% of the share.
    assert!(
        handle.gauges.cpu_permille_of_share() < 100,
        "RPC wait time leaked into the CPU share gauge: {}",
        handle.gauges.cpu_permille_of_share()
    );
}

/// A slow handler's own event queue is delayed *behind* it (single
/// thread, strict order) — a second event queued right after the slow
/// one only gets processed once the slow handler's RPC reply actually
/// lands, never before.
#[test]
fn slow_handler_delays_only_own_events() {
    let entry = r#"
        api.on("play_state_changed", function(_event)
            api.transport.play()
            api.log.info("slow-done")
        end)
        api.on("position", function(event)
            api.log.info("position:" .. tostring(event.position_ms))
        end)
        api.ready()
    "#;
    let (handle, events, requests, _shared, _playback, focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["transport.control", "playback.observe"]),
        Budgets::DEFAULT,
        None,
        None,
    );
    focus.set_holder(Some(PluginId(1)));
    wait_for_ready(&events);

    handle.send_event(HostEvent::PlayStateChanged {
        state: modplayer_capability_gateway::event::PlayState::Playing,
    });
    handle.send_event(HostEvent::Position { position_ms: 42 });

    let replied_at = answer_next_rpc_after(&requests, Duration::from_millis(80));

    let first = wait_for_log(&events, Duration::from_secs(1)).unwrap_or_default();
    let second_seen_at = Instant::now();
    let second = wait_for_log(&events, Duration::from_secs(1)).unwrap_or_default();

    assert_eq!(
        first, "slow-done",
        "the slow handler's own log must come first"
    );
    assert_eq!(second, "position:42");
    assert!(
        second_seen_at >= replied_at,
        "the queued position event must not be processed before the slow RPC reply"
    );
}

/// RT7: a host that never answers an RPC within `rpc_timeout` gets
/// `nil, { code = "invalid_state", reason = "host_busy" }` back — a short
/// custom timeout keeps this test fast rather than waiting the spec's
/// real 1 s.
#[test]
fn rpc_timeout_is_host_busy() {
    let entry = r#"
        api.on("play_state_changed", function(_event)
            local ok, err = api.transport.play()
            if not ok then
                api.log.info("refusal:" .. err.reason)
            end
        end)
        api.ready()
    "#;
    let budgets = Budgets {
        rpc_timeout: Duration::from_millis(30),
        ..Budgets::DEFAULT
    };
    let (handle, events, _requests, _shared, _playback, focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["transport.control"]),
        budgets,
        None,
        None,
    );
    focus.set_holder(Some(PluginId(1)));
    wait_for_ready(&events);

    handle.send_event(HostEvent::PlayStateChanged {
        state: modplayer_capability_gateway::event::PlayState::Playing,
    });
    // `_requests` is intentionally never drained — the RPC is left to
    // time out on its own.
    let message = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(message.as_deref(), Some("refusal:host_busy"));
}

/// RT8: `Control::Unloading` runs the `unloading` handler, then flushes
/// every dirty scope and waits (bounded by `unload_window`) for its ack
/// — the write is genuinely committed to disk, not merely queued, well
/// within 200 ms.
#[test]
fn unloading_writes_committed_within_200ms() {
    let entry = r#"
        api.state.plugin.set("greeting", "hello")
        api.ready()
    "#;
    let dir = std::env::temp_dir().join(format!(
        "modplayer-scheduler-unloading-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::create_dir_all(&dir);
    let paths = PluginStatePaths::with_dir(dir.clone());
    let writer = modplayer_capability_gateway::state::StateWriter::spawn();
    let writer_sender = writer.sender();

    let (handle, events, _requests) = spawn_test_plugin(
        entry,
        grants_with(&["state.plugin"]),
        Budgets::DEFAULT,
        Some(writer_sender),
        Some(paths.clone()),
    );
    wait_for_ready(&events);

    let start = Instant::now();
    assert!(handle.send_control(Control::Unloading {
        reason: UnloadReason::Disable,
    }));
    let exited = wait_for(&events, Duration::from_millis(500), |e| {
        matches!(e, RuntimeEvent::Exited).then_some(())
    });
    assert!(exited.is_some(), "plugin never reported Exited");
    assert!(
        start.elapsed() <= Duration::from_millis(500),
        "unloading took {:?}, expected comfortably within its 200ms budget",
        start.elapsed()
    );

    let file = paths.plugin_file("org.modplayer.test.scheduler");
    let contents = std::fs::read_to_string(&file)
        .unwrap_or_else(|e| unreachable!("expected {file:?} to exist: {e}"));
    assert!(contents.contains("hello"), "file contents: {contents}");

    writer.join();
    let _ = std::fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------------
// 011-plugin-ui-contributions Foundational subset (T029): the three new
// `HostEvent` payload renderers, `Control::SettingsWrite`'s store-then-
// dispatch ordering, and the interaction-origin flag (R16/FR-026).
// -----------------------------------------------------------------------

/// data-model.md §1.5 / contracts/plugin-api-v1.2.md §4: `panel_interaction`
/// renders `panel`, `widget` and `value` (a plain scalar for the
/// non-bulk `WidgetValue` variants) on the Lua side.
#[test]
fn panel_interaction_payload() {
    let entry = r#"
        api.on("panel_interaction", function(event)
            api.log.info("panel=" .. event.panel .. " widget=" .. event.widget .. " value=" .. tostring(event.value))
        end)
        api.ready()
    "#;
    let (handle, events, _requests) =
        spawn_test_plugin(entry, Grants::none(), Budgets::DEFAULT, None, None);
    wait_for_ready(&events);

    assert!(handle.send_event(HostEvent::PanelInteraction {
        panel: "main".to_string(),
        widget: "toggle1".to_string(),
        value: WidgetValue::Bool(true),
    }));

    let message = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(
        message.as_deref(),
        Some("panel=main widget=toggle1 value=true")
    );
}

/// data-model.md §1.5: `action_invoked` renders `action`, `source`
/// (`"keyboard"` | `"ui"`) and `value` (`nil` for a `Trigger` action).
#[test]
fn action_invoked_payload() {
    let entry = r#"
        api.on("action_invoked", function(event)
            api.log.info("action=" .. event.action .. " source=" .. event.source .. " value=" .. tostring(event.value))
        end)
        api.ready()
    "#;
    let (handle, events, _requests) =
        spawn_test_plugin(entry, Grants::none(), Budgets::DEFAULT, None, None);
    wait_for_ready(&events);

    assert!(handle.send_event(HostEvent::ActionInvoked {
        action: "org.modplayer.fixture.ui-shortcuts.take_over".to_string(),
        source: ActionSource::Keyboard,
        value: None,
    }));

    let message = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(
        message.as_deref(),
        Some("action=org.modplayer.fixture.ui-shortcuts.take_over source=keyboard value=nil")
    );
}

/// R5: `Control::SettingsWrite` applies `store.set(Scope::Settings, ..)`
/// for every change *before* dispatching `settings_changed` — a plugin
/// reading its own settings back from inside that handler (`get_settings`,
/// served locally) already observes the new value, never the old one.
#[test]
fn settings_changed_after_write() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            api.ui.register_settings({
                { id = "f1", kind = "boolean", label = "F1", default = false },
            })
        end)
        api.on("settings_changed", function(event)
            local settings = api.ui.get_settings()
            api.log.info("changed_f1=" .. tostring(event.changes.f1))
            api.log.info("stored_f1=" .. tostring(settings.f1))
        end)
        api.ready()
    "#;
    let (handle, events, requests) = spawn_test_plugin(
        entry,
        grants_with(&["ui.settings"]),
        Budgets::DEFAULT,
        None,
        None,
    );
    wait_for_ready(&events);

    let envelope = requests
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|e| unreachable!("register_settings never reached the RPC channel: {e}"));
    assert!(matches!(envelope.request, Request::RegisterSettings { .. }));
    let _ = envelope.reply.send(Ok(Response::Ok));

    let mut changes = std::collections::BTreeMap::new();
    changes.insert("f1".to_string(), serde_json::Value::Bool(true));
    assert!(handle.send_control(Control::SettingsWrite { changes }));

    let first = wait_for_log(&events, Duration::from_secs(1)).unwrap_or_default();
    let second = wait_for_log(&events, Duration::from_secs(1)).unwrap_or_default();
    assert_eq!(first, "changed_f1=true");
    assert_eq!(
        second, "stored_f1=true",
        "the store write must be visible to the handler that observes its own event"
    );
}

/// R16/FR-026: `request_focus()` called synchronously from inside a
/// `panel_interaction` (or `action_invoked`) handler sends
/// `Request::RequestFocus { interaction: true }`; called from any other
/// handler (here, `ready_ack`), it sends `interaction: false`.
#[test]
fn request_focus_flag_inside_interaction_handler() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            local ok = api.transport.request_focus()
            api.log.info("ready_request_focus:" .. tostring(ok))
        end)
        api.on("panel_interaction", function(_event)
            local ok = api.transport.request_focus()
            api.log.info("interaction_request_focus:" .. tostring(ok))
        end)
        api.ready()
    "#;
    let (handle, events, requests) = spawn_test_plugin(
        entry,
        grants_with(&["transport.control", "ui.panel"]),
        Budgets::DEFAULT,
        None,
        None,
    );
    wait_for_ready(&events);

    let ready_envelope = requests
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_else(|e| unreachable!("ready_ack's request_focus never arrived: {e}"));
    let Request::RequestFocus { interaction } = ready_envelope.request else {
        unreachable!(
            "expected Request::RequestFocus, got {:?}",
            ready_envelope.request
        );
    };
    assert!(
        !interaction,
        "a request_focus() outside an interaction handler must not carry the flag"
    );
    let _ = ready_envelope.reply.send(Ok(Response::Ok));

    assert!(handle.send_event(HostEvent::PanelInteraction {
        panel: "main".to_string(),
        widget: "btn".to_string(),
        value: WidgetValue::Bool(true),
    }));

    let interaction_envelope = requests
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_else(|e| unreachable!("panel_interaction's request_focus never arrived: {e}"));
    let Request::RequestFocus { interaction } = interaction_envelope.request else {
        unreachable!(
            "expected Request::RequestFocus, got {:?}",
            interaction_envelope.request
        );
    };
    assert!(
        interaction,
        "request_focus() called from inside panel_interaction must carry the flag"
    );
    let _ = interaction_envelope.reply.send(Ok(Response::Ok));

    let ready_log = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(ready_log.as_deref(), Some("ready_request_focus:true"));
    let interaction_log = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(
        interaction_log.as_deref(),
        Some("interaction_request_focus:true")
    );
}

// -----------------------------------------------------------------------
// US3 subset (T098): position/meter cadence (RT10), timers (RT9),
// track-state restore (RT11).
// -----------------------------------------------------------------------

fn temp_state_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "modplayer-scheduler-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// contracts/plugin-api-v1.md §3 `subscribe_position`: an out-of-range
/// rate is silently clamped (never a refusal) and, whatever the rate, a
/// static (unchanged) position is delivered exactly once, never
/// redelivered on every subsequent "due" tick — the coalescing half of
/// T095/RT10.
#[test]
fn position_rate_clamped_and_coalesced() {
    let entry = r#"
        local ok = api.playback.subscribe_position(200)
        api.on("position", function(event)
            api.log.info("position:" .. tostring(event.position_ms))
        end)
        api.ready()
    "#;
    let (_handle, events, _requests, shared, _playback, _focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["playback.observe"]),
        Budgets::DEFAULT,
        None,
        None,
    );
    shared.write_anchor(0, Instant::now(), false, 1024, 1.0);
    wait_for_ready(&events);

    let first = wait_for_log(&events, Duration::from_millis(500));
    assert_eq!(first.as_deref(), Some("position:0"));

    let second = wait_for_log(&events, Duration::from_millis(300));
    assert!(
        second.is_none(),
        "an unchanged position must not be redelivered even at a (clamped) high rate, got {second:?}"
    );
}

/// As `position_rate_clamped_and_coalesced`, framed as the spec's own
/// "silent while paused" wording (contracts/plugin-api-v1.md §4
/// `position`): once delivered, a paused/unchanging position stays
/// silent indefinitely, not just briefly.
#[test]
fn position_silent_while_paused() {
    let entry = r#"
        api.playback.subscribe_position(10)
        api.on("position", function(event)
            api.log.info("position:" .. tostring(event.position_ms))
        end)
        api.ready()
    "#;
    let (_handle, events, _requests, shared, _playback, _focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["playback.observe"]),
        Budgets::DEFAULT,
        None,
        None,
    );
    shared.write_anchor(44_100, Instant::now(), false, 1024, 1.0);
    wait_for_ready(&events);

    let first = wait_for_log(&events, Duration::from_millis(500));
    assert!(first.is_some(), "the first sample must still be delivered");

    let mut extra = 0;
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        if wait_for_log(&events, Duration::from_millis(50)).is_some() {
            extra += 1;
        }
    }
    assert_eq!(extra, 0, "a paused/unchanging position must stay silent");
}

/// NFR-1.9: consecutive `position` deliveries at a fixed subscribed rate
/// land close to their nominal interval — measured as the *median*
/// inter-delivery gap across several samples (rather than every single
/// gap) to stay robust to incidental scheduler noise. The spec's own 5 ms
/// figure (research.md R5, recorded from a dedicated, uncontended manual
/// run — quickstart.md M6/T118) is generously widened here since this
/// automated test's thread can be preempted by every other test running
/// alongside it; this is a coarse regression guard against gross timer
/// starvation, not the authoritative jitter measurement.
#[test]
#[cfg(not(windows))]
fn position_jitter_under_5ms() {
    let entry = r#"
        api.playback.subscribe_position(20)
        api.on("position", function(event)
            api.log.info("position:" .. tostring(event.position_ms))
        end)
        api.ready()
    "#;
    let (_handle, events, _requests, shared, playback, _focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["playback.observe"]),
        Budgets::DEFAULT,
        None,
        None,
    );
    playback.set_source_rate(44_100);
    // `PositionClock::now` caps its while-playing extrapolation at
    // `buffer_frames * 2` (engine-delta.md §3) — a huge `buffer_frames`
    // here (never a real buffer size, just this cap's input) keeps the
    // position advancing for this test's whole window instead of
    // freezing after ~46ms the way a realistic buffer size would.
    shared.write_anchor(0, Instant::now(), true, 10_000_000, 1.0);
    wait_for_ready(&events);

    let mut arrivals = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    while arrivals.len() < 7 && Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(500)) {
            Ok((_, RuntimeEvent::Log { message, .. })) if message.starts_with("position:") => {
                arrivals.push(Instant::now());
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(
        arrivals.len() >= 5,
        "expected several position deliveries, got {}",
        arrivals.len()
    );
    let mut deltas: Vec<Duration> = arrivals.windows(2).map(|w| w[1] - w[0]).collect();
    deltas.sort();
    let median = deltas[deltas.len() / 2];
    let expected = Duration::from_millis(50);
    let jitter = median.abs_diff(expected);
    assert!(
        jitter <= Duration::from_millis(25),
        "median inter-delivery gap {median:?} deviates {jitter:?} from the expected {expected:?}"
    );
}

/// RT9: `schedule_at_position` fires exactly once, once the sampled
/// position reaches (or passes) its target — and the event carries the
/// *actual* sampled position, not the target itself.
#[test]
fn schedule_at_position_fires_once_with_actual_position() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            api.playback.subscribe_position(30)
            api.timers.schedule_at_position(500)
        end)
        api.on("position_reached", function(event)
            api.log.info("reached:" .. tostring(event.position_ms))
        end)
        api.ready()
    "#;
    let (_handle, events, _requests, shared, playback, _focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["playback.observe"]),
        Budgets::DEFAULT,
        None,
        None,
    );
    playback.set_source_rate(44_100);
    shared.write_anchor(0, Instant::now(), false, 1024, 1.0);
    wait_for_ready(&events);

    // Move the (still paused) anchor to 600ms worth of frames — past the
    // 500ms target — so the very next position sample crosses it.
    shared.write_anchor(44_100 * 600 / 1000, Instant::now(), false, 1024, 1.0);

    let message = wait_for_log(&events, Duration::from_secs(1));
    let Some(message) = message else {
        unreachable!("position_reached was never delivered");
    };
    let ms: u64 = message
        .trim_start_matches("reached:")
        .parse()
        .unwrap_or_else(|_| unreachable!("expected an integer position in {message:?}"));
    assert!(ms >= 500, "expected the actual crossing position, got {ms}");

    let second = wait_for_log(&events, Duration::from_millis(300));
    assert!(second.is_none(), "a position timer must fire only once");
}

/// RT9: a track-generation change (a track change) silently cancels
/// every pending position timer — no `timer`/`position_reached` event,
/// ever, for a timer scheduled before the change — while the ordinary
/// `track_changed` handler still runs.
#[test]
fn track_change_cancels_position_timers() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            api.playback.subscribe_position(30)
            api.timers.schedule_at_position(500)
        end)
        api.on("position_reached", function(_event)
            api.log.info("reached")
        end)
        api.on("track_changed", function(_event)
            api.log.info("track_changed")
        end)
        api.ready()
    "#;
    let (handle, events, _requests, shared, playback, _focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["playback.observe"]),
        Budgets::DEFAULT,
        None,
        None,
    );
    playback.set_source_rate(44_100);
    shared.write_anchor(0, Instant::now(), false, 1024, 1.0);
    wait_for_ready(&events);

    playback.bump_track_generation();
    assert!(handle.send_event(HostEvent::TrackChanged { track: None }));
    let saw_track_changed = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(saw_track_changed.as_deref(), Some("track_changed"));

    // Now push the anchor well past the (cancelled) 500ms target — if the
    // timer had survived, this would fire `position_reached` right away.
    shared.write_anchor(44_100 * 2, Instant::now(), false, 1024, 1.0);
    let reached = wait_for_log(&events, Duration::from_millis(400));
    assert!(
        reached.is_none(),
        "a position timer must not survive a track change"
    );
}

/// FR-022/RT9: the 257th pending timer (of any kind) is refused
/// `timer_limit` — calls are spread across several non-overlapping
/// rolling-second windows so the assertion exercises `TimerSet`'s own
/// 256 cap, not the (separate) 100/s rate limiter.
#[test]
fn timer_limit_257() {
    let entry = r#"
        local total = 0
        api.on("position", function(_event)
            for _ = 1, 90 do
                total = total + 1
                local h, err = api.timers.set_timeout(600000)
                if not h then
                    api.log.info("limit:" .. tostring(total) .. ":" .. err.reason)
                    return
                end
            end
        end)
        api.ready()
    "#;
    let (handle, events, _requests, _shared, _playback, _focus) =
        spawn_test_plugin_ex(entry, Grants::none(), Budgets::DEFAULT, None, None);
    wait_for_ready(&events);

    let mut outcome = None;
    for _ in 0..4 {
        handle.send_event(HostEvent::Position { position_ms: 0 });
        if let Some(message) = wait_for_log(&events, Duration::from_millis(200)) {
            outcome = Some(message);
            break;
        }
        std::thread::sleep(Duration::from_millis(1_200));
    }
    let message = outcome.unwrap_or_else(|| unreachable!("the 257th timer must be refused"));
    assert_eq!(message, "limit:257:timer_limit", "message: {message}");
}

/// `api.timers.clear(handle)` on an id that was never allocated (or
/// already fired/cleared) is `not_found`.
#[test]
fn clear_unknown_timer_not_found() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            local ok, err = api.timers.clear(999999)
            if ok then
                api.log.error("clear unexpectedly succeeded")
            else
                api.log.info("clear:" .. err.reason)
            end
        end)
        api.ready()
    "#;
    let (_handle, events, _requests, _shared, _playback, _focus) =
        spawn_test_plugin_ex(entry, Grants::none(), Budgets::DEFAULT, None, None);
    wait_for_ready(&events);
    let message = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(message.as_deref(), Some("clear:unknown_id"));
}

/// RT11: a per-track state file already on disk for the incoming track
/// is loaded into the store *before* the `track_changed` handler runs —
/// not delivered first and loaded lazily after.
#[test]
fn track_state_restored_before_track_changed() {
    let dir = temp_state_dir("restore-before");
    let paths = PluginStatePaths::with_dir(dir.clone());
    let track_id = "spotify:track:restoretest";
    let file = paths.track_file("org.modplayer.test.scheduler", track_id);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&file, br#"{"version":1,"entries":{"seed":"preloaded"}}"#)
        .unwrap_or_else(|e| unreachable!("failed to seed {file:?}: {e}"));

    let entry = r#"
        api.on("track_changed", function(_event)
            local v = api.state.track.get("seed")
            api.log.info("seed=" .. tostring(v))
        end)
        api.ready()
    "#;
    let (handle, events, _requests, _shared, _playback, _focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["playback.observe", "state.track"]),
        Budgets::DEFAULT,
        None,
        Some(paths),
    );
    wait_for_ready(&events);

    assert!(handle.send_event(HostEvent::TrackChanged {
        track: Some(TrackInfo {
            id: track_id.to_string(),
            title: "Restore Test".to_string(),
            artists: vec!["Artist".to_string()],
            duration_ms: 180_000,
        }),
    }));
    let message = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(message.as_deref(), Some("seed=preloaded"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// RT11, the write race: the previous track's scope is flushed to the
/// writer thread *asynchronously*, so a `TrackChanged` straight back to
/// that track must not depend on the file having landed yet. Here the
/// writer never runs at all (its channel is held but never drained), so
/// nothing ever reaches disk — the value set on track `a` must still
/// come back on the return to `a`, from the scheduler's own record of
/// what it just flushed.
#[test]
fn track_state_survives_immediate_return_before_write_lands() {
    let dir = temp_state_dir("restore-race");
    let paths = PluginStatePaths::with_dir(dir.clone());
    // Held for the test's duration and never read from: every flush is
    // queued, none is written.
    let (writer_tx, _writer_rx) = std::sync::mpsc::channel();

    let entry = r#"
        local visits = 0
        api.on("track_changed", function(event)
            if event.track.id == "spotify:track:a" then
                visits = visits + 1
                if visits == 1 then
                    api.state.track.set("k", "va")
                    api.log.info("set")
                else
                    api.log.info("k=" .. tostring(api.state.track.get("k")))
                end
            else
                api.log.info("other")
            end
        end)
        api.ready()
    "#;
    let (handle, events, _requests, _shared, _playback, _focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["playback.observe", "state.track"]),
        Budgets::DEFAULT,
        Some(writer_tx),
        Some(paths.clone()),
    );
    wait_for_ready(&events);

    let track = |id: &str| HostEvent::TrackChanged {
        track: Some(TrackInfo {
            id: format!("spotify:track:{id}"),
            title: id.to_uppercase(),
            artists: vec!["Artist".to_string()],
            duration_ms: 180_000,
        }),
    };
    assert!(handle.send_event(track("a")));
    assert_eq!(
        wait_for_log(&events, Duration::from_secs(1)).as_deref(),
        Some("set")
    );
    assert!(handle.send_event(track("b")));
    assert_eq!(
        wait_for_log(&events, Duration::from_secs(1)).as_deref(),
        Some("other")
    );
    assert!(handle.send_event(track("a")));
    assert_eq!(
        wait_for_log(&events, Duration::from_secs(1)).as_deref(),
        Some("k=va"),
        "the value set on track a must survive an immediate return even though its write never landed"
    );
    assert!(
        !paths
            .track_file("org.modplayer.test.scheduler", "spotify:track:a")
            .exists(),
        "sanity: the writer really never ran"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// RT11: a per-track file larger than `budgets.storage` cannot have been
/// written by the store (which enforces that cap) and would only cost an
/// unbounded decode, so it is refused unread: the scope stays empty,
/// `HandlerAborted{RestoreTimeout}` is reported, and `track_changed` is
/// still delivered right afterward regardless. The seed file here is
/// ~26 MB against the 10 MB default cap.
#[test]
fn restore_timeout_delivers_event_anyway() {
    let dir = temp_state_dir("restore-timeout");
    let paths = PluginStatePaths::with_dir(dir.clone());
    let track_id = "spotify:track:restoretimeout";
    let file = paths.track_file("org.modplayer.test.scheduler", track_id);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    {
        use std::io::Write as _;
        let mut writer = std::io::BufWriter::new(
            std::fs::File::create(&file)
                .unwrap_or_else(|e| unreachable!("failed to create {file:?}: {e}")),
        );
        write!(writer, r#"{{"version":1,"entries":{{"#)
            .unwrap_or_else(|e| unreachable!("write failed: {e}"));
        for i in 0..2_000_000u32 {
            if i > 0 {
                write!(writer, ",").unwrap_or_else(|e| unreachable!("write failed: {e}"));
            }
            write!(writer, "\"k{i}\":{i}").unwrap_or_else(|e| unreachable!("write failed: {e}"));
        }
        write!(writer, "}}}}").unwrap_or_else(|e| unreachable!("write failed: {e}"));
    }

    let entry = r#"
        api.on("track_changed", function(_event)
            api.log.info("track_changed")
            local v = api.state.track.get("k0")
            api.log.info("k0=" .. tostring(v))
        end)
        api.ready()
    "#;
    let (handle, events, _requests, _shared, _playback, _focus) = spawn_test_plugin_ex(
        entry,
        grants_with(&["playback.observe", "state.track"]),
        Budgets::DEFAULT,
        None,
        Some(paths),
    );
    wait_for_ready(&events);

    assert!(handle.send_event(HostEvent::TrackChanged {
        track: Some(TrackInfo {
            id: track_id.to_string(),
            title: "Restore Timeout Test".to_string(),
            artists: vec!["Artist".to_string()],
            duration_ms: 180_000,
        }),
    }));

    let aborted = wait_for(&events, Duration::from_secs(2), |e| {
        matches!(
            e,
            RuntimeEvent::HandlerAborted {
                cause: AbortCause::RestoreTimeout,
            }
        )
        .then_some(())
    });
    assert!(
        aborted.is_some(),
        "a seed file over the storage cap must be refused as RestoreTimeout"
    );

    let track_changed = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(
        track_changed.as_deref(),
        Some("track_changed"),
        "track_changed must still be delivered after a discarded restore"
    );
    let discarded = wait_for_log(&events, Duration::from_secs(1));
    assert_eq!(
        discarded.as_deref(),
        Some("k0=nil"),
        "the refused load must leave the scope empty, not partially applied"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Binding tests (US3 T099, contracts/plugin-api-v1.md): every
//! `RequestKind` round-trips correctly between a Lua call and its
//! `Request`/`Response` (a wrong request built for the wrong Lua call
//! would either mismatch this test's canned reply shape or hit its
//! `unreachable!` fallback), an ungranted namespace call returns `nil,
//! refusal` rather than a Lua error, `api.granted`/`api.version`/
//! `api.capabilities` have the shape contract §1 promises, and
//! `api.ready()` is `true` once then `false, refusal` after.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::focus::{FocusToken, PluginId};
use modplayer_capability_gateway::grants::Grants;
use modplayer_capability_gateway::manifest::{self, ApiRange};
use modplayer_capability_gateway::request::{
    LoopEndpoint, MarkerId as GatewayMarkerId, NodeId as GatewayNodeId, OwnerInfo,
    RegionId as GatewayRegionId, RegionInfo, RepeatArg, Request, Response,
};
use modplayer_capability_gateway::state::PluginStatePaths;
use modplayer_engine::RtShared;
use modplayer_plugin_runtime::events::{PlaybackSnapshot, RuntimeEvent};
use modplayer_plugin_runtime::handle::{
    PluginHandle, PluginSnapshot, RpcEnvelope, RuntimeDeps, SpawnConfig,
};

const ALL_PERMISSIONS: &[&str] = &[
    "playback.observe",
    "transport.control",
    "queue.write",
    "markers.read",
    "markers.write",
    "audio.effects",
    "audio.meter",
    "state.plugin",
    "state.track",
];

fn grants_with(permissions: &[&str]) -> Grants {
    let mut toml = String::from(
        "identifier = \"org.modplayer.test.bindings\"\n\
         name = \"Bindings test\"\n\
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

#[allow(clippy::type_complexity)]
fn spawn_test_plugin(
    entry_source: &str,
    grants: Grants,
) -> (
    PluginHandle,
    Receiver<(PluginId, RuntimeEvent)>,
    Receiver<RpcEnvelope>,
) {
    let (requests_tx, requests_rx) = std::sync::mpsc::sync_channel::<RpcEnvelope>(64);
    let (events_tx, events_rx) = std::sync::mpsc::sync_channel(256);
    // 010-transport-focus (research R2): `request_focus()` is now an RPC
    // whose grant depends on core's `FocusArbiter`, which this bare
    // binding-level harness (no controller) does not run — so the token
    // is set directly, granting this plugin focus up front, letting the
    // eight focus-gated calls this file round-trips reach the RPC stage.
    let focus = FocusToken::new();
    focus.set_holder(Some(PluginId(1)));
    let config = SpawnConfig {
        id: PluginId(1),
        identifier: "org.modplayer.test.bindings".to_string(),
        entry_source: entry_source.to_string(),
        api_range: ApiRange {
            major: 1,
            min_minor: 0,
        },
        grants,
        budgets: Budgets::DEFAULT,
        focus,
        fixtures_enabled: false,
    };
    let deps = RuntimeDeps {
        shared: Arc::new(RtShared::new()),
        playback: Arc::new(PlaybackSnapshot::new()),
        requests: requests_tx,
        events: events_tx,
        snapshot: Arc::new(Mutex::new(PluginSnapshot::default())),
        writer: None,
        paths: None::<PluginStatePaths>,
    };
    (PluginHandle::spawn(config, deps), events_rx, requests_rx)
}

/// Like [`spawn_test_plugin`], but seeds the shared `PluginSnapshot` up
/// front — for `list_markers_exposes_regions`, which reads `markers.
/// list()`'s locally-served `regions` field with no RPC in flight.
#[allow(clippy::type_complexity)]
fn spawn_test_plugin_with_snapshot(
    entry_source: &str,
    grants: Grants,
    snapshot: PluginSnapshot,
) -> (
    PluginHandle,
    Receiver<(PluginId, RuntimeEvent)>,
    Receiver<RpcEnvelope>,
) {
    let (requests_tx, requests_rx) = std::sync::mpsc::sync_channel::<RpcEnvelope>(64);
    let (events_tx, events_rx) = std::sync::mpsc::sync_channel(256);
    let focus = FocusToken::new();
    focus.set_holder(Some(PluginId(1)));
    let config = SpawnConfig {
        id: PluginId(1),
        identifier: "org.modplayer.test.bindings".to_string(),
        entry_source: entry_source.to_string(),
        api_range: ApiRange {
            major: 1,
            min_minor: 0,
        },
        grants,
        budgets: Budgets::DEFAULT,
        focus,
        fixtures_enabled: false,
    };
    let deps = RuntimeDeps {
        shared: Arc::new(RtShared::new()),
        playback: Arc::new(PlaybackSnapshot::new()),
        requests: requests_tx,
        events: events_tx,
        snapshot: Arc::new(Mutex::new(snapshot)),
        writer: None,
        paths: None::<PluginStatePaths>,
    };
    (PluginHandle::spawn(config, deps), events_rx, requests_rx)
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

fn drain_logs(rx: &Receiver<(PluginId, RuntimeEvent)>, timeout: Duration) -> Vec<String> {
    let mut out = Vec::new();
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok((_, RuntimeEvent::Log { message, .. })) => out.push(message),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    out
}

/// A canned, shape-correct reply for `request` (the RPC-answering
/// half of the round trip) — an `unreachable!` on anything unexpected
/// means a binding that constructed the wrong `Request` for its own
/// `RequestKind` fails this test loudly rather than silently.
fn canned_response(request: &Request) -> Response {
    match request {
        Request::RequestFocus { .. }
        | Request::ReleaseFocus
        | Request::Play
        | Request::Pause
        | Request::Toggle
        | Request::Seek { .. }
        | Request::SkipNext
        | Request::SkipPrevious
        | Request::ArmLoop { .. }
        | Request::DisarmLoop
        | Request::QueueMove { .. }
        | Request::QueueRemove { .. }
        | Request::QueuePlayNext { .. }
        | Request::QueueAdd { .. }
        | Request::MoveMarker { .. }
        | Request::RenameMarker { .. }
        | Request::RecolorMarker { .. }
        | Request::DeleteMarker { .. }
        | Request::SetParam { .. }
        | Request::ScheduleParam { .. }
        | Request::SetBypass { .. }
        | Request::RemoveNode { .. } => Response::Ok,
        Request::SetCue { .. } => Response::MarkerId(GatewayMarkerId(1)),
        Request::CreateMarker { .. } => Response::MarkerId(GatewayMarkerId(2)),
        Request::CreateLoopRegion { .. } => Response::RegionId(GatewayRegionId(3)),
        Request::CreateNode { .. } => Response::NodeId(GatewayNodeId(4)),
        other => unreachable!("no canned reply wired for {other:?}"),
    }
}

/// Runs a background thread that replies to every `RpcEnvelope` this
/// test's script sends with [`canned_response`], until `count` replies
/// have gone out (or 2s pass) — joined at the end of the test.
fn spawn_responder(requests: Receiver<RpcEnvelope>, count: usize) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        for _ in 0..count {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Ok(envelope) = requests.recv_timeout(remaining) else {
                return;
            };
            let response = canned_response(&envelope.request);
            let _ = envelope.reply.send(Ok(response));
        }
    })
}

/// Every RPC-routed `RequestKind` this slice defines, called once each
/// with all 9 operable permissions granted — each must come back `ok`
/// with the shape `response_to_lua` promises for its canned reply
/// (contract §2's "every request returns `ok, value` or `nil, refusal`").
/// `request_focus` is included: 010-transport-focus (research R2) removed
/// its local dispatch arm, so it is now RPC-routed like every other call
/// here (`bindings::request_focus_is_rpc_not_local` below pins this more
/// directly).
#[test]
fn every_rpc_request_kind_round_trips() {
    let entry = r#"
        local function step(name, ok)
            api.log.info(name .. ":" .. type(ok) .. ":" .. tostring(ok))
        end

        api.on("ready_ack", function(_event)
            step("request_focus", api.transport.request_focus())
            step("play", api.transport.play())
            step("pause", api.transport.pause())
            step("toggle", api.transport.toggle())
            step("seek", api.transport.seek(1000))
            step("skip_next", api.transport.skip_next())
            step("skip_previous", api.transport.skip_previous())
            step("arm_loop", api.transport.arm_loop(1))
            step("disarm_loop", api.transport.disarm_loop())

            step("queue.move", api.queue.move(1, 0))
            step("queue.remove", api.queue.remove(1))
            step("queue.play_next", api.queue.play_next(1))
            step("queue.add", api.queue.add("spotify:track:x"))

            step("markers.move", api.markers.move(1, 1000))
            step("markers.rename", api.markers.rename(1, "n"))
            step("markers.recolor", api.markers.recolor(1, 2))
            step("markers.delete", api.markers.delete(1))
            step("markers.create_loop", api.markers.create_loop(0, 1000))
            step("markers.set_cue", api.markers.set_cue(1, 500))
            step("markers.create", api.markers.create(500))

            step("effects.create_node", api.effects.create_node("gain"))
            step("effects.set_param", api.effects.set_param(1, 0, 0.5))
            step("effects.schedule_param", api.effects.schedule_param(1, 0, 0.5, 0))
            step("effects.bypass", api.effects.bypass(1, true))
            step("effects.remove_node", api.effects.remove_node(1))

            api.log.info("done")
        end)
        api.ready()
    "#;
    let (_handle, events, requests) = spawn_test_plugin(entry, grants_with(ALL_PERMISSIONS));
    wait_for_ready(&events);

    let responder = spawn_responder(requests, 25);
    let logs = drain_logs(&events, Duration::from_secs(2));
    responder
        .join()
        .unwrap_or_else(|_| unreachable!("responder thread panicked"));

    assert!(
        logs.iter().any(|m| m == "done"),
        "the script must reach its final log; got {logs:?}"
    );

    let boolean_ok_calls = [
        "request_focus",
        "play",
        "pause",
        "toggle",
        "seek",
        "skip_next",
        "skip_previous",
        "arm_loop",
        "disarm_loop",
        "queue.move",
        "queue.remove",
        "queue.play_next",
        "queue.add",
        "markers.move",
        "markers.rename",
        "markers.recolor",
        "markers.delete",
        "effects.set_param",
        "effects.schedule_param",
        "effects.bypass",
        "effects.remove_node",
    ];
    for name in boolean_ok_calls {
        let expected = format!("{name}:boolean:true");
        assert!(
            logs.contains(&expected),
            "{name}: expected {expected:?}, got {logs:?}"
        );
    }
    for name in ["markers.create_loop", "markers.set_cue", "markers.create"] {
        let expected_prefix = format!("{name}:number:");
        assert!(
            logs.iter().any(|m| m.starts_with(&expected_prefix)),
            "{name}: expected a numeric id, got {logs:?}"
        );
    }
    let expected_prefix = "effects.create_node:number:";
    assert!(
        logs.iter().any(|m| m.starts_with(expected_prefix)),
        "effects.create_node: expected a numeric id, got {logs:?}"
    );
}

/// contract §1: an ungranted namespace call returns `nil, { code =
/// "permission_denied", reason = "not_granted" }` — never a Lua error —
/// checked before it ever reaches an RPC (`Grants::none()`, no responder
/// running at all).
#[test]
fn ungranted_call_is_refusal_not_lua_error() {
    let entry = r#"
        local ok, err = api.transport.play()
        if ok ~= nil then
            api.log.error("expected ok to be nil")
        elseif type(err) ~= "table" then
            api.log.error("expected a refusal table")
        else
            api.log.info("refusal:" .. err.code .. "/" .. err.reason)
        end
    "#;
    let (_handle, events, _requests) = spawn_test_plugin(entry, Grants::none());
    let message = wait_for(&events, Duration::from_secs(1), |e| match e {
        RuntimeEvent::Log { message, .. } => Some(message.clone()),
        _ => None,
    });
    assert_eq!(
        message.as_deref(),
        Some("refusal:permission_denied/not_granted")
    );
}

/// contract §1: `api.granted` lists exactly the granted permission names,
/// `api.version` is `{major=1, minor=0}`, and `api.capabilities` includes
/// every operable permission plus `"timers"`.
#[test]
fn identity_fields_have_the_contract_shape() {
    let entry = r#"
        local granted = {}
        for _, name in ipairs(api.granted) do
            table.insert(granted, name)
        end
        table.sort(granted)
        api.log.info("granted:" .. table.concat(granted, ","))
        api.log.info("version:" .. tostring(api.version.major) .. "." .. tostring(api.version.minor))
        local has_timers = false
        for _, name in ipairs(api.capabilities) do
            if name == "timers" then
                has_timers = true
            end
        end
        api.log.info("capabilities_has_timers:" .. tostring(has_timers))
        api.log.info("capabilities_len:" .. tostring(#api.capabilities))
    "#;
    let (_handle, events, _requests) =
        spawn_test_plugin(entry, grants_with(&["playback.observe", "state.plugin"]));

    let logs = drain_logs(&events, Duration::from_secs(1));
    assert!(
        logs.contains(&"granted:playback.observe,state.plugin".to_string()),
        "logs: {logs:?}"
    );
    assert!(logs.contains(&"version:1.3".to_string()), "logs: {logs:?}");
    assert!(
        logs.contains(&"capabilities_has_timers:true".to_string()),
        "logs: {logs:?}"
    );
    // 14 operable permissions (011-plugin-ui-contributions added the five
    // `ui.*` ones) + "timers" (contract §1 `HOST_CAPABILITIES`).
    assert!(
        logs.contains(&"capabilities_len:15".to_string()),
        "logs: {logs:?}"
    );
}

/// 010-transport-focus (research R2, RT-F1): `request_focus()` has no
/// local dispatch arm — the call must actually cross to the RPC channel
/// (`requests`) as `Request::RequestFocus`, not be answered synchronously
/// inside `dispatch` from the plugin thread's own `FocusToken`.
#[test]
fn request_focus_is_rpc_not_local() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            local ok = api.transport.request_focus()
            api.log.info("request_focus:" .. tostring(ok))
        end)
        api.ready()
    "#;
    let (_handle, events, requests) = spawn_test_plugin(entry, grants_with(&["transport.control"]));
    wait_for_ready(&events);

    let envelope = requests
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|e| unreachable!("request_focus never reached the RPC channel: {e}"));
    assert!(
        matches!(envelope.request, Request::RequestFocus { .. }),
        "expected Request::RequestFocus over the RPC channel, got {:?}",
        envelope.request
    );
    let _ = envelope.reply.send(Ok(Response::Ok));

    let logs = drain_logs(&events, Duration::from_secs(1));
    assert!(
        logs.contains(&"request_focus:true".to_string()),
        "logs: {logs:?}"
    );
}

/// Mirror of `request_focus_is_rpc_not_local` for `release_focus()`.
#[test]
fn release_focus_is_rpc_not_local() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            local ok = api.transport.release_focus()
            api.log.info("release_focus:" .. tostring(ok))
        end)
        api.ready()
    "#;
    let (_handle, events, requests) = spawn_test_plugin(entry, grants_with(&["transport.control"]));
    wait_for_ready(&events);

    let envelope = requests
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|e| unreachable!("release_focus never reached the RPC channel: {e}"));
    assert!(
        matches!(envelope.request, Request::ReleaseFocus),
        "expected Request::ReleaseFocus over the RPC channel, got {:?}",
        envelope.request
    );
    let _ = envelope.reply.send(Ok(Response::Ok));

    let logs = drain_logs(&events, Duration::from_secs(1));
    assert!(
        logs.contains(&"release_focus:true".to_string()),
        "logs: {logs:?}"
    );
}

/// 011-plugin-ui-contributions (T028, research R4): every `api.ui.*`
/// call except `get_settings` crosses to the RPC channel as its matching
/// `Request` variant, in call order — mirroring
/// `every_rpc_request_kind_round_trips` above for the new `ui` namespace.
#[test]
fn ui_calls_are_rpcs() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            local function step(name, ok)
                api.log.info(name .. ":" .. tostring(ok))
            end
            step("register_panel", api.ui.register_panel("main", "Panel", {
                { id = "lbl", kind = "label", label = "Hi" },
            }))
            step("update_widget", api.ui.update_widget("main", "lbl", "text"))
            step("add_overlays", api.ui.add_overlays({
                { id = "ov1", kind = "line", at = 100 },
            }))
            step("remove_overlays", api.ui.remove_overlays({ "ov1" }))
            step("clear_overlays", api.ui.clear_overlays())
            step("register_action", api.ui.register_action({ id = "act", label = "Act" }))
            step("register_settings", api.ui.register_settings({
                { id = "f1", kind = "boolean", label = "F1", default = true },
            }))
            step("notify", api.ui.notify("info", "hello"))
            api.log.info("done")
        end)
        api.ready()
    "#;
    let grants = grants_with(&[
        "ui.panel",
        "ui.overlay",
        "ui.shortcuts",
        "ui.settings",
        "ui.notify",
    ]);
    let (_handle, events, requests) = spawn_test_plugin(entry, grants);
    wait_for_ready(&events);

    type ReqCheck = (&'static str, fn(&Request) -> bool);
    let expected: [ReqCheck; 8] = [
        ("register_panel", |r| {
            matches!(r, Request::RegisterPanel { .. })
        }),
        ("update_widget", |r| {
            matches!(r, Request::UpdateWidget { .. })
        }),
        ("add_overlays", |r| matches!(r, Request::AddOverlays { .. })),
        ("remove_overlays", |r| {
            matches!(r, Request::RemoveOverlays { .. })
        }),
        ("clear_overlays", |r| matches!(r, Request::ClearOverlays)),
        ("register_action", |r| {
            matches!(r, Request::RegisterAction { .. })
        }),
        ("register_settings", |r| {
            matches!(r, Request::RegisterSettings { .. })
        }),
        ("notify", |r| matches!(r, Request::Notify { .. })),
    ];
    for (name, matches_kind) in expected {
        let envelope = requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|e| unreachable!("{name} never reached the RPC channel: {e}"));
        assert!(
            matches_kind(&envelope.request),
            "{name}: unexpected request shape {:?}",
            envelope.request
        );
        let _ = envelope.reply.send(Ok(Response::Ok));
    }

    let logs = drain_logs(&events, Duration::from_secs(1));
    assert!(logs.contains(&"done".to_string()), "logs: {logs:?}");
    for (name, _) in expected {
        let expected_log = format!("{name}:true");
        assert!(logs.contains(&expected_log), "{name}: logs: {logs:?}");
    }
}

/// 011-plugin-ui-contributions (T028, research R4): `get_settings` never
/// reaches the RPC channel — it is served locally from
/// `Shared.settings_schema` (set by a successful `register_settings`) and
/// the plugin's own `Scope::Settings` store, substituting each field's
/// `default` for a value that was never written.
#[test]
fn get_settings_is_local_and_substitutes_defaults() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            local ok = api.ui.register_settings({
                { id = "f1", kind = "boolean", label = "F1", default = true },
                { id = "f2", kind = "number", label = "F2", min = 0, max = 10, step = 1, default = 5 },
            })
            api.log.info("register:" .. tostring(ok))
            local settings = api.ui.get_settings()
            api.log.info("f1=" .. tostring(settings.f1))
            api.log.info("f2=" .. tostring(settings.f2))
        end)
        api.ready()
    "#;
    let (_handle, events, requests) = spawn_test_plugin(entry, grants_with(&["ui.settings"]));
    wait_for_ready(&events);

    let envelope = requests
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|e| unreachable!("register_settings never reached the RPC channel: {e}"));
    assert!(
        matches!(envelope.request, Request::RegisterSettings { .. }),
        "expected Request::RegisterSettings, got {:?}",
        envelope.request
    );
    let _ = envelope.reply.send(Ok(Response::Ok));

    let logs = drain_logs(&events, Duration::from_secs(1));
    assert!(
        logs.contains(&"register:true".to_string()),
        "logs: {logs:?}"
    );
    assert!(logs.contains(&"f1=true".to_string()), "logs: {logs:?}");
    assert!(logs.contains(&"f2=5".to_string()), "logs: {logs:?}");

    // `get_settings` never sent a second envelope over the RPC channel.
    assert!(
        requests.try_recv().is_err(),
        "get_settings must be served locally, not as an RPC"
    );
}

/// 011-plugin-ui-contributions (T028, R5/FR-018): `Scope::Settings` is
/// never installed under `api.state` — only `plugin`/`track` are, so a
/// plugin's settings-page values stay unreachable through
/// `state.plugin`/`state.track` get/set (they are host-written only, via
/// `Control::SettingsWrite`).
#[test]
fn settings_scope_unreachable_from_lua() {
    let entry = r#"
        api.log.info("settings_is_nil:" .. tostring(api.state.settings == nil))
        local names = {}
        for k, _ in pairs(api.state) do
            table.insert(names, k)
        end
        table.sort(names)
        api.log.info("state_keys:" .. table.concat(names, ","))
    "#;
    let (_handle, events, _requests) = spawn_test_plugin(entry, Grants::none());
    let logs = drain_logs(&events, Duration::from_secs(1));
    assert!(
        logs.contains(&"settings_is_nil:true".to_string()),
        "logs: {logs:?}"
    );
    assert!(
        logs.contains(&"state_keys:plugin,track".to_string()),
        "logs: {logs:?}"
    );
}

/// contract §1: `api.ready()` returns `true` the first time; every call
/// after that is the ordinary refusal shape (`nil, refusal` — contract
/// §2's convention), harmlessly rather than a Lua error either way.
#[test]
fn ready_is_true_once_then_false_with_refusal() {
    let entry = r#"
        local first_ok, first_err = api.ready()
        local second_ok, second_err = api.ready()
        api.log.info("first:" .. tostring(first_ok) .. ":" .. tostring(first_err))
        api.log.info("second:" .. tostring(second_ok) .. ":" .. type(second_err))
    "#;
    let (_handle, events, _requests) = spawn_test_plugin(entry, Grants::none());
    let logs = drain_logs(&events, Duration::from_secs(1));
    assert!(
        logs.contains(&"first:true:nil".to_string()),
        "logs: {logs:?}"
    );
    assert!(
        logs.contains(&"second:nil:table".to_string()),
        "logs: {logs:?}"
    );
}

// -- 012-section-loop-plugin (contract plugin-api-v1.3.md §7) --------------

/// `which` is validated on the plugin thread, before the RPC (data-model.md
/// §1.2/§3): a bad value never reaches the RPC channel and comes back
/// `nil, { code = "invalid_state", reason = "invalid_argument" }`, while a
/// good `"a"`/`"b"` does cross as `Request::SetLoopEndpoint`.
#[test]
fn set_loop_endpoint_which_validated() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            local ok, err = api.markers.set_loop_endpoint(nil, "x", 62000)
            api.log.info("bad:" .. tostring(ok) .. ":" .. tostring(err.code) .. "/" .. tostring(err.reason))

            local r, err2 = api.markers.set_loop_endpoint(nil, "a", 62000)
            api.log.info("good:" .. tostring(r ~= nil) .. ":" .. tostring(err2))
        end)
        api.ready()
    "#;
    let (_handle, events, requests) = spawn_test_plugin(entry, grants_with(&["markers.write"]));
    wait_for_ready(&events);

    // The bad call never touches the RPC channel at all.
    let envelope = requests
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|e| unreachable!("the valid call never reached the RPC channel: {e}"));
    assert!(
        matches!(
            envelope.request,
            Request::SetLoopEndpoint {
                which: LoopEndpoint::A,
                ..
            }
        ),
        "expected Request::SetLoopEndpoint{{which: A, ..}}, got {:?}",
        envelope.request
    );
    let _ = envelope.reply.send(Ok(Response::LoopEndpoint {
        region: GatewayRegionId(3),
        marker: GatewayMarkerId(7),
    }));
    assert!(
        requests.try_recv().is_err(),
        "the bad `which` call must never reach the RPC channel"
    );

    let logs = drain_logs(&events, Duration::from_secs(1));
    assert!(
        logs.contains(&"bad:nil:invalid_state/invalid_argument".to_string()),
        "logs: {logs:?}"
    );
    assert!(
        logs.contains(&"good:true:nil".to_string()),
        "logs: {logs:?}"
    );
}

/// `repeat` is exact — `1..=1000` or `"infinite"`, never clamped
/// (data-model.md §1.2, contract §3.2): `0`, `1001`, a fraction and any
/// other string are all refused on the plugin thread before any RPC.
#[test]
fn set_loop_repeat_range_exact() {
    let entry = r#"
        api.on("ready_ack", function(_event)
            local function bad(v)
                local ok, err = api.markers.set_loop_repeat(1, v)
                api.log.info("bad:" .. tostring(v) .. ":" .. tostring(ok) .. ":" .. tostring(err.code) .. "/" .. tostring(err.reason))
            end
            bad(0)
            bad(1001)
            bad(2.5)
            bad("forever")

            local ok1 = api.markers.set_loop_repeat(1, 4)
            api.log.info("times:" .. tostring(ok1))
            local ok2 = api.markers.set_loop_repeat(1, "infinite")
            api.log.info("infinite:" .. tostring(ok2))
        end)
        api.ready()
    "#;
    let (_handle, events, requests) = spawn_test_plugin(entry, grants_with(&["markers.write"]));
    wait_for_ready(&events);

    for _ in 0..2 {
        let envelope = requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|e| unreachable!("a valid repeat call never reached the RPC: {e}"));
        assert!(
            matches!(envelope.request, Request::SetLoopRepeat { .. }),
            "expected Request::SetLoopRepeat, got {:?}",
            envelope.request
        );
        let _ = envelope.reply.send(Ok(Response::Ok));
    }
    assert!(
        requests.try_recv().is_err(),
        "the four invalid `repeat` calls must never reach the RPC channel"
    );

    let logs = drain_logs(&events, Duration::from_secs(1));
    for v in ["0", "1001", "2.5", "forever"] {
        let expected = format!("bad:{v}:nil:invalid_state/invalid_argument");
        assert!(logs.contains(&expected), "logs: {logs:?}");
    }
    assert!(logs.contains(&"times:true".to_string()), "logs: {logs:?}");
    assert!(
        logs.contains(&"infinite:true".to_string()),
        "logs: {logs:?}"
    );
}

/// `markers.list()`'s `regions` array (contract §3.3): served locally from
/// the plugin snapshot, no RPC, with `a`/`b` absent for an incomplete
/// region and `repeat` a number or `"infinite"`.
#[test]
fn list_markers_exposes_regions() {
    let snapshot = PluginSnapshot {
        regions: vec![
            RegionInfo {
                id: GatewayRegionId(3),
                owner: OwnerInfo::Plugin("org.modplayer.section-loop".to_string()),
                a: Some(GatewayMarkerId(7)),
                b: None,
                repeat: RepeatArg::Infinite,
                armed: false,
            },
            RegionInfo {
                id: GatewayRegionId(9),
                owner: OwnerInfo::Host,
                a: Some(GatewayMarkerId(1)),
                b: Some(GatewayMarkerId(2)),
                repeat: RepeatArg::Times(4),
                armed: true,
            },
        ],
        ..PluginSnapshot::default()
    };
    let entry = r#"
        local m = api.markers.list()
        api.log.info("count:" .. tostring(#m.regions))
        local r1 = m.regions[1]
        api.log.info("r1:" .. r1.id .. "," .. r1.owner .. "," .. tostring(r1.a) .. "," .. tostring(r1.b) .. "," .. tostring(r1["repeat"]) .. "," .. tostring(r1.armed))
        local r2 = m.regions[2]
        api.log.info("r2:" .. r2.id .. "," .. r2.owner .. "," .. tostring(r2.a) .. "," .. tostring(r2.b) .. "," .. tostring(r2["repeat"]) .. "," .. tostring(r2.armed))
    "#;
    let (_handle, events, _requests) =
        spawn_test_plugin_with_snapshot(entry, grants_with(&["markers.read"]), snapshot);
    let logs = drain_logs(&events, Duration::from_secs(1));
    assert!(logs.contains(&"count:2".to_string()), "logs: {logs:?}");
    assert!(
        logs.contains(&"r1:3,org.modplayer.section-loop,7,nil,infinite,false".to_string()),
        "logs: {logs:?}"
    );
    assert!(
        logs.contains(&"r2:9,host,1,2,4,true".to_string()),
        "logs: {logs:?}"
    );
}

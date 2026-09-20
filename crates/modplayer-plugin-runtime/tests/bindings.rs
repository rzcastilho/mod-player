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
    MarkerId as GatewayMarkerId, NodeId as GatewayNodeId, RegionId as GatewayRegionId, Request,
    Response,
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
        Request::Play
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

    let responder = spawn_responder(requests, 24);
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
    assert!(logs.contains(&"version:1.0".to_string()), "logs: {logs:?}");
    assert!(
        logs.contains(&"capabilities_has_timers:true".to_string()),
        "logs: {logs:?}"
    );
    // 9 operable permissions + "timers" (contract §1 `HOST_CAPABILITIES`).
    assert!(
        logs.contains(&"capabilities_len:10".to_string()),
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

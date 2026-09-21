// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `ui.panel` end to end over a real `PlaybackController`
//! (011-plugin-ui-contributions US1, contracts/ui-panels.md §1/§3):
//! FR-001's whole-call refusal naming an unlabeled widget's path,
//! `update_widget`'s "never an event" rule, the controller's own
//! `plugin_panel_interaction` delivering exactly one `panel_interaction`,
//! `request_focus()`'s R16/FR-026 interaction-vs-timer split, and a
//! handler throw (FR-023/SC-007) staying contained. Drives the real
//! `org.modplayer.fixture.ui-panel` fixture (`MODPLAYER_PLUGIN_FIXTURES=1`),
//! mirroring `controller_transport_focus.rs`'s own harness/`call()`
//! pattern.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{Request, Response};
use modplayer_capability_gateway::ui::{
    OverlayColor, OverlayPrimitive, UiId, WidgetKind, WidgetSpec, WidgetValue,
};
use modplayer_core::PlaybackController;
use modplayer_core::actions::{ActionSource, Chord, PluginActionId};
use modplayer_core::plugins::ui::panel::PanelKey;
use modplayer_core::plugins::{FocusHolder, Lifecycle, PluginId, to_gateway_id};
use modplayer_core::settings::SettingsStore;
use modplayer_engine::{DeviceId, FrameCount, SampleRate};
use modplayer_plugin_runtime::handle::RpcEnvelope;

const UI_PANEL: &str = "org.modplayer.fixture.ui-panel";
const UI_SHORTCUTS: &str = "org.modplayer.fixture.ui-shortcuts";
const UI_SETTINGS: &str = "org.modplayer.fixture.ui-settings";
const UI_NOTIFY: &str = "org.modplayer.fixture.ui-notify";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-plugin-ui-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fresh_store() -> (SettingsStore, TempDir) {
    let dir = TempDir::new();
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    (store, dir)
}

fn fake_device(id: &str, is_default: bool) -> FakeDevice {
    FakeDevice {
        id: DeviceId::new(id).unwrap_or_else(|| unreachable!()),
        name: id.to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2_048))),
        is_default,
    }
}

static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

/// A controller that will discover every `plugins/fixtures/` package,
/// **not yet `launch()`ed** (mirrors `controller_plugins_lifecycle.rs`/
/// `controller_transport_focus.rs`'s own harness).
fn fixture_controller() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let plugin_state_dir = TempDir::new();
    let track_state_dir = TempDir::new();
    let host = ScriptedHost::new();
    let devices = vec![fake_device("dev-1", true)];
    let controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: narrowly scopes each mutation to the one synchronous
        // read `PlaybackController::new` makes of it, serialized against
        // every other test in this binary via the lock above.
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_FIXTURES", "1");
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_FIXTURES");
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    (controller, dir, plugin_state_dir, track_state_dir)
}

fn controller_plugin_id(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    identifier: &str,
) -> PluginId {
    controller
        .plugins_mut()
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == identifier)
        .map(|r| r.id)
        .unwrap_or_else(|| unreachable!("fixture '{identifier}' must be discovered"))
}

fn pump_controller_until(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    timeout: Duration,
    mut done: impl FnMut(&mut PlaybackController<FakeBackend, ScriptedHost>) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        controller.tick();
        if done(controller) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_active(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> bool {
    pump_controller_until(controller, Duration::from_secs(5), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )
    })
}

/// Waits for the fixture's `ready_ack` handler to have actually finished
/// its own `register_panel` RPC (asynchronous relative to `Active`).
fn wait_panel_registered(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> bool {
    pump_controller_until(controller, Duration::from_secs(5), |c| {
        c.plugins_mut()
            .ui()
            .panels()
            .for_plugin(id)
            .iter()
            .any(|p| p.id.as_str() == "main")
    })
}

/// Submits `request` from `plugin` straight to `drain_plugin_requests()`
/// (C1), bypassing the gateway's own admission entirely (mirrors
/// `controller_transport_focus.rs`'s own `call()`).
fn call(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    plugin: PluginId,
    request: Request,
) -> Result<Response, Refusal> {
    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    let envelope = RpcEnvelope {
        plugin: to_gateway_id(plugin),
        request,
        reply: reply_tx,
    };
    controller
        .plugins_mut()
        .debug_requests_sender()
        .send(envelope)
        .unwrap_or_else(|_| unreachable!("the request channel must accept a synthetic envelope"));
    controller.tick();
    reply_rx
        .try_recv()
        .unwrap_or_else(|_| unreachable!("drain_plugin_requests must always reply (C1)"))
}

fn debug_probe(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
    name: &str,
) -> Option<serde_json::Value> {
    match call(
        controller,
        id,
        Request::DebugProbe {
            name: name.to_string(),
        },
    ) {
        Ok(Response::Probe(value)) => Some(value),
        _ => None,
    }
}

fn wid(s: &str) -> UiId {
    UiId::parse(s).unwrap_or_else(|| unreachable!("{s:?} must be valid grammar"))
}

fn label_widget(id: &str, label: &str) -> WidgetSpec {
    WidgetSpec {
        id: wid(id),
        kind: WidgetKind::Label,
        label: label.to_string(),
        min: None,
        max: None,
        step: None,
        value: None,
        items: Vec::new(),
        selected: None,
        action: None,
        text: None,
    }
}

/// A `button` widget, optionally naming a `ui.shortcuts` action (D3) — the
/// registration side of T064's `button_names_unknown_action_inert`/
/// `button_action_source_ui`.
fn button_widget(id: &str, label: &str, action: Option<&str>) -> WidgetSpec {
    WidgetSpec {
        id: wid(id),
        kind: WidgetKind::Button,
        label: label.to_string(),
        min: None,
        max: None,
        step: None,
        value: None,
        items: Vec::new(),
        selected: None,
        action: action.map(str::to_string),
        text: None,
    }
}

/// FR-001/FR-002a: a `register_panel` call naming an unlabeled widget
/// anywhere in its layout is refused whole, naming the widget's path.
#[test]
fn unlabeled_widget_refused_with_path() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_PANEL);
    assert!(wait_active(&mut controller, id));

    let result = call(
        &mut controller,
        id,
        Request::RegisterPanel {
            panel: wid("other"),
            title: "Other".to_string(),
            widgets: vec![label_widget("hdr", "Section"), label_widget("bad", "")],
        },
    );
    let err = result.unwrap_err();
    assert_eq!(err.reason, "unlabeled_widget");
    assert!(
        err.message.contains("widgets[1]"),
        "message: {}",
        err.message
    );
    assert!(err.message.contains("(bad)"), "message: {}", err.message);
    assert!(
        controller
            .plugins_mut()
            .ui()
            .panels()
            .for_plugin(id)
            .iter()
            .all(|p| p.id.as_str() != "other"),
        "a refused call must register nothing"
    );
}

/// P4 (contracts/ui-panels.md): a UI-driven interaction stores the value
/// and delivers exactly one `panel_interaction` to the owning plugin.
#[test]
fn interaction_delivers_once() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_PANEL);
    assert!(wait_active(&mut controller, id));
    assert!(wait_panel_registered(&mut controller, id));

    let identifier = controller
        .plugins_mut()
        .record(id)
        .unwrap_or_else(|| unreachable!())
        .identifier
        .clone();
    let key = PanelKey::new(identifier, wid("main"));
    controller.plugin_panel_interaction(id, &key.panel, &wid("power"), WidgetValue::Bool(true));

    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| { debug_probe(c, id, "interaction_count") == Some(serde_json::Value::from(1)) }
    ));
}

/// FR-007: `update_widget` applies the value but never itself delivers a
/// `panel_interaction`.
#[test]
fn update_widget_no_event() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_PANEL);
    assert!(wait_active(&mut controller, id));
    assert!(wait_panel_registered(&mut controller, id));

    let result = call(
        &mut controller,
        id,
        Request::UpdateWidget {
            panel: wid("main"),
            widget: wid("power"),
            value: WidgetValue::Bool(true),
        },
    );
    assert_eq!(result, Ok(Response::Ok));

    // Give the fixture a few ticks that *would* have delivered an event
    // if `update_widget` incorrectly fired one.
    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        debug_probe(&mut controller, id, "interaction_count"),
        Some(serde_json::Value::from(0)),
        "update_widget must never itself deliver panel_interaction"
    );
}

/// R16/FR-026: a `request_focus()` made synchronously inside a
/// `panel_interaction` handler counts as a user interaction — under the
/// default `AutoOnInteraction` policy, it is granted immediately.
#[test]
fn request_focus_in_handler_auto_grants() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_PANEL);
    assert!(wait_active(&mut controller, id));
    assert!(wait_panel_registered(&mut controller, id));

    controller.plugin_panel_interaction(
        id,
        &wid("main"),
        &wid("take_over"),
        WidgetValue::Bool(true),
    );

    // 15s (not 2s): this file's own tests each `launch()` all 12 fixtures
    // (011-plugin-ui-contributions US2 T079 added `ui-shortcuts`, the
    // 12th), and running the whole file spawns/tears down several dozen
    // real plugin threads across the earlier tests in this same process —
    // real headroom for scheduling, not a correctness margin. 5s was still
    // seen to miss once in a full `cargo test --workspace` run.
    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(15),
        |c| { c.plugins_mut().arbiter().holder() == FocusHolder::Plugin(id) }
    ));
}

/// R16/FR-026, the contrast case: a `request_focus()` made from a timer
/// handler (never inside `panel_interaction`/`action_invoked`) is only
/// ever recorded — never auto-granted, even under `AutoOnInteraction`.
#[test]
fn request_focus_from_timer_recorded_only() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_PANEL);
    assert!(wait_active(&mut controller, id));

    // The fixture's own `ready_ack` handler schedules a 20ms timer whose
    // handler calls `request_focus()` — wait for that request to land in
    // the arbiter's pending queue.
    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| { c.plugins_mut().arbiter().pending().contains(&id) }
    ));
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "a timer-originated request_focus must never be auto-granted"
    );
}

/// FR-023/SC-007: a handler throw during `panel_interaction` is
/// contained by 009's existing per-handler budget path — the plugin
/// stays `Active` (one `Exception` abort never suspends it outright, L5).
#[test]
fn handler_throw_contained() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_PANEL);
    assert!(wait_active(&mut controller, id));
    assert!(wait_panel_registered(&mut controller, id));

    controller.plugin_panel_interaction(id, &wid("main"), &wid("throw"), WidgetValue::Bool(true));

    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| {
            c.plugins_mut()
                .record(id)
                .is_some_and(|r| !r.abort_window.is_empty())
        }
    ));
    assert!(
        matches!(
            controller.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        ),
        "a single contained throw must not suspend the plugin"
    );
}

// -- T064 (011-plugin-ui-contributions US2, contracts/action-registry-
// plugins.md D1-D4): `invoke_plugin_action`'s own gate/dispatch, driven
// directly at the controller level (the actual chord-to-action matching
// is `modplayer-ui::actions::dispatch`'s own job — the plugin-ui/tests/
// actions.rs::dispatch_returns_plugin_action_id, T065, covers that half).

fn wait_plugin_action_registered(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: &PluginActionId,
) -> bool {
    pump_controller_until(controller, Duration::from_secs(5), |c| {
        c.actions().plugin_action_registered(id)
    })
}

fn plugin_identifier(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    plugin: PluginId,
) -> String {
    controller
        .plugins_mut()
        .record(plugin)
        .unwrap_or_else(|| unreachable!())
        .identifier
        .to_string()
}

/// D4: `invoke_plugin_action` with `ActionSource::Keyboard` — the exact
/// call `modplayer-ui::actions::invoke`'s own `ActionId::Plugin` arm makes
/// (D2) — delivers `action_invoked` to the owning plugin. `tab_bound`
/// ships unbound (its own default, `Tab`, is rejected by G10), so a fresh
/// binding here disturbs no other fixture's own conflict fixture data.
#[test]
fn keyboard_invokes_plugin_action() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_SHORTCUTS);
    assert!(wait_active(&mut controller, id));

    let identifier = plugin_identifier(&mut controller, id);
    let action_id =
        PluginActionId::parse(&format!("{identifier}.tab_bound")).unwrap_or_else(|| unreachable!());
    assert!(wait_plugin_action_registered(&mut controller, &action_id));

    let chord = Chord::parse("F8").unwrap_or_else(|_| unreachable!());
    controller
        .add_binding(action_id.clone(), chord)
        .unwrap_or_else(|_| unreachable!());
    assert!(
        !controller
            .actions()
            .is_conflicting(action_id.clone(), chord)
    );

    controller.invoke_plugin_action(&action_id, ActionSource::Keyboard);

    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| { debug_probe(c, id, "invoked_tab_bound") == Some(serde_json::Value::from(1)) }
    ));
}

/// D4: a `Continuous` action's `action_invoked` carries `value = Some(1.0)`
/// (never `None`, unlike a `Trigger`). `nudge` ships bound to `Shift+K`,
/// which the `ui-panel` fixture's own `focus_me` (also `Shift+K`, both
/// `Bundled`) deliberately collides with for M6 — rebind away from that
/// conflict first (FR-011's "the user always able to resolve a conflict")
/// so this test proves the value shape, not G13's tier rule (already
/// covered by `modplayer-core/tests/actions.rs::two_bundled_both_flagged`).
#[test]
fn continuous_value_one() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_SHORTCUTS);
    assert!(wait_active(&mut controller, id));

    let identifier = plugin_identifier(&mut controller, id);
    let action_id =
        PluginActionId::parse(&format!("{identifier}.nudge")).unwrap_or_else(|| unreachable!());
    assert!(wait_plugin_action_registered(&mut controller, &action_id));

    let shift_k = Chord::parse("Shift+K").unwrap_or_else(|_| unreachable!());
    controller.remove_binding(action_id.clone(), shift_k);
    let free = Chord::parse("F9").unwrap_or_else(|_| unreachable!());
    controller
        .add_binding(action_id.clone(), free)
        .unwrap_or_else(|_| unreachable!());
    assert!(!controller.actions().is_conflicting(action_id.clone(), free));

    controller.invoke_plugin_action(&action_id, ActionSource::Keyboard);

    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| { debug_probe(c, id, "invoked_nudge") == Some(serde_json::Value::from(1)) }
    ));
    let last_value = debug_probe(&mut controller, id, "last_value");
    assert_eq!(
        last_value.as_ref().and_then(serde_json::Value::as_f64),
        Some(1.0),
        "a Continuous action's value must be exactly 1.0, got {last_value:?}"
    );
}

/// G14: `nudge`'s own shipped default (`Shift+K`) stays flagged (it
/// collides with the `ui-panel` fixture's own `focus_me`, both `Bundled`,
/// M6) — `invoke_plugin_action` on a non-invocable action delivers
/// nothing at all (FR-012), the keyboard-path mirror of
/// `button_names_unknown_action_inert` below.
#[test]
fn inactive_action_not_dispatched() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_SHORTCUTS);
    assert!(wait_active(&mut controller, id));

    let identifier = plugin_identifier(&mut controller, id);
    let action_id =
        PluginActionId::parse(&format!("{identifier}.nudge")).unwrap_or_else(|| unreachable!());
    let shift_k = Chord::parse("Shift+K").unwrap_or_else(|_| unreachable!());
    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| { c.actions().is_conflicting(action_id.clone(), shift_k) }
    ));

    controller.invoke_plugin_action(&action_id, ActionSource::Keyboard);

    // A few settle ticks that *would* have delivered the event if the
    // conflict gate were not enforced.
    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        debug_probe(&mut controller, id, "invoked_nudge"),
        Some(serde_json::Value::from(0)),
        "a flagged (non-invocable) action must never deliver action_invoked"
    );
}

/// D3: a panel button naming an action id that never registered is inert
/// — no `action_invoked`, no panic — and logs a console line naming the
/// unregistered action (`plugins::apply`'s own dispatch target,
/// `PlaybackController::plugin_panel_interaction`).
#[test]
fn button_names_unknown_action_inert() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_PANEL);
    assert!(wait_active(&mut controller, id));
    assert!(wait_panel_registered(&mut controller, id));

    let result = call(
        &mut controller,
        id,
        Request::RegisterPanel {
            panel: wid("probe"),
            title: "Probe".to_string(),
            widgets: vec![button_widget("btn", "Btn", Some("no_such_action"))],
        },
    );
    assert_eq!(result, Ok(Response::Ok));

    controller.plugin_panel_interaction(id, &wid("probe"), &wid("btn"), WidgetValue::Bool(true));

    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        debug_probe(&mut controller, id, "action_invoked_count"),
        Some(serde_json::Value::from(0)),
        "an unregistered action name must never deliver action_invoked"
    );
    assert!(
        controller
            .plugin_log()
            .entries()
            .any(|e| e.message.contains("unregistered action")
                && e.message.contains("no_such_action")),
        "the unresolved button target must log a console line naming it"
    );
}

/// D3/D4: a panel button naming a *registered* action resolves and
/// invokes it with `source = ActionSource::Ui` (never `Keyboard`),
/// mirroring exactly what `plugin_panels.rs`'s own "Take Over" button
/// exercises end to end (`request_focus_in_handler_auto_grants` above) —
/// this test isolates the `source` field itself.
#[test]
fn button_action_source_ui() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_PANEL);
    assert!(wait_active(&mut controller, id));
    assert!(wait_panel_registered(&mut controller, id));

    controller.plugin_panel_interaction(
        id,
        &wid("main"),
        &wid("take_over"),
        WidgetValue::Bool(true),
    );

    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| { debug_probe(c, id, "action_invoked_count") == Some(serde_json::Value::from(1)) }
    ));
    let last = debug_probe(&mut controller, id, "last_action_invoked")
        .unwrap_or_else(|| unreachable!("last_action_invoked must be set"));
    assert_eq!(
        last.get("source").and_then(serde_json::Value::as_str),
        Some("ui"),
        "a panel-button invocation must carry source = \"ui\", got {last:?}"
    );
}

// -- US3: ui.overlay (contracts/overlays-settings-notify.md §1.1 "O4")
// ---------------------------------------------------------------------

/// O4: `plugin_overlays()` orders layers by each plugin's own
/// first-registration sequence — the plugin that registers overlays
/// *first* sorts first, regardless of discovery order or numeric
/// `PluginId`. Drives `add_overlays` directly against two already-
/// discovered fixtures (`call()` bypasses the gateway's own admission,
/// exactly like every other test in this file — the permission check
/// itself is the gateway crate's own `ui_validation.rs`/`gateway.rs`
/// suites' job, not this one's).
#[test]
fn overlay_layers_in_registration_order() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let panel_id = controller_plugin_id(&mut controller, UI_PANEL);
    let shortcuts_id = controller_plugin_id(&mut controller, UI_SHORTCUTS);
    assert!(wait_active(&mut controller, panel_id));
    assert!(wait_active(&mut controller, shortcuts_id));

    // Register the *second*-discovered plugin's overlay first, so a bug
    // that instead sorted by discovery order or numeric `PluginId` would
    // still be caught.
    assert_eq!(
        call(
            &mut controller,
            shortcuts_id,
            Request::AddOverlays {
                primitives: vec![OverlayPrimitive::Line {
                    id: wid("s1"),
                    at_ms: 1_000,
                    color: OverlayColor::Accent,
                }],
            },
        ),
        Ok(Response::Ok)
    );
    assert_eq!(
        call(
            &mut controller,
            panel_id,
            Request::AddOverlays {
                primitives: vec![OverlayPrimitive::Line {
                    id: wid("p1"),
                    at_ms: 2_000,
                    color: OverlayColor::Accent,
                }],
            },
        ),
        Ok(Response::Ok)
    );

    // Index-based, not an exact `layers.len()`, on purpose: this harness
    // (`fixture_controller`) discovers and launches *every* fixture, and
    // the `ui-overlay` fixture (US3 T090) may itself already be `Active`
    // and have registered its own per-`track_changed` overlay set by now
    // — a third, unrelated layer whose presence/position this test must
    // not depend on to stay meaningful.
    let layers = controller.plugin_overlays();
    let shortcuts_idx = layers
        .iter()
        .position(|l| l.plugin == shortcuts_id)
        .unwrap_or_else(|| unreachable!("shortcuts_id must have a layer: {layers:?}"));
    let panel_idx = layers
        .iter()
        .position(|l| l.plugin == panel_id)
        .unwrap_or_else(|| unreachable!("panel_id must have a layer: {layers:?}"));
    assert!(
        shortcuts_idx < panel_idx,
        "the plugin that registered overlays first must sort first: {layers:?}"
    );
}

// -- US4: ui.settings (contracts/overlays-settings-notify.md §2 "S" rules)
// ---------------------------------------------------------------------

fn wait_settings_registered(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> bool {
    pump_controller_until(controller, Duration::from_secs(5), |c| {
        c.plugin_settings_views().iter().any(|v| v.plugin == id)
    })
}

/// S4: a host-applied edit updates the page's own live value immediately
/// and delivers exactly one `settings_changed` to the owning plugin,
/// carrying the changed field.
#[test]
fn settings_edit_delivers_changed() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_SETTINGS);
    assert!(wait_active(&mut controller, id));
    assert!(wait_settings_registered(&mut controller, id));

    controller.plugin_settings_edit(id, "shift", serde_json::json!(3.0));

    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| { debug_probe(c, id, "settings_changed_count") == Some(serde_json::Value::from(1)) }
    ));
    let last = debug_probe(&mut controller, id, "last_changes")
        .unwrap_or_else(|| unreachable!("last_changes must be set"));
    assert_eq!(
        last.get("shift").and_then(serde_json::Value::as_f64),
        Some(3.0),
        "settings_changed must carry the changed field, got {last:?}"
    );
    assert_eq!(
        controller
            .plugin_settings_views()
            .iter()
            .find(|v| v.plugin == id)
            .and_then(|v| v.page.values.get("shift"))
            .cloned(),
        Some(serde_json::json!(3.0)),
        "the page's own live value must reflect the edit immediately"
    );
}

/// R5/FR-018: an edited value survives a full plugin restart. Disabling
/// flushes `settings.json` to disk on teardown (`run_unloading`'s own
/// dirty-scope flush); enabling respawns a fresh thread that loads it
/// back at start — `get_settings()` on that fresh thread reflects it,
/// proving real disk persistence rather than the host's own in-memory
/// registry (which never resets a value on its own, `settings_reregister_
/// keeps_values`'s own point).
#[test]
fn settings_persist_across_restart() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_SETTINGS);
    assert!(wait_active(&mut controller, id));
    assert!(wait_settings_registered(&mut controller, id));

    controller.plugin_settings_edit(id, "shift", serde_json::json!(3.0));
    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| { debug_probe(c, id, "settings_changed_count") == Some(serde_json::Value::from(1)) }
    ));

    controller.plugin_disable(id);
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(5), |c| {
            matches!(
                c.plugins_mut().record(id).map(|r| &r.lifecycle),
                Some(Lifecycle::Disabled)
            )
        }),
        "disable must run the plugin's teardown (and its settings.json flush) to completion"
    );
    controller.plugin_enable(id);
    assert!(wait_active(&mut controller, id));
    // Not `wait_settings_registered`: the *core* registry's page for this
    // plugin never cleared across the restart (S1/data-model.md §8 —
    // values survive a plugin going inactive), so that view would already
    // be non-empty the instant lifecycle flips back to `Active`, before
    // the *fresh* thread's own `ready_ack` has necessarily reached its own
    // `register_settings` call. Waiting on the probe itself instead proves
    // the new thread's own `Shared.settings_schema` is populated — the
    // actual precondition `get_settings_local` needs.
    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| {
            debug_probe(c, id, "get")
                .is_some_and(|v| v.get("shift").and_then(serde_json::Value::as_f64).is_some())
        }
    ));

    let values = debug_probe(&mut controller, id, "get")
        .unwrap_or_else(|| unreachable!("get_settings must reply"));
    assert_eq!(
        values.get("shift").and_then(serde_json::Value::as_f64),
        Some(3.0),
        "the edited value must survive a restart via settings.json, got {values:?}"
    );
}

/// FR-018: `Scope::Settings` is a separate store from `Scope::Plugin` —
/// `state.plugin.get` must never see a settings-page value, even one that
/// shares the same key name.
#[test]
fn settings_not_visible_via_state_plugin() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_SETTINGS);
    assert!(wait_active(&mut controller, id));
    assert!(wait_settings_registered(&mut controller, id));

    controller.plugin_settings_edit(id, "shift", serde_json::json!(3.0));
    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| { debug_probe(c, id, "settings_changed_count") == Some(serde_json::Value::from(1)) }
    ));

    let result = debug_probe(&mut controller, id, "state_shift")
        .unwrap_or_else(|| unreachable!("state_shift probe must reply"));
    assert_eq!(
        result.get("found").and_then(serde_json::Value::as_bool),
        Some(false),
        "state.plugin must never see a settings-page value (FR-018), got {result:?}"
    );
}

/// S2: a disabled plugin's settings page is hidden from Settings ›
/// Plugins (values intact, data-model.md §8) — re-enabling brings it
/// back.
#[test]
fn settings_hidden_when_disabled() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_SETTINGS);
    assert!(wait_active(&mut controller, id));
    assert!(wait_settings_registered(&mut controller, id));

    controller.plugin_disable(id);
    assert!(pump_controller_until(
        &mut controller,
        Duration::from_secs(5),
        |c| {
            matches!(
                c.plugins_mut().record(id).map(|r| &r.lifecycle),
                Some(Lifecycle::Disabled)
            )
        }
    ));
    assert!(
        controller
            .plugin_settings_views()
            .iter()
            .all(|v| v.plugin != id),
        "a disabled plugin's settings page must not appear in Settings › Plugins"
    );

    controller.plugin_enable(id);
    assert!(wait_active(&mut controller, id));
    assert!(wait_settings_registered(&mut controller, id));
    assert!(
        controller
            .plugin_settings_views()
            .iter()
            .any(|v| v.plugin == id),
        "re-enabling must bring the page back"
    );
}

// -- US5: notifications (contracts/overlays-settings-notify.md §3) ---------

/// The `ui-notify` fixture's own `arm:<level>:<n>`/`arm_invalid:<n>`
/// probe (fixture `main.luau`): queues `n` `notify` calls, fired one per
/// its own 5ms timer tick — never in a burst inside this very
/// `debug_probe` call (that file's own comment explains why: a nested
/// RPC inside the host's bounded wait for this reply would deadlock the
/// drain loop against itself).
fn arm_notify(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
    spec: &str,
) {
    let result =
        debug_probe(controller, id, spec).unwrap_or_else(|| unreachable!("{spec} must reply"));
    assert_eq!(
        result.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "{spec} must ack: {result:?}"
    );
}

/// Polls the fixture's `"counts"` probe (a fast, non-nested read) until
/// its queue has fully drained (`done`), for up to `timeout` —
/// `arm_notify` alone cannot report this synchronously since the queued
/// calls fire one per timer tick, not inside that call.
fn wait_notify_counts(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
    timeout: Duration,
) -> serde_json::Value {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = debug_probe(controller, id, "counts")
            && value.get("done").and_then(serde_json::Value::as_bool) == Some(true)
        {
            return value;
        }
        if Instant::now() >= deadline {
            unreachable!("ui-notify's 'counts' probe never reported done within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// N1/SC-005: a 7th `notify` within the rolling 60s window is refused
/// `rate_limited` and never shown — the first 6 post normally, each
/// attributed to this plugin.
#[test]
fn notify_seventh_rate_limited() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_NOTIFY);
    assert!(wait_active(&mut controller, id));

    arm_notify(&mut controller, id, "arm:info:7");
    let counts = wait_notify_counts(&mut controller, id, Duration::from_secs(5));
    assert_eq!(
        counts.get("posted").and_then(serde_json::Value::as_i64),
        Some(6),
        "{counts:?}"
    );
    assert_eq!(
        counts.get("refused").and_then(serde_json::Value::as_i64),
        Some(1),
        "{counts:?}"
    );
    assert_eq!(
        counts
            .get("last_reason")
            .and_then(serde_json::Value::as_str),
        Some("rate_limited"),
        "{counts:?}"
    );

    let attributed = controller
        .notifications()
        .visible()
        .filter(|n| n.attribution.as_ref().is_some_and(|a| a.id == id))
        .count();
    assert_eq!(attributed, 6, "the refused 7th must never be shown");
}

/// N1: an admitted-then-refused `notify` (a >200-char text, refused by
/// `validate_notify` in `plugins::apply`, *after* the notify bucket has
/// already admitted it) still consumed its slot in the window — only 5
/// more succeed, not 6, on the very next batch (mirrors the gateway-level
/// `notify_refused_at_validation_still_counts`, T016, one layer up).
#[test]
fn notify_invalid_consumes_slot() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_NOTIFY);
    assert!(wait_active(&mut controller, id));

    arm_notify(&mut controller, id, "arm_invalid:1");
    let first = wait_notify_counts(&mut controller, id, Duration::from_secs(5));
    assert_eq!(
        first.get("posted").and_then(serde_json::Value::as_i64),
        Some(0),
        "{first:?}"
    );
    assert_eq!(
        first.get("refused").and_then(serde_json::Value::as_i64),
        Some(1),
        "{first:?}"
    );
    assert_eq!(
        first.get("last_reason").and_then(serde_json::Value::as_str),
        Some("invalid_value"),
        "{first:?}"
    );

    arm_notify(&mut controller, id, "arm:info:6");
    let second = wait_notify_counts(&mut controller, id, Duration::from_secs(5));
    assert_eq!(
        second.get("posted").and_then(serde_json::Value::as_i64),
        Some(5),
        "only 5 of 6 must fit: the invalid call already spent one slot: {second:?}"
    );
    assert_eq!(
        second.get("refused").and_then(serde_json::Value::as_i64),
        Some(1),
        "{second:?}"
    );
    assert_eq!(
        second
            .get("last_reason")
            .and_then(serde_json::Value::as_str),
        Some("rate_limited"),
        "{second:?}"
    );
}

/// N1: the 60s window is a rolling one, not a fixed reset point — once it
/// rolls past the batch that filled it, a further `notify` succeeds
/// again. No fast-forward clock hook exists on the plugin thread's own
/// `Gateway` (it always admits against a real `Instant::now()`, RT7), so
/// this genuinely waits out the window in real time — the one test in
/// this suite that does.
#[test]
fn notify_window_rolls() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller();
    controller.launch();
    let id = controller_plugin_id(&mut controller, UI_NOTIFY);
    assert!(wait_active(&mut controller, id));

    arm_notify(&mut controller, id, "arm:info:6");
    let filled = wait_notify_counts(&mut controller, id, Duration::from_secs(5));
    assert_eq!(
        filled.get("posted").and_then(serde_json::Value::as_i64),
        Some(6),
        "{filled:?}"
    );

    arm_notify(&mut controller, id, "arm:info:1");
    let seventh = wait_notify_counts(&mut controller, id, Duration::from_secs(5));
    assert_eq!(
        seventh.get("refused").and_then(serde_json::Value::as_i64),
        Some(1),
        "the window must still be full: {seventh:?}"
    );

    std::thread::sleep(Duration::from_secs(61));

    arm_notify(&mut controller, id, "arm:info:1");
    let rolled = wait_notify_counts(&mut controller, id, Duration::from_secs(5));
    assert_eq!(
        rolled.get("posted").and_then(serde_json::Value::as_i64),
        Some(1),
        "a further notify must succeed once the 60s window has rolled: {rolled:?}"
    );
}

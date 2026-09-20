// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T042 (US1, 011-plugin-ui-contributions, contracts/ui-panels.md §3):
//! `plugin_panels::show_dock`/`show_floated_windows` end to end, driven
//! headlessly through the real `org.modplayer.fixture.ui-panel` fixture —
//! every widget kind's AccessKit role/name/value (A1), the slider/knob's
//! keyboard and drag interaction (§3's table, P4), a list row's own
//! click-to-select, an unclaimed key's fall-through (A3), a theme switch
//! that calls zero plugin code (A4), dock ordering (L1), a floated
//! window's clamp-into-bounds (L2/L4) and its suspended placeholder (P5),
//! and the header's plugin/panel attribution (L5). Mirrors
//! `controller_plugin_ui.rs`'s own fixture harness (core) and
//! `plugins_view.rs`'s own AccessKit-node render harness (this crate).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use egui::accesskit::{NodeId, Role, Toggled};
use egui::{Context, Event, Key, Modifiers, Pos2, RawInput, Rect};
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{Request, Response};
use modplayer_capability_gateway::ui::{UiId, WidgetValue};
use modplayer_core::plugins::{Lifecycle, PluginId, to_gateway_id};
use modplayer_core::settings::{PanelPersisted, PanelPlacement, SettingsStore};
use modplayer_core::{PlaybackController, tr, tr_args};
use modplayer_plugin_runtime::handle::RpcEnvelope;
use modplayer_ui::plugin_panels;

const UI_PANEL: &str = "org.modplayer.fixture.ui-panel";
const PLUGIN_NAME: &str = "UI Panel fixture";

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-plugin-panels-{label}-{}-{unique}",
            std::process::id(),
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

fn fresh_store(label: &str) -> (SettingsStore, TempDir) {
    let dir = TempDir::new(label);
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    (store, dir)
}

/// `MODPLAYER_PLUGIN_FIXTURES`/`MODPLAYER_PLUGIN_STATE_DIR`/
/// `MODPLAYER_TRACK_STATE_DIR` are process-global, so every
/// `PlaybackController::new` in this binary must be serialized against
/// every other's brief mutation of them (mirrors `plugins_view.rs`'s own
/// `PLUGIN_ENV_LOCK`).
static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

fn fixture_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store(label);
    let plugin_state_dir = TempDir::new(&format!("{label}-plugin-state"));
    let track_state_dir = TempDir::new(&format!("{label}-track-state"));
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
        let controller =
            PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_FIXTURES");
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    (controller, dir, plugin_state_dir, track_state_dir)
}

fn fixture_id(
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

fn pump_until(
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

fn is_active(controller: &mut PlaybackController<FakeBackend, ScriptedHost>, id: PluginId) -> bool {
    matches!(
        controller.plugins_mut().record(id).map(|r| &r.lifecycle),
        Some(Lifecycle::Active)
    )
}

fn is_suspended(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> bool {
    matches!(
        controller.plugins_mut().record(id).map(|r| &r.lifecycle),
        Some(Lifecycle::Suspended { .. })
    )
}

/// A controller with the `ui-panel` fixture spawned, `Active`, and its
/// "Controls" panel already registered (waits for the fixture's own
/// asynchronous `ready_ack` handler).
fn launch_ui_panel(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
    PluginId,
) {
    let (mut controller, dir, psd, tsd) = fixture_controller(label);
    let id = fixture_id(&mut controller, UI_PANEL);
    let shared = Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| is_active(
            c, id
        )),
        "the ui-panel fixture must reach Active on its own"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            !c.plugin_panels_view().docked.is_empty()
        }),
        "the fixture's ready_ack handler must have registered its panel"
    );
    (controller, dir, psd, tsd, id)
}

fn wid(s: &str) -> UiId {
    UiId::parse(s).unwrap_or_else(|| unreachable!("{s:?} must be valid grammar"))
}

/// Submits `request` from `plugin` straight to `drain_plugin_requests()`,
/// bypassing the gateway's own admission entirely (mirrors
/// `controller_plugin_ui.rs`'s own `call()`).
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

fn interaction_count(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> i64 {
    match call(
        controller,
        id,
        Request::DebugProbe {
            name: "interaction_count".to_string(),
        },
    ) {
        Ok(Response::Probe(value)) => value.as_i64().unwrap_or(-1),
        _ => -1,
    }
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 2000.0))),
        ..Default::default()
    }
}

/// One AccessKit node's accessibility-relevant fields, plus its own id
/// (needed to track `update.focus`) and its raw numeric value (a
/// slider's own `WidgetInfo::slider` value, contracts/ui-panels.md A1 —
/// distinct from `node.value()`, the *string* value some other widget
/// kinds set instead).
#[derive(Debug, Clone)]
struct AccessNode {
    id: NodeId,
    role: Role,
    label: Option<String>,
    value: Option<String>,
    numeric_value: Option<f64>,
    toggled: Option<Toggled>,
    #[allow(dead_code)]
    disabled: bool,
    bounds: Option<Rect>,
}

impl AccessNode {
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

/// Runs one frame of `render` over `input`, returning every AccessKit node
/// plus which one (if any) holds focus.
fn run_frame(
    ctx: &Context,
    input: RawInput,
    mut render: impl FnMut(&mut egui::Ui),
) -> (Option<NodeId>, Vec<AccessNode>) {
    let mut output = ctx.run_ui(input, |ui| render(ui));
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let focus = update.focus;
    output.drop_without_applying_deltas();

    let nodes = update
        .nodes
        .iter()
        .map(|(id, node)| AccessNode {
            id: *id,
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            numeric_value: node.numeric_value(),
            toggled: node.toggled(),
            disabled: node.is_disabled(),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
        })
        .collect();
    (Some(focus), nodes)
}

fn render_dock(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> Vec<AccessNode> {
    let (_, nodes) = run_frame(ctx, default_input(), |ui| {
        plugin_panels::show_dock(ui, controller);
    });
    nodes
}

fn render_floated(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> Vec<AccessNode> {
    let (_, nodes) = run_frame(ctx, default_input(), |ui| {
        plugin_panels::show_floated_windows(ui.ctx(), controller);
    });
    nodes
}

fn find_all<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> Vec<&'a AccessNode> {
    nodes
        .iter()
        .filter(|node| node.role == role && node.accessible_name() == Some(name))
        .collect()
}

fn find_one<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> &'a AccessNode {
    let matches = find_all(nodes, role, name);
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {role:?} node named `{name}`, found {}: {nodes:?}",
        matches.len()
    );
    matches[0]
}

/// Repeatedly sends `Tab` (one press per frame) until the node named
/// `name` has focus, or panics after a generous bound (mirrors
/// `now_playing.rs`'s own `tab_focus_named`).
fn tab_focus_named(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    name: &str,
) {
    for _ in 0..60 {
        let (focus, nodes) = run_frame(ctx, key_input(Key::Tab, Modifiers::default()), |ui| {
            plugin_panels::show_dock(ui, controller);
        });
        if focus.is_some_and(|focus| {
            nodes
                .iter()
                .any(|n| n.id == focus && n.accessible_name() == Some(name))
        }) {
            return;
        }
    }
    panic!("Tab never reached the `{name}` widget within 60 presses");
}

fn key_input(key: Key, modifiers: Modifiers) -> RawInput {
    let mut input = default_input();
    // egui only updates its per-frame `InputState::modifiers` from a
    // dedicated `ModifiersChanged` event (mirrors `now_playing.rs`'s own
    // `run_key_frame`).
    input.events.push(Event::ModifiersChanged(modifiers));
    input.events.push(Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    input.events.push(Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers,
    });
    input
}

fn press_key(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    key: Key,
    modifiers: Modifiers,
) -> Vec<AccessNode> {
    let (_, nodes) = run_frame(ctx, key_input(key, modifiers), |ui| {
        plugin_panels::show_dock(ui, controller);
    });
    nodes
}

/// Press then release the primary button at `pos` over the dock, in two
/// separate frames (mirrors `plugins_view.rs`'s own `click_at`).
fn click_at(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    pos: Pos2,
) {
    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    run_frame(ctx, press, |ui| plugin_panels::show_dock(ui, controller));

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    run_frame(ctx, release, |ui| plugin_panels::show_dock(ui, controller));
}

fn tempo_value(controller: &PlaybackController<FakeBackend, ScriptedHost>) -> Option<f64> {
    controller
        .plugin_panels_view()
        .docked
        .iter()
        .find(|p| p.key.panel.as_str() == "main")
        .and_then(|p| match &p.body {
            modplayer_core::plugins::PanelBody::Live(widgets) => widgets
                .iter()
                .find(|w| w.spec.id.as_str() == "tempo")
                .map(|w| &w.value),
            modplayer_core::plugins::PanelBody::Placeholder { .. } => None,
        })
        .and_then(|v| match v {
            WidgetValue::Number(n) => Some(*n),
            _ => None,
        })
}

// -----------------------------------------------------------------------

/// A1/contracts/ui-panels.md §3's table: every widget kind the fixture
/// registers exposes the right role and a non-empty accessible name.
#[test]
fn every_kind_has_role_and_name() {
    let (mut controller, _dir, _psd, _tsd, _id) = launch_ui_panel("every-kind");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_dock(&ctx, &mut controller);

    // `label` (info): initial text is non-empty, so it wins over the
    // widget's own `label` field (`show_widget`'s own rule).
    find_one(&nodes, Role::Label, "Fixture panel");

    // `button` ×3.
    for label in ["Take Over", "Hang", "Throw"] {
        find_one(&nodes, Role::Button, label);
    }

    // `toggle`: starts unchecked (FR-002 initial value).
    let power = find_one(&nodes, Role::CheckBox, "Power");
    assert_eq!(power.toggled, Some(Toggled::False));

    // `slider`/`knob`: both painted as `Role::Slider`.
    find_one(&nodes, Role::Slider, "Tempo");
    find_one(&nodes, Role::Slider, "Gain");

    // `list`: its own label, plus one row per item — `selectable_label`
    // renders as `Role::Button` with a `toggled` state (egui's own
    // accesskit mapping), not a literal `ListBox`/`ListBoxOption` — the
    // fixture's default selection ("a"/"Mode A") is toggled on.
    find_one(&nodes, Role::Label, "Mode");
    let mode_a = find_one(&nodes, Role::Button, "Mode A");
    assert_eq!(mode_a.toggled, Some(Toggled::True));
    let mode_b = find_one(&nodes, Role::Button, "Mode B");
    assert_eq!(mode_b.toggled, Some(Toggled::False));

    // `text`.
    find_one(&nodes, Role::Label, "Status: Idle");

    // `marker_list`: its own label, plus the "no markers" placeholder
    // (no track loaded in this harness).
    find_one(&nodes, Role::Label, "Markers");
    find_one(&nodes, Role::Label, &tr("plugin-marker-list-empty"));

    // `meter`: `Role::ProgressIndicator`, starts at 0%.
    find_one(&nodes, Role::ProgressIndicator, "Level: 0%");
}

/// A1: the Tempo slider's own accessible value announces its number (not
/// just its label) — `WidgetInfo::slider`'s `value` field, read back here
/// as AccessKit's `numeric_value`.
#[test]
fn tempo_slider_announces() {
    let (mut controller, _dir, _psd, _tsd, _id) = launch_ui_panel("tempo-announces");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_dock(&ctx, &mut controller);

    let tempo = find_one(&nodes, Role::Slider, "Tempo");
    assert_eq!(
        tempo.numeric_value,
        Some(120.0),
        "the fixture's own initial value: {tempo:?}"
    );
}

/// contracts/ui-panels.md §3: `←/→` step a focused slider by its own
/// `step` and each key press commits (delivers exactly one
/// `panel_interaction`). Re-acquires Tab-focus before each press (rather
/// than assuming it survives an intervening commit): egui's own focus
/// tracking is per-frame state this harness's synthetic, single-event
/// frames don't reproduce identically to a continuously-repainting real
/// window, so re-focusing between presses is what a real keyboard user's
/// own continuous focus (never lost in a real app) is standing in for
/// here.
#[test]
fn arrows_step_and_emit() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_panel("arrows-step");
    let ctx = Context::default();
    ctx.enable_accesskit();
    render_dock(&ctx, &mut controller);

    tab_focus_named(&ctx, &mut controller, "Tempo");
    press_key(&ctx, &mut controller, Key::ArrowRight, Modifiers::default());
    assert_eq!(
        tempo_value(&controller),
        Some(121.0),
        "→ must step the Tempo slider by its own step (1)"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            interaction_count(c, id) == 1
        }),
        "exactly one panel_interaction must be delivered for the → step"
    );

    tab_focus_named(&ctx, &mut controller, "Tempo");
    press_key(&ctx, &mut controller, Key::ArrowLeft, Modifiers::default());
    assert_eq!(
        tempo_value(&controller),
        Some(120.0),
        "← must step back down by 1"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            interaction_count(c, id) == 2
        }),
        "exactly one further panel_interaction must be delivered for the ← step"
    );
}

/// P4/contracts/ui-panels.md §3: a pointer drag on the Tempo slider
/// commits — and delivers `panel_interaction` — exactly once, on
/// release, not on every intermediate drag frame.
#[test]
fn drag_emits_once() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_panel("drag-once");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_dock(&ctx, &mut controller);
    let tempo = find_one(&nodes, Role::Slider, "Tempo");
    let rect = tempo.bounds.expect("the Tempo slider must have bounds");

    let start = rect.left_center() + egui::vec2(2.0, 0.0);
    let end = rect.right_center() - egui::vec2(2.0, 0.0);

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    run_frame(&ctx, press, |ui| {
        plugin_panels::show_dock(ui, &mut controller)
    });

    let mut mv = default_input();
    mv.events.push(Event::PointerMoved(end));
    run_frame(&ctx, mv, |ui| plugin_panels::show_dock(ui, &mut controller));

    // Still dragging: no interaction delivered yet.
    for _ in 0..10 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        interaction_count(&mut controller, id),
        0,
        "an in-progress drag must not itself deliver panel_interaction"
    );

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    run_frame(&ctx, release, |ui| {
        plugin_panels::show_dock(ui, &mut controller)
    });

    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            interaction_count(c, id) == 1
        }),
        "drag-release must deliver exactly one panel_interaction"
    );
    assert!(
        tempo_value(&controller).is_some_and(|v| v > 120.0),
        "the drag must have moved the value up from its 120 starting point: {:?}",
        tempo_value(&controller)
    );
}

/// contracts/ui-panels.md §3: clicking a `list` row selects it (and the
/// previously-selected row's own accessible `toggled` state flips off).
#[test]
fn list_selection_follows_focus() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_panel("list-selection");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_dock(&ctx, &mut controller);
    let mode_b = find_one(&nodes, Role::Button, "Mode B");
    assert_eq!(mode_b.toggled, Some(Toggled::False));
    let pos = mode_b.bounds.expect("Mode B must have bounds").center();

    click_at(&ctx, &mut controller, pos);

    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            interaction_count(c, id) == 1
        }),
        "selecting a different row must deliver exactly one panel_interaction"
    );

    let nodes = render_dock(&ctx, &mut controller);
    let mode_a = find_one(&nodes, Role::Button, "Mode A");
    let mode_b = find_one(&nodes, Role::Button, "Mode B");
    assert_eq!(
        mode_a.toggled,
        Some(Toggled::False),
        "the old selection must no longer read toggled: {mode_a:?}"
    );
    assert_eq!(
        mode_b.toggled,
        Some(Toggled::True),
        "the newly-clicked row must read toggled: {mode_b:?}"
    );
}

/// A3: a key no panel widget claims (a plain letter, not in
/// `numeric_claims`/`list_claims`) is left completely untouched — the
/// focused slider's own value never changes, so it is free to fall
/// through to `actions::dispatch` (007 FR-019 step 2) exactly as any
/// other unclaimed key would.
#[test]
fn unclaimed_key_falls_through() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_panel("unclaimed-key");
    let ctx = Context::default();
    ctx.enable_accesskit();
    render_dock(&ctx, &mut controller);
    tab_focus_named(&ctx, &mut controller, "Tempo");

    press_key(&ctx, &mut controller, Key::A, Modifiers::default());

    assert_eq!(
        tempo_value(&controller),
        Some(120.0),
        "an unclaimed key must never step or commit the focused slider"
    );
    for _ in 0..10 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        interaction_count(&mut controller, id),
        0,
        "an unclaimed key must never deliver panel_interaction"
    );
}

/// A4: a theme switch alone repaints with zero controller calls into any
/// plugin — no widget value changes, no interaction is delivered.
#[test]
fn theme_switch_no_events() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_panel("theme-switch");
    let ctx = Context::default();
    ctx.enable_accesskit();
    render_dock(&ctx, &mut controller);
    let before = tempo_value(&controller);

    ctx.set_visuals(egui::Visuals::dark());
    render_dock(&ctx, &mut controller);
    ctx.set_visuals(egui::Visuals::light());
    let nodes = render_dock(&ctx, &mut controller);

    assert_eq!(
        before,
        tempo_value(&controller),
        "a theme switch alone must never change a widget's stored value"
    );
    find_one(&nodes, Role::Slider, "Tempo");
    for _ in 0..10 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        interaction_count(&mut controller, id),
        0,
        "a theme switch must never itself deliver panel_interaction"
    );
}

/// L1: panels stack in (plugin name, registration seq) order — proven
/// here on one plugin's own two panels (same name, so only `seq`
/// decides): the fixture's "Controls" (seq 0, registered on `ready_ack`)
/// must sit above a second panel registered afterwards, even though the
/// second panel's own title ("AAA Panel") would sort first alphabetically
/// if title, not seq, controlled the order.
#[test]
fn dock_order_by_name_then_seq() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_panel("dock-order");

    call(
        &mut controller,
        id,
        Request::RegisterPanel {
            panel: wid("second"),
            title: "AAA Panel".to_string(),
            widgets: vec![modplayer_capability_gateway::ui::WidgetSpec {
                id: wid("only"),
                kind: modplayer_capability_gateway::ui::WidgetKind::Label,
                label: "Only".to_string(),
                min: None,
                max: None,
                step: None,
                value: None,
                items: Vec::new(),
                selected: None,
                action: None,
                text: Some("hi".to_string()),
            }],
        },
    )
    .unwrap_or_else(|e| unreachable!("second panel must register: {e:?}"));

    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_dock(&ctx, &mut controller);

    let main_header = find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "plugin-panel-header",
            &[
                ("plugin", PLUGIN_NAME.to_string()),
                ("title", "Controls".to_string()),
            ],
        ),
    );
    let second_header = find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "plugin-panel-header",
            &[
                ("plugin", PLUGIN_NAME.to_string()),
                ("title", "AAA Panel".to_string()),
            ],
        ),
    );
    let main_y = main_header
        .bounds
        .expect("main header must have bounds")
        .min
        .y;
    let second_y = second_header
        .bounds
        .expect("second header must have bounds")
        .min
        .y;
    assert!(
        main_y < second_y,
        "the first-registered panel (seq 0) must sit above the second (seq 1): main={main_y} second={second_y}"
    );
}

/// L2/L4: a floated window whose persisted geometry sits (or has grown)
/// far outside the current content rect is clamped back inside it —
/// proven on the header's own accessible bounds (`clamp_into`'s pos/size
/// are what `Window::default_pos`/`default_size` are built from).
#[test]
fn float_clamped_into_window() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_panel("float-clamped");
    let identifier = controller
        .plugins_mut()
        .record(id)
        .map(|r| r.identifier.clone())
        .unwrap_or_else(|| unreachable!());
    let key = modplayer_core::plugins::ui::panel::PanelKey::new(identifier, wid("main"));
    controller.plugin_panel_set_placement(
        &key,
        PanelPersisted {
            placement: PanelPlacement::Floated,
            x: Some(-50_000.0),
            y: Some(-50_000.0),
            w: Some(500_000.0),
            h: Some(500_000.0),
            ..Default::default()
        },
    );

    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_floated(&ctx, &mut controller);

    let header = find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "plugin-panel-header",
            &[
                ("plugin", PLUGIN_NAME.to_string()),
                ("title", "Controls".to_string()),
            ],
        ),
    );
    let bounds = header.bounds.expect("floated header must have bounds");
    let available = Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 2000.0));
    assert!(
        available.expand(4.0).contains_rect(bounds),
        "a wildly out-of-bounds persisted geometry must be clamped back into the content rect: {bounds:?} vs {available:?}"
    );
}

/// P5: while its plugin is `Suspended`, a panel keeps its place (still
/// visible) but renders the placeholder — plugin/cause text plus a
/// Restart control — never its own widgets, and zero plugin code runs.
#[test]
fn placeholder_on_suspend() {
    // Not `launch_ui_panel`: the running plugin thread captures its
    // `Budgets` once, at spawn (mirrors `plugins_view.rs`'s own
    // `suspended_row_shows_dash_gauges`), so the 1 ms share override must
    // land *before* `spawn`, not after the fixture is already `Active`.
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("placeholder-suspend");
    let id = fixture_id(&mut controller, UI_PANEL);
    if let Some(record) = controller.plugins_mut().record_mut(id) {
        record.budgets = modplayer_capability_gateway::budgets::Budgets {
            share: Duration::from_millis(1),
            ..modplayer_capability_gateway::budgets::Budgets::DEFAULT
        };
    }
    let shared = Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| is_active(
            c, id
        )),
        "the ui-panel fixture must reach Active on its own"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            !c.plugin_panels_view().docked.is_empty()
        }),
        "the fixture's ready_ack handler must have registered its panel"
    );

    // A 1 ms share suspends the fixture's own "hang" busy-loop probe the
    // moment it runs.
    controller.plugin_panel_interaction(id, &wid("main"), &wid("hang"), WidgetValue::Bool(true));
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| is_suspended(
            c, id
        )),
        "the hang probe must suspend the fixture under a 1ms share"
    );

    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_dock(&ctx, &mut controller);

    find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "plugin-panel-suspended",
            &[
                ("plugin", PLUGIN_NAME.to_string()),
                ("cause", tr("plugin-suspended-cause-hang")),
            ],
        ),
    );
    find_one(&nodes, Role::Button, &tr("plugin-panel-restart"));
    assert!(
        nodes
            .iter()
            .all(|n| n.role != Role::CheckBox && n.role != Role::Slider),
        "a suspended panel must render no live widgets at all: {nodes:?}"
    );
}

/// L5: the header carries both the plugin's own name and the panel's own
/// title, plus an icon (its own decoded asset, or the generic-glyph
/// fallback — either way a real, named `Role::Image` node, A5).
#[test]
fn header_attribution() {
    let (mut controller, _dir, _psd, _tsd, _id) = launch_ui_panel("header-attribution");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_dock(&ctx, &mut controller);

    find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "plugin-panel-header",
            &[
                ("plugin", PLUGIN_NAME.to_string()),
                ("title", "Controls".to_string()),
            ],
        ),
    );
    assert!(
        nodes
            .iter()
            .any(|n| n.role == Role::Image && n.accessible_name().is_some()),
        "the header must carry a named icon (real asset or generic-glyph fallback): {nodes:?}"
    );
}

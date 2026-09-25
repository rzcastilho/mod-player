// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! 018-window-sizing-and-responsive-dock, contract D3, data-model.md §4:
//! the dock's resize splitter (D10.7, Phase 4/US2), plus Phase 5/US3's
//! overlay/header/pseudo-localization coverage (D10.5/D10.6/D10.8). Driven
//! headlessly through the real `org.modplayer.fixture.ui-panel` fixture,
//! mirroring `plugin_panels.rs`'s own harness (`launch_ui_panel`,
//! `run_frame`/`AccessNode`) so the dock/overlay/toggle are exercised
//! against real docked panels exactly as the app renders them.

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
use modplayer_capability_gateway::ui::{UiId, WidgetKind, WidgetSpec};
use modplayer_core::i18n::with_pseudo_expansion;
use modplayer_core::plugins::{Lifecycle, PluginId, to_gateway_id};
use modplayer_core::settings::{DOCK_WIDTH_MAX, DOCK_WIDTH_MIN, SettingsStore};
use modplayer_core::{PlaybackController, tr, tr_args};
use modplayer_plugin_runtime::handle::RpcEnvelope;
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::layout::DOCK_KEY_STEP;
use modplayer_ui::waveform::WaveformState;
use modplayer_ui::{now_playing, plugin_panels};

const UI_PANEL: &str = "org.modplayer.fixture.ui-panel";

/// 014-design-tokens-and-type-scale (US2, T034): install the token
/// `Style` the panel chrome header now reaches (mirrors `plugin_panels.rs`
/// test's identically-named helper).
fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-responsive-dock-{label}-{}-{unique}",
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

/// Process-global plugin-fixture env vars — serialized against every other
/// `PlaybackController::new` in this binary (mirrors `plugin_panels.rs`'s
/// own `PLUGIN_ENV_LOCK`).
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

/// Waits until the `ui-panel` fixture's own docked panel has registered
/// (mirrors `plugin_panels.rs`'s own `wait_ui_panel_quiescent`, trimmed to
/// just the part this file needs — the splitter only needs a docked panel
/// to exist, not the fixture's full start-up-timer quiescence).
fn wait_ui_panel_docked(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) {
    assert!(
        pump_until(controller, Duration::from_secs(2), |c| {
            !c.plugin_panels_view().docked.is_empty()
        }),
        "the fixture's ready_ack handler must have registered its panel"
    );
}

/// A controller with the `ui-panel` fixture spawned, `Active`, and its
/// panel already docked.
fn launch_ui_panel(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
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
    wait_ui_panel_docked(&mut controller);
    (controller, dir, psd, tsd)
}

/// 1200-wide screen (mirrors `plugin_panels.rs`'s own `default_input`):
/// wide enough that `layout::effective_dock_width`'s second clamp bound
/// (`max(240, min(480, content_width - 560))`) resolves to the full
/// `DOCK_WIDTH_MAX` (480), so this file's clamp assertions exercise both
/// of `effective_dock_width`'s bounds (240 and 480) rather than a
/// width-dependent third value.
fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 2000.0))),
        ..Default::default()
    }
}

#[derive(Debug, Clone)]
struct AccessNode {
    id: NodeId,
    role: Role,
    label: Option<String>,
    value: Option<String>,
    numeric_value: Option<f64>,
    toggled: Option<Toggled>,
    bounds: Option<Rect>,
}

impl AccessNode {
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

/// Runs one frame of `render` over `input`, returning every AccessKit node
/// plus which one (if any) holds focus (mirrors `plugin_panels.rs`'s own
/// `run_frame`).
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

fn key_input(key: Key, modifiers: Modifiers) -> RawInput {
    let mut input = default_input();
    // egui only updates its per-frame `InputState::modifiers` from a
    // dedicated `ModifiersChanged` event (mirrors `plugin_panels.rs`'s own
    // `key_input`).
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
) -> Vec<AccessNode> {
    let (_, nodes) = run_frame(ctx, key_input(key, Modifiers::default()), |ui| {
        plugin_panels::show_dock(ui, controller);
    });
    nodes
}

/// Repeatedly sends `Tab` until the splitter has focus, or panics after a
/// generous bound (mirrors `plugin_panels.rs`'s own `tab_focus_named`).
fn tab_focus_splitter(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) {
    const SPLITTER: &str = "Resize plugin dock";
    for _ in 0..60 {
        let (focus, nodes) = run_frame(ctx, key_input(Key::Tab, Modifiers::default()), |ui| {
            plugin_panels::show_dock(ui, controller);
        });
        if focus.is_some_and(|focus| {
            nodes
                .iter()
                .any(|n| n.id == focus && n.accessible_name() == Some(SPLITTER))
        }) {
            return;
        }
    }
    panic!("Tab never reached the splitter within 60 presses");
}

// -----------------------------------------------------------------------
// Phase 5/US3 helpers: a second docked panel, and driving the whole
// `now_playing::show` screen (dock/overlay + the "Panels" toggle live in
// two different modules, so D10.5/D10.6/D10.8 need the real integration
// point, not `plugin_panels::show_dock` alone).
// -----------------------------------------------------------------------

fn wid(s: &str) -> UiId {
    UiId::parse(s).unwrap_or_else(|| unreachable!("{s:?} must be valid grammar"))
}

/// Submits `request` from `plugin` straight to `drain_plugin_requests()`,
/// bypassing the gateway's own admission entirely (mirrors
/// `plugin_panels.rs`'s own `call`).
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

/// Registers a second docked panel on the already-`Active` `ui-panel`
/// fixture (mirrors `plugin_panels.rs`'s own `dock_order_by_name_then_seq`
/// setup) — D10.5/D10.6/D10.8 all need `docked_count == 2`.
fn register_second_panel(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
    title: &str,
) {
    call(
        controller,
        id,
        Request::RegisterPanel {
            panel: wid("second"),
            title: title.to_string(),
            widgets: vec![WidgetSpec {
                id: wid("only"),
                kind: WidgetKind::Label,
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
}

fn now_playing_input(width: f32, height: f32) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, height))),
        ..Default::default()
    }
}

/// Runs one frame of the *whole* Now Playing screen (dock/overlay via
/// `plugin_panels`, the "Panels" toggle via `now_playing`'s own transport
/// row) over `input`, mirroring `run_frame` above.
fn run_now_playing_frame(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    input: RawInput,
) -> (Option<NodeId>, Vec<AccessNode>) {
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    run_frame(ctx, input, |ui| {
        now_playing::show(ui, controller, &mut artwork, &mut waveform);
    })
}

fn render_now_playing(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    width: f32,
    height: f32,
) -> (Option<NodeId>, Vec<AccessNode>) {
    run_now_playing_frame(ctx, controller, now_playing_input(width, height))
}

fn key_input_np(width: f32, height: f32, key: Key, modifiers: Modifiers) -> RawInput {
    let mut input = now_playing_input(width, height);
    // egui only updates its per-frame `InputState::modifiers` from a
    // dedicated `ModifiersChanged` event (mirrors `key_input` above).
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

fn press_key_np(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    width: f32,
    height: f32,
    key: Key,
) -> (Option<NodeId>, Vec<AccessNode>) {
    run_now_playing_frame(
        ctx,
        controller,
        key_input_np(width, height, key, Modifiers::default()),
    )
}

/// Press then release the primary button at `pos` over the Now Playing
/// screen, in two separate frames (mirrors `plugin_panels.rs`'s own
/// `click_at`).
fn click_at_np(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    width: f32,
    height: f32,
    pos: Pos2,
) {
    let mut press = now_playing_input(width, height);
    press.events.push(Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    run_now_playing_frame(ctx, controller, press);

    let mut release = now_playing_input(width, height);
    release.events.push(Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    run_now_playing_frame(ctx, controller, release);
}

/// Repeatedly sends `Tab` (one Now Playing frame per press) until the node
/// named `name` has focus, or panics after a generous bound (mirrors
/// `tab_focus_splitter` above and `plugin_panels.rs`'s own
/// `tab_focus_named`).
fn tab_focus_named_np(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    width: f32,
    height: f32,
    name: &str,
) {
    for _ in 0..80 {
        let (focus, nodes) = run_now_playing_frame(
            ctx,
            controller,
            key_input_np(width, height, Key::Tab, Modifiers::default()),
        );
        if focus.is_some_and(|focus| {
            nodes
                .iter()
                .any(|n| n.id == focus && n.accessible_name() == Some(name))
        }) {
            return;
        }
    }
    panic!("Tab never reached the `{name}` widget within 80 presses");
}

/// As [`run_now_playing_frame`], but also returns the painted text of
/// every `Shape::Text` whose `Galley` reports `elided` (D10.8/SC-007):
/// `Label::truncate()`'s own truncation flag, the precise signal for "this
/// text did not fit" — distinct from the accessible name, which every
/// header button/toggle in this file pins to its full, un-elided label
/// regardless of what the painted galley itself had room for.
fn run_now_playing_frame_with_elisions(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    width: f32,
    height: f32,
) -> (Vec<AccessNode>, Vec<String>) {
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let mut output = ctx.run_ui(now_playing_input(width, height), |ui| {
        now_playing::show(ui, controller, &mut artwork, &mut waveform);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let elided_texts: Vec<String> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(t) if t.galley.elided => Some(t.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    let nodes: Vec<AccessNode> = update
        .nodes
        .iter()
        .map(|(id, node)| AccessNode {
            id: *id,
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            numeric_value: node.numeric_value(),
            toggled: node.toggled(),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
        })
        .collect();
    output.drop_without_applying_deltas();
    (nodes, elided_texts)
}

/// D10.8/SC-007's other half: no two interactive (`Button`/`CheckBox`/
/// `Splitter`) widgets' own accessible bounds actually overlap (a shared
/// edge, area ~0, is fine — real 2D overlap is not).
fn assert_no_overlapping_interactive_rects(nodes: &[AccessNode], context: &str) {
    let items: Vec<(&AccessNode, Rect)> = nodes
        .iter()
        .filter(|n| matches!(n.role, Role::Button | Role::CheckBox | Role::Splitter))
        .filter_map(|n| n.bounds.map(|b| (n, b)))
        .collect();
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            let (na, ra) = items[i];
            let (nb, rb) = items[j];
            let overlap = ra.intersect(rb);
            let overlapping = overlap.width() > 0.5 && overlap.height() > 0.5;
            assert!(
                !overlapping,
                "{context}: interactive rects overlap: {:?} ({:?}) vs {:?} ({:?})",
                na, ra, nb, rb
            );
        }
    }
}

/// D10.7: a pointer drag on the splitter tracks the pointer live every
/// frame (SC-003, its AccessKit numeric value updates mid-drag) but only
/// commits `controller.set_dock_width` once, on release — never during
/// the drag itself.
#[test]
fn drag_tracks_pointer_and_commits_once_on_release() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("splitter-drag");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_dock(&ctx, &mut controller);
    let splitter = find_one(&nodes, Role::Splitter, "Resize plugin dock");
    let rect = splitter.bounds.expect("the splitter must have bounds");
    let start_width = controller.window_settings().dock_width;
    assert_eq!(
        splitter.numeric_value,
        Some(f64::from(start_width)),
        "the splitter's own AccessKit value must start at the stored width"
    );

    let start = rect.center();
    // Moving the pointer left grows the dock (data-model.md §4: `live_width
    // = drag_start_width - pointer_delta_x`; a leftward move is a negative
    // delta_x).
    let mid = start - egui::vec2(40.0, 0.0);

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
    mv.events.push(Event::PointerMoved(mid));
    let (_, nodes) = run_frame(&ctx, mv, |ui| plugin_panels::show_dock(ui, &mut controller));
    let live = find_one(&nodes, Role::Splitter, "Resize plugin dock");
    assert_eq!(
        live.numeric_value,
        Some(f64::from(start_width + 40.0)),
        "mid-drag, the splitter's rendered width must already track the pointer"
    );
    assert_eq!(
        controller.window_settings().dock_width,
        start_width,
        "an in-progress drag must not itself persist a width"
    );

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: mid,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    run_frame(&ctx, release, |ui| {
        plugin_panels::show_dock(ui, &mut controller)
    });
    assert_eq!(
        controller.window_settings().dock_width,
        start_width + 40.0,
        "release must commit the dragged width exactly once"
    );
}

/// D10.7: a drag clamps at `DOCK_WIDTH_MIN` (dragging the splitter far to
/// the right, shrinking the dock) and at the effective max (dragging far
/// to the left, growing it) — never past either bound.
#[test]
fn drag_clamps_at_min_and_effective_max() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("splitter-drag-clamp");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_dock(&ctx, &mut controller);
    let start = nodes
        .iter()
        .find(|n| n.role == Role::Splitter)
        .and_then(|n| n.bounds)
        .expect("the splitter must have bounds")
        .center();

    // Drag far right: shrinks past DOCK_WIDTH_MIN, must clamp there.
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
    mv.events
        .push(Event::PointerMoved(start + egui::vec2(2000.0, 0.0)));
    run_frame(&ctx, mv, |ui| plugin_panels::show_dock(ui, &mut controller));
    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: start + egui::vec2(2000.0, 0.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    run_frame(&ctx, release, |ui| {
        plugin_panels::show_dock(ui, &mut controller)
    });
    assert_eq!(
        controller.window_settings().dock_width,
        DOCK_WIDTH_MIN,
        "a far-rightward drag must clamp at DOCK_WIDTH_MIN"
    );

    // Drag far left from the new (post-clamp) splitter position: grows
    // past the effective max, must clamp there (1200-wide screen: max is
    // the full DOCK_WIDTH_MAX, contract D2).
    let nodes = render_dock(&ctx, &mut controller);
    let start = nodes
        .iter()
        .find(|n| n.role == Role::Splitter)
        .and_then(|n| n.bounds)
        .expect("the splitter must have bounds")
        .center();
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
    mv.events
        .push(Event::PointerMoved(start - egui::vec2(2000.0, 0.0)));
    run_frame(&ctx, mv, |ui| plugin_panels::show_dock(ui, &mut controller));
    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: start - egui::vec2(2000.0, 0.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    run_frame(&ctx, release, |ui| {
        plugin_panels::show_dock(ui, &mut controller)
    });
    assert_eq!(
        controller.window_settings().dock_width,
        DOCK_WIDTH_MAX,
        "a far-leftward drag must clamp at the effective max (480 at 1200pt wide)"
    );
}

/// D10.7: `←`/`→` step the width by [`DOCK_KEY_STEP`] and each commits
/// immediately (no drag/release needed).
#[test]
fn keyboard_left_right_steps_and_commits() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("splitter-keys-step");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    render_dock(&ctx, &mut controller);
    let start_width = controller.window_settings().dock_width;

    // A single `Tab` acquisition: plain arrow/Home/End presses never move
    // egui's own focus (only `Tab`/`Shift+Tab` do), so the splitter keeps
    // focus across every subsequent press in this test.
    tab_focus_splitter(&ctx, &mut controller);
    press_key(&ctx, &mut controller, Key::ArrowLeft);
    assert_eq!(
        controller.window_settings().dock_width,
        start_width + DOCK_KEY_STEP,
        "← must grow the dock by DOCK_KEY_STEP and commit immediately"
    );

    press_key(&ctx, &mut controller, Key::ArrowRight);
    assert_eq!(
        controller.window_settings().dock_width,
        start_width,
        "→ must shrink the dock back down by DOCK_KEY_STEP"
    );
}

/// D10.7: `Home` jumps to `DOCK_WIDTH_MIN`, `End` jumps to the effective
/// max, each committing immediately.
#[test]
fn keyboard_home_end_jump_and_clamp() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("splitter-keys-home-end");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    render_dock(&ctx, &mut controller);

    tab_focus_splitter(&ctx, &mut controller);
    press_key(&ctx, &mut controller, Key::Home);
    assert_eq!(
        controller.window_settings().dock_width,
        DOCK_WIDTH_MIN,
        "Home must jump straight to DOCK_WIDTH_MIN"
    );

    press_key(&ctx, &mut controller, Key::End);
    assert_eq!(
        controller.window_settings().dock_width,
        DOCK_WIDTH_MAX,
        "End must jump to the effective max (480 at 1200pt wide)"
    );
}

/// D3 (manual walk M6 regression, 2026-09-25): on the full Now Playing
/// screen — where, unlike the bare dock above, there *are* widgets to the
/// splitter's left and right — consecutive `←`/`→` presses keep the
/// splitter focused (egui's own arrow-key focus navigation must not steal
/// it), so the whole ←×3, →, Home, End sequence lands.
#[test]
fn keyboard_sequence_keeps_focus_on_full_screen() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("splitter-keys-full-screen");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let (w, h) = (1400.0, 800.0);
    controller.set_dock_width(300.0);
    render_now_playing(&ctx, &mut controller, w, h);

    tab_focus_named_np(&ctx, &mut controller, w, h, &tr("plugin-dock-resize"));
    // One idle frame, as the real ≥ 30 Hz app always has between a `Tab`
    // and the next key: egui only accepts a focus-lock filter from a widget
    // that already had focus on the previous frame.
    render_now_playing(&ctx, &mut controller, w, h);
    for _ in 0..3 {
        press_key_np(&ctx, &mut controller, w, h, Key::ArrowLeft);
    }
    assert_eq!(
        controller.window_settings().dock_width,
        300.0 + 3.0 * DOCK_KEY_STEP
    );
    press_key_np(&ctx, &mut controller, w, h, Key::ArrowRight);
    assert_eq!(
        controller.window_settings().dock_width,
        300.0 + 2.0 * DOCK_KEY_STEP
    );
    press_key_np(&ctx, &mut controller, w, h, Key::Home);
    assert_eq!(controller.window_settings().dock_width, DOCK_WIDTH_MIN);
    press_key_np(&ctx, &mut controller, w, h, Key::End);
    assert_eq!(controller.window_settings().dock_width, DOCK_WIDTH_MAX);
}

// -- Phase 5/US3 --------------------------------------------------------

/// D10.6: window 960×640 with 2 docked panels — no dock content renders
/// and the "Panels" toggle is present (off); activating it opens the
/// overlay with the same panels, in the same order; `Escape` while focus
/// is inside the overlay closes it and moves focus to the toggle; widening
/// the window back to ≥ 1024 pt redocks them and removes the toggle.
#[test]
fn overlay_toggle_escape_and_widen() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("d10-6-overlay");
    let id = fixture_id(&mut controller, UI_PANEL);
    register_second_panel(&mut controller, id, "Second Panel");

    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // Narrow: no dock/overlay content, "Panels" toggle present and off.
    let (_, nodes) = render_now_playing(&ctx, &mut controller, 960.0, 640.0);
    assert!(
        nodes.iter().all(|n| n.role != Role::Splitter),
        "no splitter/dock content must render while Hidden: {nodes:?}"
    );
    let toggle = find_one(&nodes, Role::Button, &tr("plugin-dock-panels-toggle"));
    assert_eq!(toggle.toggled, Some(Toggled::False));
    let toggle_pos = toggle.bounds.expect("the toggle must have bounds").center();

    // Activate: the overlay opens with both panels (their own content and
    // the shared splitter both present), toggle now on.
    click_at_np(&ctx, &mut controller, 960.0, 640.0, toggle_pos);
    let (_, nodes) = render_now_playing(&ctx, &mut controller, 960.0, 640.0);
    find_one(&nodes, Role::Splitter, &tr("plugin-dock-resize"));
    find_one(&nodes, Role::Button, "Take Over");
    // The second panel's own `Label` widget: its initial `WidgetValue::
    // Text("hi")` wins over its `spec.label` ("Only", `show_widget`'s own
    // rule).
    find_one(&nodes, Role::Label, "hi");
    let toggle = find_one(&nodes, Role::Button, &tr("plugin-dock-panels-toggle"));
    assert_eq!(toggle.toggled, Some(Toggled::True));

    // Focus a widget inside the overlay, then `Escape` closes it — in the
    // very same frame, focus already lands on the toggle (D5/D6), even
    // though that frame still painted the overlay's own content.
    tab_focus_named_np(
        &ctx,
        &mut controller,
        960.0,
        640.0,
        &tr("plugin-dock-resize"),
    );
    // One settled frame with focus already in place: `Memory::
    // set_focus_lock_filter` (`plugin_panels::hold_focus_through_escape`)
    // only arms once the widget *had* focus last frame too, so this is
    // what lets the very next `Escape` press act on the overlay itself
    // instead of egui's own default (clear focus outright) firing first —
    // exactly what a real user tabbing to the splitter and then pressing
    // `Escape` a beat later already gets for free.
    render_now_playing(&ctx, &mut controller, 960.0, 640.0);
    let (focus, nodes) = press_key_np(&ctx, &mut controller, 960.0, 640.0, Key::Escape);
    let toggle_this_frame = find_one(&nodes, Role::Button, &tr("plugin-dock-panels-toggle"));
    assert_eq!(
        toggle_this_frame.toggled,
        Some(Toggled::False),
        "the toggle must already read off the same frame Escape closes the overlay"
    );
    assert_eq!(
        focus,
        Some(toggle_this_frame.id),
        "focus must move to the 'Panels' toggle the same frame Escape closes the overlay"
    );

    // The following frame is the steady state: the overlay is actually
    // gone.
    let (_, nodes) = render_now_playing(&ctx, &mut controller, 960.0, 640.0);
    assert!(
        nodes.iter().all(|n| n.role != Role::Splitter),
        "the overlay must be gone by the next frame: {nodes:?}"
    );
    find_one(&nodes, Role::Button, &tr("plugin-dock-panels-toggle"));

    // Widen: redocked, toggle gone.
    let (_, nodes) = render_now_playing(&ctx, &mut controller, 1200.0, 640.0);
    find_one(&nodes, Role::Splitter, &tr("plugin-dock-resize"));
    assert!(
        nodes
            .iter()
            .all(|n| n.accessible_name() != Some(tr("plugin-dock-panels-toggle").as_str())),
        "the toggle must be gone once docked: {nodes:?}"
    );
}

/// D10.5: window 1100 wide, dock at 240, two docked panels — every header
/// button across both panels renders with its own full accessible name
/// (none culled), and the title's full-text tooltip appears once keyboard
/// focus lands on it.
#[test]
fn docked_header_buttons_present_and_title_tooltip_on_focus() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("d10-5-header");
    let id = fixture_id(&mut controller, UI_PANEL);
    register_second_panel(&mut controller, id, "Second Panel");

    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let (_, nodes) = render_now_playing(&ctx, &mut controller, 1100.0, 800.0);
    find_one(&nodes, Role::Splitter, &tr("plugin-dock-resize"));
    assert_eq!(
        find_all(&nodes, Role::Button, &tr("plugin-panel-float")).len(),
        2,
        "both panels' Float button must render: {nodes:?}"
    );
    assert_eq!(
        find_all(&nodes, Role::Button, &tr("plugin-panel-close")).len(),
        2,
        "both panels' Close button must render: {nodes:?}"
    );
    assert_eq!(
        find_all(&nodes, Role::Button, &tr("plugin-panel-disable")).len(),
        2,
        "both panels' Disable button must render: {nodes:?}"
    );

    let full_header = tr_args(
        "plugin-panel-header",
        &[
            ("plugin", "UI Panel fixture".to_string()),
            ("title", "Controls".to_string()),
        ],
    );
    // Unfocused: only the title's own pinned AccessKit label carries the
    // full header text (D7).
    assert_eq!(
        find_all(&nodes, Role::Label, &full_header).len(),
        1,
        "unfocused: only the title's own label should carry the full header text: {nodes:?}"
    );

    // Keyboard focus on the title shows its full-text tooltip — a second
    // `Label` node with the same text (`show_header_title`'s own
    // `show_tooltip_ui` call, contract D7).
    tab_focus_named_np(&ctx, &mut controller, 1100.0, 800.0, &full_header);
    let (_, nodes) = render_now_playing(&ctx, &mut controller, 1100.0, 800.0);
    assert!(
        find_all(&nodes, Role::Label, &full_header).len() >= 2,
        "focused: the full-text tooltip must add a second Label node with the same text: {nodes:?}"
    );
}

/// D7 (manual walk M5 regression, 2026-09-25): at every dock width
/// between 240 and 480 — not just the two extremes, where the header
/// either wraps or its title fits outright — a long title truncates
/// instead of pushing the Float/Close/Disable buttons past the dock's
/// right edge (seen at a restored width of 354).
#[test]
fn header_buttons_stay_inside_dock_at_every_width() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("d7-mid-width");
    let id = fixture_id(&mut controller, UI_PANEL);
    register_second_panel(&mut controller, id, &"Long panel title ".repeat(3));

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let (window_w, window_h) = (1400.0, 800.0);

    let mut width = 240.0;
    while width <= 480.0 {
        controller.set_dock_width(width);
        render_now_playing(&ctx, &mut controller, window_w, window_h);
        let (_, nodes) = render_now_playing(&ctx, &mut controller, window_w, window_h);
        let splitter = find_one(&nodes, Role::Splitter, &tr("plugin-dock-resize"))
            .bounds
            .expect("splitter bounds");
        for key in [
            "plugin-panel-float",
            "plugin-panel-close",
            "plugin-panel-disable",
        ] {
            let buttons = find_all(&nodes, Role::Button, &tr(key));
            assert_eq!(
                buttons.len(),
                2,
                "dock {width}: both {key} buttons: {nodes:?}"
            );
            for b in buttons {
                let r = b.bounds.expect("button bounds");
                assert!(
                    r.min.x >= splitter.min.x && r.max.x <= window_w,
                    "dock {width}: {key} at {r:?} escapes the dock (splitter {splitter:?})"
                );
            }
        }
        width += 16.0;
    }
}

/// D1/FR-013 (manual walk M8 regression, 2026-09-25): with the dock
/// docked at its widest, no Now Playing control left of the dock — the
/// transport row's switches in particular, which `horizontal_wrapped`
/// never wraps by itself — runs across the dock's edge at any window
/// width from the 1024 threshold up.
#[test]
fn transport_row_never_crosses_docked_edge() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("d1-row-vs-dock");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    controller.set_dock_width(DOCK_WIDTH_MAX);

    let mut window_w = 1024.0;
    while window_w <= 1400.0 {
        render_now_playing(&ctx, &mut controller, window_w, 800.0);
        let (_, nodes) = render_now_playing(&ctx, &mut controller, window_w, 800.0);
        let edge = find_one(&nodes, Role::Splitter, &tr("plugin-dock-resize"))
            .bounds
            .expect("splitter bounds")
            .center()
            .x;
        for n in nodes
            .iter()
            .filter(|n| matches!(n.role, Role::Button | Role::CheckBox | Role::Switch))
        {
            let Some(r) = n.bounds else { continue };
            if r.min.x < edge {
                assert!(
                    r.max.x <= edge,
                    "window {window_w}: {:?} at {r:?} crosses the dock edge {edge}",
                    n.accessible_name()
                );
            }
        }
        window_w += 16.0;
    }
}

/// D10.8/SC-007: under a +40% pseudo-localization expansion and a
/// 60-character second-panel title, neither (a) a docked window at
/// 1100×800 (dock at 240) nor (b) a 960×640 window with the overlay open
/// ever elides a button/toggle label (only the title itself may truncate,
/// D7) or lets two interactive widgets' rects actually overlap.
#[test]
fn pseudo_localization_no_elision_or_overlap() {
    let (mut controller, _dir, _psd, _tsd) = launch_ui_panel("d10-8-pseudo");
    let id = fixture_id(&mut controller, UI_PANEL);
    let long_title = "A".repeat(60);
    register_second_panel(&mut controller, id, &long_title);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    with_pseudo_expansion(40, || {
        // `tr` itself is padded while this closure runs (T006), so the
        // control labels compared against below must be resolved here
        // too, not hoisted above `with_pseudo_expansion`.
        let control_labels = [
            tr("plugin-panel-float"),
            tr("plugin-panel-dock"),
            tr("plugin-panel-close"),
            tr("plugin-panel-disable"),
            tr("plugin-dock-panels-toggle"),
        ];

        // (a) ~1100×800, docked at 240.
        let (nodes, elided) =
            run_now_playing_frame_with_elisions(&ctx, &mut controller, 1100.0, 800.0);
        for text in &elided {
            assert!(
                !control_labels.contains(text),
                "a control label elided at 1100×800 under +40% pseudo-localization: {text:?}"
            );
        }
        assert_no_overlapping_interactive_rects(&nodes, "1100x800 docked");

        // (b) 960×640, overlay open.
        plugin_panels::set_overlay_open(&ctx, true);
        let (nodes, elided) =
            run_now_playing_frame_with_elisions(&ctx, &mut controller, 960.0, 640.0);
        for text in &elided {
            assert!(
                !control_labels.contains(text),
                "a control label elided at 960x640 overlay under +40% pseudo-localization: {text:?}"
            );
        }
        assert_no_overlapping_interactive_rects(&nodes, "960x640 overlay");
    });
}

// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T097 (US4, 011-plugin-ui-contributions, contracts/overlays-settings-
//! notify.md §2 "S2"-"S4"/"S6"): `settings::plugins::show` end to end,
//! driven headlessly through the real `org.modplayer.fixture.ui-settings`
//! fixture (T105) — the settings-page list names the plugin (S2), every
//! field kind exposes the right AccessKit role and a meaningful
//! accessible name (S3), a `boolean` field applies immediately on change
//! while a `number` field applies only once its edit ends (S4), and
//! `settings_registry::search_plugin_settings` finds a field by its own
//! label and carries a path that `plugins::show`'s own `focus` parameter
//! actually opens and focuses (S6). Mirrors `plugin_panels.rs`'s own
//! fixture harness and AccessKit-node render technique (this crate) and
//! `controller_plugin_ui.rs`'s own settings fixture harness (core).
//!
//! The whole Settings shell's own search box + category list
//! (`settings/mod.rs::show`) needs a live `AccountService` (secure store,
//! auth service, clock) to drive at all — orthogonal to this file's own
//! scope, the Plugins category screen itself. `search_finds_field_with_
//! path` instead proves the two halves `settings/mod.rs`'s search-hit
//! handler actually chains: the pure search function, and `plugins::
//! show`'s own `(plugin, field_id)` focus hand-off.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use egui::accesskit::{NodeId, Role, Toggled};
use egui::{Context, Event, Pos2, RawInput, Rect};
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::plugins::Lifecycle;
use modplayer_core::settings::SettingsStore;
use modplayer_core::settings_registry::search_plugin_settings;
use modplayer_core::{PlaybackController, PluginId};
use modplayer_ui::settings::plugins::{self, PluginsScreen};

/// 014-design-tokens-and-type-scale (US2, T032/T033): a bare
/// `Context::default()` has none of the token `Style`'s
/// `Name("section")` text style installed, which the Plugins settings
/// sub-page heading now reaches — panicking on layout otherwise. Install
/// it once, exactly as `App::new`/`App::update` do (mirrors `controls.rs`
/// test's identically-named helper).
fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx
}

const UI_SETTINGS: &str = "org.modplayer.fixture.ui-settings";
const PLUGIN_NAME: &str = "UI settings fixture";

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-settings-plugins-{label}-{}-{unique}",
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
/// every other's brief mutation of them (mirrors `plugin_panels.rs`'s own
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

/// A controller with the `ui-settings` fixture spawned, `Active`, and its
/// settings page already registered (waits for the fixture's own
/// asynchronous `ready_ack` handler).
fn launch_ui_settings(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
    PluginId,
) {
    let (mut controller, dir, psd, tsd) = fixture_controller(label);
    let id = fixture_id(&mut controller, UI_SETTINGS);
    let shared = Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| is_active(
            c, id
        )),
        "the ui-settings fixture must reach Active on its own"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            c.plugin_settings_views().iter().any(|v| v.plugin == id)
        }),
        "the fixture's ready_ack handler must have registered its settings page"
    );
    (controller, dir, psd, tsd, id)
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 1200.0))),
        ..Default::default()
    }
}

/// One AccessKit node's accessibility-relevant fields, plus its own id
/// (needed to check `update.focus`) and its raw bounds (a slider/
/// checkbox's own rect, to drive a synthetic pointer event at).
#[derive(Debug, Clone)]
struct AccessNode {
    id: NodeId,
    role: Role,
    label: Option<String>,
    value: Option<String>,
    toggled: Option<Toggled>,
    labelled_by_something: bool,
    bounds: Option<Rect>,
}

impl AccessNode {
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

/// Runs one frame of `render`, returning every AccessKit node plus which
/// one (if any) holds focus.
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
            toggled: node.toggled(),
            labelled_by_something: !node.labelled_by().is_empty(),
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

fn render_page(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    screen: &mut PluginsScreen,
    focus: Option<(PluginId, &str)>,
) -> Vec<AccessNode> {
    run_frame(ctx, default_input(), |ui| {
        plugins::show(ui, controller, screen, focus);
    })
    .1
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

/// S2: the plugin's settings page is listed by its own name on the
/// Plugins category screen before it is ever opened.
#[test]
fn page_listed_by_name() {
    let (mut controller, _dir, _psd, _tsd, _id) = launch_ui_settings("page-listed");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = PluginsScreen::new();

    let nodes = render_page(&ctx, &mut controller, &mut screen, None);
    find_one(&nodes, Role::Button, PLUGIN_NAME);
}

/// S3: every field kind renders with the right AccessKit role and a
/// meaningful accessible name — `boolean`/`number` directly from their
/// own label (`Checkbox`/`Slider` both set it as their own `WidgetInfo`
/// label), `string`/`choice` via `labelled_by` (`egui`'s own `TextEdit`/
/// `ComboBox` name themselves after their live text/selection instead).
#[test]
fn field_roles_and_names() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_settings("field-roles");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = PluginsScreen::new();

    let nodes = render_page(&ctx, &mut controller, &mut screen, Some((id, "snap")));

    let snap = find_one(&nodes, Role::CheckBox, "Snap to beat");
    assert_eq!(
        snap.toggled,
        Some(Toggled::False),
        "the fixture's own default"
    );

    find_one(&nodes, Role::Slider, "Semitone shift");

    let note = nodes
        .iter()
        .find(|n| n.role == Role::TextInput)
        .unwrap_or_else(|| {
            unreachable!("the `note` string field must render a TextInput: {nodes:?}")
        });
    assert!(
        note.labelled_by_something,
        "the string field's TextInput must be labelled by its own label text: {note:?}"
    );

    let mode = nodes
        .iter()
        .find(|n| n.role == Role::ComboBox)
        .unwrap_or_else(|| {
            unreachable!("the `mode` choice field must render a ComboBox: {nodes:?}")
        });
    assert!(
        mode.labelled_by_something,
        "the choice field's ComboBox must be labelled by its own label text: {mode:?}"
    );
}

/// S4: a `boolean` field applies to the controller immediately on
/// change — clicking the checkbox is enough, no further commit needed.
#[test]
fn boolean_applies_on_change() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_settings("boolean-change");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = PluginsScreen::new();

    let nodes = render_page(&ctx, &mut controller, &mut screen, Some((id, "snap")));
    let rect = find_one(&nodes, Role::CheckBox, "Snap to beat")
        .bounds
        .expect("the Snap to beat checkbox must have bounds");

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    run_frame(&ctx, press, |ui| {
        plugins::show(ui, &mut controller, &mut screen, None);
    });
    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    run_frame(&ctx, release, |ui| {
        plugins::show(ui, &mut controller, &mut screen, None);
    });

    assert_eq!(
        controller
            .plugin_settings_views()
            .iter()
            .find(|v| v.plugin == id)
            .and_then(|v| v.page.values.get("snap"))
            .cloned(),
        Some(serde_json::json!(true)),
        "clicking the checkbox must apply immediately (S4)"
    );
}

/// S4: a `number` field's edit only reaches the controller once the
/// pointer drag ends (`drag_stopped`) — an in-progress drag must not
/// itself apply anything (a `Slider` drag never claims keyboard focus in
/// `egui`, so `lost_focus` alone would never fire for it, hence
/// `show_number`'s own `drag_stopped` half of "on commit").
#[test]
fn number_applies_on_commit() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_settings("number-commit");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = PluginsScreen::new();

    let nodes = render_page(&ctx, &mut controller, &mut screen, Some((id, "shift")));
    let rect = find_one(&nodes, Role::Slider, "Semitone shift")
        .bounds
        .expect("the Semitone shift slider must have bounds");

    let start = rect.left_center() + egui::vec2(2.0, 0.0);
    let end = rect.right_center() - egui::vec2(2.0, 0.0);

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    run_frame(&ctx, press, |ui| {
        plugins::show(ui, &mut controller, &mut screen, None);
    });

    let mut mv = default_input();
    mv.events.push(Event::PointerMoved(end));
    run_frame(&ctx, mv, |ui| {
        plugins::show(ui, &mut controller, &mut screen, None);
    });

    assert_eq!(
        controller
            .plugin_settings_views()
            .iter()
            .find(|v| v.plugin == id)
            .and_then(|v| v.page.values.get("shift"))
            .and_then(serde_json::Value::as_f64),
        Some(0.0),
        "an in-progress drag must not itself apply the edit"
    );

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    run_frame(&ctx, release, |ui| {
        plugins::show(ui, &mut controller, &mut screen, None);
    });

    let applied = controller
        .plugin_settings_views()
        .iter()
        .find(|v| v.plugin == id)
        .and_then(|v| v.page.values.get("shift"))
        .and_then(serde_json::Value::as_f64);
    assert!(
        applied.is_some_and(|v| v > 0.0),
        "drag-release must commit the shift field's new value, got {applied:?}"
    );
}

/// S6: `search_plugin_settings` finds this fixture's field by its own
/// label and carries the "Plugins › <plugin> › <label>" path — and that
/// hit's `(plugin, field_id)` actually opens and focuses the right field
/// in `plugins::show`, the hand-off `settings/mod.rs`'s own search-hit
/// handler performs.
#[test]
fn search_finds_field_with_path() {
    let (mut controller, _dir, _psd, _tsd, id) = launch_ui_settings("search-finds");

    let views = controller.plugin_settings_views();
    let hits = search_plugin_settings("semitone", &views);
    assert_eq!(hits.len(), 1, "expected exactly one hit, got {hits:?}");
    let hit = &hits[0];
    assert_eq!(hit.plugin, id);
    assert_eq!(hit.field_id, "shift");
    assert_eq!(hit.path, "Plugins › UI settings fixture › Semitone shift");

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = PluginsScreen::new();
    let focus = Some((hit.plugin, hit.field_id.as_str()));

    // Frame 1: opens the page and requests focus onto "shift".
    run_frame(&ctx, default_input(), |ui| {
        plugins::show(ui, &mut controller, &mut screen, focus);
    });
    // Frame 2: the requested focus is now reflected in `update.focus`
    // (mirrors `playback.rs`'s own `nudge_step_drag_value_commits`).
    let (focused, nodes) = run_frame(&ctx, default_input(), |ui| {
        plugins::show(ui, &mut controller, &mut screen, None);
    });

    let shift = find_one(&nodes, Role::Slider, "Semitone shift");
    assert_eq!(
        focused,
        Some(shift.id),
        "the search hit must focus the `shift` field's own slider: {nodes:?}"
    );
}

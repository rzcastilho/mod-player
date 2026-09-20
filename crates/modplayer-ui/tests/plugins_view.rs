// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T109 (US4, 009-plugin-runtime-and-permissions, contracts/ui-plugins.md
//! §5): the Plugins section's column list, one-action enable/disable
//! toggle, invalid-manifest row, live-gauge dashes while not `Active`, the
//! absence of any uninstall control, and the suspension notification's
//! Restart/Disable action buttons.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use egui::accesskit::{Role, Toggled};
use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::event::{HostEvent, PlayState};
use modplayer_core::notifications::{NotificationAction, NotificationCenter, Severity};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{Lifecycle, PlaybackController, PluginHost, PluginId, tr, tr_args};
use modplayer_ui::{notifications, plugins_view};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-plugins-view-{label}-{}-{unique}",
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
/// every other's brief mutation of them (mirrors
/// `controller_plugins_lifecycle.rs`'s own `PLUGIN_ENV_LOCK`).
static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

/// A controller that has discovered every `plugins/fixtures/` package —
/// **not `launch()`ed**, so a caller can spawn just the one fixture it
/// needs (`PluginHost::spawn`) rather than every fixture at once.
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

/// A controller with no fixtures discovered (the empty-state case: only
/// `plugins/bundled/`, empty this slice).
///
/// `PlaybackController::new`'s one synchronous read of
/// `MODPLAYER_PLUGIN_FIXTURES` still must be serialized against every
/// other test's brief mutation of it via `PLUGIN_ENV_LOCK` — this
/// controller never sets the var itself, but another test in this binary
/// concurrently setting it inside its own locked section would otherwise
/// be visible here for the moment before it is unset, discovering
/// fixtures that should not exist for this test (T109/`empty_state_
/// without_fixtures`'s own asserted case).
fn plain_controller(label: &str) -> (PlaybackController<FakeBackend, ScriptedHost>, TempDir) {
    let (store, dir) = fresh_store(label);
    let controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store)
    };
    (controller, dir)
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

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 2000.0))),
        ..Default::default()
    }
}

/// One AccessKit node's accessibility-relevant fields (mirrors
/// `tests/accessibility.rs`'s own `AccessNode`, plus `bounds` for the
/// click tests below).
#[derive(Debug, Clone)]
struct AccessNode {
    role: Role,
    label: Option<String>,
    value: Option<String>,
    toggled: Option<Toggled>,
    disabled: bool,
    bounds: Option<Rect>,
}

impl AccessNode {
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

fn render_nodes_on(ctx: &Context, mut render: impl FnMut(&mut egui::Ui)) -> Vec<AccessNode> {
    let mut output = ctx.run_ui(default_input(), |ui| render(ui));
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    update
        .nodes
        .iter()
        .map(|(_, node)| AccessNode {
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            toggled: node.toggled(),
            disabled: node.is_disabled(),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
        })
        .collect()
}

fn render_plugins(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> Vec<AccessNode> {
    render_nodes_on(ctx, |ui| plugins_view::show(ui, controller))
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

/// Press then release the primary button at `pos`, in two separate frames
/// (mirrors `tests/effects_view.rs`'s own `click_at`).
fn click_at(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    pos: Pos2,
) {
    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| plugins_view::show(ui, controller));
    output.drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| plugins_view::show(ui, controller));
    output.drop_without_applying_deltas();
}

// -----------------------------------------------------------------------

/// 012-section-loop-plugin (research R5), extended by
/// 013-key-and-tempo-plugin: `plugins/bundled/` now always ships both
/// Section Loop and Key & Tempo, so a controller discovered with no
/// fixtures shows their two rows (and enable checkboxes), not the empty
/// state.
#[test]
fn section_loop_row_without_fixtures() {
    let (mut controller, _dir) = plain_controller("section-loop-row");
    let ctx = Context::default();
    ctx.enable_accesskit();

    let nodes = render_plugins(&ctx, &mut controller);

    assert!(
        nodes
            .iter()
            .all(|n| n.accessible_name() != Some(&tr("plugins-empty"))),
        "the empty-state label must not render once the bundled plugins are discovered: {nodes:?}"
    );
    let checkbox_count = nodes.iter().filter(|n| n.role == Role::CheckBox).count();
    assert_eq!(
        checkbox_count, 2,
        "exactly two plugin rows (Section Loop, Key & Tempo) must render with no fixtures discovered: {nodes:?}"
    );
}

/// 012-section-loop-plugin (research R5): the 009 FR-023 zero-row empty
/// state is still implemented even though the real `bundled::packages()`
/// can no longer produce it — exercised here by replacing the
/// controller's `PluginHost` with one built from an explicit empty
/// package list.
#[test]
fn empty_state_with_explicit_empty_host() {
    let (mut controller, _dir) = plain_controller("empty-state");
    *controller.plugins_mut() = PluginHost::discover_packages(Vec::new());
    let ctx = Context::default();
    ctx.enable_accesskit();

    let nodes = render_plugins(&ctx, &mut controller);

    find_one(&nodes, Role::Label, &tr("plugins-empty"));
    assert!(
        nodes.iter().all(|n| n.role != Role::CheckBox),
        "no plugin row (and so no enable checkbox) may render with an explicit empty package list: {nodes:?}"
    );
}

#[test]
fn rows_show_every_column_sorted_by_name() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("rows-sorted");
    let ctx = Context::default();
    ctx.enable_accesskit();

    let nodes = render_plugins(&ctx, &mut controller);

    // Column headers (contracts/ui-plugins.md §2).
    for key in [
        "plugins-col-name",
        "plugins-col-version",
        "plugins-col-source",
        "plugins-col-enabled",
        "plugins-col-health",
        "plugins-col-permissions",
        "plugins-col-cpu",
        "plugins-col-memory",
    ] {
        find_one(&nodes, Role::Label, &tr(key));
    }

    // Every one of the 17 fixtures (010-transport-focus adds `focus-a`/
    // `focus-b`; 011-plugin-ui-contributions US1 adds `ui-panel` (T060),
    // US2 adds `ui-shortcuts` (T079), US3 adds `ui-overlay` and
    // `ui-icons` (T092), US4 adds `ui-settings` (T106), US5 adds
    // `ui-notify` (T115); 013-key-and-tempo-plugin's Foundational phase
    // adds `effects-observer` (T005/T011's `effect_chain_changed`
    // regression fixture)) plus the two bundled packages,
    // 012-section-loop-plugin's "Section Loop" and
    // 013-key-and-tempo-plugin's own "Key & Tempo" (both always
    // discovered, `plugins/bundled/`'s two real packages), sorted
    // case-insensitively by name — already alphabetical for this fixture
    // set. The invalid fixture's manifest never parses into a `Manifest`
    // (only a `ManifestError`), so its row falls back to its raw
    // identifier (`plugins/view.rs::row`, mirrors `plugins/
    // host.rs::record_sort_key`'s own fallback) rather than the `name`
    // its TOML never validated into.
    let expected_order = [
        "Effects observer fixture",
        "Flood fixture",
        "Focus fixture A",
        "Focus fixture B",
        "Hang fixture",
        "Key & Tempo",
        "Leak fixture",
        "Never-ready fixture",
        "Observer fixture",
        "org.modplayer.fixture.invalid",
        "Section Loop",
        "Throw fixture",
        "UI Icons fixture",
        "UI notify fixture",
        "UI Overlay fixture",
        "UI Panel fixture",
        "UI settings fixture",
        "UI Shortcuts fixture",
        "Well-behaved fixture",
    ];
    let mut ys: Vec<(f32, &str)> = expected_order
        .iter()
        .map(|name| {
            let node = find_one(&nodes, Role::Label, name);
            (
                node.bounds.expect("name label must have bounds").min.y,
                *name,
            )
        })
        .collect();
    ys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let actual_order: Vec<&str> = ys.iter().map(|(_, name)| *name).collect();
    assert_eq!(
        actual_order, expected_order,
        "rows must be sorted by name, case-insensitively"
    );

    // Source column (only `Bundled` is reachable this slice).
    assert!(
        !find_all(&nodes, Role::Label, &tr("plugins-source-bundled")).is_empty(),
        "expected at least one `bundled` source label: {nodes:?}"
    );

    // Permissions column, catalog order, single- and multi-permission
    // cases (contracts/ui-plugins.md §2). Four fixtures (hang, throw,
    // leak, observer) hold only `playback.observe`.
    assert_eq!(
        find_all(&nodes, Role::Label, &tr("permission-playback-observe")).len(),
        4,
        "expected 4 single-permission rows reading just `playback.observe`'s explanation: {nodes:?}"
    );
    // Flood and focus-b (010-transport-focus) hold the identical two-
    // permission pair, so this explanation string renders twice.
    let flood_permissions = format!(
        "{}{}{}",
        tr("permission-playback-observe"),
        tr("plugins-list-separator"),
        tr("permission-transport-control")
    );
    assert_eq!(
        find_all(&nodes, Role::Label, &flood_permissions).len(),
        2,
        "expected flood and focus-b to share the same two-permission explanation: {nodes:?}"
    );

    // Every non-invalid fixture, plus both bundled packages, is
    // `Disabled` pre-launch, so its health is `Ok` and its CPU/memory are
    // both the dash (not yet `Active`).
    assert_eq!(
        find_all(&nodes, Role::Label, &tr("plugins-health-ok")).len(),
        18,
        "16 valid fixtures plus Section Loop and Key & Tempo must all read `ok` before any is launched: {nodes:?}"
    );
}

#[test]
fn invalid_row_shows_reason_and_inert_toggle() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("invalid-row");
    let ctx = Context::default();
    ctx.enable_accesskit();

    let nodes = render_plugins(&ctx, &mut controller);

    let reason = "The required permission 'teleport.everywhere' is not in the permission catalog.";
    find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "plugins-invalid-manifest",
            &[("reason", reason.to_string())],
        ),
    );

    // The invalid fixture's manifest never parses into a `Manifest`, so
    // its row's name (and so its toggle's `$plugin`) falls back to its
    // raw identifier (see `rows_show_every_column_sorted_by_name`'s own
    // note on this).
    let toggle_name = tr_args(
        "plugins-enable-toggle",
        &[("plugin", "org.modplayer.fixture.invalid".to_string())],
    );
    let toggle = find_one(&nodes, Role::CheckBox, &toggle_name);
    assert!(
        toggle.disabled,
        "the invalid fixture's toggle must be inert: {toggle:?}"
    );
    assert_eq!(
        toggle.toggled,
        Some(Toggled::False),
        "the invalid fixture is never enabled: {toggle:?}"
    );
}

#[test]
fn no_uninstall_control() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("no-uninstall");
    let ctx = Context::default();
    ctx.enable_accesskit();

    let nodes = render_plugins(&ctx, &mut controller);

    assert!(
        nodes.iter().all(|n| !n
            .accessible_name()
            .is_some_and(|name| name.to_lowercase().contains("uninstall"))),
        "no widget anywhere in the Plugins section may mention uninstall: {nodes:?}"
    );
}

#[test]
fn toggle_disables_in_one_action() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("toggle-disable");
    let id = fixture_id(&mut controller, "org.modplayer.fixture.observer");
    let shared = Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| is_active(
            c, id
        )),
        "the observer fixture must reach Active on its own (ready_ack needs no permission)"
    );

    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_plugins(&ctx, &mut controller);
    let toggle_name = tr_args(
        "plugins-enable-toggle",
        &[("plugin", "Observer fixture".to_string())],
    );
    let toggle = find_one(&nodes, Role::CheckBox, &toggle_name);
    assert_eq!(toggle.toggled, Some(Toggled::True));
    let center = toggle.bounds.expect("toggle must have bounds").center();

    // A single click — no confirmation dialog anywhere (FR-024) — calls
    // `plugin_disable` immediately.
    click_at(&ctx, &mut controller, center);

    assert!(
        !controller
            .plugins_mut()
            .record(id)
            .unwrap_or_else(|| unreachable!())
            .enabled,
        "one click must disable the plugin with no further action"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Draining | Lifecycle::Disabled)
        )),
        "the one-action disable must actually run the plugin's teardown"
    );
}

#[test]
fn suspended_row_shows_dash_gauges() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("suspended-dash");
    let id = fixture_id(&mut controller, "org.modplayer.fixture.hang");
    // A 1 ms share (well under the default 100 ms) suspends the hang
    // fixture the first time its handler ever runs (mirrors
    // `controller_plugins_lifecycle.rs`'s own `restart_keeps_session_
    // counter`), rather than needing three aborts.
    if let Some(record) = controller.plugins_mut().record_mut(id) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    let shared = Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    assert!(pump_until(&mut controller, Duration::from_secs(2), |c| {
        is_active(c, id)
    }));

    let now = controller.now();
    controller.plugins_mut().fan_out(
        &HostEvent::PlayStateChanged {
            state: PlayState::Playing,
        },
        now,
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Suspended { .. })
        )),
        "the hang fixture must suspend under a 1ms share"
    );

    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_plugins(&ctx, &mut controller);

    // Only the hang fixture ever spawned this test, so it is the one
    // `Suspended` row; every other fixture stayed `Disabled` — meaning no
    // row anywhere is `Active`, so no CPU/memory cell may show a real
    // number.
    assert_eq!(
        find_all(&nodes, Role::Label, &tr("plugins-health-suspended")).len(),
        1,
        "exactly the hang fixture must read `suspended`: {nodes:?}"
    );
    assert!(
        nodes.iter().all(|n| !n
            .accessible_name()
            .is_some_and(|name| name.contains('%') || name.contains("MB /"))),
        "no row may show a real CPU/memory figure while nothing is Active: {nodes:?}"
    );
}

/// T043 (US1, 011-plugin-ui-contributions, contracts/ui-panels.md L6):
/// once the `ui-panel` fixture's "Controls" panel registers, the Plugins
/// list grows a per-panel control line — the panel's own title, a
/// session-only Show/Hide button and a persisted Enable/Disable button —
/// and both buttons round-trip through the controller exactly as
/// `plugin_panels_view()`/`plugin_panel_close`/`show`/`set_disabled`
/// (T051) promise: Hide removes the panel from the dock without
/// persisting anything, Show restores it, and Disable removes it from the
/// dock and survives being read back (persisted).
#[test]
fn panel_controls_listed() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("panel-controls-listed");
    let id = fixture_id(&mut controller, "org.modplayer.fixture.ui-panel");
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

    let ctx = Context::default();
    ctx.enable_accesskit();

    // Baseline: title label, `Hide` (not yet closed) and `Disable` (not
    // yet persisted-disabled) — contracts/ui-panels.md L6.
    let nodes = render_plugins(&ctx, &mut controller);
    find_one(&nodes, Role::Label, "Controls");
    let hide = find_one(&nodes, Role::Button, &tr("plugin-panel-hide"));
    let hide_pos = hide.bounds.expect("Hide button must have bounds").center();

    // Click Hide: session-only, calls `plugin_panel_close`, no event to
    // the plugin, and the panel drops out of the dock immediately.
    click_at(&ctx, &mut controller, hide_pos);
    assert!(
        controller.plugin_panels_view().docked.is_empty(),
        "a closed panel must not render in the dock"
    );
    let nodes = render_plugins(&ctx, &mut controller);
    let show = find_one(&nodes, Role::Button, &tr("plugin-panel-show"));
    let show_pos = show.bounds.expect("Show button must have bounds").center();

    // Click Show: reopens it.
    click_at(&ctx, &mut controller, show_pos);
    assert!(
        !controller.plugin_panels_view().docked.is_empty(),
        "Show must restore the panel to the dock"
    );
    let nodes = render_plugins(&ctx, &mut controller);
    let disable = find_one(&nodes, Role::Button, &tr("plugin-panel-disable"));
    let disable_pos = disable
        .bounds
        .expect("Disable button must have bounds")
        .center();

    // Click Disable: persisted (`[plugin_panels]`), also drops it from the
    // dock; the Plugins-list control line flips to `Enable`.
    click_at(&ctx, &mut controller, disable_pos);
    assert!(
        controller.plugin_panels_view().docked.is_empty(),
        "a disabled panel must not render in the dock"
    );
    let nodes = render_plugins(&ctx, &mut controller);
    find_one(&nodes, Role::Button, &tr("plugin-panel-enable"));
}

#[test]
fn notification_actions_call_facade() {
    let plugin_id = PluginId(0);
    let mut center = NotificationCenter::new();
    let notification_id = center.raise_keyed(
        Severity::Warning,
        "plugin-suspended",
        vec![
            ("plugin", "Hang fixture".to_string()),
            ("cause", "it stopped responding".to_string()),
        ],
        vec![
            NotificationAction::RestartPlugin(plugin_id),
            NotificationAction::DisablePlugin(plugin_id),
        ],
        "plugin-suspended:org.modplayer.fixture.hang",
    );

    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut interaction = notifications::NotificationInteraction::default();
    let nodes = render_nodes_on(&ctx, |ui| {
        interaction = notifications::show(ui, &center);
    });
    let restart = find_one(
        &nodes,
        Role::Button,
        &tr("notification-action-restart-plugin"),
    );
    let restart_pos = restart.bounds.expect("button must have bounds").center();

    click_notification_action(&ctx, &center, &mut interaction, restart_pos);
    assert_eq!(
        interaction.action_clicked,
        Some((
            notification_id,
            NotificationAction::RestartPlugin(plugin_id)
        )),
        "clicking Restart must report the plugin's id via the RestartPlugin action"
    );

    let nodes = render_nodes_on(&ctx, |ui| {
        interaction = notifications::show(ui, &center);
    });
    let disable = find_one(
        &nodes,
        Role::Button,
        &tr("notification-action-disable-plugin"),
    );
    let disable_pos = disable.bounds.expect("button must have bounds").center();
    click_notification_action(&ctx, &center, &mut interaction, disable_pos);
    assert_eq!(
        interaction.action_clicked,
        Some((
            notification_id,
            NotificationAction::DisablePlugin(plugin_id)
        )),
        "clicking Disable must report the plugin's id via the DisablePlugin action"
    );
}

/// Press then release the primary button at `pos` over `notifications::
/// show`, capturing the release frame's `NotificationInteraction` into
/// `interaction` (mirrors this file's own `click_at`, generalized to a
/// non-unit render closure the way `app.rs` itself drives `notifications::
/// show`).
fn click_notification_action(
    ctx: &Context,
    center: &NotificationCenter,
    interaction: &mut notifications::NotificationInteraction,
    pos: Pos2,
) {
    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        let _ = notifications::show(ui, center);
    });
    output.drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| {
        *interaction = notifications::show(ui, center);
    });
    output.drop_without_applying_deltas();
}

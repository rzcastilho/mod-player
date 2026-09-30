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
    let mut state = plugins_view::PluginsViewState::default();
    let mut memory = modplayer_ui::section_memory::SectionMemory::default();
    render_nodes_on(ctx, |ui| {
        plugins_view::show(ui, controller, &mut state, &mut memory);
    })
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
    let mut state = plugins_view::PluginsViewState::default();
    click_at_with_state(ctx, controller, &mut state, pos);
}

/// [`click_at`] against a caller-owned [`plugins_view::PluginsViewState`],
/// so disclosure clicks persist for the assertions that follow.
fn click_at_with_state(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    state: &mut plugins_view::PluginsViewState,
    pos: Pos2,
) {
    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    let mut memory = modplayer_ui::section_memory::SectionMemory::default();
    let output = ctx.run_ui(press, |ui| {
        plugins_view::show(ui, controller, state, &mut memory);
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
        plugins_view::show(ui, controller, state, &mut memory);
    });
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
        "plugins-col-source",
        "plugins-col-enabled",
        "plugins-col-health",
        "plugins-col-permissions",
        "plugins-col-resource",
        "plugins-col-actions",
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
            // The Name cell is one label: name + two spaces + version (T7).
            let matches: Vec<_> = nodes
                .iter()
                .filter(|n| {
                    n.role == Role::Label
                        && n.accessible_name()
                            .is_some_and(|t| t == *name || t.starts_with(&format!("{name}  ")))
                })
                .collect();
            assert_eq!(matches.len(), 1, "one name cell for `{name}`");
            let node = matches[0];
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

    // Permissions column: the count now (the explanations move into the
    // disclosure, US2 — asserted by `permissions_*` there).

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
    let plugin_name = ui_panel_row_name(&mut controller);
    let hide = find_one(
        &nodes,
        Role::Button,
        &panel_a11y("plugins-panel-hide-a11y", "Controls", &plugin_name),
    );
    let hide_pos = hide.bounds.expect("Hide button must have bounds").center();

    // Click Hide: session-only, calls `plugin_panel_close`, no event to
    // the plugin, and the panel drops out of the dock immediately.
    click_at(&ctx, &mut controller, hide_pos);
    assert!(
        controller.plugin_panels_view().docked.is_empty(),
        "a closed panel must not render in the dock"
    );
    let nodes = render_plugins(&ctx, &mut controller);
    let show = find_one(
        &nodes,
        Role::Button,
        &panel_a11y("plugins-panel-show-a11y", "Controls", &plugin_name),
    );
    let show_pos = show.bounds.expect("Show button must have bounds").center();

    // Click Show: reopens it.
    click_at(&ctx, &mut controller, show_pos);
    assert!(
        !controller.plugin_panels_view().docked.is_empty(),
        "Show must restore the panel to the dock"
    );
    let nodes = render_plugins(&ctx, &mut controller);
    let disable = find_one(
        &nodes,
        Role::Button,
        &panel_a11y("plugins-panel-disable-a11y", "Controls", &plugin_name),
    );
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
    find_one(
        &nodes,
        Role::Button,
        &panel_a11y("plugins-panel-enable-a11y", "Controls", &plugin_name),
    );
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
    let mut stack_state = notifications::StackState::default();
    let nodes = render_nodes_on(&ctx, |ui| {
        interaction = notifications::show(ui, &center, &mut stack_state);
    });
    let restart = find_one(
        &nodes,
        Role::Button,
        &tr("notification-action-restart-plugin"),
    );
    let restart_pos = restart.bounds.expect("button must have bounds").center();

    click_notification_action(
        &ctx,
        &center,
        &mut interaction,
        &mut stack_state,
        restart_pos,
    );
    assert_eq!(
        interaction.action_clicked,
        Some((
            notification_id,
            NotificationAction::RestartPlugin(plugin_id)
        )),
        "clicking Restart must report the plugin's id via the RestartPlugin action"
    );

    let nodes = render_nodes_on(&ctx, |ui| {
        interaction = notifications::show(ui, &center, &mut stack_state);
    });
    let disable = find_one(
        &nodes,
        Role::Button,
        &tr("notification-action-disable-plugin"),
    );
    let disable_pos = disable.bounds.expect("button must have bounds").center();
    click_notification_action(
        &ctx,
        &center,
        &mut interaction,
        &mut stack_state,
        disable_pos,
    );
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
    stack_state: &mut notifications::StackState,
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
        let _ = notifications::show(ui, center, stack_state);
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
        *interaction = notifications::show(ui, center, stack_state);
    });
    output.drop_without_applying_deltas();
}

// -----------------------------------------------------------------------
// 027-plugins-list-as-table, US1 (contracts/plugins-table.md T1–T7, T16, T19)
// -----------------------------------------------------------------------

/// Render the Plugins view in a root ui exactly `width` points wide (one
/// frame) and return the AccessKit nodes plus the root ui's `max_rect`.
fn render_plugins_at_width(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    width: f32,
    hover: Option<Pos2>,
) -> (Vec<AccessNode>, Rect) {
    let mut state = plugins_view::PluginsViewState::default();
    let mut memory = modplayer_ui::section_memory::SectionMemory::default();
    let mut input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 2000.0))),
        ..Default::default()
    };
    if let Some(pos) = hover {
        input.events.push(Event::PointerMoved(pos));
    }
    let mut available = Rect::NOTHING;
    let mut output = ctx.run_ui(input, |ui| {
        available = ui.max_rect();
        plugins_view::show(ui, controller, &mut state, &mut memory);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    let nodes = update
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
        .collect();
    (nodes, available)
}

/// Every leaf control/label of the table (not the scroll container).
fn leaf_nodes(nodes: &[AccessNode]) -> Vec<&AccessNode> {
    nodes
        .iter()
        .filter(|n| matches!(n.role, Role::Label | Role::CheckBox | Role::Button))
        .filter(|n| n.bounds.is_some())
        .collect()
}

fn assert_table_fits(width: f32, label: &str) {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller(label);
    let ctx = Context::default();
    ctx.enable_accesskit();
    let (nodes, available) = render_plugins_at_width(&ctx, &mut controller, width, None);

    let leaves = leaf_nodes(&nodes);
    assert!(
        leaves.len() > 19 * 4,
        "19 rows of cells must render: {}",
        leaves.len()
    );
    for node in &leaves {
        let b = node.bounds.expect("filtered");
        assert!(
            b.min.x >= available.min.x - 0.5 && b.max.x <= available.max.x + 0.5,
            "`{:?}` ({b:?}) overflows the available rect {available:?} at {width} pt",
            node.accessible_name()
        );
    }
    for (i, a) in leaves.iter().enumerate() {
        for b in &leaves[i + 1..] {
            let (ra, rb) = (
                a.bounds.expect("filtered").shrink(0.5),
                b.bounds.expect("filtered").shrink(0.5),
            );
            assert!(
                !ra.intersects(rb),
                "`{:?}` {ra:?} overlaps `{:?}` {rb:?} at {width} pt",
                a.accessible_name(),
                b.accessible_name()
            );
        }
    }
}

/// T3–T5, SC-001/SC-002: all 17 fixtures (+ 2 bundled) at the minimum
/// window width, no cell overflows or overlaps another.
#[test]
fn seventeen_fixtures_fit_at_min_window_width() {
    assert_table_fits(960.0, "fit-960");
}

/// T20, FR-011: with every string padded by 40 % (`apply_pseudo_expansion`)
/// the table still fits at the minimum window width; truncation absorbs the
/// growth instead of overflowing or overlapping.
#[test]
fn pseudo_expansion_fits_at_min_window_width() {
    modplayer_core::i18n::with_pseudo_expansion(40, || {
        assert_table_fits(960.0, "fit-960-pseudo");
    });
}

/// T3–T5, SC-001/SC-002: the same at the section's content floor.
#[test]
fn seventeen_fixtures_fit_at_section_floor() {
    assert_table_fits(560.0, "fit-560");
}

fn header_x(nodes: &[AccessNode], key: &str) -> f32 {
    find_one(nodes, Role::Label, &tr(key))
        .bounds
        .expect("header label must have bounds")
        .min
        .x
}

/// T2, T3: seven headers in order, and each column's cells start at the
/// header's x.
#[test]
fn header_aligns_with_columns() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("header-align");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let (nodes, _) = render_plugins_at_width(&ctx, &mut controller, 960.0, None);

    let keys = [
        "plugins-col-name",
        "plugins-col-source",
        "plugins-col-enabled",
        "plugins-col-health",
        "plugins-col-permissions",
        "plugins-col-resource",
        "plugins-col-actions",
    ];
    let xs: Vec<f32> = keys.iter().map(|k| header_x(&nodes, k)).collect();
    assert!(
        xs.windows(2).all(|w| w[0] < w[1]),
        "headers must run left to right in order: {xs:?}"
    );
    // The retired Version / CPU / Memory headers (their keys are gone).
    for removed in ["Version", "CPU", "Memory"] {
        assert!(
            find_all(&nodes, Role::Label, removed).is_empty(),
            "`{removed}` header was removed"
        );
    }

    let near = |a: f32, b: f32, what: &str| {
        assert!((a - b).abs() <= 0.5, "{what}: cell x {a} vs header x {b}");
    };
    let name = nodes
        .iter()
        .find(|n| {
            n.role == Role::Label
                && n.accessible_name()
                    .is_some_and(|t| t.starts_with("Well-behaved fixture"))
        })
        .expect("Well-behaved fixture name cell");
    near(name.bounds.expect("bounds").min.x, xs[0], "name");
    for source in find_all(&nodes, Role::Label, &tr("plugins-source-bundled")) {
        near(source.bounds.expect("bounds").min.x, xs[1], "source");
    }
    let checkboxes: Vec<_> = nodes.iter().filter(|n| n.role == Role::CheckBox).collect();
    assert_eq!(checkboxes.len(), 19);
    for checkbox in checkboxes {
        near(checkbox.bounds.expect("bounds").min.x, xs[2], "enabled");
    }
    for health in find_all(&nodes, Role::Label, &tr("plugins-health-ok")) {
        // The word follows the decorative dot inside the Health extent.
        let x = health.bounds.expect("bounds").min.x;
        assert!(
            x >= xs[3] && x < xs[4],
            "health word x {x} outside its column"
        );
    }
    for dash in find_all(&nodes, Role::Label, &tr("plugins-dash")) {
        near(dash.bounds.expect("bounds").min.x, xs[5], "resource");
    }
}

/// T6, T7: a name too long for its column truncates, its accessible name
/// stays the full text (name + version), and hovering shows the full text.
#[test]
fn long_name_truncates_with_tooltip_and_full_accessible_name() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("long-name");
    let row = controller
        .plugins_view()
        .rows
        .into_iter()
        .find(|r| r.name == "Effects observer fixture")
        .expect("effects-observer fixture row");
    let full = format!("{}  {}", row.name, row.version);
    let ctx = Context::default();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|s| {
        s.interaction.tooltip_delay = 0.0;
        s.interaction.show_tooltips_only_when_still = false;
    });

    let (nodes, _) = render_plugins_at_width(&ctx, &mut controller, 560.0, None);
    let name = find_one(&nodes, Role::Label, &full);
    let bounds = name.bounds.expect("bounds");
    let source_x = header_x(&nodes, "plugins-col-source");
    assert!(
        bounds.max.x <= source_x,
        "truncated name {bounds:?} must end before the Source column at {source_x}"
    );

    // Hover it: the full text appears exactly once more, as the one
    // tooltip (egui's own elided-label tooltip is off — the 027 manual
    // walk, M3, saw it stacked under ours).
    let pos = bounds.center();
    let _ = render_plugins_at_width(&ctx, &mut controller, 560.0, Some(pos));
    let (nodes, _) = render_plugins_at_width(&ctx, &mut controller, 560.0, Some(pos));
    assert_eq!(
        find_all(&nodes, Role::Label, &full).len(),
        2,
        "hovering a truncated name must show the full text as one tooltip"
    );

    // A short name does not get a tooltip.
    let (nodes, _) = render_plugins_at_width(&ctx, &mut controller, 960.0, None);
    let short = nodes
        .iter()
        .find(|n| {
            n.role == Role::Label
                && n.accessible_name()
                    .is_some_and(|t| t.starts_with("Flood fixture"))
        })
        .expect("Flood fixture name cell");
    let pos = short.bounds.expect("bounds").center();
    let _ = render_plugins_at_width(&ctx, &mut controller, 960.0, Some(pos));
    let (nodes, _) = render_plugins_at_width(&ctx, &mut controller, 960.0, Some(pos));
    let short_full = short.accessible_name().expect("name").to_string();
    assert_eq!(
        find_all(&nodes, Role::Label, &short_full).len(),
        1,
        "an untruncated name must not grow a tooltip"
    );
}

/// T16: the invalid row's reason is one label spanning Health..Resource,
/// with no per-column cells of its own there.
#[test]
fn invalid_row_span() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("invalid-span");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let (nodes, _) = render_plugins_at_width(&ctx, &mut controller, 960.0, None);

    let reason = "The required permission 'teleport.everywhere' is not in the permission catalog.";
    let label = find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "plugins-invalid-manifest",
            &[("reason", reason.to_string())],
        ),
    );
    let b = label.bounds.expect("bounds");
    let health_x = header_x(&nodes, "plugins-col-health");
    let actions_x = header_x(&nodes, "plugins-col-actions");
    assert!((b.min.x - health_x).abs() <= 0.5, "starts at Health");
    assert!(b.max.x <= actions_x, "ends before Actions");
}

/// T1: no rows → only the heading and the empty label; no header labels,
/// no scroll area.
#[test]
fn empty_state_has_no_header_or_scroll_area() {
    let (mut controller, _dir) = plain_controller("empty-no-header");
    *controller.plugins_mut() = PluginHost::discover_packages(Vec::new());
    let ctx = Context::default();
    ctx.enable_accesskit();
    let (nodes, _) = render_plugins_at_width(&ctx, &mut controller, 960.0, None);

    find_one(&nodes, Role::Label, &tr("plugins-empty"));
    for key in [
        "plugins-col-name",
        "plugins-col-source",
        "plugins-col-enabled",
        "plugins-col-health",
        "plugins-col-permissions",
        "plugins-col-resource",
        "plugins-col-actions",
    ] {
        assert!(find_all(&nodes, Role::Label, &tr(key)).is_empty(), "{key}");
    }
    assert!(
        nodes.iter().all(|n| n.role != Role::ScrollView),
        "the empty state must not create a scroll area"
    );
}

/// T19, FR-009, SC-004: `plugins_view.rs` holds no colour literal; every
/// colour comes from a theme role.
#[test]
fn plugins_view_has_no_colour_literals() {
    let source = include_str!("../src/plugins_view.rs");
    let code: String = source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .filter(|line| !line.trim_start().starts_with("use "))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(!code.contains("Color32::from_"), "Color32::from_* literal");
    assert!(!code.contains("rgb("), "rgb( literal");
    assert!(!code.contains("hex_color!"), "hex_color! literal");
    let mut rest = code.as_str();
    while let Some(at) = rest.find("Color32::") {
        rest = &rest[at + "Color32::".len()..];
        let ident: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        assert!(
            ident.is_empty()
                || ident == "TRANSPARENT"
                || !ident.chars().next().is_some_and(char::is_uppercase),
            "named colour constant `Color32::{ident}`"
        );
    }

    use modplayer_core::Health;
    use modplayer_ui::theme::tokens::{DARK, DARK_HIGH_CONTRAST, LIGHT, LIGHT_HIGH_CONTRAST};
    for roles in [&LIGHT, &DARK, &LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
        assert_eq!(
            plugins_view::health_color(roles, Health::Ok),
            roles.positive
        );
        assert_eq!(
            plugins_view::health_color(roles, Health::Warning),
            roles.warning
        );
        assert_eq!(
            plugins_view::health_color(roles, Health::Suspended),
            roles.danger
        );
    }
}

// -----------------------------------------------------------------------
// US2 (027-plugins-list-as-table): permissions as count + disclosure.

fn render_with_state(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    state: &mut plugins_view::PluginsViewState,
) -> Vec<AccessNode> {
    let mut memory = modplayer_ui::section_memory::SectionMemory::default();
    render_nodes_on(ctx, |ui| {
        plugins_view::show(ui, controller, state, &mut memory);
    })
}

fn disclosure_name(key: &str, count: usize, plugin: &str) -> String {
    tr_args(
        key,
        &[("count", count.to_string()), ("plugin", plugin.to_string())],
    )
}

/// T10: a plugin with permissions shows `"{count} ▸"` as a button named by
/// `plugins-permissions-show`; clicking it flips to `"{count} ▾"` /
/// `plugins-permissions-hide`, and clicking again collapses it.
#[test]
fn permissions_count_and_disclosure() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("perm-disclosure");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut state = plugins_view::PluginsViewState::default();

    let nodes = render_with_state(&ctx, &mut controller, &mut state);
    let show = disclosure_name("plugins-permissions-show", 2, "Flood fixture");
    let button = find_one(&nodes, Role::Button, &show);
    let bounds = button.bounds.expect("disclosure button has bounds");

    click_at_with_state(&ctx, &mut controller, &mut state, bounds.center());
    let id = fixture_id(&mut controller, "org.modplayer.fixture.flood");
    assert!(state.permissions_open.contains(&id), "click must expand");

    let nodes = render_with_state(&ctx, &mut controller, &mut state);
    let hide = disclosure_name("plugins-permissions-hide", 2, "Flood fixture");
    let bounds = find_one(&nodes, Role::Button, &hide)
        .bounds
        .expect("hide button has bounds");

    click_at_with_state(&ctx, &mut controller, &mut state, bounds.center());
    assert!(!state.permissions_open.contains(&id), "click must collapse");
}

/// T14, SC-005: an expanded row lists one explanation per permission, in
/// catalog order, each fully present, below the cells line and inside the
/// window.
#[test]
fn permissions_expand_one_per_line() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("perm-expand");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut state = plugins_view::PluginsViewState::default();
    let id = fixture_id(&mut controller, "org.modplayer.fixture.flood");

    let closed = render_with_state(&ctx, &mut controller, &mut state);
    assert!(
        find_all(&closed, Role::Label, &tr("permission-transport-control")).is_empty(),
        "collapsed rows show no explanations"
    );

    state.permissions_open.insert(id);
    let nodes = render_with_state(&ctx, &mut controller, &mut state);
    let observe = find_one(&nodes, Role::Label, &tr("permission-playback-observe"));
    let transport = find_one(&nodes, Role::Label, &tr("permission-transport-control"));
    let (observe, transport) = (
        observe.bounds.expect("bounds"),
        transport.bounds.expect("bounds"),
    );
    assert!(
        observe.min.y < transport.min.y,
        "catalog order: playback.observe before transport.control"
    );
    assert!(
        observe.max.y <= transport.min.y + 0.5,
        "one explanation per line, no overlap"
    );
    for rect in [observe, transport] {
        assert!(
            rect.max.x <= 1200.0,
            "explanation must stay inside the window: {rect:?}"
        );
    }
    // Inside the row: below the row's own name cell.
    let name = nodes
        .iter()
        .find(|n| {
            n.role == Role::Label
                && n.accessible_name()
                    .is_some_and(|t| t.starts_with("Flood fixture  "))
        })
        .and_then(|n| n.bounds)
        .expect("flood name cell");
    assert!(observe.min.y >= name.max.y - 0.5, "below the cells line");
}

/// A plugin with no permissions shows the label `0` and no disclosure.
#[test]
fn zero_permissions_no_disclosure() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("perm-zero");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let rows = controller.plugins_view().rows;
    let zero = rows
        .iter()
        .find(|r| r.invalid_reason.is_none() && r.permissions.is_empty());
    let Some(zero) = zero else {
        // No permission-less valid plugin discovered: nothing to assert.
        return;
    };
    let nodes = render_plugins(&ctx, &mut controller);
    assert!(
        find_all(
            &nodes,
            Role::Button,
            &disclosure_name("plugins-permissions-show", 0, &zero.name)
        )
        .is_empty(),
        "0 permissions must not render a disclosure"
    );
    assert!(!find_all(&nodes, Role::Label, "0").is_empty());
}

/// T18: expansion state is keyed by `PluginId`; it survives repaints and a
/// health/lifecycle change, and is pruned for vanished ids.
#[test]
fn expansion_survives_repaint_and_health_change() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("perm-survive");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut state = plugins_view::PluginsViewState::default();
    let id = fixture_id(&mut controller, "org.modplayer.fixture.flood");
    state.permissions_open.insert(id);

    for _ in 0..3 {
        render_with_state(&ctx, &mut controller, &mut state);
    }
    assert!(state.permissions_open.contains(&id), "survives repaint");

    controller.plugin_enable(id);
    controller.tick();
    render_with_state(&ctx, &mut controller, &mut state);
    assert!(
        state.permissions_open.contains(&id),
        "survives a lifecycle/health change"
    );

    state.prune(&[]);
    assert!(state.permissions_open.is_empty(), "vanished ids are pruned");
}

// -- US3: health words, suspension reason and Restart (027, T9/T13) -------

/// Spawn the hang fixture under a 1 ms share and drive it to `Suspended`.
fn suspend_hang_fixture(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> PluginId {
    let id = fixture_id(controller, "org.modplayer.fixture.hang");
    if let Some(record) = controller.plugins_mut().record_mut(id) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    let shared = Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    assert!(pump_until(controller, Duration::from_secs(2), |c| {
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
        pump_until(controller, Duration::from_secs(2), |c| matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Suspended { .. })
        )),
        "the hang fixture must suspend under a 1ms share"
    );
    id
}

const CAUSE_KEYS: [&str; 4] = [
    "plugin-suspended-cause-hang",
    "plugin-suspended-cause-cpu-share",
    "plugin-suspended-cause-memory",
    "plugin-suspended-cause-did-not-start",
];

#[test]
fn health_words_healthy_degraded_suspended() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("health-words");
    let ctx = Context::default();
    ctx.enable_accesskit();

    // Nothing active yet: every valid row reads "healthy", none degraded
    // or suspended.
    let nodes = render_plugins(&ctx, &mut controller);
    assert_eq!(tr("plugins-health-ok"), "healthy");
    assert_eq!(tr("plugins-health-warning"), "degraded");
    assert_eq!(tr("plugins-health-suspended"), "suspended");
    assert!(!find_all(&nodes, Role::Label, "healthy").is_empty());
    assert!(find_all(&nodes, Role::Label, "suspended").is_empty());
    assert!(find_all(&nodes, Role::Label, "degraded").is_empty());

    suspend_hang_fixture(&mut controller);
    let nodes = render_plugins(&ctx, &mut controller);
    assert_eq!(find_all(&nodes, Role::Label, "suspended").len(), 1);
}

/// T9: the Health dot is painted, never a `●` text glyph — the app's
/// fonts have no `●`, so the 027 manual walk (M1) saw a tofu box in
/// every Health cell. With the app's own fonts installed, every valid row
/// gets a filled circle in its health colour and no text shape holds `●`.
#[test]
fn health_dot_is_painted_not_a_font_glyph() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("health-dot");
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    let mut state = plugins_view::PluginsViewState::default();
    let mut memory = modplayer_ui::section_memory::SectionMemory::default();
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 2000.0))),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        plugins_view::show(ui, &mut controller, &mut state, &mut memory);
    });
    let shapes = std::mem::take(&mut output.shapes);
    output.drop_without_applying_deltas();
    let positive = modplayer_ui::theme::roles(&ctx.global_style().visuals).positive;
    let healthy_dots = shapes
        .iter()
        .filter(|c| matches!(&c.shape, egui::Shape::Circle(circle) if circle.fill == positive))
        .count();
    let healthy_rows = controller
        .plugins_view()
        .rows
        .iter()
        .filter(|r| r.health == Some(modplayer_core::Health::Ok))
        .count();
    assert!(healthy_rows > 0);
    assert_eq!(
        healthy_dots, healthy_rows,
        "one painted dot per healthy row"
    );
    let glyph_text = shapes.iter().any(|c| match &c.shape {
        egui::Shape::Text(text) => text.galley.job.text.contains('●'),
        _ => false,
    });
    assert!(!glyph_text, "`●` has no glyph in the app fonts (tofu)");
}

#[test]
fn suspended_row_shows_reason_and_restart_restarts() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("suspended-restart");
    let ctx = Context::default();
    ctx.enable_accesskit();

    // No Restart anywhere while nothing is suspended.
    let nodes = render_plugins(&ctx, &mut controller);
    assert!(
        nodes
            .iter()
            .all(|n| n.role != Role::Button || n.accessible_name() != Some("Restart")),
        "no row may offer Restart before any suspension: {nodes:?}"
    );

    let id = suspend_hang_fixture(&mut controller);
    let nodes = render_plugins(&ctx, &mut controller);

    // The reason sits in the row, one of the four cause sentences.
    let reasons: Vec<String> = CAUSE_KEYS.iter().map(|k| tr(k)).collect();
    let reason_nodes = nodes
        .iter()
        .filter(|n| {
            n.role == Role::Label
                && n.accessible_name()
                    .is_some_and(|name| reasons.iter().any(|r| name == r))
        })
        .count();
    assert_eq!(reason_nodes, 1, "one suspension reason expected: {nodes:?}");

    // Exactly one Restart, named for the plugin, and only for that row.
    let name = controller
        .plugins_view()
        .rows
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.name.clone())
        .expect("hang row");
    let restart_name = tr_args("plugins-restart-a11y", &[("plugin", name)]);
    let restart = find_one(&nodes, Role::Button, &restart_name);
    let pos = restart.bounds.expect("Restart must have bounds").center();

    click_at(&ctx, &mut controller, pos);
    assert!(
        !matches!(
            controller.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Suspended { .. })
        ),
        "clicking Restart must call plugin_restart and leave Suspended"
    );
}

// -----------------------------------------------------------------------
// US4 (027-plugins-list-as-table): resource use against budgets and
// row-owned panel controls.

/// Fluent wraps placeables in bidi isolates; strip them to compare.
fn plain(text: &str) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn panel_a11y(key: &str, title: &str, plugin: &str) -> String {
    tr_args(
        key,
        &[("title", title.to_string()), ("plugin", plugin.to_string())],
    )
}

fn ui_panel_row_name(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) -> String {
    let id = fixture_id(controller, "org.modplayer.fixture.ui-panel");
    controller
        .plugins_view()
        .rows
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.name.clone())
        .expect("ui-panel row")
}

/// Spawn the `ui-panel` fixture and wait until its "Controls" panel is
/// registered.
fn spawn_ui_panel(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) -> PluginId {
    let id = fixture_id(controller, "org.modplayer.fixture.ui-panel");
    let shared = Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    assert!(pump_until(controller, Duration::from_secs(2), |c| {
        is_active(c, id)
    }));
    assert!(pump_until(controller, Duration::from_secs(2), |c| {
        !c.plugin_panels_view().docked.is_empty()
    }));
    id
}

/// T11: an Active row shows `CPU {used} % / {budget} %` and
/// `Mem {used} MB / {budget} MB` from its own budgets; every non-Active row
/// shows the dash forms.
#[test]
fn resource_cell_shows_budgets_and_dashes() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("resource-cell");
    spawn_ui_panel(&mut controller);
    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_plugins(&ctx, &mut controller);

    let cpu: Vec<&AccessNode> = nodes
        .iter()
        .filter(|n| {
            n.role == Role::Label
                && n.accessible_name()
                    .map(plain)
                    .is_some_and(|t| t.starts_with("CPU ") && t.contains("% / 10 %"))
        })
        .collect();
    assert_eq!(
        cpu.len(),
        1,
        "one Active row shows CPU vs budget: {nodes:?}"
    );
    let memory: Vec<&AccessNode> = nodes
        .iter()
        .filter(|n| {
            n.role == Role::Label
                && n.accessible_name()
                    .map(plain)
                    .is_some_and(|t| t.starts_with("Mem ") && t.contains("MB / 64 MB"))
        })
        .collect();
    assert_eq!(
        memory.len(),
        1,
        "one Active row shows Mem vs budget: {nodes:?}"
    );

    let rows = controller.plugins_view().rows.len();
    let cpu_none = find_all(&nodes, Role::Label, &tr("plugins-resource-cpu-none")).len();
    let mem_none = find_all(&nodes, Role::Label, &tr("plugins-resource-memory-none")).len();
    // Invalid rows have no resource cell; none exist among the fixtures'
    // healthy rows except the invalid-manifest fixture.
    assert!(
        cpu_none >= rows - 3,
        "non-Active rows show the CPU dash: {nodes:?}"
    );
    assert_eq!(cpu_none, mem_none);
}

/// T13, T15: a plugin with exactly one panel gets Show/Hide and
/// Enable/Disable buttons inline in its row, each named by the a11y key
/// carrying panel title and plugin name, and no disclosure.
#[test]
fn single_panel_controls_inline_in_row() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("single-panel-inline");
    spawn_ui_panel(&mut controller);
    let plugin = ui_panel_row_name(&mut controller);
    let ctx = Context::default();
    ctx.enable_accesskit();
    let nodes = render_plugins(&ctx, &mut controller);

    let hide = find_one(
        &nodes,
        Role::Button,
        &panel_a11y("plugins-panel-hide-a11y", "Controls", &plugin),
    );
    let disable = find_one(
        &nodes,
        Role::Button,
        &panel_a11y("plugins-panel-disable-a11y", "Controls", &plugin),
    );
    // Both sit inside the window and on the row that owns the plugin.
    let window = Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 2000.0));
    for node in [hide, disable] {
        let bounds = node.bounds.expect("panel button has bounds");
        assert!(window.contains_rect(bounds), "{bounds:?}");
    }
    let count_name = tr_args("plugins-panels-count", &[("count", "1".to_string())]);
    assert!(
        find_all(&nodes, Role::Button, &count_name).is_empty(),
        "one panel needs no disclosure"
    );
}

/// T13, T14: two panels collapse behind a `Panels (2)` disclosure; opening
/// it lists one line per panel with its own buttons.
#[test]
fn multi_panel_disclosure() {
    use modplayer_capability_gateway::ui::{UiId, WidgetKind, WidgetSpec};

    let (mut controller, _dir, _psd, _tsd) = fixture_controller("multi-panel");
    let id = spawn_ui_panel(&mut controller);
    let second = WidgetSpec {
        id: UiId::parse("info").expect("valid id"),
        kind: WidgetKind::Label,
        label: "Label".to_string(),
        min: None,
        max: None,
        step: None,
        value: None,
        items: Vec::new(),
        selected: None,
        action: None,
        text: Some("hello".to_string()),
    };
    controller
        .plugins_mut()
        .ui_mut()
        .panels_mut()
        .register(
            id,
            UiId::parse("second").expect("valid id"),
            "Second".to_string(),
            vec![second],
        )
        .expect("second panel registers");

    let plugin = ui_panel_row_name(&mut controller);
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut state = plugins_view::PluginsViewState::default();

    let nodes = render_with_state(&ctx, &mut controller, &mut state);
    // Collapsed: no per-panel buttons, one disclosure.
    assert!(
        find_all(
            &nodes,
            Role::Button,
            &panel_a11y("plugins-panel-hide-a11y", "Controls", &plugin)
        )
        .is_empty(),
        "collapsed: no per-panel buttons"
    );
    let show = tr_args("plugins-panels-show", &[("plugin", plugin.clone())]);
    let bounds = find_one(&nodes, Role::Button, &show)
        .bounds
        .expect("disclosure bounds");
    click_at_with_state(&ctx, &mut controller, &mut state, bounds.center());
    assert!(state.panels_open.contains(&id), "click must expand panels");

    let nodes = render_with_state(&ctx, &mut controller, &mut state);
    for title in ["Controls", "Second"] {
        find_one(
            &nodes,
            Role::Button,
            &panel_a11y("plugins-panel-hide-a11y", title, &plugin),
        );
        find_one(
            &nodes,
            Role::Button,
            &panel_a11y("plugins-panel-disable-a11y", title, &plugin),
        );
    }
    let hide = tr_args("plugins-panels-hide", &[("plugin", plugin)]);
    let bounds = find_one(&nodes, Role::Button, &hide)
        .bounds
        .expect("hide disclosure bounds");
    click_at_with_state(&ctx, &mut controller, &mut state, bounds.center());
    assert!(
        !state.panels_open.contains(&id),
        "click must collapse panels"
    );
}

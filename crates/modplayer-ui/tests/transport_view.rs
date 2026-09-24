// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T048 (US3, 010-transport-focus, contracts/ui-transport-panel.md): the
//! Transport panel — row filtering (only `transport.control`-granted,
//! Loading/Active plugins), the holder/requesting badges, "Give focus"/
//! "Take back" click-through, the policy `ComboBox`'s selection sticking
//! across renders, the empty state, and a suspended holder's row/holder
//! label following its own teardown. Drives the real `plugins/fixtures/`
//! packages on a full `PlaybackController<FakeBackend, ScriptedHost>`
//! (`MODPLAYER_PLUGIN_FIXTURES=1`), exactly like `controller_transport_
//! focus.rs`'s own US1/US2 harness (crates/modplayer-core/tests/).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::event::{HostEvent, PlayState};
use modplayer_core::plugins::{Lifecycle, PluginId};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{FocusHolder, FocusPolicy, PlaybackController, tr, tr_args};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::transport_view;

// -----------------------------------------------------------------------
// Harness (mirrors controller_transport_focus.rs's own pattern, adapted
// for this crate's `render_nodes_on`/`find_one` accessibility idiom —
// effects_view.rs's own tests).
// -----------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-transport-view-{label}-{}-{unique}",
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

fn fake_device() -> FakeDevice {
    FakeDevice {
        id: DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2_048))),
        is_default: true,
    }
}

fn track(id: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!()),
        id,
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

/// `MODPLAYER_PLUGIN_FIXTURES`/`MODPLAYER_PLUGIN_STATE_DIR`/
/// `MODPLAYER_TRACK_STATE_DIR` are process-global, so every construction
/// in this binary is serialized against every other's brief mutation of
/// them (mirrors every `controller_plugins_*.rs`/`accessibility.rs` file's
/// own pattern) — including the one test below that discovers no
/// fixtures at all, since it must still be serialized against a sibling
/// test's brief window with the var set.
static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

/// A controller that has discovered every `plugins/fixtures/` package,
/// **not yet `launch()`ed** — a caller can override a fixture's `Budgets`
/// first. Also hands back the temp dirs so they stay alive for the test's
/// duration.
fn fixture_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store(label);
    let plugin_state_dir = TempDir::new(&format!("{label}-plugin-state"));
    let track_state_dir = TempDir::new(&format!("{label}-track-state"));
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device()];
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
    (controller, handle, dir, plugin_state_dir, track_state_dir)
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
    pump_controller_until(controller, Duration::from_secs(2), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )
    })
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
        ..Default::default()
    }
}

/// 016-list-row-and-panel-components: `transport_view::show` now wraps its
/// content in `panel_card`, whose header renders through `theme::
/// section_label` — the `section` role text style only exists once the
/// token `Style` is installed (mirrors `now_playing.rs`/`markers.rs`'s own
/// identically-named helper; a bare `Context::default()` panics resolving
/// it).
fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx
}

/// One AccessKit node's accessibility-relevant fields (mirrors
/// `tests/accessibility.rs`/`tests/effects_view.rs`'s own `AccessNode`).
#[derive(Debug, Clone)]
struct AccessNode {
    role: Role,
    label: Option<String>,
    value: Option<String>,
    bounds: Option<Rect>,
    disabled: bool,
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
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
            disabled: node.is_disabled(),
        })
        .collect()
}

fn render_panel(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> Vec<AccessNode> {
    render_nodes_on(ctx, |ui| transport_view::show(ui, controller))
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
/// on `ctx` (mirrors `effects_view.rs`/`queue_view.rs`'s own `click_at`).
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
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| transport_view::show(ui, controller));
    output.drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| transport_view::show(ui, controller));
    output.drop_without_applying_deltas();
}

/// `focus-a`/`focus-b`'s own `play_state_changed` trigger (mirrors
/// `controller_transport_focus.rs`'s own `fan_out_play_state`).
fn fan_out_play_state(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    state: PlayState,
) {
    let now = controller.now();
    controller
        .plugins_mut()
        .fan_out(&HostEvent::PlayStateChanged { state }, now);
}

/// (identifier, name) for every fixture whose manifest actually *grants*
/// `transport.control` (not merely mentions it, like `observer`'s own
/// permission-denied justification text — its manifest holds only
/// `playback.observe`, per its `[[permissions.required]]` table).
const EXPECTED_ROWS: [(&str, &str); 4] = [
    ("org.modplayer.fixture.flood", "Flood fixture"),
    ("org.modplayer.fixture.focus-a", "Focus fixture A"),
    ("org.modplayer.fixture.focus-b", "Focus fixture B"),
    ("org.modplayer.fixture.wellbehaved", "Well-behaved fixture"),
];

const EXCLUDED_FIXTURE_NAMES: [&str; 6] = [
    "Hang fixture",
    "Invalid fixture",
    "Leak fixture",
    "Never-ready fixture",
    "Observer fixture",
    "Throw fixture",
];

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

/// Only the fixtures granted `transport.control` — and only once
/// `Loading`/`Active` — appear as rows (contracts/ui-transport-panel.md
/// §1/§2's `from_records_and_arbiter` contract, exercised end to end
/// through the real panel).
#[test]
fn panel_lists_only_transport_control_plugins() {
    let (mut controller, _handle, _dir, _psd, _tsd) =
        fixture_controller("lists-only-transport-control");
    controller.launch();

    for (identifier, _name) in EXPECTED_ROWS {
        let id = controller_plugin_id(&mut controller, identifier);
        assert!(
            wait_active(&mut controller, id),
            "{identifier} must reach Active"
        );
    }

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);

    for (_identifier, name) in EXPECTED_ROWS {
        find_one(&nodes, Role::Label, name);
    }
    for name in EXCLUDED_FIXTURE_NAMES {
        assert!(
            find_all(&nodes, Role::Label, name).is_empty(),
            "{name} must not render as a Transport panel row: {nodes:?}"
        );
    }
}

/// Every pending plugin's row carries a "requesting (n)" badge; the
/// holder's row carries "holds focus" instead — both read straight from
/// `TransportFocusView` (contracts/ui-transport-panel.md §2 rows,
/// "requesting badge"/row `holds`).
#[test]
fn holder_and_requesting_badges_render() {
    let (mut controller, _handle, _dir, _psd, _tsd) =
        fixture_controller("holder-and-requesting-badges");
    controller.launch();
    let id_a = controller_plugin_id(&mut controller, "org.modplayer.fixture.focus-a");
    let id_b = controller_plugin_id(&mut controller, "org.modplayer.fixture.focus-b");
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    assert!(
        wait_active(&mut controller, id_b),
        "focus-b must reach Active"
    );

    let view = controller.transport_focus_view();
    let order_a = view
        .rows
        .iter()
        .find(|r| r.id == id_a)
        .and_then(|r| r.request_order)
        .unwrap_or_else(|| unreachable!("focus-a must already be requesting (RT5)"));
    let order_b = view
        .rows
        .iter()
        .find(|r| r.id == id_b)
        .and_then(|r| r.request_order)
        .unwrap_or_else(|| unreachable!("focus-b must already be requesting (RT5)"));

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);
    find_one(
        &nodes,
        Role::Label,
        &tr_args("transport-requesting", &[("order", order_a.to_string())]),
    );
    find_one(
        &nodes,
        Role::Label,
        &tr_args("transport-requesting", &[("order", order_b.to_string())]),
    );
    assert!(
        find_all(&nodes, Role::Label, &tr("transport-holds")).is_empty(),
        "no row may read 'holds focus' while the host holds it: {nodes:?}"
    );

    controller.focus_give(id_a);
    let nodes = render_panel(&ctx, &mut controller);
    find_one(&nodes, Role::Label, &tr("transport-holds"));

    let order_b_after = controller
        .transport_focus_view()
        .rows
        .iter()
        .find(|r| r.id == id_b)
        .and_then(|r| r.request_order)
        .unwrap_or_else(|| unreachable!("focus-b must still be requesting"));
    find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "transport-requesting",
            &[("order", order_b_after.to_string())],
        ),
    );
}

/// Clicking a row's "Give focus" button grants that plugin focus (U1,
/// FR-008).
#[test]
fn give_focus_click_changes_holder() {
    let (mut controller, _handle, _dir, _psd, _tsd) = fixture_controller("give-focus-click");
    controller.launch();
    let id_a = controller_plugin_id(&mut controller, "org.modplayer.fixture.focus-a");
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);
    let give = find_one(
        &nodes,
        Role::Button,
        &tr_args(
            "transport-give-focus",
            &[("plugin", "Focus fixture A".to_string())],
        ),
    );
    assert!(
        !give.disabled,
        "Give focus must be enabled while not yet the holder: {give:?}"
    );
    let bounds = give.bounds.expect("give-focus button bounds");
    click_at(&ctx, &mut controller, bounds.center());

    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id_a),
        "clicking Give focus must grant that plugin focus"
    );
}

/// Clicking "Take back" revokes the current plugin holder back to the
/// host (U1, FR-008).
#[test]
fn take_back_click_returns_host() {
    let (mut controller, _handle, _dir, _psd, _tsd) = fixture_controller("take-back-click");
    controller.launch();
    let id_a = controller_plugin_id(&mut controller, "org.modplayer.fixture.focus-a");
    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    controller.focus_give(id_a);
    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Plugin(id_a),
        "test setup: focus_give must grant focus-a"
    );

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);
    let take_back = find_one(&nodes, Role::Button, &tr("transport-take-back"));
    assert!(
        !take_back.disabled,
        "Take back must be enabled once a plugin holds focus: {take_back:?}"
    );
    let bounds = take_back.bounds.expect("take-back button bounds");
    click_at(&ctx, &mut controller, bounds.center());

    assert_eq!(
        controller.plugins_mut().arbiter().holder(),
        FocusHolder::Host,
        "clicking Take back must revoke to the host"
    );
}

/// Changing the Focus policy `ComboBox` sticks across renders — never
/// reverting to a stale value on the next frame (contracts/
/// ui-transport-panel.md §2 "policy", U1).
#[test]
fn policy_combo_persists_selection() {
    let (mut controller, _handle, _dir, _psd, _tsd) = fixture_controller("policy-combo-persists");
    controller.launch();

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);
    find_one(
        &nodes,
        Role::ComboBox,
        &tr(FocusPolicy::AutoOnInteraction.label_key()),
    );

    controller.set_focus_policy(FocusPolicy::Manual);
    let nodes = render_panel(&ctx, &mut controller);
    find_one(&nodes, Role::ComboBox, &tr(FocusPolicy::Manual.label_key()));
    assert!(
        find_all(
            &nodes,
            Role::ComboBox,
            &tr(FocusPolicy::AutoOnInteraction.label_key())
        )
        .is_empty(),
        "the stale Auto-on-interaction selection must not linger: {nodes:?}"
    );

    controller.set_focus_policy(FocusPolicy::FirstRequestWins);
    let nodes = render_panel(&ctx, &mut controller);
    find_one(
        &nodes,
        Role::ComboBox,
        &tr(FocusPolicy::FirstRequestWins.label_key()),
    );
    assert_eq!(controller.focus_policy(), FocusPolicy::FirstRequestWins);
}

/// With no fixture discovered and Section Loop — the only bundled
/// package, and (012) the only one that ever requests
/// `transport.control` here — disabled, the panel shows the empty state
/// and the holder label reads "host" (contracts/ui-transport-panel.md §2
/// "empty state"). Section Loop discovers and goes `Active` regardless of
/// fixtures (research R5), so with it left enabled the panel always lists
/// its row; disabling it is the only way to reach this state now.
#[test]
fn empty_state_when_no_eligible_plugin() {
    let (store, _dir) = fresh_store("empty-state");
    let mut controller = {
        // Serialized against every fixture-loading construction above too
        // (see `PLUGIN_ENV_LOCK`'s own doc comment) — this one just never
        // sets the var itself.
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store)
    };
    controller.launch();

    let section_loop = controller_plugin_id(&mut controller, "org.modplayer.section-loop");
    assert!(
        wait_active(&mut controller, section_loop),
        "Section Loop must reach Active before it can be disabled"
    );
    controller.plugin_disable(section_loop);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);
    find_one(&nodes, Role::Label, &tr("transport-empty"));
    find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "transport-holder",
            &[("holder", tr("transport-holder-host"))],
        ),
    );
}

/// A holder that gets suspended (the scripted hang's own watchdog
/// teardown) disappears from the panel's rows on the next frame, and the
/// holder label reads "host" — same frame the controller's `tick` drained
/// the event (FR-007, SC-003, U3).
#[test]
fn suspended_holder_row_disappears_and_holder_reads_host() {
    let (mut controller, _handle, _dir, _psd, _tsd) =
        fixture_controller("suspended-holder-row-disappears");
    let id_a = controller_plugin_id(&mut controller, "org.modplayer.fixture.focus-a");
    // A tiny `share` budget makes the scripted hang's own suspension
    // deterministic and fast (mirrors `controller_transport_focus.rs`'s
    // own `suspended_holder_returns_to_host_and_disarms_loop`).
    if let Some(record) = controller.plugins_mut().record_mut(id_a) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    assert!(
        wait_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    controller.focus_give(id_a);
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            c.plugins_mut().arbiter().holder() == FocusHolder::Plugin(id_a)
        }),
        "focus_give must grant focus-a"
    );

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let before = render_panel(&ctx, &mut controller);
    find_one(&before, Role::Label, "Focus fixture A");
    find_one(&before, Role::Label, &tr("transport-holds"));

    // focus-a hangs on its *third* play_state_changed (data-model.md §5).
    for _ in 0..3 {
        fan_out_play_state(&mut controller, PlayState::Playing);
    }
    assert!(
        pump_controller_until(&mut controller, Duration::from_secs(2), |c| {
            matches!(
                c.plugins_mut().record(id_a).map(|r| &r.lifecycle),
                Some(Lifecycle::Suspended { .. })
            )
        }),
        "the scripted hang must suspend focus-a"
    );

    let after = render_panel(&ctx, &mut controller);
    assert!(
        find_all(&after, Role::Label, "Focus fixture A").is_empty(),
        "the suspended plugin's row must leave the panel: {after:?}"
    );
    find_one(
        &after,
        Role::Label,
        &tr_args(
            "transport-holder",
            &[("holder", tr("transport-holder-host"))],
        ),
    );
}

/// C7 (016-list-row-and-panel-components, contracts/panel-card.md, FR-018's
/// stated correction): the panel's header is the shared card's
/// `section`-role (`Role::Heading`) treatment, not the `title` role a bare
/// `ui.heading()` gave it before.
#[test]
fn header_is_section_role_not_title_role() {
    let (mut controller, _handle, _dir, _psd, _tsd) = fixture_controller("header-role");
    controller.launch();

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);

    find_one(&nodes, Role::Heading, &tr("transport-panel-title"));
    assert!(
        find_all(&nodes, Role::Label, &tr("transport-panel-title")).is_empty(),
        "the header must not also expose a plain Label node: {nodes:?}"
    );
}

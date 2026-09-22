// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T059/T061/T074 (US2/US3, 008-effect-chain-and-built-in-nodes,
//! contracts/ui-effect-chain.md): the Effect Chain panel — toggle
//! persistence (`E`/header, survives a track change), row listing in
//! processing order (type/owner/bypass/handle/cost), add-at-capacity
//! inline refusal, pointer drag-and-drop and keyboard (`↑`/`↓`) reorder,
//! bypass/remove, the FR-008 "quality mode auto-switched" note, the
//! SC-005 clamped-value display (Gain, and Phase 5's EQ/Filter), adding
//! one of each of the full six-kind catalog, and stereo tools' phase-
//! invert `add_enabled(mono_sum)` gating.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::Role;
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::plugins::PluginId;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{NodeId, PlaybackController, tr};
use modplayer_effects::catalog::{NodeKind, ParamId};
use modplayer_engine::{BufferPreset, DeviceId, Event as EngineEvent, FrameCount, SampleRate};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::effects_view;
use modplayer_ui::waveform::WaveformState;

/// 014-design-tokens-and-type-scale (US2, T022): a bare `Context::default()`
/// has none of the token `Style`'s `Name("display")` text style installed,
/// which `now_playing::show` (hosting this panel) now reaches — panicking
/// on layout otherwise. Install it once, exactly as `App::new`/
/// `App::update` do (mirrors `controls.rs` test's identically-named
/// helper).
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
            "modplayer-ui-effects-view-{label}-{}-{unique}",
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
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
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

/// A controller over a confirmed device, no track needed — the panel
/// renders regardless (contracts/ui-effect-chain.md §1).
fn ready_controller(label: &str) -> (PlaybackController<FakeBackend, ScriptedHost>, TempDir) {
    let (store, dir) = fresh_store(label);
    let devices = vec![fake_device()];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), ScriptedHost::new(), store);
    // Bundled plugins stay un-launched: `launch()` only spawns records
    // flagged `enabled`, and Key & Tempo (013) would otherwise add its
    // own pitch/stretch nodes to the chain asynchronously, racing every
    // test here that asserts on exactly the nodes it added itself.
    let bundled: Vec<PluginId> = controller
        .plugins_mut()
        .records()
        .iter()
        .map(|r| r.id)
        .collect();
    for id in bundled {
        if let Some(record) = controller.plugins_mut().record_mut(id) {
            record.enabled = false;
        }
    }
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    (controller, dir)
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
        ..Default::default()
    }
}

/// One AccessKit node's accessibility-relevant fields (mirrors
/// `tests/accessibility.rs`'s own `AccessNode`, plus `numeric_value` for
/// pinning a `Slider`'s displayed value, SC-005).
#[derive(Debug, Clone)]
struct AccessNode {
    role: Role,
    label: Option<String>,
    value: Option<String>,
    numeric_value: Option<f64>,
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
            numeric_value: node.numeric_value(),
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
    render_nodes_on(ctx, |ui| effects_view::show(ui, controller))
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

/// The bounds of the `nth` (top-to-bottom) `role` node named `name`
/// (mirrors `queue_view.rs`'s own `nth_button_bounds`) — rows repeat the
/// same control label once per row, so row order disambiguates.
fn nth_bounds(nodes: &[AccessNode], role: Role, name: &str, nth: usize) -> Rect {
    let mut matches: Vec<Rect> = nodes
        .iter()
        .filter(|node| node.role == role && node.accessible_name() == Some(name))
        .filter_map(|node| node.bounds)
        .collect();
    matches.sort_by(|a, b| {
        a.min
            .y
            .partial_cmp(&b.min.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches.get(nth).copied().unwrap_or_else(|| {
        panic!(
            "expected at least {} `{name}` node(s), found {}",
            nth + 1,
            matches.len()
        )
    })
}

/// Press then release the primary button at `pos`, in two separate frames
/// (mirrors `queue_view.rs`'s own `click_at`).
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
    let output = ctx.run_ui(press, |ui| effects_view::show(ui, controller));
    output.drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| effects_view::show(ui, controller));
    output.drop_without_applying_deltas();
}

#[test]
fn e_and_header_toggle_panel_and_it_survives_track_change() {
    let (mut controller, _dir) = ready_controller("toggle-survive");
    controller.queue_replace(vec![track("a"), track("b")]);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let texts = |controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
                 artwork: &mut ArtworkCache,
                 waveform: &mut WaveformState| {
        render_nodes_on(&ctx, |ui| {
            modplayer_ui::now_playing::show(ui, controller, artwork, waveform)
        })
        .iter()
        .filter_map(|node| node.accessible_name().map(str::to_string))
        .collect::<Vec<_>>()
    };

    let initial = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        !initial.contains(&tr("effects-panel-title")),
        "the panel starts closed"
    );

    // The `E` action calls this directly (ui/tests/actions.rs pins the
    // dispatcher wiring itself); here it stands in for a keyboard `E`.
    effects_view::toggle_effect_chain_panel(&ctx);
    let opened = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        opened.contains(&tr("effects-panel-title")),
        "E must open the panel"
    );

    // A track change must not close it.
    controller.skip_forward();
    let after_track_change = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        after_track_change.contains(&tr("effects-panel-title")),
        "the panel must survive a track change"
    );

    // The header toggle button closes it again.
    let nodes = render_nodes_on(&ctx, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    let bounds = find_one(&nodes, Role::Button, &tr("effects-toggle"))
        .bounds
        .expect("effects-toggle bounds");
    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: bounds.center(),
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: bounds.center(),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();

    let closed = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        !closed.contains(&tr("effects-panel-title")),
        "the header toggle button must close the panel again"
    );
}

#[test]
fn rows_list_nodes_in_processing_order_with_type_owner_bypass_handle_cost() {
    let (mut controller, _dir) = ready_controller("rows-order");
    controller.chain_add_node(NodeKind::Gain).expect("add gain");
    controller
        .chain_add_node(NodeKind::PitchShift)
        .expect("add pitch shift");
    controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);

    // AccessKit's node map has no defined iteration order (`update.nodes`
    // is a map, not a paint-ordered list — `tests/accessibility.rs`'s own
    // doc comment notes the same), so "processing order" is checked by
    // each row label's vertical position, not by list position.
    let row_top = |label: String| {
        nodes
            .iter()
            .find(|node| node.accessible_name() == Some(label.as_str()))
            .and_then(|node| node.bounds)
            .unwrap_or_else(|| panic!("row label `{label}` not found"))
            .min
            .y
    };
    let y_gain = row_top(format!("1. {}", tr("effects-kind-gain")));
    let y_pitch = row_top(format!("2. {}", tr("effects-kind-pitch-shift")));
    let y_stretch = row_top(format!("3. {}", tr("effects-kind-time-stretch")));
    assert!(
        y_gain < y_pitch && y_pitch < y_stretch,
        "rows must list top-to-bottom in processing order (gain, pitch shift, time stretch): \
         y = {y_gain}, {y_pitch}, {y_stretch}"
    );

    // AccessKit represents a plain text label as a `Label` node wrapping a
    // child `TextRun` node that carries the same text — counting from
    // `texts` (built from every node, both roles) would double-count, so
    // these two counts are restricted to `Role::Label`.
    assert_eq!(
        find_all(&nodes, Role::Label, &tr("effects-owner-host")).len(),
        3,
        "every row must show its owner"
    );
    assert_eq!(
        find_all(&nodes, Role::Button, &tr("effects-bypass")).len(),
        3,
        "every row must have a bypass control"
    );
    assert_eq!(
        find_all(&nodes, Role::Button, &tr("effects-reorder-handle")).len(),
        3,
        "every row must have a drag handle"
    );
    assert_eq!(
        find_all(&nodes, Role::Label, &tr_args_cpu(0.0)).len(),
        3,
        "every row must show its (idle, zero) CPU cost"
    );
}

fn tr_args_cpu(pct: f32) -> String {
    modplayer_core::tr_args("effects-cpu", &[("pct", format!("{pct:.0}"))])
}

#[test]
fn seventeenth_add_is_refused_inline() {
    let (mut controller, _dir) = ready_controller("full");
    for _ in 0..16 {
        controller.chain_add_node(NodeKind::Gain).expect("add");
    }
    assert!(
        controller.chain_add_node(NodeKind::Gain).is_err(),
        "the model itself must refuse a 17th node"
    );
    assert_eq!(
        controller.chain().nodes().len(),
        16,
        "chain must stay at 16, not corrupt"
    );

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);
    let texts: Vec<String> = nodes
        .iter()
        .filter_map(|node| node.accessible_name().map(str::to_string))
        .collect();
    assert!(
        texts.contains(&tr("effects-chain-full")),
        "the inline refusal label must show at capacity: {texts:?}"
    );
}

#[test]
fn drag_drop_reorders_and_calls_move_to() {
    let (mut controller, _dir) = ready_controller("drag-drop");
    let a = controller.chain_add_node(NodeKind::Gain).expect("add a");
    let b = controller
        .chain_add_node(NodeKind::PitchShift)
        .expect("add b");
    let c = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add c");

    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // Row 0 belongs to `a` — drop `c`'s handle payload onto it.
    let nodes = render_panel(&ctx, &mut controller);
    let target = nth_bounds(&nodes, Role::Button, &tr("effects-remove"), 0);

    egui::DragAndDrop::set_payload(&ctx, c);
    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: target.center(),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| effects_view::show(ui, &mut controller));
    output.drop_without_applying_deltas();

    let order: Vec<NodeId> = controller
        .chain()
        .nodes()
        .iter()
        .map(|node| node.id)
        .collect();
    assert_eq!(
        order,
        vec![c, a, b],
        "dropping c's handle on row 0 must move it to index 0 (chain_move_node)"
    );
}

#[test]
fn arrow_up_on_focused_handle_moves_node_and_keeps_focus() {
    let (mut controller, _dir) = ready_controller("arrow-handle");
    let a = controller.chain_add_node(NodeKind::Gain).expect("add a");
    let b = controller
        .chain_add_node(NodeKind::PitchShift)
        .expect("add b");

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let handle_b = effects_view::handle_id(b);
    ctx.memory_mut(|memory| memory.request_focus(handle_b));

    // One frame with no key event first: the handle's own
    // `set_focus_lock_filter(vertical_arrows: true)` call (which needs
    // `had_focus_last_frame` — true only once this has actually drawn
    // with focus already requested) must land *before* `↑` arrives, or
    // egui's own spatial arrow-key focus navigation would reassign focus
    // to whichever widget sits above `b`'s handle instead of leaving it
    // for our own move-and-keep-focus handling (mirrors real usage: the
    // handle already had focus, drawn, on an earlier frame).
    let output = ctx.run_ui(default_input(), |ui| {
        effects_view::show(ui, &mut controller)
    });
    output.drop_without_applying_deltas();

    let mut input = default_input();
    input.events.push(Event::Key {
        key: Key::ArrowUp,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(input, |ui| {
        effects_view::show(ui, &mut controller);
        effects_view::handle_focused_handle_keys(ui, &mut controller);
    });
    output.drop_without_applying_deltas();

    let order: Vec<NodeId> = controller
        .chain()
        .nodes()
        .iter()
        .map(|node| node.id)
        .collect();
    assert_eq!(
        order,
        vec![b, a],
        "ArrowUp on b's focused handle must move b above a"
    );
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(handle_b),
        "focus must stay on b's handle across the move"
    );
}

#[test]
fn bypass_toggle_updates_state_immediately() {
    let (mut controller, _dir) = ready_controller("bypass");
    let id = controller.chain_add_node(NodeKind::Gain).expect("add");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let nodes = render_panel(&ctx, &mut controller);
    let bounds = find_one(&nodes, Role::Button, &tr("effects-bypass"))
        .bounds
        .expect("bypass bounds");
    click_at(&ctx, &mut controller, bounds.center());

    let node = controller
        .chain()
        .nodes()
        .iter()
        .find(|node| node.id == id)
        .expect("node still present");
    assert!(
        node.bypassed,
        "clicking Bypass must update state immediately"
    );
}

#[test]
fn remove_row_disappears_order_preserved() {
    let (mut controller, _dir) = ready_controller("remove");
    let _a = controller.chain_add_node(NodeKind::Gain).expect("add a");
    let b = controller
        .chain_add_node(NodeKind::PitchShift)
        .expect("add b");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // The topmost "Remove" belongs to row 0 (`a`, added first).
    let nodes = render_panel(&ctx, &mut controller);
    let bounds = nth_bounds(&nodes, Role::Button, &tr("effects-remove"), 0);
    click_at(&ctx, &mut controller, bounds.center());

    let order: Vec<NodeId> = controller
        .chain()
        .nodes()
        .iter()
        .map(|node| node.id)
        .collect();
    assert_eq!(
        order,
        vec![b],
        "removing the first row must leave the second node in place, in order"
    );
}

#[test]
fn quality_note_appears_at_25_percent_and_clears_at_100() {
    let (mut controller, _dir) = ready_controller("mode-note");
    let id = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    controller
        .chain_set_param(id, ParamId(0), 0.25)
        .expect("set 25%");
    let nodes = render_panel(&ctx, &mut controller);
    let texts: Vec<String> = nodes
        .iter()
        .filter_map(|node| node.accessible_name().map(str::to_string))
        .collect();
    assert!(
        texts.contains(&tr("effects-mode-note")),
        "25% tempo is outside stage-use range and must auto-switch to quality: {texts:?}"
    );

    controller
        .chain_set_param(id, ParamId(0), 1.0)
        .expect("set 100%");
    let nodes = render_panel(&ctx, &mut controller);
    let texts: Vec<String> = nodes
        .iter()
        .filter_map(|node| node.accessible_name().map(str::to_string))
        .collect();
    assert!(
        !texts.contains(&tr("effects-mode-note")),
        "100% tempo is back in stage-use range and must auto-revert, clearing the note: {texts:?}"
    );
}

#[test]
fn out_of_range_entry_displays_clamped_value() {
    let (mut controller, _dir) = ready_controller("clamp");
    let id = controller.chain_add_node(NodeKind::Gain).expect("add");
    let clamped = controller
        .chain_set_param(id, ParamId(0), 20.0)
        .expect("set_param");
    assert_eq!(
        clamped, 12.0,
        "SC-005: the model must return the clamped value"
    );

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);
    let slider = nodes
        .iter()
        .find(|node| {
            node.role == Role::Slider
                && node.label.as_deref() == Some(tr("effects-param-level").as_str())
        })
        .expect("gain level slider");
    assert_eq!(
        slider.numeric_value,
        Some(12.0),
        "the slider must display the clamped value, not the raw 20 dB entry"
    );

    // US3 AS4/SC-005: an EQ band frequency past 20 kHz Nyquist-clamps to
    // `0.45 * 44_100 = 19_845` Hz. `DragValue` exposes its current value
    // as accessible *text* (prefix + number + suffix), not a numeric
    // AccessKit value, so the check is a substring match on that text
    // (research: egui's own `DragValue` accessibility code, `builder.
    // set_value(value_text)`).
    let eq_id = controller
        .chain_add_node(NodeKind::Equalizer)
        .expect("add eq");
    let clamped_freq = controller
        .chain_set_param(eq_id, ParamId::eq_band(0, 0), 30_000.0)
        .expect("set band 0 freq");
    assert_eq!(
        clamped_freq, 19_845.0,
        "SC-005: 30 000 Hz clamps to 19 845 Hz at 44.1 kHz"
    );
    let nodes = render_panel(&ctx, &mut controller);
    assert!(
        nodes
            .iter()
            .any(|node| node.value.as_deref().is_some_and(|v| v.contains("19845"))),
        "the EQ frequency control must display the clamped value, not 30 000: {:?}",
        nodes
            .iter()
            .filter_map(|n| n.value.clone())
            .collect::<Vec<_>>()
    );

    // US3 AS4b/SC-005: filter resonance past 1.0 clamps to 1.0 — a
    // `Slider`, so this checks `numeric_value` directly like the gain
    // case above.
    let filter_id = controller
        .chain_add_node(NodeKind::Filter)
        .expect("add filter");
    let clamped_res = controller
        .chain_set_param(filter_id, ParamId(2), 1.5)
        .expect("set resonance");
    assert_eq!(clamped_res, 1.0, "SC-005: resonance 1.5 clamps to 1.0");
    let nodes = render_panel(&ctx, &mut controller);
    let resonance_slider = nodes
        .iter()
        .find(|node| {
            node.role == Role::Slider
                && node.label.as_deref() == Some(tr("effects-param-resonance").as_str())
        })
        .expect("filter resonance slider");
    assert_eq!(
        resonance_slider.numeric_value,
        Some(1.0),
        "the resonance slider must display the clamped value, not the raw 1.5 entry"
    );
}

/// US2 AS1, US4 AS2: adding one node of each of the six built-in kinds
/// appends it at its catalog defaults and (idle, not playing) shows zero
/// cost.
#[test]
fn add_each_kind_appends_at_defaults_and_shows_zero_cost() {
    let (mut controller, _dir) = ready_controller("add-each-kind");
    for kind in NodeKind::ALL {
        controller.chain_add_node(kind).expect("add");
    }
    let nodes = controller.chain().nodes();
    assert_eq!(nodes.len(), 6, "one node per built-in kind");
    for (node, kind) in nodes.iter().zip(NodeKind::ALL) {
        assert_eq!(node.kind, kind);
        let defaults: Vec<f32> = modplayer_effects::catalog::params(kind)
            .iter()
            .map(|p| p.default)
            .collect();
        assert_eq!(
            node.params, defaults,
            "{kind:?} must add at catalog defaults"
        );
    }

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let panel_nodes = render_panel(&ctx, &mut controller);
    assert_eq!(
        find_all(&panel_nodes, Role::Label, &tr_args_cpu(0.0)).len(),
        6,
        "every one of the six kinds must show zero cost while idle"
    );
}

/// US3 AS6: stereo tools' phase-invert toggle is only meaningful while
/// mono sum is on, and the panel disables it otherwise (`add_enabled`).
#[test]
fn phase_invert_disabled_unless_mono_sum() {
    let (mut controller, _dir) = ready_controller("phase-invert");
    let id = controller
        .chain_add_node(NodeKind::StereoTools)
        .expect("add stereo tools");

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);
    let phase_invert = find_one(&nodes, Role::Button, &tr("effects-param-phase-invert"));
    assert!(
        phase_invert.disabled,
        "phase invert must be disabled while mono sum is off"
    );

    controller
        .chain_set_param(id, ParamId(2), 1.0) // mono_sum on
        .expect("set mono_sum");
    let nodes = render_panel(&ctx, &mut controller);
    let phase_invert = find_one(&nodes, Role::Button, &tr("effects-param-phase-invert"));
    assert!(
        !phase_invert.disabled,
        "phase invert must be enabled once mono sum is on"
    );
}

// -----------------------------------------------------------------------
// Phase 6 (US4): meters, spectrum, overload counter, over-budget badge,
// per-row auto-bypassed label — contracts/ui-effect-chain.md §2, §8.
// -----------------------------------------------------------------------

/// US4 AS5: a row's "auto-bypassed" label follows `NodeRow::auto_bypassed`
/// (itself set from `Event::AutoBypassed` via `ChainModel::
/// mark_auto_bypassed`, `debug_inject_engine_event` standing in for the
/// real RT event this test has no live overload to produce).
#[test]
fn auto_bypassed_label_shown_from_view_flag() {
    let (mut controller, _dir) = ready_controller("auto-bypassed-label");
    let id = controller.chain_add_node(NodeKind::Gain).expect("add");
    let slot = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .slot;

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);
    assert!(
        find_all(&nodes, Role::Label, &tr("effects-auto-bypassed")).is_empty(),
        "label must not show before any auto-bypass"
    );

    controller.debug_inject_engine_event(EngineEvent::AutoBypassed { slot });
    controller.tick();

    let nodes = render_panel(&ctx, &mut controller);
    assert_eq!(
        find_all(&nodes, Role::Label, &tr("effects-auto-bypassed")).len(),
        1,
        "the auto-bypassed row must show the label once the view flag is set"
    );
}

/// US4 AS4: the header's over-budget badge and overload counter follow
/// `ChainView::over_budget`/`overload_count`, themselves mirrored from
/// `RtShared` (contracts/effects-service.md §2 rule C9).
#[test]
fn over_budget_badge_and_counter_follow_view() {
    let (mut controller, _dir) = ready_controller("over-budget-badge");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let nodes = render_panel(&ctx, &mut controller);
    assert!(
        find_all(&nodes, Role::Label, &tr("effects-over-budget-badge")).is_empty(),
        "badge must not show while not over budget"
    );
    assert_eq!(
        find_all(
            &nodes,
            Role::Label,
            &modplayer_core::tr_args("effects-overloads", &[("count", "0".to_string())]),
        )
        .len(),
        1,
        "the counter must read 0 with no overload yet"
    );

    controller.shared().set_over_budget(true);
    controller.shared().set_overload_count(3);

    let nodes = render_panel(&ctx, &mut controller);
    assert_eq!(
        find_all(&nodes, Role::Label, &tr("effects-over-budget-badge")).len(),
        1,
        "badge must show once the view reports over budget"
    );
    assert_eq!(
        find_all(
            &nodes,
            Role::Label,
            &modplayer_core::tr_args("effects-overloads", &[("count", "3".to_string())]),
        )
        .len(),
        1,
        "the overload counter must follow RtShared::overload_count()"
    );
}

/// FR-011/SC-008: the pre-/post-chain level pairs and the spectrum render
/// from a `MeterSnapshot` read straight from `RtShared`.
#[test]
fn meters_and_spectrum_render_from_snapshot() {
    let (mut controller, _dir) = ready_controller("meters-spectrum");
    controller.shared().set_pre_level(0.5, 0.4, 0.3, 0.2);
    controller.shared().set_post_level(0.1, 0.1, 0.05, 0.05);
    let mut bands = [0.0f32; 64];
    bands[10] = 0.9;
    controller.shared().set_spectrum(&bands, 1);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let nodes = render_panel(&ctx, &mut controller);

    let pre = nodes
        .iter()
        .find(|n| {
            n.role == Role::ProgressIndicator
                && n.accessible_name()
                    .is_some_and(|name| name.starts_with(&tr("effects-pre")))
        })
        .expect("pre-chain level widget");
    assert!(
        pre.accessible_name().unwrap_or_default().contains("dB"),
        "pre level's accessible name must carry a dB value: {pre:?}"
    );

    let post = nodes
        .iter()
        .find(|n| {
            n.role == Role::ProgressIndicator
                && n.accessible_name()
                    .is_some_and(|name| name.starts_with(&tr("effects-post")))
        })
        .expect("post-chain level widget");
    assert!(
        post.accessible_name().unwrap_or_default().contains("dB"),
        "post level's accessible name must carry a dB value: {post:?}"
    );

    let spectrum = nodes
        .iter()
        .find(|n| {
            n.role == Role::ProgressIndicator
                && n.accessible_name()
                    .is_some_and(|name| name.starts_with(&tr("effects-spectrum")))
        })
        .expect("spectrum widget");
    assert!(
        spectrum
            .accessible_name()
            .unwrap_or_default()
            .contains("64"),
        "spectrum's accessible name must report its band count: {spectrum:?}"
    );
}

/// US1 AS8 / SC-012 ("click the waveform, press `+` → the waveform
/// zooms"): a pointer click on the waveform must also take keyboard
/// focus, so the dispatcher's focused-widget claim (pinned in
/// `tests/actions.rs::plus_minus_step_tempo_unless_waveform_focused`)
/// actually engages and `+` reaches the waveform's own zoom row. The
/// 2026-09-19 manual walk (M6) found the click seeking but leaving focus
/// unset, so `+` stepped tempo instead.
#[test]
fn click_on_waveform_takes_focus_so_plus_zooms() {
    let (mut controller, _dir) = ready_controller("waveform-click-focus");
    // The waveform only senses clicks while the transport is available.
    controller.set_playback_permitted(true, None);
    controller.tick();
    controller.queue_replace(vec![track("a")]);
    let _ = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let nodes = render_nodes_on(&ctx, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    // The overview strip is the top-most `Slider` on the screen.
    let mut sliders: Vec<Rect> = nodes
        .iter()
        .filter(|node| node.role == Role::Slider)
        .filter_map(|node| node.bounds)
        .collect();
    sliders.sort_by(|a, b| {
        a.min
            .y
            .partial_cmp(&b.min.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let overview = sliders.first().copied().expect("an overview waveform");
    assert!(
        ctx.memory(|memory| memory.focused()).is_none(),
        "sanity: nothing is focused before the click"
    );

    let pos = overview.center();
    for pressed in [true, false] {
        let mut input = default_input();
        input.events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        });
        let output = ctx.run_ui(input, |ui| {
            modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
        });
        output.drop_without_applying_deltas();
    }
    let focused = ctx.memory(|memory| memory.focused());
    assert!(
        focused.is_some(),
        "a click on the waveform must leave it keyboard-focused"
    );

    let before = waveform
        .detail
        .expect("a detail window exists after rendering");
    let mut plus = default_input();
    plus.events.push(Event::Key {
        key: Key::Equals,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(plus, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    let after = waveform.detail.expect("detail window still exists");
    assert!(
        after.width_frames < before.width_frames,
        "`+` on the focused waveform must zoom in ({} -> {} frames)",
        before.width_frames,
        after.width_frames
    );
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        focused,
        "zooming keeps focus on the waveform"
    );
}

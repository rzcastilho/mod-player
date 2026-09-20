// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `PlaybackController` end-to-end tests for the bundled Key & Tempo
//! plugin (013-key-and-tempo-plugin, contracts/key-tempo-plugin.md),
//! driving the real `org.modplayer.key-tempo` package (always bundled,
//! `bundled::packages()`, research R8) — mirrors
//! `controller_section_loop.rs`'s own harness shape. Phase 2
//! (Foundational) covers only registration and node adoption/creation
//! (G1-G3); Phases 3-5 add the action-handler tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_capability_gateway::event::ActionSource;
use modplayer_capability_gateway::state::PluginStatePaths;
use modplayer_capability_gateway::ui::{UiId, WidgetValue};
use modplayer_core::PlaybackController;
use modplayer_core::actions::{ActionId, Chord, PluginActionId};
use modplayer_core::plugins::{Lifecycle, PluginId};
use modplayer_core::settings::SettingsStore;
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};

/// Float tolerance for f32-round-tripped parameter comparisons.
const EPS: f32 = 1e-4;

const KEY_TEMPO: &str = "org.modplayer.key-tempo";

type KeyTempoController = PlaybackController<FakeBackend, ScriptedHost>;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-key-tempo-{}-{}",
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

/// `MODPLAYER_PLUGIN_STATE_DIR`/`MODPLAYER_TRACK_STATE_DIR` are
/// process-global, so every `PlaybackController::new` in this binary must
/// be serialized against every other's brief mutation of them (mirrors
/// `controller_section_loop.rs`'s own `PLUGIN_ENV_LOCK`). No
/// `MODPLAYER_PLUGIN_FIXTURES`: Key & Tempo is bundled and discovers
/// regardless (research R8).
static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

fn key_tempo_controller() -> (KeyTempoController, TempDir, TempDir, TempDir) {
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
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    (controller, dir, plugin_state_dir, track_state_dir)
}

fn key_tempo_id(controller: &mut KeyTempoController) -> PluginId {
    controller
        .plugins_mut()
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == KEY_TEMPO)
        .map(|r| r.id)
        .unwrap_or_else(|| unreachable!("{KEY_TEMPO} must be discovered"))
}

fn pump_until(
    controller: &mut KeyTempoController,
    timeout: Duration,
    mut done: impl FnMut(&mut KeyTempoController) -> bool,
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

fn wait_active(controller: &mut KeyTempoController, id: PluginId) -> bool {
    pump_until(controller, Duration::from_secs(5), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )
    })
}

fn wait_disabled(controller: &mut KeyTempoController, id: PluginId) -> bool {
    pump_until(controller, Duration::from_secs(5), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Disabled)
        )
    })
}

/// G1-G3 (contracts/key-tempo-plugin.md §2): from a cold start (neither
/// node exists), `ready_ack` registers the 8 actions and the `main`
/// panel's 16 widgets, then creates exactly one `pitch_shift` and one
/// `time_stretch` node, adjacent, pitch before stretch, both owned by
/// Key & Tempo.
#[test]
fn ready_creates_adjacent_pitch_and_stretch_nodes() {
    let (mut controller, _dir, _psd, _tsd) = key_tempo_controller();
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    let id = key_tempo_id(&mut controller);
    assert!(wait_active(&mut controller, id), "Key & Tempo must launch");

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            c.chain().nodes().len() >= 2
        }),
        "Key & Tempo must create both of its nodes on a cold start"
    );

    let nodes = controller.chain().nodes();
    assert_eq!(
        nodes.len(),
        2,
        "exactly the two nodes Key & Tempo owns, nothing else"
    );
    assert_eq!(
        nodes[0].kind,
        modplayer_effects::catalog::NodeKind::PitchShift,
        "pitch_shift must come first"
    );
    assert_eq!(
        nodes[1].kind,
        modplayer_effects::catalog::NodeKind::TimeStretch,
        "time_stretch must come right after it"
    );
    assert_eq!(nodes[0].owner, NodeOwner::Plugin(id));
    assert_eq!(nodes[1].owner, NodeOwner::Plugin(id));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            c.plugins_mut()
                .ui()
                .panels()
                .for_plugin(id)
                .iter()
                .any(|p| p.id.as_str() == "main")
        }),
        "Key & Tempo's ready_ack handler must register its panel"
    );
    let panels = controller
        .plugins_mut()
        .ui()
        .panels()
        .for_plugin(id)
        .to_vec();
    assert_eq!(panels.len(), 1, "exactly one panel, \"main\"");
    assert_eq!(
        panels[0].widgets.len(),
        16,
        "the panel must carry all 16 widgets data-model.md §3.4 lists"
    );

    let plugin_actions: Vec<_> = controller
        .actions()
        .rows()
        .filter(|row| matches!(&row.id, ActionId::Plugin(p) if p.plugin.as_str() == KEY_TEMPO))
        .collect();
    assert_eq!(
        plugin_actions.len(),
        8,
        "exactly the 8 trigger actions data-model.md §3.5 lists"
    );
}

// -----------------------------------------------------------------------
// User Story 1 (T032, T033): Key/Fine tune/Formant/Quality — contracts
// A1, A2, A6, A7, A8 and the E1 mirroring tie-break (T031).
// -----------------------------------------------------------------------

/// Launches Key & Tempo through to both nodes existing (G1-G3), same
/// cold-start shape `ready_creates_adjacent_pitch_and_stretch_nodes`
/// verifies above — the shared starting point for every US1 test below.
fn key_tempo_ready() -> (KeyTempoController, TempDir, TempDir, TempDir, PluginId) {
    let (mut controller, dir, psd, tsd) = key_tempo_controller();
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    let id = key_tempo_id(&mut controller);
    assert!(wait_active(&mut controller, id), "Key & Tempo must launch");
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            c.chain().nodes().len() >= 2
        }),
        "Key & Tempo must create both of its nodes on a cold start"
    );
    // G2 "neither present" (T045/T047): right after creating both nodes,
    // the script runs the FR-012 default-reset logic for the (here,
    // absent) current track — a handful of `set_param` calls that are
    // no-ops value-wise (every node is already at its default) but are
    // still in-flight RPCs. Drain them before handing control back, so a
    // leftover default-apply can never race a test's own first change.
    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    (controller, dir, psd, tsd, id)
}

fn wid(s: &str) -> UiId {
    UiId::parse(s).unwrap_or_else(|| unreachable!("{s:?} must be valid grammar"))
}

/// A panel widget commit (slider/toggle) or a plain-button click (R6) —
/// both reach the plugin as the same `panel_interaction` event.
fn click(controller: &mut KeyTempoController, id: PluginId, widget: &str, value: WidgetValue) {
    controller.plugin_panel_interaction(id, &wid("main"), &wid(widget), value);
}

fn widget_value(
    controller: &mut KeyTempoController,
    id: PluginId,
    widget: &str,
) -> Option<WidgetValue> {
    controller
        .plugins_mut()
        .ui()
        .panels()
        .for_plugin(id)
        .iter()
        .find(|p| p.id.as_str() == "main")
        .and_then(|p| p.widgets.iter().find(|w| w.spec.id.as_str() == widget))
        .map(|w| w.value.clone())
}

fn widget_number(controller: &mut KeyTempoController, id: PluginId, widget: &str) -> Option<f64> {
    match widget_value(controller, id, widget) {
        Some(WidgetValue::Number(v)) => Some(v),
        _ => None,
    }
}

fn pitch_node_id(controller: &mut KeyTempoController) -> modplayer_core::NodeId {
    controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::PitchShift)
        .map(|n| n.id)
        .unwrap_or_else(|| unreachable!("pitch node must exist"))
}

fn pitch_semitones(controller: &mut KeyTempoController) -> f32 {
    controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::PitchShift)
        .map(|n| n.params[0])
        .unwrap_or_else(|| unreachable!("pitch node must exist"))
}

fn pitch_formant(controller: &mut KeyTempoController) -> f32 {
    controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::PitchShift)
        .map(|n| n.params[1])
        .unwrap_or_else(|| unreachable!("pitch node must exist"))
}

fn pitch_quality(controller: &mut KeyTempoController) -> f32 {
    controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::PitchShift)
        .map(|n| n.params[2])
        .unwrap_or_else(|| unreachable!("pitch node must exist"))
}

fn pitch_auto_switched(controller: &mut KeyTempoController) -> bool {
    controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::PitchShift)
        .and_then(|n| n.mode_state)
        .is_some_and(|m| m.auto_switched)
}

fn stretch_node_id(controller: &mut KeyTempoController) -> modplayer_core::NodeId {
    controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::TimeStretch)
        .map(|n| n.id)
        .unwrap_or_else(|| unreachable!("stretch node must exist"))
}

fn stretch_ratio(controller: &mut KeyTempoController) -> f32 {
    controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::TimeStretch)
        .map(|n| n.params[0])
        .unwrap_or_else(|| unreachable!("stretch node must exist"))
}

fn stretch_quality(controller: &mut KeyTempoController) -> f32 {
    controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::TimeStretch)
        .map(|n| n.params[1])
        .unwrap_or_else(|| unreachable!("stretch node must exist"))
}

/// A1, A7 (US1-1): two `key_down` clicks compose straight to the pitch
/// node's `semitones`; the stretch node's `ratio` is never touched.
#[test]
fn key_minus_two_sets_semitones_not_ratio() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    click(&mut controller, id, "key_down", WidgetValue::Bool(true));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - -2.0).abs() <= EPS
        }),
        "two key_down clicks must set semitones to -2.0"
    );
    assert!(
        (stretch_ratio(&mut controller) - 1.0).abs() <= EPS,
        "key_down must never touch the stretch node's ratio"
    );
}

/// A8 first half (US1-2): from +7/+30, `reset_key` zeroes both Key and
/// Fine tune, and the pitch node's `semitones` with them.
#[test]
fn reset_key_zeroes_key_and_cents() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();

    // Each commit settles (its own echo confirmed) before the next one
    // fires — two genuinely separate user actions, as M1/M2 walk them.
    click(&mut controller, id, "key", WidgetValue::Number(7.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 7.0).abs() <= EPS
        }),
        "key +7 must compose to semitones 7.0"
    );
    click(&mut controller, id, "cents", WidgetValue::Number(30.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 7.3).abs() <= EPS
        }),
        "key +7 / cents +30 must compose to semitones 7.3 before reset"
    );

    click(&mut controller, id, "reset_key", WidgetValue::Bool(true));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 0.0).abs() <= EPS
                && widget_number(c, id, "key") == Some(0.0)
                && widget_number(c, id, "cents") == Some(0.0)
        }),
        "reset_key must zero semitones and both the Key and Fine tune widgets"
    );
}

/// A2, A7 (US1-3): Key −5 and Fine tune +30 compose to −4.70, and stay
/// exactly −5 / +30 on the panel (not the rounded decomposition of
/// −4.7).
#[test]
fn fine_tune_composes_with_key() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();

    // Each commit settles before the next one fires (two separate user
    // actions, as M2 walks them).
    click(&mut controller, id, "key", WidgetValue::Number(-5.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - -5.0).abs() <= EPS
        }),
        "key -5 must compose to semitones -5.0"
    );
    click(&mut controller, id, "cents", WidgetValue::Number(30.0));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - -4.7).abs() <= EPS
                && widget_number(c, id, "key") == Some(-5.0)
                && widget_number(c, id, "cents") == Some(30.0)
        }),
        "key -5 / cents +30 must compose to semitones -4.70 and the panel must keep -5 / +30, not a rounded decomposition of -4.7"
    );
}

/// A6 first half (US1-4): Formant preservation touches only the pitch
/// node.
#[test]
fn formant_touches_only_pitch_node() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    let ratio_before = stretch_ratio(&mut controller);
    let stretch_quality_before = stretch_quality(&mut controller);

    click(&mut controller, id, "formant", WidgetValue::Bool(true));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_formant(c) - 1.0).abs() <= EPS
        }),
        "the formant toggle must set the pitch node's formant param"
    );
    assert!(
        (stretch_ratio(&mut controller) - ratio_before).abs() <= EPS,
        "formant must never touch the stretch node's ratio"
    );
    assert!(
        (stretch_quality(&mut controller) - stretch_quality_before).abs() <= EPS,
        "formant must never touch the stretch node's quality_mode"
    );
}

/// E1 step 3, contract E1 (T031): the plugin's own composed send
/// survives the resulting `effect_chain_changed` echo unsplit — key +4 /
/// cents +50 stays +4 / +50, not re-decomposed from 4.5.
#[test]
fn own_split_survives_echo() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();

    // Each commit settles before the next one fires (two separate user
    // actions, as M2 walks them) — isolates the tie-break itself (T031)
    // from the cross-tick race a same-tick double commit would create.
    click(&mut controller, id, "key", WidgetValue::Number(4.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 4.0).abs() <= EPS
        }),
        "key +4 must compose to semitones 4.0"
    );
    click(&mut controller, id, "cents", WidgetValue::Number(50.0));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 4.5).abs() <= EPS
                && widget_number(c, id, "key") == Some(4.0)
                && widget_number(c, id, "cents") == Some(50.0)
        }),
        "key +4 / cents +50 must compose to semitones 4.5 and stay +4 / +50 on the panel"
    );
    // Give the resulting `effect_chain_changed` echo time to reach the
    // plugin's own thread and run E1 at least once more: no counter-split.
    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        widget_number(&mut controller, id, "key"),
        Some(4.0),
        "the plugin's own echo must not re-split its own composed value"
    );
    assert_eq!(widget_number(&mut controller, id, "cents"), Some(50.0));
}

/// E1 step 3, contract L4 (T031): a host-side Effect Chain panel edit
/// (never sent by the plugin) is decomposed round-half-up and mirrored,
/// never reverted.
#[test]
fn host_panel_edit_is_mirrored_not_reverted() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    let pitch_id = pitch_node_id(&mut controller);

    controller
        .chain_set_param(pitch_id, ParamId(0), -4.7)
        .unwrap_or_else(|e| unreachable!("chain_set_param must succeed: {e:?}"));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            widget_number(c, id, "key") == Some(-5.0) && widget_number(c, id, "cents") == Some(30.0)
        }),
        "a host edit to -4.7 must decompose round-half-up to key -5 / cents 30"
    );
    // No counter-write: the mirrored value must settle, not bounce.
    for _ in 0..20 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        (pitch_semitones(&mut controller) - -4.7).abs() <= EPS,
        "the plugin must never call set_param back for a value it only observed"
    );
}

/// A6, FR-008/FR-009 (T029/T030): an excursion past ±3 semitones
/// auto-switches the pitch node to Quality with no `set_param` from the
/// plugin; the mirrored `quality` widget follows.
#[test]
fn auto_switch_shows_quality_on_without_plugin_set_param() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();

    click(&mut controller, id, "key", WidgetValue::Number(7.0));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            pitch_auto_switched(c)
        }),
        "semitones = 7 must auto-switch the pitch node to Quality"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            widget_value(c, id, "quality") == Some(WidgetValue::Bool(true))
        }),
        "the quality widget must mirror the auto-switch"
    );
    assert!(
        (pitch_quality(&mut controller) - 1.0).abs() <= EPS,
        "the pitch node's quality_mode must read Quality"
    );
}

/// A6 second half (T030): a user flip of the Quality toggle sets
/// **both** nodes explicitly and clears the auto-switch flag.
#[test]
fn user_quality_flip_sets_both_nodes_and_clears_auto() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    click(&mut controller, id, "key", WidgetValue::Number(7.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            pitch_auto_switched(c)
        }),
        "semitones = 7 must auto-switch the pitch node first"
    );

    click(&mut controller, id, "quality", WidgetValue::Bool(false));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_quality(c) - 0.0).abs() <= EPS && (stretch_quality(c) - 0.0).abs() <= EPS
        }),
        "the user flip must set both nodes' quality_mode to Performance"
    );
    assert!(
        !pitch_auto_switched(&mut controller),
        "an explicit user mode choice must clear auto_switched"
    );
}

// -----------------------------------------------------------------------
// User Story 2 (T039, T040): Tempo/step slider, tempo_up/tempo_down,
// reset_tempo — contracts A3, A4, A5, A8 second half, E1, research R7.
// -----------------------------------------------------------------------

const TRACK_RATE: u64 = 44_100;

/// The resolved `@step_note_text` string (`plugin.toml`'s
/// `[strings.en-US]`) — widget text is resolved server-side (`apply.rs`
/// `resolve_string`), so tests compare against the resolved value, never
/// the raw `@key`.
const STEP_NOTE_TEXT: &str =
    "+ / - still steps by the default 10% until rebound in Settings › Controls";

fn ms_frames(ms: u64) -> u64 {
    ms * TRACK_RATE / 1000
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

fn seek_ms(controller: &mut KeyTempoController, ms: u64) {
    controller.seek_frames(ms_frames(ms));
    let _ = controller.backend_mut().render_buffers(1);
}

/// A3 (US2-1): the tempo slider commit sends `ratio` directly and never
/// touches the pitch node's `semitones`.
#[test]
fn tempo_sixty_percent_keeps_semitones() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    let semitones_before = pitch_semitones(&mut controller);

    click(&mut controller, id, "tempo", WidgetValue::Number(60.0));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 0.6).abs() <= EPS
        }),
        "tempo 60 must set the stretch node's ratio to 0.6"
    );
    assert!(
        (pitch_semitones(&mut controller) - semitones_before).abs() <= EPS,
        "a tempo change must never touch the pitch node's semitones"
    );
}

/// A3, L3 (US2-2): with a loop region armed on the current track, a
/// tempo change never disturbs the armed region, and the effect chain —
/// which holds no concept of looping at all — still lists exactly Key &
/// Tempo's own two nodes. Section Loop and Key & Tempo hold disjoint
/// permissions and state (contract L3).
#[test]
fn tempo_change_while_section_loop_armed() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    seek_ms(&mut controller, 1_000);
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a must succeed: {e:?}"));
    seek_ms(&mut controller, 5_000);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b must succeed: {e:?}"));
    let region = controller
        .markers()
        .and_then(|m| m.current_region())
        .unwrap_or_else(|| unreachable!("a region must exist after set_loop_a/set_loop_b"));
    controller
        .arm_loop(region)
        .unwrap_or_else(|e| unreachable!("arm_loop must succeed: {e:?}"));
    assert!(
        controller
            .markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.armed),
        "the region must be armed before the tempo change"
    );

    click(&mut controller, id, "tempo", WidgetValue::Number(60.0));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 0.6).abs() <= EPS
        }),
        "tempo 60 must still take effect while a region is armed"
    );
    assert!(
        controller
            .markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.armed),
        "the armed region must stay armed through the tempo change"
    );
    assert_eq!(
        controller.chain().nodes().len(),
        2,
        "the chain must still hold exactly Key & Tempo's own two nodes"
    );
}

/// A4, A5 (US2-3, US2-5): four `tempo_up` clicks at the default step
/// (10) reach 140%; lowering `step` to 5 then moves the next click by 5.
#[test]
fn tempo_up_button_uses_configured_step() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();

    let mut expected = 1.0_f32;
    for _ in 0..4 {
        expected += 0.10;
        click(&mut controller, id, "tempo_up", WidgetValue::Bool(true));
        assert!(
            pump_until(&mut controller, Duration::from_secs(5), |c| {
                (stretch_ratio(c) - expected).abs() <= EPS
            }),
            "each tempo_up click must move ratio by the configured step (10)"
        );
    }
    assert!(
        (stretch_ratio(&mut controller) - 1.4).abs() <= EPS,
        "4 tempo_up clicks at the default step 10 must reach 140%"
    );

    click(&mut controller, id, "step", WidgetValue::Number(5.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            widget_value(c, id, "step_note") == Some(WidgetValue::Text(STEP_NOTE_TEXT.to_string()))
        }),
        "a non-default step must show the step_note text"
    );

    click(&mut controller, id, "tempo_up", WidgetValue::Bool(true));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 1.45).abs() <= EPS
        }),
        "tempo_up at step 5 must move ratio by 5 (to 145%)"
    );
}

/// A5, FR-013 (US2-5): the step_note text row is empty at the default
/// step (10) and reads the resolved `@step_note_text` string at any
/// other step.
#[test]
fn step_note_shown_iff_step_not_ten() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    assert_eq!(
        widget_value(&mut controller, id, "step_note"),
        Some(WidgetValue::Text(String::new())),
        "the default step (10) must show no note"
    );

    click(&mut controller, id, "step", WidgetValue::Number(5.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            widget_value(c, id, "step_note") == Some(WidgetValue::Text(STEP_NOTE_TEXT.to_string()))
        }),
        "a non-default step must show the step_note text"
    );

    click(&mut controller, id, "step", WidgetValue::Number(10.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            widget_value(c, id, "step_note") == Some(WidgetValue::Text(String::new()))
        }),
        "returning the step to 10 must clear the note"
    );
}

/// A8 second half (US2-6): `reset_tempo` returns `ratio` to 1.0.
#[test]
fn reset_tempo_returns_to_one() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    click(&mut controller, id, "tempo", WidgetValue::Number(60.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 0.6).abs() <= EPS
        }),
        "tempo 60 must first take effect"
    );

    click(&mut controller, id, "reset_tempo", WidgetValue::Bool(true));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 1.0).abs() <= EPS
        }),
        "reset_tempo must return ratio to 1.0"
    );
}

/// E1, research R7 (US2-4): the host's own fixed-step `+`/`-` action
/// (008 FR-017) drives the plugin's own stretch node, and the panel's
/// `tempo` widget mirrors it once the resulting `effect_chain_changed`
/// reaches the plugin; Key & Tempo's own `Plus` binding is flagged in
/// conflict with the host's `TempoStepUp` on a fresh install (011
/// FR-011).
#[test]
fn host_plus_minus_drives_stretch_and_panel_follows() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();

    controller.tempo_step(1);

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 1.1).abs() <= EPS
        }),
        "the host's fixed-step action must drive the stretch node's ratio"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            widget_number(c, id, "tempo") == Some(110.0)
        }),
        "the tempo widget must mirror the host's own +/- step"
    );

    let plugin_tempo_up = PluginActionId::parse(&format!("{KEY_TEMPO}.tempo_up"))
        .unwrap_or_else(|| unreachable!("tempo_up must parse as a plugin action id"));
    let plus = Chord::parse("Plus").unwrap_or_else(|_| unreachable!("Plus must parse"));
    assert!(
        controller.actions().is_conflicting(plugin_tempo_up, plus),
        "Key & Tempo's own Plus binding must be flagged conflicting with the host's TempoStepUp on a fresh install"
    );
}

// -----------------------------------------------------------------------
// User Story 3 (T048-T050): per-track memory — contracts A9-A11, E2, M1-M6,
// G2 case (a), L1/L2.
// -----------------------------------------------------------------------

/// The resolved `@restored_text` string.
const RESTORED_TEXT: &str = "Restored from this track's memory";
/// The resolved `@node_removed_text` string.
const NODE_REMOVED_TEXT: &str =
    "Effect node removed — disable and re-enable Key & Tempo to recreate it";
/// `Refusal::no_track()`'s own message (contracts/plugin-api-v1.4.md),
/// echoed verbatim into `restored` (A9, spec.md clarify item 7).
const NO_TRACK_MESSAGE: &str = "There is no current track.";

fn track_id_str(label: &str) -> String {
    format!("spotify:track:{label}")
}

/// `<plugin_state_dir>/<hex(identifier)>/tracks/<hex(track)>.json` — the
/// file the runtime's own `flush_scope`/`restore_track_state` (RT11, G7)
/// read and write; a host-observable proxy for the plugin's own
/// `state.track` store (no plugin probe, research R11).
fn track_settings_path(plugin_state_dir: &Path, label: &str) -> PathBuf {
    PluginStatePaths::with_dir(plugin_state_dir).track_file(KEY_TEMPO, &track_id_str(label))
}

/// `None` if the file does not exist yet, or exists with no `settings`
/// key (M5's "removed" case).
fn read_settings_entry(plugin_state_dir: &Path, label: &str) -> Option<serde_json::Value> {
    let bytes = std::fs::read(track_settings_path(plugin_state_dir, label)).ok()?;
    let file: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    file.get("entries")?.get("settings").cloned()
}

/// A dirty `Track` scope only reaches disk on the next track switch (or
/// plugin teardown) — poll rather than assume any one `tick()` suffices.
fn wait_for_settings(
    controller: &mut KeyTempoController,
    plugin_state_dir: &Path,
    label: &str,
    timeout: Duration,
) -> Option<serde_json::Value> {
    let deadline = Instant::now() + timeout;
    loop {
        controller.tick();
        if let Some(entry) = read_settings_entry(plugin_state_dir, label) {
            return Some(entry);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Like [`wait_for_settings`], but for a file that may already exist
/// (seeded by the test) — waits for the flushed content itself to match
/// `done`, not merely for the file to be readable.
fn wait_for_settings_matching(
    controller: &mut KeyTempoController,
    plugin_state_dir: &Path,
    label: &str,
    timeout: Duration,
    mut done: impl FnMut(&serde_json::Value) -> bool,
) -> Option<serde_json::Value> {
    let deadline = Instant::now() + timeout;
    loop {
        controller.tick();
        if let Some(entry) = read_settings_entry(plugin_state_dir, label)
            && done(&entry)
        {
            return Some(entry);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_for_settings_absent(
    controller: &mut KeyTempoController,
    plugin_state_dir: &Path,
    label: &str,
    timeout: Duration,
) -> bool {
    pump_until(controller, timeout, |_| {
        read_settings_entry(plugin_state_dir, label).is_none()
    })
}

/// M4 (test-side seeding): write a raw, possibly malformed `settings`
/// entry straight to disk before the plugin ever loads that track, the
/// same shape `flush_scope`'s own `PluginStateStore::encode` produces.
fn seed_track_entry(plugin_state_dir: &Path, label: &str, entry: serde_json::Value) {
    let path = track_settings_path(plugin_state_dir, label);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|e| unreachable!("create_dir_all must succeed: {e}"));
    }
    let file = serde_json::json!({ "version": 1, "entries": { "settings": entry } });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&file).unwrap_or_else(|e| unreachable!("encode: {e}")),
    )
    .unwrap_or_else(|e| unreachable!("write settings file must succeed: {e}"));
}

/// Section Loop's own per-track marker file is flushed to disk by an
/// async writer thread (same shape as the plugin state store's own),
/// so a switch away from a track and straight back can race that
/// flush — poll for the file rather than assume one `tick()` suffices.
fn wait_for_marker_file(track_state_dir: &Path, label: &str, timeout: Duration) -> bool {
    let track_id = TrackId::new(track_id_str(label))
        .unwrap_or_else(|_| unreachable!("{label} must be a valid track id"));
    let path = modplayer_core::markers::store::TrackStatePaths::with_dir(track_state_dir)
        .file_for(&track_id);
    let deadline = Instant::now() + timeout;
    loop {
        if path.is_file() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn queue_two_tracks_and_play(controller: &mut KeyTempoController) {
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();
}

/// Invokes a plugin's own registered action directly (the keyboard-
/// shortcut path), distinct from `click`'s panel-button path — both
/// reach the same handler function in `main.luau`.
fn invoke_action(controller: &mut KeyTempoController, name: &str) {
    let action_id = PluginActionId::parse(&format!("{KEY_TEMPO}.{name}"))
        .unwrap_or_else(|| unreachable!("{name} must parse as a plugin action id"));
    controller.invoke_plugin_action(&action_id, ActionSource::Keyboard);
}

/// A9, M1 (US3-1): turning "Remember" on writes the one `settings` entry
/// from the values then in effect; the write reaches disk on the next
/// track switch (RT11).
#[test]
fn remember_on_writes_settings_immediately() {
    let (mut controller, _dir, psd, _tsd, id) = key_tempo_ready();
    queue_two_tracks_and_play(&mut controller);

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - -2.0).abs() <= EPS
        }),
        "key -2 must take effect before remember is turned on"
    );

    click(&mut controller, id, "remember", WidgetValue::Bool(true));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
        }),
        "remember must turn on"
    );

    controller.skip_forward();
    let entry = wait_for_settings(&mut controller, psd.path(), "a", Duration::from_secs(5))
        .unwrap_or_else(|| unreachable!("settings must be written for track a"));
    assert_eq!(
        entry,
        serde_json::json!({
            "key": -2, "cents": 0, "tempo": 100, "formant": false, "quality": "performance"
        }),
        "the store must hold exactly the values then in effect"
    );
}

/// E1 step 5 (US3-1, SC-009): every mirrored change, whatever its actor —
/// a panel commit, the plugin's own registered action, the host's own
/// fixed-step `+`/`-`, and a direct Effect Chain panel edit — keeps the
/// remembered entry in sync.
#[test]
fn every_actor_updates_remembered_entry() {
    let (mut controller, _dir, psd, _tsd, id) = key_tempo_ready();
    queue_two_tracks_and_play(&mut controller);
    let pitch_id = pitch_node_id(&mut controller);

    click(&mut controller, id, "remember", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
    }));

    // Actor 1: a panel commit.
    click(&mut controller, id, "tempo", WidgetValue::Number(60.0));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 0.6).abs() <= EPS
        }),
        "panel commit must take effect"
    );

    // Actor 2: the plugin's own registered action.
    invoke_action(&mut controller, "key_down");
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - -1.0).abs() <= EPS
        }),
        "the plugin's own action must take effect"
    );

    // Actor 3: the host's own fixed-step `+`/`-` (008 FR-017).
    controller.tempo_step(1);
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 0.7).abs() <= EPS
        }),
        "the host's own step action must take effect"
    );

    // Actor 4: a direct Effect Chain panel edit (formant, param 1).
    controller
        .chain_set_param(pitch_id, ParamId(1), 1.0)
        .unwrap_or_else(|e| unreachable!("chain_set_param must succeed: {e:?}"));
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_formant(c) - 1.0).abs() <= EPS
        }),
        "the host edit must take effect"
    );

    controller.skip_forward();
    let entry = wait_for_settings(&mut controller, psd.path(), "a", Duration::from_secs(5))
        .unwrap_or_else(|| unreachable!("settings must be written for track a"));
    assert_eq!(
        entry,
        serde_json::json!({
            "key": -1, "cents": 0, "tempo": 70, "formant": true, "quality": "performance"
        }),
        "the stored entry must reflect every actor's change"
    );
}

/// E2 step 4 (US3-2): a track with no `settings` entry of its own always
/// resets, with no badge — whether or not another track is remembered.
#[test]
fn next_track_without_entry_resets_and_no_badge() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    queue_two_tracks_and_play(&mut controller);

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - -2.0).abs() <= EPS
    }));
    click(&mut controller, id, "remember", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
    }));

    controller.skip_forward();
    controller.tick();
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 0.0).abs() <= EPS
                && (stretch_ratio(c) - 1.0).abs() <= EPS
                && widget_value(c, id, "remember") == Some(WidgetValue::Bool(false))
                && widget_value(c, id, "restored") == Some(WidgetValue::Text(String::new()))
        }),
        "track b, with no entry of its own, must reset to defaults with no badge"
    );
}

/// E2 step 3 (US3-3): returning to a remembered track restores its
/// values and shows the badge.
#[test]
fn return_to_remembered_track_restores_with_badge() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    queue_two_tracks_and_play(&mut controller);

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - -2.0).abs() <= EPS
    }));
    click(&mut controller, id, "remember", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
    }));

    controller.skip_forward();
    controller.tick();
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 0.0).abs() <= EPS
        }),
        "track b must reset first"
    );

    controller.skip_back();
    controller.tick();
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - -2.0).abs() <= EPS
                && widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
                && widget_value(c, id, "restored")
                    == Some(WidgetValue::Text(RESTORED_TEXT.to_string()))
        }),
        "returning to track a must restore -2 and show the badge"
    );
}

/// E2, L3 (US3-4): a never-remembered track always resets tempo/key on
/// return, and Section Loop's own per-track markers — disjoint state,
/// disjoint permissions — are untouched by it.
#[test]
fn never_remembered_track_resets_tempo_markers_untouched() {
    let (mut controller, _dir, _psd, tsd, id) = key_tempo_ready();
    queue_two_tracks_and_play(&mut controller);

    click(&mut controller, id, "tempo", WidgetValue::Number(60.0));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (stretch_ratio(c) - 0.6).abs() <= EPS
    }));

    seek_ms(&mut controller, 1_000);
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a must succeed: {e:?}"));
    seek_ms(&mut controller, 5_000);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b must succeed: {e:?}"));
    let region = controller
        .markers()
        .and_then(|m| m.current_region())
        .unwrap_or_else(|| unreachable!("a region must exist after set_loop_a/set_loop_b"));
    controller
        .arm_loop(region)
        .unwrap_or_else(|e| unreachable!("arm_loop must succeed: {e:?}"));

    controller.skip_forward();
    controller.tick();
    assert!(
        wait_for_marker_file(tsd.path(), "a", Duration::from_secs(5)),
        "leaving track a must flush its armed region to disk before it can reload correctly"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 1.0).abs() <= EPS
        }),
        "track b must be at 100% tempo"
    );

    // `skip_back`'s 3 s "restart the current item" threshold (T10) reads
    // real elapsed wall-clock time through this test's own polling
    // waits, so it is not the reliable way back here; replacing the
    // queue's context back onto track a is (the same "current track id
    // changed" trigger `fan_out_plugin_playback_events`/`sync_marker_
    // attachment` key off, research R11).
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (stretch_ratio(c) - 1.0).abs() <= EPS
        }),
        "a never-remembered track a must reset to 100% tempo, not the 60% it had before"
    );
    assert!(
        controller
            .markers()
            .and_then(|m| m.region(region))
            .is_some(),
        "Section Loop's own persisted loop region for track a must survive Key & Tempo's own \
         per-track reset (arming itself is session-only and always reloads disarmed, by the \
         host's own rule, unrelated to Key & Tempo)"
    );
}

/// E2 step 5 (US3-5): with "keep across tracks" on, a no-entry track
/// keeps the current values (no `set_param` at all) and shows no badge.
#[test]
fn keep_across_tracks_carries_values_without_badge() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    queue_two_tracks_and_play(&mut controller);

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - -2.0).abs() <= EPS
    }));

    click(&mut controller, id, "keep_across", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "keep_across") == Some(WidgetValue::Bool(true))
    }));

    controller.skip_forward();
    controller.tick();
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - -2.0).abs() <= EPS
                && widget_value(c, id, "remember") == Some(WidgetValue::Bool(false))
                && widget_value(c, id, "restored") == Some(WidgetValue::Text(String::new()))
        }),
        "keep_across must carry -2 to track b with no badge and remember shown off"
    );
}

/// A9, M5 (US3-6): turning "Remember" off removes the entry; the next
/// visit to that track resets.
#[test]
fn remember_off_removes_entry() {
    let (mut controller, _dir, psd, _tsd, id) = key_tempo_ready();
    queue_two_tracks_and_play(&mut controller);

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - -2.0).abs() <= EPS
    }));
    click(&mut controller, id, "remember", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
    }));
    controller.skip_forward();
    assert!(
        wait_for_settings(&mut controller, psd.path(), "a", Duration::from_secs(5)).is_some(),
        "the entry must exist on disk before it is removed"
    );

    controller.skip_back();
    controller.tick();
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
    }));

    click(&mut controller, id, "remember", WidgetValue::Bool(false));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "remember") == Some(WidgetValue::Bool(false))
    }));

    controller.skip_forward();
    assert!(
        wait_for_settings_absent(&mut controller, psd.path(), "a", Duration::from_secs(5)),
        "the entry must be removed from disk"
    );

    controller.skip_back();
    controller.tick();
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 0.0).abs() <= EPS
                && widget_value(c, id, "remember") == Some(WidgetValue::Bool(false))
        }),
        "track a must reset now that its entry is gone"
    );
}

/// A9 (spec.md clarify item 7): `toggle_remember` with no current track
/// reverts the toggle and shows the refusal message as the badge.
#[test]
fn remember_with_no_track_reverts_toggle_shows_message() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();

    click(&mut controller, id, "remember", WidgetValue::Bool(true));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            widget_value(c, id, "remember") == Some(WidgetValue::Bool(false))
                && widget_value(c, id, "restored")
                    == Some(WidgetValue::Text(NO_TRACK_MESSAGE.to_string()))
        }),
        "with no current track, the toggle must revert and show the refusal message"
    );
}

/// M4 (US3, malformed entry): an out-of-range/wrong-typed field is read
/// as its default, never refused, and the entry is rewritten in full on
/// the next mirrored change.
#[test]
fn malformed_entry_defaults_field_by_field_and_rewrites() {
    let (mut controller, _dir, psd, _tsd, id) = key_tempo_ready();
    seed_track_entry(
        psd.path(),
        "a",
        serde_json::json!({ "key": "x", "tempo": 999 }),
    );

    queue_two_tracks_and_play(&mut controller);

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 0.0).abs() <= EPS
                && (stretch_ratio(c) - 1.0).abs() <= EPS
                && widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
                && widget_value(c, id, "restored")
                    == Some(WidgetValue::Text(RESTORED_TEXT.to_string()))
        }),
        "a malformed entry must never be refused: every field reads as its default"
    );

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - -1.0).abs() <= EPS
    }));

    let expected = serde_json::json!({
        "key": -1, "cents": 0, "tempo": 100, "formant": false, "quality": "performance"
    });
    controller.skip_forward();
    let entry = wait_for_settings_matching(
        &mut controller,
        psd.path(),
        "a",
        Duration::from_secs(5),
        |v| *v == expected,
    )
    .unwrap_or_else(|| unreachable!("settings for track a must be rewritten, corrected"));
    assert_eq!(
        entry, expected,
        "the next mirrored change must rewrite the entry in full, corrected"
    );
}

/// G2 case (a), L2 (US3-7): a restart/re-enable that finds both own nodes
/// adopts them untouched — no `set_param`, same node ids, widgets
/// re-mirrored from the nodes' own `params`, and the badge left empty
/// (the earlier restore's context, if any, is gone). A restart's fresh
/// script instance has not yet seen a `track_changed` of its own — its
/// `state.track` scope only hydrates on one (RT11), so, mid-track,
/// `seed_remember`'s own store read sees no scope yet and starts
/// `remember` at `false`; the very next real track switch (still the
/// same on-disk entry, untouched by the restart) restores it correctly,
/// proving the entry itself survived the whole cycle.
#[test]
fn restart_after_suspend_adopts_nodes_without_set_param() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - -2.0).abs() <= EPS
    }));
    click(&mut controller, id, "remember", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
    }));

    let pitch_before = pitch_node_id(&mut controller);
    let stretch_before = stretch_node_id(&mut controller);

    controller.plugin_disable(id);
    assert!(wait_disabled(&mut controller, id), "plugin must disable");
    controller.plugin_enable(id);
    assert!(wait_active(&mut controller, id), "plugin must re-enable");

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            widget_value(c, id, "key") == Some(WidgetValue::Number(-2.0))
                && widget_value(c, id, "restored") == Some(WidgetValue::Text(String::new()))
        }),
        "re-enable must re-mirror -2 from the adopted nodes, with an empty badge"
    );
    assert_eq!(
        pitch_node_id(&mut controller),
        pitch_before,
        "re-enable must adopt the same pitch node, never recreate it"
    );
    assert_eq!(
        stretch_node_id(&mut controller),
        stretch_before,
        "re-enable must adopt the same stretch node, never recreate it"
    );
    assert!(
        (pitch_semitones(&mut controller) - -2.0).abs() <= EPS,
        "adoption must never issue its own set_param — the value must be exactly what it was"
    );

    // The entry itself was flushed to disk when the plugin disabled
    // (RT8's `run_unloading`); a real track switch is the fresh
    // instance's first `track_changed`, and reloads it correctly.
    controller.skip_forward();
    controller.tick();
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - 0.0).abs() <= EPS
    }));
    controller.skip_back();
    controller.tick();
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - -2.0).abs() <= EPS
                && widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
                && widget_value(c, id, "restored")
                    == Some(WidgetValue::Text(RESTORED_TEXT.to_string()))
        }),
        "the entry written before disabling must have survived the whole restart cycle"
    );
}

/// G2 case (b) (US3-8): re-enabling with neither own node present (the
/// user removed both while the plugin was disabled) creates fresh nodes
/// and runs the same reset/apply logic `track_changed` would. A fresh
/// script instance has no `state.track` scope loaded yet at that exact
/// moment (RT11: hydrated only by an actual `track_changed`), so — same
/// as the case-(a) restart — the immediate outcome is the same as "no
/// entry" (key/tempo at defaults); the entry itself, untouched by any
/// of this, applies correctly on the very next real track switch.
#[test]
fn enable_mid_track_applies_entry_like_track_changed() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a"), track("b")]);
    controller.play();
    controller.tick();

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - -2.0).abs() <= EPS
    }));
    click(&mut controller, id, "remember", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
    }));

    let pitch_before = pitch_node_id(&mut controller);
    let stretch_before = stretch_node_id(&mut controller);
    controller.plugin_disable(id);
    assert!(wait_disabled(&mut controller, id), "plugin must disable");

    controller
        .chain_remove_node(pitch_before)
        .unwrap_or_else(|e| unreachable!("chain_remove_node(pitch) must succeed: {e:?}"));
    controller
        .chain_remove_node(stretch_before)
        .unwrap_or_else(|e| unreachable!("chain_remove_node(stretch) must succeed: {e:?}"));
    assert_eq!(controller.chain().nodes().len(), 0);

    controller.plugin_enable(id);
    assert!(wait_active(&mut controller, id), "plugin must re-enable");

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            c.chain().nodes().len() >= 2
        }),
        "re-enable with neither own node present must create both nodes fresh"
    );
    assert_ne!(
        pitch_node_id(&mut controller),
        pitch_before,
        "the pitch node must be a genuinely fresh one, not the removed one"
    );
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - 0.0).abs() <= EPS && (stretch_ratio(c) - 1.0).abs() <= EPS
        }),
        "the fresh nodes must start at plain defaults"
    );

    // The remembered entry itself was never touched by any of the
    // above; the next real track switch applies it exactly like
    // `track_changed` always does.
    controller.skip_forward();
    controller.tick();
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - 0.0).abs() <= EPS
    }));
    controller.skip_back();
    controller.tick();
    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            (pitch_semitones(c) - -2.0).abs() <= EPS
                && widget_value(c, id, "remember") == Some(WidgetValue::Bool(true))
                && widget_value(c, id, "restored")
                    == Some(WidgetValue::Text(RESTORED_TEXT.to_string()))
        }),
        "the entry written before disabling must still apply once a real track_changed occurs"
    );
}

/// A11, L4 (removal while Active): the host may remove either node at
/// any time; the plugin never recreates it while Active, shows the
/// node-removed text, and the controls that depended on it become no-ops.
#[test]
fn removed_node_is_not_recreated_while_active() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();
    let pitch_id = pitch_node_id(&mut controller);

    controller
        .chain_remove_node(pitch_id)
        .unwrap_or_else(|e| unreachable!("chain_remove_node must succeed: {e:?}"));

    assert!(
        pump_until(&mut controller, Duration::from_secs(5), |c| {
            c.chain().nodes().len() == 1
                && widget_value(c, id, "restored")
                    == Some(WidgetValue::Text(NODE_REMOVED_TEXT.to_string()))
        }),
        "removing the pitch node must leave exactly 1 node and show the node-removed text"
    );

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    for _ in 0..10 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        controller.chain().nodes().len(),
        1,
        "a key control must be a no-op while its node is gone -- the node is never recreated while Active"
    );
}

/// L1 (US3): disable orphans both nodes in place -- same params, no
/// plugin code running, panel gone.
#[test]
fn disable_orphans_nodes_with_last_params() {
    let (mut controller, _dir, _psd, _tsd, id) = key_tempo_ready();

    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    click(&mut controller, id, "key_down", WidgetValue::Bool(true));
    assert!(pump_until(&mut controller, Duration::from_secs(5), |c| {
        (pitch_semitones(c) - -2.0).abs() <= EPS
    }));

    controller.plugin_disable(id);
    assert!(wait_disabled(&mut controller, id), "plugin must disable");

    let nodes = controller.chain().nodes();
    assert_eq!(nodes.len(), 2, "disable must not remove either node");
    assert!(
        nodes.iter().all(|n| n.orphaned),
        "both nodes must be orphaned"
    );
    assert!(
        (pitch_semitones(&mut controller) - -2.0).abs() <= EPS,
        "an orphaned node must keep its last parameters"
    );
    assert!(
        controller
            .plugins_mut()
            .ui()
            .panels()
            .for_plugin(id)
            .is_empty(),
        "a disabled plugin's panel must be gone"
    );
}

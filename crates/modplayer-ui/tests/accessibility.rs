// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T096 (Phase 7 Polish, FR-023): every control 003-streaming-playback-and-
//! queue adds — the full transport, the queue panel's rows/actions, the
//! Settings › Playback device-name field, the transfer banner's **Play
//! here** button, and stream-notification action buttons — exposes a
//! non-empty accessible name and the correct AccessKit role/state, driven
//! headlessly through `ScriptedHost` + `FakeBackend` exactly like
//! `now_playing.rs`/`queue_view.rs`'s own behavioural tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use std::time::{Duration, Instant};

use egui::accesskit::{Role, Toggled};
use egui::{Context, Event, Key, Modifiers, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{
    ArtistId, ArtistRef, Availability, CatalogError, LibraryItem, LibraryPage, LibrarySet, Repeat,
    SearchGroupPage, SearchHit, SearchKind, SearchPage, TrackId, TrackList, TrackListSource,
    TrackRef,
};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_audio_source_synthetic::scripted::HydratedReply;
use modplayer_core::actions::{Chord, HostAction, Platform};
use modplayer_core::markers::CueSlot;
use modplayer_core::plugins::{Lifecycle, PluginId};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{
    ActiveState, NotificationAction, NotificationCenter, PlaybackController, Severity, tr, tr_args,
};
use modplayer_effects::catalog::NodeKind;
use modplayer_engine::{BufferPreset, DeviceId, Event as EngineEvent, FrameCount, SampleRate};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::detail_view::{self, DetailTarget};
use modplayer_ui::library_view::{self, LibraryTab, LibraryViewState};
use modplayer_ui::settings::controls::{self, ControlsScreen};
use modplayer_ui::waveform::WaveformState;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-accessibility-{label}-{}-{unique}",
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

/// Serializes any test in this binary that briefly overrides the
/// process-global `MODPLAYER_TRACK_STATE_DIR` for `PlaybackController::
/// new`'s one synchronous read of it (Phase 7 polish, T089: 006 US2's
/// `sync_marker_attachment` now runs on every `dispatch`, so any test here
/// that queues a track would otherwise read/write the real per-user
/// track-state directory; mirrors `markers.rs`'s own lock/pattern, which
/// this file didn't need before 006).
static TRACK_STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Both temp dirs `active_controller` allocates (settings, track-state);
/// kept alive together so either can be dropped only once the test itself
/// is done with the controller.
struct TestDirs(#[allow(dead_code)] TempDir, #[allow(dead_code)] TempDir);

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

/// A controller over a confirmed device with playback permitted (mirrors
/// `now_playing.rs`'s `active_controller`), plus the `ScriptedHostHandle`
/// needed to script a transfer-away for the banner test.
fn active_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
    TestDirs,
) {
    let (store, dir) = fresh_store(label);
    let track_state_dir = TempDir::new(&format!("{label}-track-state"));
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device()];
    let mut controller = {
        let _guard = TRACK_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Safety: narrowly scopes the mutation to the one synchronous read
        // `PlaybackController::new` does of this var, serialized against
        // every other test in this binary via the lock above.
        unsafe { std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path()) };
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe { std::env::remove_var("MODPLAYER_TRACK_STATE_DIR") };
        controller
    };
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, handle, TestDirs(dir, track_state_dir))
}

fn default_input() -> RawInput {
    RawInput {
        // Tall enough that a 20-row search "Show more" page (T033) stays
        // fully within the clip rect — every other test here renders far
        // fewer widgets and is unaffected by the extra headroom.
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 2000.0))),
        ..Default::default()
    }
}

/// One AccessKit node's accessibility-relevant fields, gathered into an
/// owned snapshot so it outlives the frame's `accesskit_update`.
#[derive(Debug, Clone)]
struct AccessNode {
    role: Role,
    label: Option<String>,
    value: Option<String>,
    description: Option<String>,
    toggled: Option<Toggled>,
    disabled: bool,
    labelled_by_something: bool,
}

impl AccessNode {
    /// The text a screen reader would announce as this node's name: its
    /// `label` for every role but `Role::Label` (whose own text sits in
    /// `value` — `Response::fill_accesskit_node_from_widget_info`), falling
    /// back to `value` so a plain content-only node still counts.
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

/// Render `render` in a fresh headless, AccessKit-enabled context and
/// return every node it produced (mirrors `now_playing.rs`'s
/// `rendered_texts`, kept structured instead of flattened to text so role/
/// toggled/disabled state survive too).
fn render_nodes(render: impl FnMut(&mut egui::Ui)) -> Vec<AccessNode> {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(default_input(), render);
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
            description: node.description().map(str::to_string),
            toggled: node.toggled(),
            disabled: node.is_disabled(),
            labelled_by_something: !node.labelled_by().is_empty(),
        })
        .collect()
}

/// Every node of `role` whose accessible name equals `name`, most recently
/// rendered on top (`nodes` preserves AccessKit's own frame order, which is
/// good enough since these tests only ever assert existence/state, never
/// order — `queue_view.rs`'s own tests already cover row ordering).
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

#[test]
fn transport_controls_expose_accessible_names() {
    let (mut controller, _handle, _dir) = active_controller("transport");
    controller.queue_replace(vec![track("a")]);
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let nodes = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });

    // Play/pause, stop, skip back/forward, and the Queue toggle are plain
    // `Button`s whose accessible name is their own label text.
    let play = find_one(&nodes, Role::Button, &tr("transport-play"));
    assert!(!play.disabled, "transport must be enabled: {play:?}");
    find_one(&nodes, Role::Button, &tr("transport-stop"));
    find_one(&nodes, Role::Button, &tr("transport-skip-back"));
    find_one(&nodes, Role::Button, &tr("transport-skip-forward"));
    find_one(&nodes, Role::Button, &tr("queue-toggle"));

    // The seek slider is a real `Role::Slider` (not a generic control) and
    // carries its own non-empty name (`transport-position`'s
    // "{position} / {duration}"), not merely a bare number.
    let sliders: Vec<_> = nodes.iter().filter(|n| n.role == Role::Slider).collect();
    assert!(
        !sliders.is_empty(),
        "Now Playing must render at least one Role::Slider node (seek + volume)"
    );
    assert!(
        sliders
            .iter()
            .any(|n| n.accessible_name().is_some_and(|name| !name.is_empty())),
        "every slider must have a non-empty accessible name, got {sliders:?}"
    );
}

#[test]
fn transport_controls_are_disabled_when_playback_is_not_permitted() {
    let (store, _dir) = fresh_store("transport-disabled");
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    controller.launch();
    assert!(!controller.transport_enabled());
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let nodes = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    let play = find_one(&nodes, Role::Button, &tr("transport-play"));
    assert!(
        play.disabled,
        "the play button must report AccessKit's disabled state when transport is off: {play:?}"
    );
}

#[test]
fn queue_shuffle_toggle_reports_its_toggled_state() {
    let (mut controller, _handle, _dir) = active_controller("queue-shuffle-state");
    controller.queue_replace(vec![track("a"), track("b")]);

    let off = render_nodes(|ui| modplayer_ui::queue_view::show(ui, &mut controller));
    let shuffle = find_one(&off, Role::Button, &tr("queue-shuffle"));
    assert_eq!(
        shuffle.toggled,
        Some(Toggled::False),
        "shuffle off must report Toggled::False: {shuffle:?}"
    );
    // Off/One/All are the three fixed labels the repeat-cycle button shows
    // (queue_view.rs); it is a plain button, not a tri-state toggle, so its
    // *name itself* carries the current mode.
    find_one(&off, Role::Button, &tr("queue-repeat-off"));

    controller.set_shuffle(true);
    controller.set_repeat(Repeat::All);
    let on = render_nodes(|ui| modplayer_ui::queue_view::show(ui, &mut controller));
    let shuffle = find_one(&on, Role::Button, &tr("queue-shuffle"));
    assert_eq!(
        shuffle.toggled,
        Some(Toggled::True),
        "shuffle on must report Toggled::True: {shuffle:?}"
    );
    find_one(&on, Role::Button, &tr("queue-repeat-all"));
}

#[test]
fn queue_row_actions_expose_accessible_names() {
    let (mut controller, _handle, _dir) = active_controller("queue-row-actions");
    controller.queue_replace(vec![track("a"), track("b")]);

    let nodes = render_nodes(|ui| modplayer_ui::queue_view::show(ui, &mut controller));

    // Two rows -> at least two of each row action, each a real `Button`
    // whose name is exactly the action (never blank/icon-only).
    for key in [
        "queue-move-up",
        "queue-move-down",
        "queue-remove",
        "queue-play-next",
    ] {
        let matches = find_all(&nodes, Role::Button, &tr(key));
        assert!(
            !matches.is_empty(),
            "expected at least one `{key}` button, found none in {nodes:?}"
        );
    }
}

/// 008 Phase 4 (US2, FR-015, contracts/ui-effect-chain.md §2): the Effect
/// Chain panel's handle, bypass, remove, level slider and mode combo each
/// expose a non-empty accessible name, the correct role, and — for
/// bypass — the correct AccessKit toggled state.
#[test]
fn effect_chain_controls_expose_accessible_names_and_states() {
    let (mut controller, _handle, _dir) = active_controller("effect-chain-a11y");
    let gain = controller
        .chain_add_node(NodeKind::Gain)
        .unwrap_or_else(|_| unreachable!());
    controller
        .chain_add_node(NodeKind::PitchShift)
        .unwrap_or_else(|_| unreachable!());

    let nodes = render_nodes(|ui| modplayer_ui::effects_view::show(ui, &mut controller));

    assert_eq!(
        find_all(&nodes, Role::Button, &tr("effects-reorder-handle")).len(),
        2,
        "every row must have a named, focusable drag handle"
    );
    assert_eq!(
        find_all(&nodes, Role::Button, &tr("effects-remove")).len(),
        2,
        "every row must have a named remove button"
    );

    let bypass = find_all(&nodes, Role::Button, &tr("effects-bypass"));
    assert_eq!(bypass.len(), 2);
    assert!(
        bypass.iter().all(|b| b.toggled == Some(Toggled::False)),
        "bypass must start off for every row: {bypass:?}"
    );

    controller
        .chain_set_bypass(gain, true)
        .unwrap_or_else(|_| unreachable!());
    let nodes = render_nodes(|ui| modplayer_ui::effects_view::show(ui, &mut controller));
    let bypass = find_all(&nodes, Role::Button, &tr("effects-bypass"));
    assert!(
        bypass.iter().any(|b| b.toggled == Some(Toggled::True)),
        "a bypassed node's toggle must report Toggled::True: {bypass:?}"
    );

    // Parameter controls: Gain's level slider, Pitch shift's mode combo
    // (at its default `Performance` value).
    find_one(&nodes, Role::Slider, &tr("effects-param-level"));
    find_one(&nodes, Role::ComboBox, &tr("effects-mode-performance"));
}

/// 008 Phase 5 (US3, FR-015, contracts/ui-effect-chain.md §3): the full
/// six-kind catalog's remaining controls — the equalizer's 8 bands
/// (`DragValue` × 3 + type combo each), the filter's mode combo/cutoff/
/// resonance, and stereo tools' width/balance/mono-sum/phase-invert/swap
/// (including phase invert's disabled-unless-mono-sum state) — each
/// expose a non-empty accessible name/value and the correct role/state.
#[test]
fn eq_filter_stereo_controls_expose_accessible_names_and_states() {
    let (mut controller, _handle, _dir) = active_controller("eq-filter-stereo-a11y");
    controller
        .chain_add_node(NodeKind::Equalizer)
        .unwrap_or_else(|_| unreachable!());
    controller
        .chain_add_node(NodeKind::Filter)
        .unwrap_or_else(|_| unreachable!());
    let stereo = controller
        .chain_add_node(NodeKind::StereoTools)
        .unwrap_or_else(|_| unreachable!());

    let nodes = render_nodes(|ui| modplayer_ui::effects_view::show(ui, &mut controller));

    // Equalizer: 8 bands × (freq/gain/q `DragValue` + type `ComboBox`).
    let eq_drag_values = nodes
        .iter()
        .filter(|node| {
            node.role == Role::SpinButton
                && node
                    .value
                    .as_deref()
                    .is_some_and(|v| v.contains("Hz") || v.contains("dB"))
        })
        .count();
    assert!(
        eq_drag_values >= 8 * 2, // freq + gain per band at minimum (Q has no unit suffix to match on)
        "expected every EQ band's freq/gain DragValues to expose a non-empty value"
    );
    assert_eq!(
        find_all(&nodes, Role::ComboBox, &tr("effects-band-type-peak")).len(),
        8,
        "every band must default to the peak type combo"
    );

    // Filter: mode combo (default high-pass), cutoff DragValue, resonance
    // slider.
    find_one(&nodes, Role::ComboBox, &tr("effects-filter-high-pass"));
    find_one(&nodes, Role::Slider, &tr("effects-param-resonance"));

    // Stereo tools: width/balance sliders, mono-sum/channel-swap toggles,
    // phase-invert disabled while mono sum is off.
    find_one(&nodes, Role::Slider, &tr("effects-param-width"));
    find_one(&nodes, Role::Slider, &tr("effects-param-balance"));
    find_one(&nodes, Role::Button, &tr("effects-param-mono-sum"));
    find_one(&nodes, Role::Button, &tr("effects-param-channel-swap"));
    let phase_invert = find_one(&nodes, Role::Button, &tr("effects-param-phase-invert"));
    assert!(
        phase_invert.disabled,
        "phase invert must be disabled while mono sum is off: {phase_invert:?}"
    );

    controller
        .chain_set_param(stereo, modplayer_effects::catalog::ParamId(2), 1.0)
        .unwrap_or_else(|_| unreachable!());
    let nodes = render_nodes(|ui| modplayer_ui::effects_view::show(ui, &mut controller));
    let phase_invert = find_one(&nodes, Role::Button, &tr("effects-param-phase-invert"));
    assert!(
        !phase_invert.disabled,
        "phase invert must be enabled once mono sum is on: {phase_invert:?}"
    );
}

/// 008 Phase 6 (US4, FR-011/FR-012/FR-015, contracts/ui-effect-chain.md
/// §2/§6): the header's pre-/post-chain level pairs and 64-band spectrum
/// each expose a non-empty `Role::ProgressIndicator` accessible name, the
/// over-budget badge and overload counter follow `ChainView`, and a
/// row's "auto-bypassed" label appears only once that row's
/// `auto_bypassed` flag is set.
#[test]
fn effect_chain_meters_spectrum_and_overload_controls_expose_accessible_names() {
    let (mut controller, _handle, _dir) = active_controller("effect-chain-overload-a11y");
    let id = controller
        .chain_add_node(NodeKind::Gain)
        .unwrap_or_else(|_| unreachable!());
    let slot = controller
        .chain()
        .nodes()
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| unreachable!())
        .slot;

    // Baseline: no badge, a zero overload counter, no auto-bypassed label.
    let nodes = render_nodes(|ui| modplayer_ui::effects_view::show(ui, &mut controller));
    assert!(
        find_all(&nodes, Role::Label, &tr("effects-over-budget-badge")).is_empty(),
        "badge must not show while not over budget: {nodes:?}"
    );
    find_one(
        &nodes,
        Role::Label,
        &tr_args("effects-overloads", &[("count", "0".to_string())]),
    );
    assert!(
        find_all(&nodes, Role::Label, &tr("effects-auto-bypassed")).is_empty(),
        "auto-bypassed label must not show before any auto-bypass: {nodes:?}"
    );

    // Pre-/post-chain level pairs and the spectrum are always present, each
    // a real `Role::ProgressIndicator` with a non-empty, informative name.
    let pre = nodes
        .iter()
        .find(|n| {
            n.role == Role::ProgressIndicator
                && n.accessible_name()
                    .is_some_and(|name| name.starts_with(&tr("effects-pre")))
        })
        .unwrap_or_else(|| panic!("pre-chain level widget not found: {nodes:?}"));
    assert!(!pre.disabled, "{pre:?}");
    let post = nodes
        .iter()
        .find(|n| {
            n.role == Role::ProgressIndicator
                && n.accessible_name()
                    .is_some_and(|name| name.starts_with(&tr("effects-post")))
        })
        .unwrap_or_else(|| panic!("post-chain level widget not found: {nodes:?}"));
    assert!(!post.disabled, "{post:?}");
    let spectrum = nodes
        .iter()
        .find(|n| {
            n.role == Role::ProgressIndicator
                && n.accessible_name()
                    .is_some_and(|name| name.starts_with(&tr("effects-spectrum")))
        })
        .unwrap_or_else(|| panic!("spectrum widget not found: {nodes:?}"));
    assert!(!spectrum.disabled, "{spectrum:?}");

    // Drive the RtShared atomics an overload/auto-bypass would set and
    // confirm the badge, counter and per-row label follow.
    controller.shared().set_over_budget(true);
    controller.shared().set_overload_count(2);
    controller.debug_inject_engine_event(EngineEvent::AutoBypassed { slot });
    controller.tick();

    let nodes = render_nodes(|ui| modplayer_ui::effects_view::show(ui, &mut controller));
    find_one(&nodes, Role::Label, &tr("effects-over-budget-badge"));
    find_one(
        &nodes,
        Role::Label,
        &tr_args("effects-overloads", &[("count", "2".to_string())]),
    );
    find_one(&nodes, Role::Label, &tr("effects-auto-bypassed"));
}

#[test]
fn transfer_banner_play_here_button_exposes_its_accessible_name() {
    let (mut controller, handle, _dir) = active_controller("transfer-banner");
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();

    handle.transfer_out();
    controller.tick();
    assert!(
        matches!(controller.active_state(), ActiveState::Inactive { .. }),
        "test setup: expected Inactive, got {:?}",
        controller.active_state()
    );
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let nodes = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    let play_here = find_one(&nodes, Role::Button, &tr("banner-play-here"));
    assert!(
        !play_here.disabled,
        "Play here must stay enabled: {play_here:?}"
    );

    // The banner's own label text ("Playing on <device>", falling back to
    // `banner-unknown-device` when `ScriptedHostHandle::transfer_out` sends
    // no device name) must also be a real, non-empty accessible node —
    // never a colour/icon-only cue (Constitution/FR-022).
    assert!(
        nodes.iter().any(|n| n
            .accessible_name()
            .is_some_and(|name| name.contains(&tr("banner-unknown-device")))),
        "expected the \"Playing on <device>\" banner text, got {nodes:?}"
    );
}

#[test]
fn device_name_field_is_labelled_and_shows_the_current_name() {
    let (store, _dir) = fresh_store("device-name-field");
    let controller = PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    let mut screen = modplayer_ui::settings::playback::PlaybackScreen::new(&controller);
    let mut controller = controller;

    let nodes = render_nodes(|ui| {
        modplayer_ui::settings::playback::show(ui, &mut controller, &mut screen, None)
    });

    let text_inputs: Vec<_> = nodes.iter().filter(|n| n.role == Role::TextInput).collect();
    assert_eq!(
        text_inputs.len(),
        1,
        "expected exactly one Role::TextInput node, got {text_inputs:?}"
    );
    let field = text_inputs[0];
    assert!(
        field.labelled_by_something,
        "the device-name field must be associated with the \"{}\" label via labelled_by: {field:?}",
        tr("setting-device-name")
    );
    assert!(
        field.value.as_deref().is_some_and(|v| !v.is_empty()),
        "the device-name field must show the current effective name as its value: {field:?}"
    );
}

#[test]
fn notification_action_buttons_expose_accessible_names() {
    let mut center = NotificationCenter::new();
    center.raise_with_actions(
        Severity::Critical,
        "stream-source-unavailable",
        Vec::new(),
        vec![
            NotificationAction::OpenStatusPage,
            NotificationAction::RetrySource,
        ],
    );

    let nodes = render_nodes(|ui| {
        let _ = modplayer_ui::notifications::show(ui, &center);
    });

    find_one(&nodes, Role::Button, &tr("action-status-page"));
    find_one(&nodes, Role::Button, &tr("action-retry"));
    find_one(&nodes, Role::Button, &tr("notification-dismiss"));
}

// -- Search (004-search-and-library-browse, T033) ---------------------------

fn search_track_hit(id: &str, title: &str) -> SearchHit {
    SearchHit::Track(TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap(),
        title,
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    ))
}

/// A full four-group reply with `track_count` tracks and `next_offset` set
/// on the Tracks group whenever it is a full page (so "Show more" renders
/// too) — the other three groups empty.
fn search_reply(track_count: usize) -> SearchPage {
    let items: Vec<SearchHit> = (0..track_count)
        .map(|i| search_track_hit(&i.to_string(), &format!("Track {i}")))
        .collect();
    let next_offset = if track_count >= 20 {
        Some(track_count as u32)
    } else {
        None
    };
    SearchPage {
        groups: vec![
            SearchGroupPage {
                kind: SearchKind::Track,
                items,
                next_offset,
            },
            SearchGroupPage {
                kind: SearchKind::Album,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Artist,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Playlist,
                items: vec![],
                next_offset: None,
            },
        ],
        unsupported: vec![],
    }
}

/// Drive one query to a `Loaded` Tracks group with `track_count` items,
/// scripted through `handle`.
fn search_and_settle(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    handle: &modplayer_audio_source_synthetic::ScriptedHostHandle,
    query: &str,
    track_count: usize,
) {
    handle.script_search(query, Ok(search_reply(track_count)));
    let now = controller.now();
    controller.search_mut().set_query(query, now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick(); // issues the request
    controller.tick(); // drains the reply
}

#[test]
fn search_box_exposes_a_text_input_labelled_search() {
    let (mut controller, _handle, _dir) = active_controller("search-box");
    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut focus = false;

    let nodes = render_nodes(|ui| {
        modplayer_ui::search_view::show(ui, &mut controller, &mut artwork, &mut focus);
    });

    let text_inputs: Vec<_> = nodes.iter().filter(|n| n.role == Role::TextInput).collect();
    assert_eq!(
        text_inputs.len(),
        1,
        "expected exactly one Role::TextInput node, got {text_inputs:?}"
    );
    assert!(
        text_inputs[0].labelled_by_something,
        "the search box must be associated with its \"{}\" label: {:?}",
        tr("search-placeholder"),
        text_inputs[0]
    );
}

#[test]
fn group_headers_expose_role_header_and_the_fixed_names() {
    let (mut controller, handle, _dir) = active_controller("search-headers");
    search_and_settle(&mut controller, &handle, "abba", 2);

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut focus = false;
    let nodes = render_nodes(|ui| {
        modplayer_ui::search_view::show(ui, &mut controller, &mut artwork, &mut focus);
    });

    find_one(&nodes, Role::Header, &tr("search-group-tracks"));
    // Empty groups (Albums/Artists/Playlists) must not render a header at
    // all (contracts/ui-surface.md §2: "omitted when Empty/Unsupported/
    // Idle").
    assert!(find_all(&nodes, Role::Header, &tr("search-group-albums")).is_empty());
    assert!(find_all(&nodes, Role::Header, &tr("search-group-artists")).is_empty());
    assert!(find_all(&nodes, Role::Header, &tr("search-group-playlists")).is_empty());
}

#[test]
fn show_more_button_exposes_its_accessible_name_when_a_further_page_exists() {
    let (mut controller, handle, _dir) = active_controller("search-show-more");
    search_and_settle(&mut controller, &handle, "abba", 20);

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut focus = false;
    let nodes = render_nodes(|ui| {
        modplayer_ui::search_view::show(ui, &mut controller, &mut artwork, &mut focus);
    });

    let expected = tr_args("search-show-more", &[("group", tr("search-group-tracks"))]);
    let button = find_one(&nodes, Role::Button, &expected);
    assert!(!button.disabled, "Show more must start enabled: {button:?}");
}

#[test]
fn each_row_exposes_a_list_item_and_an_actions_button() {
    let (mut controller, handle, _dir) = active_controller("search-row-menu");
    search_and_settle(&mut controller, &handle, "abba", 1);

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut focus = false;
    let nodes = render_nodes(|ui| {
        modplayer_ui::search_view::show(ui, &mut controller, &mut artwork, &mut focus);
    });

    let row_name = "Track 0 — Artist";
    find_one(&nodes, Role::ListItem, row_name);
    // The trailing "…" button that opens the six-action menu
    // (contracts/ui-surface.md §5).
    find_one(
        &nodes,
        Role::Button,
        &tr_args("row-actions", &[("name", row_name.to_string())]),
    );
}

// -- Library & detail (004-search-and-library-browse, T070) -----------------
//
// Search's own three tests above already pin the row/menu contract that
// `rows::list_row` renders identically everywhere (FR-004) — this section
// covers the elements unique to the Library and detail surfaces: the tab
// row, the "Refreshing…" status label reachable only through an actual
// resync (unlike Search's, driven straight off `SearchSession`), and the
// detail view's Back control.

fn library_track(id: &str, title: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(id).unwrap(),
        title,
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

fn empty_page(set: LibrarySet) -> LibraryPage {
    LibraryPage {
        set,
        items: vec![],
        next_page: None,
        sync_token: None,
    }
}

/// Script and settle a one-track Saved Tracks sync (the other three sets
/// empty), so the Library view renders real content rather than an empty
/// or loading state.
fn sync_one_saved_track(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    handle: &modplayer_audio_source_synthetic::ScriptedHostHandle,
) {
    let track = library_track("spotify:track:a", "Track A");
    handle.script_hydrate(HydratedReply {
        tracks: vec![track.clone()],
        ..Default::default()
    });
    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Ok(LibraryPage {
            set: LibrarySet::SavedTracks,
            items: vec![LibraryItem::Track {
                track,
                added_at: None,
            }],
            next_page: None,
            sync_token: None,
        })],
    );
    handle.script_library(
        LibrarySet::SavedAlbums,
        vec![Ok(empty_page(LibrarySet::SavedAlbums))],
    );
    handle.script_library(
        LibrarySet::FollowedArtists,
        vec![Ok(empty_page(LibrarySet::FollowedArtists))],
    );
    handle.script_library(
        LibrarySet::Playlists,
        vec![Ok(empty_page(LibrarySet::Playlists))],
    );
    controller.library_retry_sync();
    for _ in 0..12 {
        controller.tick();
    }
}

#[test]
fn library_tabs_expose_role_tab_in_the_fixed_order() {
    let (mut controller, _handle, _dir) = active_controller("library-tabs");
    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut state = LibraryViewState::default();

    let nodes = render_nodes(|ui| {
        let _ = library_view::show(ui, &mut controller, &mut artwork, &mut state);
    });

    for key in [
        "library-tab-saved-tracks",
        "library-tab-saved-albums",
        "library-tab-followed-artists",
        "library-tab-playlists",
        "library-tab-recently-played",
    ] {
        find_one(&nodes, Role::Tab, &tr(key));
    }
}

#[test]
fn library_row_exposes_a_list_item_and_an_actions_button() {
    let (mut controller, handle, _dir) = active_controller("library-row-menu");
    sync_one_saved_track(&mut controller, &handle);

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::SavedTracks,
        ..LibraryViewState::default()
    };
    let nodes = render_nodes(|ui| {
        let _ = library_view::show(ui, &mut controller, &mut artwork, &mut state);
    });

    let row_name = "Track A — Artist";
    find_one(&nodes, Role::ListItem, row_name);
    find_one(
        &nodes,
        Role::Button,
        &tr_args("row-actions", &[("name", row_name.to_string())]),
    );
}

#[test]
fn refreshing_status_label_exposes_role_status_after_a_rate_limited_resync() {
    let (mut controller, handle, _dir) = active_controller("library-refreshing");
    sync_one_saved_track(&mut controller, &handle);
    assert!(
        !controller.library_status().refreshing,
        "test setup: a clean sync must not start out refreshing"
    );

    // A forced resync whose Saved Tracks page comes back rate-limited
    // (contracts/library-and-search-core.md §3 "Syncing --RateLimited-->
    // BackingOff") drives `library_status().refreshing` true while the
    // existing snapshot stays on screen (FR-015/SC-005).
    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Err(CatalogError::RateLimited {
            retry_after_ms: None,
        })],
    );
    controller.library_retry_sync();
    controller.tick();
    assert!(
        controller.library_status().refreshing,
        "test setup: the scripted rate limit must trip the scheduler's backoff"
    );

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut state = LibraryViewState {
        tab: LibraryTab::SavedTracks,
        ..LibraryViewState::default()
    };
    let nodes = render_nodes(|ui| {
        let _ = library_view::show(ui, &mut controller, &mut artwork, &mut state);
    });

    let status = find_one(&nodes, Role::Status, &tr("refreshing"));
    assert!(!status.disabled, "{status:?}");
    // The prior snapshot must stay visible underneath the status label
    // (FR-015: "stale + Refreshing…", never an empty/error state).
    find_one(&nodes, Role::ListItem, "Track A — Artist");
}

#[test]
fn detail_back_button_exposes_its_accessible_name() {
    let (mut controller, handle, _dir) = active_controller("detail-back-a11y");
    let id = ArtistId::new("spotify:artist:a").unwrap();
    let artist = ArtistRef {
        id: id.clone(),
        name: "The Artist".to_string(),
        artwork_url: None,
    };
    handle.script_hydrate(HydratedReply {
        artists: vec![artist.clone()],
        ..Default::default()
    });
    handle.script_library(
        LibrarySet::FollowedArtists,
        vec![Ok(LibraryPage {
            set: LibrarySet::FollowedArtists,
            items: vec![LibraryItem::Artist(artist)],
            next_page: None,
            sync_token: None,
        })],
    );
    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Ok(empty_page(LibrarySet::SavedTracks))],
    );
    handle.script_library(
        LibrarySet::SavedAlbums,
        vec![Ok(empty_page(LibrarySet::SavedAlbums))],
    );
    handle.script_library(
        LibrarySet::Playlists,
        vec![Ok(empty_page(LibrarySet::Playlists))],
    );
    controller.library_retry_sync();
    for _ in 0..12 {
        controller.tick();
    }
    handle.script_track_list(
        TrackListSource::ArtistTop(id.clone()),
        Ok(TrackList {
            source: TrackListSource::ArtistTop(id.clone()),
            tracks: vec![],
        }),
    );

    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let target = DetailTarget::Artist(id);
    let nodes = render_nodes(|ui| {
        let _ = detail_view::show(ui, &mut controller, &mut artwork, &target);
    });
    // First frame only issues `FetchTrackList`; settle it before asserting.
    controller.tick();
    let nodes2 = render_nodes(|ui| {
        let _ = detail_view::show(ui, &mut controller, &mut artwork, &target);
    });

    for nodes in [&nodes, &nodes2] {
        let back = find_one(nodes, Role::Button, &tr("detail-back"));
        assert!(!back.disabled, "{back:?}");
    }
}

// 005-now-playing-waveform (T037, contracts/ui-waveform.md §1/§7): the
// waveform overview is a real `Role::Slider` named `transport-seek` with
// value text `m:ss / m:ss`; the empty state exposes `now-playing-pick-a-
// track`; 003's `transport-position` key is gone for good.

#[test]
fn waveform_overview_is_a_named_slider_with_mmss_value_text() {
    let (mut controller, _handle, _dir) = active_controller("waveform-overview-a11y");
    controller.queue_replace(vec![track("a")]);
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let nodes = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });

    let overview = find_one(&nodes, Role::Slider, &tr("transport-seek"));
    assert!(!overview.disabled, "{overview:?}");
    let value = overview
        .value
        .as_deref()
        .expect("the overview slider must carry an `m:ss / m:ss` value text");
    assert!(
        value.contains('/'),
        "expected `m:ss / m:ss`, got `{value}`: {overview:?}"
    );
}

#[test]
fn empty_state_exposes_pick_a_track_and_no_waveform_slider() {
    let (mut controller, _handle, _dir) = active_controller("waveform-empty-a11y");
    // No `queue_replace`: `current_track()` stays `None`.
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let nodes = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });

    assert!(
        nodes
            .iter()
            .any(|n| n.accessible_name() == Some(tr("now-playing-pick-a-track").as_str())),
        "expected the pick-a-track hint, got {nodes:?}"
    );
    assert!(
        find_all(&nodes, Role::Slider, &tr("transport-seek")).is_empty(),
        "no waveform overview must render with no current track: {nodes:?}"
    );
}

// 005-now-playing-waveform (T047, US2, contracts/ui-waveform.md §1/§3):
// the waveform detail view is a real `Role::Slider` named `waveform-detail`
// whose description carries its current window's `m:ss` bounds, and every
// key of §3 (including the US2 zoom/pan/reset rows) is reachable while a
// waveform has focus.

#[test]
fn waveform_detail_is_a_named_slider_with_windowed_description() {
    let (mut controller, _handle, _dir) = active_controller("waveform-detail-a11y");
    controller.queue_replace(vec![track("a")]); // 180s — long enough to hold a 30s window off both ends

    // Land the playhead at exactly 1:25 so a never-zoomed (30s, centred)
    // detail window's bounds are the round `1:10`/`1:40` the description
    // must contain (data-model.md §5.1's `initial`).
    controller.play();
    controller.tick();
    let sample_rate = controller.source_sample_rate();
    controller.seek_frames(85 * u64::from(sample_rate));
    controller.tick();
    // One render lets the engine apply the queued `Command::Seek` and
    // publish the new position anchor (Constitution I: commands apply at
    // buffer boundaries) — mirrors `now_playing.rs`'s own
    // `position_readout_updates_as_playback_advances`.
    let _ = controller.backend_mut().render_buffers(1);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let nodes = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });

    let detail = find_one(&nodes, Role::Slider, &tr("waveform-detail"));
    assert!(!detail.disabled, "{detail:?}");
    let description = detail
        .description
        .as_deref()
        .expect("the detail slider must carry a windowed description");
    assert!(
        description.contains("1:10") && description.contains("1:40"),
        "expected the 30s window bounds around 1:25 (`1:10`..`1:40`), got `{description}`"
    );
}

#[test]
fn every_waveform_key_in_the_contract_table_is_reachable() {
    let (mut controller, handle, _dir) = active_controller("waveform-keys-a11y");
    controller.queue_replace(vec![track("a")]);
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    // Tab to the overview (egui's default per-frame focus-advance model —
    // several `Tab`s within one frame's events would only count as one, so
    // this sends exactly one per frame, like `now_playing.rs`'s own
    // `tab_focus_named`).
    let overview_name = tr("transport-seek");
    let mut focused = false;
    for _ in 0..40 {
        let mut input = default_input();
        input.events.push(Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
        });
        let mut output = ctx.run_ui(input, |ui| {
            modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .expect("accesskit_update should be populated once enabled");
        output.drop_without_applying_deltas();
        if update
            .nodes
            .iter()
            .any(|(id, node)| *id == update.focus && node.label() == Some(overview_name.as_str()))
        {
            focused = true;
            break;
        }
    }
    assert!(focused, "Tab must be able to reach the waveform overview");

    let mut press = |key: Key, modifiers: Modifiers| {
        let mut input = default_input();
        // `Event::Key`'s own `modifiers` field doesn't reach
        // `InputState::modifiers` on its own — only `ModifiersChanged`
        // does (egui 0.36's `begin_pass`).
        input.events.push(Event::ModifiersChanged(modifiers));
        input.events.push(Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
        let output = ctx.run_ui(input, |ui| {
            modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
        });
        output.drop_without_applying_deltas();
    };

    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
    for (key, modifiers) in [
        (Key::ArrowRight, Modifiers::default()),
        (Key::ArrowLeft, Modifiers::default()),
        (Key::ArrowRight, Modifiers::SHIFT),
        (Key::ArrowLeft, Modifiers::SHIFT),
        (Key::Home, Modifiers::default()),
        (Key::End, Modifiers::default()),
        (Key::Plus, Modifiers::default()),
        (Key::Minus, Modifiers::default()),
        (Key::Num0, Modifiers::default()),
        (Key::ArrowRight, Modifiers::ALT),
        (Key::ArrowLeft, Modifiers::ALT),
        (Key::ArrowRight, alt_shift),
        (Key::ArrowLeft, alt_shift),
    ] {
        press(key, modifiers);
    }

    // Every row above must have reached the waveform (not been swallowed
    // or misrouted): the seek rows produced at least one
    // `SourceCommand::Seek`, and the detail window — touched by every
    // zoom/pan/reset row — is still a valid, non-empty window.
    assert!(
        !handle.record_commands().is_empty(),
        "the seek rows (←/→/Shift+←/→/Home/End) must have reached the source as \
         `SourceCommand::Seek`"
    );
    let detail = waveform
        .detail
        .expect("the detail window must still exist after the zoom/pan/reset rows");
    assert!(detail.width_frames > 0, "{detail:?}");
}

#[test]
fn transport_position_key_no_longer_resolves_or_exists() {
    let resolved = tr("transport-position");
    assert_eq!(
        &resolved, "transport-position",
        "`transport-position` still resolves to a real string — it was expected \
         removed once the waveform overview replaced the seek slider (005-now-\
         playing-waveform)"
    );

    let playback_ftl = include_str!("../../../locales/en-US/playback.ftl");
    assert!(
        !playback_ftl
            .lines()
            .any(|line| line.trim().starts_with("transport-position ")),
        "`transport-position` is still defined in playback.ftl — remove it with the seek slider"
    );
}

// -- Markers, loop regions and cues (006, T089, contracts/ui-markers.md) ---
//
// Every marker glyph is `Role::Button` named per `marker-glyph`, panel rows
// are `Role::ListItem`, the arm toggle is `Role::CheckBox` with state,
// numeric fields (repeat/crossfade/nudge-step) are labelled, the panel and
// empty state are exposed, and every key in contracts/ui-markers.md §2–§3
// is reachable (FR-022). `active_controller`'s track-state isolation above
// covers every test in this section, since each one queues a track.

fn key_event(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Run one Now Playing frame with `key`(+`modifiers`) pressed, on the same
/// `ctx` across calls so egui's own focus/input bookkeeping persists
/// between them (mirrors `markers.rs`'s own `press_key`/`press_key_with`).
fn press_marker_key(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    key: Key,
    modifiers: Modifiers,
) {
    let mut input = default_input();
    // `Event::Key`'s own `modifiers` field alone doesn't reach
    // `InputState::modifiers` — only a dedicated `ModifiersChanged` event
    // does (egui 0.36's `begin_pass`; same note as `now_playing.rs`'s own
    // `every_waveform_key_in_the_contract_table_is_reachable`).
    input.events.push(Event::ModifiersChanged(modifiers));
    input.events.push(key_event(key, modifiers));
    // A release in the same frame: egui's own `InputState::begin_pass`
    // derives `repeat` from whether `key` is still in its `keys_down` set
    // (it ignores the `repeat` field an integration sends), so without
    // this the *next* press of the same physical key (e.g. `Num1` for
    // both `Shift+1` and plain `1`) would arrive auto-marked
    // `repeat: true` and FR-019 would correctly, but here unwantedly,
    // drop it for a non-`repeats_while_held` action.
    input.events.push(Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers,
    });
    let output = ctx.run_ui(input, |ui| {
        // 007, contracts/ui-actions.md §1: run the dispatcher exactly as
        // `App::ui` does, since the view-level marker/loop/cue shortcuts
        // and the focused-marker nudge keys are now the catalog, not
        // `now_playing::show`'s own (removed) handlers.
        let claims = modplayer_ui::actions::claims_snapshot(&ui.ctx().clone());
        modplayer_ui::actions::clear_claims(&ui.ctx().clone());
        let scope = modplayer_core::actions::ScopeState {
            now_playing_shown: true,
            marker_focused: waveform.focused_marker.is_some(),
        };
        let mut shell = modplayer_ui::Shell::default();
        modplayer_ui::actions::dispatch_and_invoke(
            &ui.ctx().clone(),
            &claims,
            &scope,
            controller,
            &mut shell,
            waveform,
        );
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform)
    });
    output.drop_without_applying_deltas();
}

/// A playing controller with one queued track, the baseline every test in
/// this section starts from (markers only exist for a current track).
fn marker_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TestDirs,
    ArtworkCache,
    WaveformState,
) {
    let (mut controller, _handle, dirs) = active_controller(label);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    (
        controller,
        dirs,
        ArtworkCache::new(),
        WaveformState::default(),
    )
}

#[test]
fn every_marker_kind_glyph_exposes_role_button_named_marker_glyph() {
    let (mut controller, _dirs, mut artwork, mut waveform) =
        marker_controller("marker-glyph-roles");

    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    controller
        .set_cue(CueSlot::new(1).unwrap_or_else(|| unreachable!()))
        .unwrap_or_else(|e| unreachable!("set_cue: {e}"));

    let nodes = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });

    // At least one `Role::Button` glyph per marker (one per lane: the
    // overview and detail marker lanes both render every visible marker,
    // contracts/ui-markers.md §1), its name containing the `marker-glyph`
    // template's role segment (contracts/ui-markers.md §3 — Fluent wraps
    // each interpolated segment in bidi-isolate marks, so `contains` is
    // used rather than an exact prefix match; the trailing `{ $name } {
    // $time }` isn't pinned here since exact position/name aren't this
    // test's concern — markers.rs already pins those).
    for role_text in [
        tr("marker-role-a"),
        tr("marker-role-b"),
        tr("marker-role-point"),
        tr_args("marker-role-cue", &[("slot", "1".to_string())]),
    ] {
        let matches: Vec<_> = nodes
            .iter()
            .filter(|n| {
                n.role == Role::Button
                    && n.accessible_name()
                        .is_some_and(|name| name.contains(&role_text))
            })
            .collect();
        assert!(
            !matches.is_empty(),
            "expected at least one Role::Button glyph named `marker-glyph` containing `{role_text}`, found none: {nodes:?}"
        );
    }
}

#[test]
fn panel_row_exposes_role_list_item() {
    let (mut controller, _dirs, mut artwork, mut waveform) = marker_controller("panel-row-role");
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let nodes = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });

    let rows: Vec<_> = nodes.iter().filter(|n| n.role == Role::ListItem).collect();
    assert!(
        !rows.is_empty(),
        "expected at least one Role::ListItem panel row, found none: {nodes:?}"
    );
    assert!(
        rows.iter().any(|n| n
            .accessible_name()
            .is_some_and(|name| name.contains(&tr("marker-role-point")))),
        "expected the point marker's row to be a named Role::ListItem, got {rows:?}"
    );
}

#[test]
fn arm_toggle_exposes_role_checkbox_and_reports_toggled_state() {
    let (mut controller, _dirs, mut artwork, mut waveform) = marker_controller("arm-checkbox");
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let region = controller
        .markers()
        .and_then(modplayer_core::markers::TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!("region must exist"));

    let off = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    let checkbox = find_one(&off, Role::CheckBox, &tr("loop-arm"));
    assert_eq!(
        checkbox.toggled,
        Some(Toggled::False),
        "disarmed must report Toggled::False: {checkbox:?}"
    );

    controller
        .arm_loop(region)
        .unwrap_or_else(|e| unreachable!("arm_loop: {e}"));
    let on = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    let checkbox = find_one(&on, Role::CheckBox, &tr("loop-disarm"));
    assert_eq!(
        checkbox.toggled,
        Some(Toggled::True),
        "armed must report Toggled::True (and relabel to `loop-disarm`): {checkbox:?}"
    );
}

#[test]
fn loop_numeric_fields_and_nudge_step_are_labelled() {
    let (mut controller, _dirs, mut artwork, mut waveform) = marker_controller("numeric-labels");
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));

    let nodes = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    let spin_fields: Vec<_> = nodes
        .iter()
        .filter(|n| n.role == Role::SpinButton)
        .collect();
    assert!(
        spin_fields.len() >= 2,
        "expected the repeat and crossfade DragValues (Role::SpinButton), got {nodes:?}"
    );
    assert!(
        spin_fields.iter().all(|n| n.labelled_by_something),
        "every loop numeric field must be associated with its label via labelled_by: {spin_fields:?}"
    );

    // `setting-nudge-step` (Settings › Playback, contracts/ui-markers.md
    // §7) is labelled the same way — pinned here too since it shares this
    // section's "numeric fields ... are labelled" requirement.
    let (store, _dir) = fresh_store("nudge-step-labelled");
    let playback_controller =
        PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    let mut screen = modplayer_ui::settings::playback::PlaybackScreen::new(&playback_controller);
    let mut playback_controller = playback_controller;
    let settings_nodes = render_nodes(|ui| {
        modplayer_ui::settings::playback::show(ui, &mut playback_controller, &mut screen, None)
    });
    let nudge_field = settings_nodes
        .iter()
        .find(|n| n.role == Role::SpinButton)
        .expect("the nudge-step DragValue must render");
    assert!(
        nudge_field.labelled_by_something,
        "`setting-nudge-step`'s field must be associated with its label: {nudge_field:?}"
    );
}

#[test]
fn markers_panel_and_empty_state_are_exposed() {
    let (mut controller, _dirs, mut artwork, mut waveform) = marker_controller("panel-exposed");

    let empty = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    assert!(
        empty
            .iter()
            .any(|n| n.accessible_name() == Some(tr("markers-panel").as_str())),
        "the Markers panel heading (`markers-panel`) must always be exposed, got {empty:?}"
    );
    assert!(
        empty
            .iter()
            .any(|n| n.accessible_name() == Some(tr("markers-empty").as_str())),
        "the empty state (`markers-empty`) must be exposed with no markers, got {empty:?}"
    );

    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let with_marker = render_nodes(|ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    assert!(
        with_marker
            .iter()
            .any(|n| n.accessible_name() == Some(tr("markers-panel").as_str())),
        "the panel heading must still be exposed once markers exist, got {with_marker:?}"
    );
    assert!(
        !with_marker
            .iter()
            .any(|n| n.accessible_name() == Some(tr("markers-empty").as_str())),
        "the empty state must not render once a marker exists, got {with_marker:?}"
    );
}

#[test]
fn every_view_level_marker_shortcut_key_is_reachable() {
    let (mut controller, _dirs, mut artwork, mut waveform) =
        marker_controller("view-shortcuts-reachable");
    let ctx = Context::default();
    ctx.enable_accesskit();

    // `I`/`O`: create and complete the current region.
    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::I,
        Modifiers::NONE,
    );
    assert!(
        controller
            .markers()
            .and_then(modplayer_core::markers::TrackMarkers::current_region)
            .is_some(),
        "`I` must reach set_loop_a and create a region"
    );
    let _ = controller.backend_mut().render_buffers(1);
    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::O,
        Modifiers::NONE,
    );
    let region = controller
        .markers()
        .and_then(modplayer_core::markers::TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!("region must exist"));
    let complete_before_l = controller
        .markers()
        .and_then(|m| m.region(region))
        .is_some_and(modplayer_core::markers::LoopRegion::is_complete);
    assert!(
        complete_before_l,
        "`O` must reach set_loop_b and complete the region"
    );

    // `L`: toggle the current region's armed state.
    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::L,
        Modifiers::NONE,
    );
    let armed = controller
        .markers()
        .and_then(|m| m.region(region))
        .is_some_and(|r| r.armed);
    assert!(
        armed,
        "`L` must reach toggle_current_loop and arm the complete region"
    );

    // `M`: add a point marker.
    let count_before_m = controller
        .markers()
        .map(modplayer_core::markers::TrackMarkers::count)
        .unwrap_or(0);
    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::M,
        Modifiers::NONE,
    );
    let count_after_m = controller
        .markers()
        .map(modplayer_core::markers::TrackMarkers::count)
        .unwrap_or(0);
    assert_eq!(
        count_after_m,
        count_before_m + 1,
        "`M` must reach add_point_marker"
    );

    // `Shift+1`: set cue 1. `1`: jump to cue 1 (never a refusal/no-op here
    // since the slot is now occupied).
    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
        Modifiers::SHIFT,
    );
    let cue_slot = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    assert!(
        controller
            .markers()
            .is_some_and(|m| m.cue(cue_slot).is_some()),
        "`Shift+1` must reach set_cue and occupy slot 1"
    );
    let intent_before_jump = controller.transport_state().intent;
    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
        Modifiers::NONE,
    );
    assert_eq!(
        controller.transport_state().intent,
        intent_before_jump,
        "`1` must reach jump_to_cue without changing play/pause state"
    );
}

#[test]
fn every_focused_marker_key_is_reachable() {
    let (mut controller, _dirs, mut artwork, mut waveform) =
        marker_controller("focused-keys-reachable");
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    waveform.focused_marker = Some(id);
    let ctx = Context::default();
    ctx.enable_accesskit();

    let start = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());
    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::ArrowRight,
        Modifiers::NONE,
    );
    let after_nudge = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());
    assert!(
        after_nudge > start,
        "`→` must reach nudge_marker and move it forward"
    );

    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::C,
        Modifiers::NONE,
    );
    // `cycle_marker_color` always succeeds on an existing marker — reaching
    // it is what matters here (the exact palette cycle is markers.rs's).
    assert!(
        controller.markers().and_then(|m| m.marker(id)).is_some(),
        "`C` must reach cycle_marker_color without erroring"
    );

    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::F2,
        Modifiers::NONE,
    );
    assert_eq!(
        waveform.rename,
        Some((
            id,
            controller
                .markers()
                .and_then(|m| m.marker(id))
                .unwrap_or_else(|| unreachable!())
                .name
                .clone()
        )),
        "`F2` must reach the inline-rename open action"
    );

    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Escape,
        Modifiers::NONE,
    );
    assert_eq!(waveform.rename, None, "`Esc` must cancel the open rename");

    press_marker_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Delete,
        Modifiers::NONE,
    );
    assert!(
        controller.markers().and_then(|m| m.marker(id)).is_none(),
        "`Delete` must reach delete_marker"
    );
    assert_eq!(
        waveform.focused_marker, None,
        "`Delete` must also return focus to the detail waveform"
    );
}

// -- 007-keyboard-actions-and-shortcuts, T067: Settings › Controls --------

/// A bare controller with no device/track state — `settings::controls`
/// only ever reads/mutates `controller.actions()` (mirrors `tests/
/// controls.rs`'s own `fresh_controller`, minus the accesskit-specific
/// bits already provided by this file's `render_nodes`).
fn fresh_bare_controller(label: &str) -> (PlaybackController<FakeBackend, ScriptedHost>, TempDir) {
    let (store, dir) = fresh_store(label);
    let controller = PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    (controller, dir)
}

fn platform_now() -> Platform {
    if Context::default().os().is_mac() {
        Platform::Mac
    } else {
        Platform::Other
    }
}

#[test]
fn controls_filter_box_is_labelled() {
    let (mut controller, _dir) = fresh_bare_controller("controls-filter-a11y");
    let mut screen = ControlsScreen::default();
    let nodes = render_nodes(|ui| controls::show(ui, &mut controller, &mut screen, None));

    let text_inputs: Vec<_> = nodes.iter().filter(|n| n.role == Role::TextInput).collect();
    assert_eq!(
        text_inputs.len(),
        1,
        "expected exactly one Role::TextInput node, got {text_inputs:?}"
    );
    assert!(
        text_inputs[0].labelled_by_something,
        "the filter box must be associated with the \"{}\" label via labelled_by: {:?}",
        tr("controls-filter"),
        text_inputs[0]
    );
}

#[test]
fn controls_reset_all_and_row_controls_expose_accessible_names_and_stay_enabled() {
    let (mut controller, _dir) = fresh_bare_controller("controls-row-a11y");
    let mut screen = ControlsScreen::default();
    let nodes = render_nodes(|ui| controls::show(ui, &mut controller, &mut screen, None));

    let reset_all = find_one(&nodes, Role::Button, &tr("controls-reset-all"));
    assert!(!reset_all.disabled);

    assert!(
        !find_all(&nodes, Role::Button, &tr("controls-add-binding")).is_empty(),
        "every row must expose an \"Add binding\" button"
    );

    let action = HostAction::NavLibrary;
    let reset_name = tr_args(
        "controls-reset-action",
        &[("action", tr(action.label_key()))],
    );
    let reset = find_one(&nodes, Role::Button, &reset_name);
    assert!(!reset.disabled);
}

#[test]
fn controls_binding_chip_and_remove_button_expose_accessible_names() {
    let (mut controller, _dir) = fresh_bare_controller("controls-chip-a11y");
    let mut screen = ControlsScreen::default();
    let nodes = render_nodes(|ui| controls::show(ui, &mut controller, &mut screen, None));

    let chord = Chord::parse("L").unwrap_or_else(|_| unreachable!()); // ToggleLoop's default.
    let display = chord.display(platform_now());
    let chip_text = tr_args("controls-binding-chip", &[("binding", display.clone())]);
    assert!(
        nodes
            .iter()
            .any(|n| n.accessible_name() == Some(chip_text.as_str())),
        "the binding chip must expose its accessible name: {nodes:?}"
    );

    let remove_name = tr_args("controls-remove-binding", &[("binding", display)]);
    let remove = find_one(&nodes, Role::Button, &remove_name);
    assert!(!remove.disabled);
}

#[test]
fn controls_capture_control_exposes_accessible_name_and_role() {
    let (mut controller, _dir) = fresh_bare_controller("controls-capture-a11y");
    let action = HostAction::NavLibrary;
    let mut screen = ControlsScreen {
        capture: Some(action),
        ..Default::default()
    };
    let nodes = render_nodes(|ui| controls::show(ui, &mut controller, &mut screen, None));

    let accessible = tr_args("controls-capture", &[("action", tr(action.label_key()))]);
    let node = find_one(&nodes, Role::Button, &accessible);
    assert!(!node.disabled);
}

#[test]
fn controls_disabled_row_shows_inactive_suffix_and_keeps_controls_enabled() {
    let (mut controller, _dir) = fresh_bare_controller("controls-disabled-a11y");
    // 008 flips this action's shipped default to enabled (T049); disable
    // it explicitly to exercise the disabled-row rendering this test pins.
    let action = HostAction::TempoStepUp;
    controller.set_action_enabled(action, false);
    let mut screen = ControlsScreen::default();
    let nodes = render_nodes(|ui| controls::show(ui, &mut controller, &mut screen, None));
    let expected_label = format!("{} {}", tr(action.label_key()), tr("controls-inactive"));
    assert!(
        nodes
            .iter()
            .any(|n| n.accessible_name() == Some(expected_label.as_str())),
        "a disabled row's label must expose the `(inactive)` suffix: {nodes:?}"
    );

    let reset_name = tr_args(
        "controls-reset-action",
        &[("action", tr(action.label_key()))],
    );
    let reset = find_one(&nodes, Role::Button, &reset_name);
    assert!(
        !reset.disabled,
        "a disabled action's own controls must stay enabled (FR-012)"
    );
}

/// FR-014's "logical Tab order": the filter box must precede "Reset all to
/// defaults" (`render_nodes`'s node list preserves AccessKit's own frame
/// order for widgets drawn directly one after another, this file's own
/// doc comment on [`find_all`]) — both sit at the top of the page, ahead
/// of the per-category, per-row content.
#[test]
fn controls_tab_order_is_filter_then_reset_all() {
    let (mut controller, _dir) = fresh_bare_controller("controls-tab-order");
    let mut screen = ControlsScreen::default();
    let nodes = render_nodes(|ui| controls::show(ui, &mut controller, &mut screen, None));

    let filter_index = nodes
        .iter()
        .position(|n| n.role == Role::TextInput)
        .unwrap_or_else(|| panic!("filter box not found: {nodes:?}"));
    let reset_all_index = nodes
        .iter()
        .position(|n| {
            n.role == Role::Button && n.accessible_name() == Some(tr("controls-reset-all").as_str())
        })
        .unwrap_or_else(|| panic!("\"Reset all to defaults\" button not found: {nodes:?}"));

    assert!(
        filter_index < reset_all_index,
        "the filter box must precede \"Reset all to defaults\" in the reading/Tab order"
    );
}

// -- Plugins (009-plugin-runtime-and-permissions, US4, T110; contracts/
// ui-plugins.md §4) ----------------------------------------------------

/// `MODPLAYER_PLUGIN_FIXTURES`/`MODPLAYER_PLUGIN_STATE_DIR` are process-
/// global, so every `PlaybackController::new` in this binary that sets
/// them must be serialized against every other's brief mutation (mirrors
/// `tests/plugins_view.rs`'s own `PLUGIN_ENV_LOCK`).
static PLUGIN_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A controller that has discovered every `plugins/fixtures/` package,
/// not yet `launch()`ed (every fixture starts `Disabled`, which is enough
/// to walk every control the Plugins section renders per row — the
/// checkbox, the health/permissions/CPU/memory labels — without spawning
/// any real plugin thread).
fn fixture_controller_for_a11y(
    label: &str,
) -> (PlaybackController<FakeBackend, ScriptedHost>, TempDir) {
    let (store, dir) = fresh_store(label);
    let plugin_state_dir = TempDir::new(&format!("{label}-plugin-state"));
    let controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Safety: narrowly scopes each mutation to the one synchronous
        // read `PlaybackController::new` makes of it, serialized against
        // every other test in this binary via the lock above.
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_FIXTURES", "1");
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
        }
        let controller =
            PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_FIXTURES");
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
        }
        controller
    };
    (controller, dir)
}

/// Every control the Plugins section renders — the per-row enable
/// `Role::CheckBox` (including the invalid fixture's inert one), the
/// health/source/CPU/memory `Role::Label`s, and the section's own empty-
/// state label — exposes a non-empty accessible name, and the checkbox
/// carries the row's own plugin name rather than a bare "Enable"
/// (contracts/ui-plugins.md §4).
#[test]
fn plugins_section_controls_named() {
    let (mut controller, _dir) = fixture_controller_for_a11y("plugins-section");
    let nodes = render_nodes(|ui| modplayer_ui::plugins_view::show(ui, &mut controller));

    let checkboxes: Vec<_> = nodes.iter().filter(|n| n.role == Role::CheckBox).collect();
    // 10 fixtures (`plugins/bundled/` is empty this slice;
    // 010-transport-focus adds `focus-a`/`focus-b`) => 10 toggles.
    assert_eq!(
        checkboxes.len(),
        10,
        "expected one toggle per fixture: {nodes:?}"
    );
    for checkbox in &checkboxes {
        let name = checkbox
            .accessible_name()
            .expect("every enable toggle must have a non-empty accessible name");
        assert!(
            name.starts_with("Enable ") && name.len() > "Enable ".len(),
            "the toggle's name must carry the plugin's own name, got `{name}`"
        );
    }
    // The invalid fixture's toggle is inert and unchecked; every other
    // fixture defaults to enabled.
    let invalid_toggle = checkboxes
        .iter()
        .find(|c| {
            c.accessible_name()
                .is_some_and(|name| name.contains("org.modplayer.fixture.invalid"))
        })
        .unwrap_or_else(|| panic!("invalid fixture's toggle not found: {nodes:?}"));
    assert!(invalid_toggle.disabled);
    assert_eq!(invalid_toggle.toggled, Some(Toggled::False));

    // No plugin is `Active` pre-launch, so every health label reads `ok`
    // (the 9 valid fixtures) — each a real, non-empty accessible name,
    // never a bare colour dot.
    let ok_labels = find_all(&nodes, Role::Label, &tr("plugins-health-ok"));
    assert_eq!(
        ok_labels.len(),
        9,
        "expected 9 `ok` health labels: {nodes:?}"
    );
}

// -- Transport panel (010-transport-focus, Phase 5 (US3), contracts/
// ui-transport-panel.md §2) --------------------------------------------

/// A controller with every `plugins/fixtures/` package discovered,
/// `launch()`ed, a confirmed device, and a track playing — enough for
/// `focus-a`/`focus-b`'s own `ready_ack`-triggered `request_focus()`
/// (RT5) to have already run by the time each reaches `Active` (mirrors
/// `controller_transport_focus.rs`'s own `fixture_controller`/
/// `controller_with_track`).
fn fixture_controller_with_track(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
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
    let mut controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    controller.set_playback_permitted(true, None);
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    (controller, handle, dir, plugin_state_dir, track_state_dir)
}

fn plugin_id_by_identifier(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    identifier: &str,
) -> PluginId {
    controller
        .plugins_mut()
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == identifier)
        .map(|r| r.id)
        .unwrap_or_else(|| panic!("fixture '{identifier}' must be discovered"))
}

fn wait_plugin_active(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        controller.tick();
        if matches!(
            controller.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        ) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Every control the Transport panel renders — title, holder label,
/// policy `ComboBox`, "Take back" button, per-row give buttons and the
/// row group's own accessible name — exposes a non-empty accessible name
/// and the correct disabled/enabled state, both while the host holds
/// focus and once the user gives it to a plugin (contracts/
/// ui-transport-panel.md §2, U1).
#[test]
fn transport_panel_controls_expose_accessible_names_and_states() {
    let (mut controller, _handle, _dir, _psd, _tsd) =
        fixture_controller_with_track("transport-panel-a11y");
    let id_a = plugin_id_by_identifier(&mut controller, "org.modplayer.fixture.focus-a");
    let id_b = plugin_id_by_identifier(&mut controller, "org.modplayer.fixture.focus-b");
    assert!(
        wait_plugin_active(&mut controller, id_a),
        "focus-a must reach Active"
    );
    assert!(
        wait_plugin_active(&mut controller, id_b),
        "focus-b must reach Active"
    );

    // Baseline: the host holds focus (default `AutoOnInteraction` never
    // auto-grants a plugin's own `request_focus()`), so both fixtures are
    // merely pending by now (RT5's `ready_ack` -> `request_focus()`).
    let view = controller.transport_focus_view();
    let order_a = view
        .rows
        .iter()
        .find(|r| r.id == id_a)
        .and_then(|r| r.request_order);
    let order_b = view
        .rows
        .iter()
        .find(|r| r.id == id_b)
        .and_then(|r| r.request_order);
    assert!(
        order_a.is_some() && order_b.is_some(),
        "both fixtures must already be requesting: {view:?}"
    );

    let nodes = render_nodes(|ui| modplayer_ui::transport_view::show(ui, &mut controller));

    let title = find_one(&nodes, Role::Label, &tr("transport-panel-title"));
    assert!(!title.disabled, "{title:?}");

    let holder = find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "transport-holder",
            &[("holder", tr("transport-holder-host"))],
        ),
    );
    assert!(!holder.disabled, "{holder:?}");

    find_one(&nodes, Role::ComboBox, &tr("transport-policy-auto"));

    let take_back = find_one(&nodes, Role::Button, &tr("transport-take-back"));
    assert!(
        take_back.disabled,
        "Take back must be disabled while the host holds focus: {take_back:?}"
    );

    for (name, order) in [
        ("Focus fixture A", order_a.unwrap_or_else(|| unreachable!())),
        ("Focus fixture B", order_b.unwrap_or_else(|| unreachable!())),
    ] {
        let give = find_one(
            &nodes,
            Role::Button,
            &tr_args("transport-give-focus", &[("plugin", name.to_string())]),
        );
        assert!(!give.disabled, "{give:?}");

        let expected_state = tr_args("transport-requesting", &[("order", order.to_string())]);
        find_one(
            &nodes,
            Role::Unknown,
            &tr_args(
                "transport-row-a11y",
                &[("plugin", name.to_string()), ("state", expected_state)],
            ),
        );
    }

    // Give focus to A: the holder label, "Take back" and A's own row/give
    // button all follow.
    controller.focus_give(id_a);
    let nodes = render_nodes(|ui| modplayer_ui::transport_view::show(ui, &mut controller));

    let holder = find_one(
        &nodes,
        Role::Label,
        &tr_args(
            "transport-holder",
            &[("holder", "Focus fixture A".to_string())],
        ),
    );
    assert!(!holder.disabled, "{holder:?}");

    let take_back = find_one(&nodes, Role::Button, &tr("transport-take-back"));
    assert!(
        !take_back.disabled,
        "Take back must enable once a plugin holds focus: {take_back:?}"
    );

    let give_a = find_one(
        &nodes,
        Role::Button,
        &tr_args(
            "transport-give-focus",
            &[("plugin", "Focus fixture A".to_string())],
        ),
    );
    assert!(
        give_a.disabled,
        "Give focus must disable for the current holder: {give_a:?}"
    );

    find_one(
        &nodes,
        Role::Unknown,
        &tr_args(
            "transport-row-a11y",
            &[
                ("plugin", "Focus fixture A".to_string()),
                ("state", tr("transport-holds")),
            ],
        ),
    );
}

/// The empty state (contracts/ui-transport-panel.md §2: "no plugin can
/// hold transport focus") when no fixture is discovered at all — its own
/// non-empty accessible name.
#[test]
fn transport_panel_empty_state_exposes_its_accessible_name() {
    let (store, _dir) = fresh_store("transport-panel-empty");
    let mut controller = {
        // `MODPLAYER_PLUGIN_FIXTURES` is process-global: this construction
        // must serialize against every other's brief mutation of it too,
        // even though this one never sets it itself (mirrors every other
        // `PLUGIN_ENV_LOCK` guard in this file), so a concurrently
        // running fixture test's own set/unset window can never leak a
        // stray "1" into this one's read of it.
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store)
    };
    controller.launch();

    let nodes = render_nodes(|ui| modplayer_ui::transport_view::show(ui, &mut controller));
    let empty = find_one(&nodes, Role::Label, &tr("transport-empty"));
    assert!(!empty.disabled, "{empty:?}");
}

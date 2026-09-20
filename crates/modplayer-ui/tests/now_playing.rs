// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T065 (US1, 003) / T036 (US1, 005-now-playing-waveform): Now Playing
//! (contracts/ui-surface.md §1, contracts/ui-waveform.md) — inline disabled
//! reasons per `ActiveState`/health, the position readout updating as
//! playback advances, and the waveform overview's click/drag/keyboard seek
//! behaviour (retargeting 003's seek-slider tests at the overview,
//! contracts/ui-waveform.md §7).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use egui::accesskit::{NodeId, Role};
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, SourceCommand, SourceHealth, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::actions::ScopeState;
use modplayer_core::markers::{CueSlot, TrackMarkers};
use modplayer_core::settings::SettingsStore;
use modplayer_core::transport::Intent;
use modplayer_core::{NotRegisteredReason, PlaybackController, tr};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::artwork::{ArtworkCache, ArtworkState};
use modplayer_ui::waveform::{DetailWindow, DragOrigin, DragPreview, TimeSpace, WaveformState};
use modplayer_ui::{Shell, actions};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-now-playing-{label}-{}-{unique}",
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

fn track(id: &str, duration_ms: u32) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!()),
        id,
        vec!["Artist".to_string()],
        None,
        None,
        duration_ms,
        Availability::Available,
    )
}

/// Serializes any test in this binary that briefly overrides the
/// process-global `MODPLAYER_TRACK_STATE_DIR` for `PlaybackController::
/// new`'s one synchronous read of it (Phase 7 polish, T091: 006 US2's
/// `sync_marker_attachment` now runs on every `dispatch`, so any test here
/// that queues a track — every test below does — would otherwise read/
/// write the real per-user track-state directory; mirrors `markers.rs`'s
/// own lock/pattern, which this file didn't need before 006).
static TRACK_STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Both temp dirs `active_controller` allocates (settings, track-state);
/// kept alive together so either can be dropped only once the test itself
/// is done with the controller.
struct TestDirs(#[allow(dead_code)] TempDir, #[allow(dead_code)] TempDir);

/// A controller over a confirmed device with an `ActiveState::Active`
/// Connect registration — the baseline "healthy, transport enabled" state
/// every test below starts from and deviates from as needed. Also returns
/// the `ScriptedHostHandle` so tests can inspect `SourceCommand`s the UI
/// caused (contracts/ui-waveform.md §7's exact-frame tests).
fn active_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
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

/// Render `now_playing::show` in a fresh headless, AccessKit-enabled
/// context and return every non-empty accessible text (`value`+`label`) the
/// frame produced — robust to whichever of the two properties a given
/// widget role stores its text under (contracts/ui-surface.md §1;
/// `egui::Response::fill_accesskit_node_from_widget_info` puts `Role::Label`
/// text in `value` and every other role's in `label`).
fn rendered_texts<B: modplayer_audio_io::OutputBackend, H: modplayer_audio_source::SourceHost>(
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Vec<String> {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform);
    });
    let Some(update) = output.platform_output.accesskit_update.take() else {
        panic!("accesskit_update should be populated once enabled");
    };
    output.drop_without_applying_deltas();

    update
        .nodes
        .iter()
        .flat_map(|(_, node)| [node.value(), node.label()])
        .filter_map(|text| text.map(str::to_string))
        .filter(|text| !text.is_empty())
        .collect()
}

/// Render `now_playing::show` in a fresh headless context and return every
/// literal string drawn as a `Painter::text` shape (Polish Phase 7, T071):
/// `paint::paint`'s "analysis unavailable" label is drawn directly via
/// `Painter::text` rather than an accessible egui widget, so it never
/// reaches [`rendered_texts`]'s AccessKit-node scan — `output.shapes` (raw,
/// pre-tessellation) is the only place that text appears.
fn rendered_painted_texts<
    B: modplayer_audio_io::OutputBackend,
    H: modplayer_audio_source::SourceHost,
>(
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Vec<String> {
    let ctx = Context::default();
    let output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform);
    });
    let texts = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text_shape) => Some(text_shape.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    texts
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

/// One AccessKit node's role, gathered into an owned snapshot (mirrors
/// `accessibility.rs`'s `AccessNode`, kept minimal here).
struct NodeInfo {
    role: Role,
}

fn render_nodes<B: modplayer_audio_io::OutputBackend, H: modplayer_audio_source::SourceHost>(
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    input: RawInput,
) -> Vec<NodeInfo> {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(input, |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    update
        .nodes
        .iter()
        .map(|(_, node)| NodeInfo { role: node.role() })
        .collect()
}

/// The overview's screen rect: the topmost `Role::Slider` node (it is
/// drawn above the master-volume slider, contracts/ui-waveform.md §1). Takes
/// the same `ctx` the caller will keep driving interaction on afterwards —
/// egui's own drag-threshold/focus bookkeeping lives on the `Context`
/// across frames, so measuring on one and interacting on another would
/// desync it.
fn overview_bounds<B: modplayer_audio_io::OutputBackend, H: modplayer_audio_source::SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Rect {
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let mut best: Option<egui::accesskit::Rect> = None;
    for (_, node) in &update.nodes {
        if node.role() != Role::Slider {
            continue;
        }
        if let Some(bounds) = node.bounds()
            && best.as_ref().is_none_or(|b| bounds.y0 < b.y0)
        {
            best = Some(bounds);
        }
    }
    let bounds = best.expect("Now Playing must render at least one Role::Slider node");
    Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    )
}

/// The most recent `SourceCommand::Seek` millisecond value the handle has
/// recorded, if any.
fn last_seek_ms(handle: &ScriptedHostHandle) -> Option<u32> {
    handle
        .record_commands()
        .into_iter()
        .rev()
        .find_map(|cmd| match cmd {
            SourceCommand::Seek(ms) => Some(ms),
            _ => None,
        })
}

#[test]
fn no_device_disables_transport_and_shows_status_no_device() {
    // No `confirm_device` call: `launch()` over zero/no confirmed devices
    // leaves `active_device` unset (device_policy.rs, mirrored by
    // controller::disabled_reason's first check).
    let (store, _dir) = fresh_store("no-device");
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    controller.launch();

    assert!(!controller.transport_enabled());
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let texts = rendered_texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&tr("status-no-device")),
        "expected the `status-no-device` line, got {texts:?}"
    );
}

#[test]
fn not_registered_reasons_render_their_matching_status_line() {
    let cases = [
        (
            NotRegisteredReason::PremiumRequired,
            "status-premium-required",
        ),
        (
            NotRegisteredReason::SubscriptionNotVerified,
            "status-subscription-not-verified",
        ),
        (NotRegisteredReason::SignedOut, "status-signed-out"),
        (NotRegisteredReason::Unknown, "status-not-registered"),
    ];

    for (reason, key) in cases {
        let (mut controller, _handle, _dir) = active_controller(&format!("not-registered-{key}"));
        controller.set_playback_permitted(false, Some(reason));

        assert!(
            !controller.transport_enabled(),
            "{key}: transport must be disabled"
        );
        let mut artwork = ArtworkCache::new();
        let mut waveform = WaveformState::default();
        let texts = rendered_texts(&mut controller, &mut artwork, &mut waveform);
        assert!(
            texts.contains(&tr(key)),
            "{key}: expected that status line, got {texts:?}"
        );
    }
}

#[test]
fn source_unavailable_disables_transport_and_shows_its_status_line() {
    let (store, _dir) = fresh_store("source-unavailable");
    let host = ScriptedHost::new();
    let handle = host.handle();
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![fake_device()]), host, store);
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    controller.set_playback_permitted(true, None);
    controller.tick();
    assert!(controller.transport_enabled(), "sanity: healthy and active");

    handle.health(SourceHealth::Unavailable {
        client_update_required: false,
    });
    controller.tick();

    assert!(!controller.transport_enabled());
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let texts = rendered_texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&tr("status-source-unavailable")),
        "expected the `status-source-unavailable` line, got {texts:?}"
    );
}

#[test]
fn healthy_active_state_shows_no_disabled_reason() {
    let (mut controller, _handle, _dir) = active_controller("healthy");
    assert!(controller.transport_enabled());
    assert_eq!(controller.disabled_reason(), None);

    let every_status_key = [
        "status-no-device",
        "status-premium-required",
        "status-subscription-not-verified",
        "status-signed-out",
        "status-not-registered",
        "status-source-unavailable",
    ];
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let texts = rendered_texts(&mut controller, &mut artwork, &mut waveform);
    for key in every_status_key {
        assert!(
            !texts.contains(&tr(key)),
            "healthy/active must not show `{key}`, got {texts:?}"
        );
    }
}

#[test]
fn empty_state_shows_pick_a_track() {
    let (mut controller, _handle, _dir) = active_controller("empty-state");
    // No `queue_replace`: `current_track()` stays `None`.
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let texts = rendered_texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&tr("now-playing-pick-a-track")),
        "expected the pick-a-track hint, got {texts:?}"
    );
    assert!(
        !texts
            .iter()
            .any(|t| t.contains("m:ss") || t == &tr("transport-seek")),
        "no waveform overview must render with no current track: {texts:?}"
    );
}

#[test]
fn artwork_falls_back_to_initials() {
    /// Cleared even on an early return via `?`/panic in this test (the
    /// debug-only toggle is process-global, `artwork.rs`'s
    /// `force_artwork_fail`).
    struct ClearOnDrop;
    impl Drop for ClearOnDrop {
        fn drop(&mut self) {
            unsafe { std::env::remove_var("MODPLAYER_ARTWORK_FORCE_FAIL") };
        }
    }
    // Safety: this test owns the variable's lifecycle end-to-end (set here,
    // cleared by `ClearOnDrop`) and asserts on its own effect only.
    unsafe { std::env::set_var("MODPLAYER_ARTWORK_FORCE_FAIL", "1") };
    let _clear = ClearOnDrop;

    let url = "https://i.scdn.co/image/deadbeef";
    let mut artwork = ArtworkCache::new();
    let ctx = Context::default();
    // The force-fail toggle short-circuits before any network call
    // (`fetch_and_decode`), so this converges almost immediately.
    let mut state = artwork.get(&ctx, url);
    for _ in 0..200 {
        if state == ArtworkState::Failed {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
        state = artwork.get(&ctx, url);
    }
    assert_eq!(
        state,
        ArtworkState::Failed,
        "sanity: the fetch must resolve to Failed"
    );

    let (mut controller, _handle, _dir) = active_controller("artwork-fail");
    let mut with_art = track("a", 200_000);
    with_art.artwork_url = Some(url.to_string());
    controller.queue_replace(vec![with_art]);

    let mut waveform = WaveformState::default();
    let nodes = render_nodes(
        &mut controller,
        &mut artwork,
        &mut waveform,
        default_input(),
    );
    assert!(
        !nodes.iter().any(|n| n.role == Role::Image),
        "a Failed artwork fetch must fall back to the initials placeholder, never an Image node"
    );
}

#[test]
fn click_on_overview_seeks_to_exact_frame() {
    let (mut controller, handle, _dir) = active_controller("click-seek");
    controller.queue_replace(vec![track("a", 200_000)]);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let bounds = overview_bounds(&ctx, &mut controller, &mut artwork, &mut waveform);
    let sample_rate = controller.source_sample_rate();
    let len_frames = (200_000u64 * u64::from(sample_rate)) / 1000;
    let space = TimeSpace::new(bounds, 0..len_frames, sample_rate);
    let target_x = bounds.left() + bounds.width() * 0.417; // deliberately not a round fraction
    let expected_frame = space.frame_at(target_x);

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: Pos2::new(target_x, bounds.center().y),
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
        pos: Pos2::new(target_x, bounds.center().y),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();

    let expected_ms = (expected_frame * 1000) / u64::from(sample_rate);
    let seeked_ms = last_seek_ms(&handle).expect("expected a SourceCommand::Seek");
    assert_eq!(u64::from(seeked_ms), expected_ms);
    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "T6: a seek while stopped becomes Paused"
    );
}

#[test]
fn drag_previews_without_seeking_and_esc_cancels() {
    let (mut controller, handle, _dir) = active_controller("drag-preview");
    controller.queue_replace(vec![track("a", 200_000)]);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let bounds = overview_bounds(&ctx, &mut controller, &mut artwork, &mut waveform);
    let left = Pos2::new(bounds.left() + 4.0, bounds.center().y);
    let right = Pos2::new(bounds.right() - 4.0, bounds.center().y);

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: left,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();

    let mut drag = default_input();
    drag.events.push(Event::PointerMoved(right));
    let output = ctx.run_ui(drag, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();

    assert!(
        waveform.drag.is_some(),
        "dragging must set a live preview, no seek yet"
    );
    assert!(
        last_seek_ms(&handle).is_none(),
        "no SourceCommand::Seek must be sent while merely dragging"
    );
    assert_eq!(
        controller.transport_state().intent,
        Intent::Stopped,
        "dragging must not commit a seek"
    );

    // Esc cancels: the preview clears and no seek ever lands.
    let mut esc = default_input();
    esc.events.push(Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(esc, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();

    assert_eq!(waveform.drag, None, "Esc must clear the drag preview");
    assert!(last_seek_ms(&handle).is_none());
    assert_eq!(controller.transport_state().intent, Intent::Stopped);
}

#[test]
fn seek_slider_commits_once_per_release() {
    // 003's seek-slider test, retargeted at the waveform overview
    // (contracts/ui-waveform.md §7): a drag from one edge to the other
    // commits exactly once, on release.
    let (mut controller, _handle, _dir) = active_controller("seek-commit");
    controller.queue_replace(vec![track("a", 200_000)]);
    assert_eq!(controller.transport_state().intent, Intent::Stopped);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let bounds = overview_bounds(&ctx, &mut controller, &mut artwork, &mut waveform);
    let left = Pos2::new(bounds.left() + 4.0, bounds.center().y);
    let right = Pos2::new(bounds.right() - 4.0, bounds.center().y);

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: left,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        controller.transport_state().intent,
        Intent::Stopped,
        "pressing must not commit a seek"
    );

    let mut drag = default_input();
    drag.events.push(Event::PointerMoved(right));
    let output = ctx.run_ui(drag, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        controller.transport_state().intent,
        Intent::Stopped,
        "dragging must not commit a seek before release"
    );

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: right,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "release must commit the seek exactly once (T6: stopped -> Paused)"
    );
    let position_after_release = controller.position();

    let output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    assert_eq!(controller.transport_state().intent, Intent::Paused);
    assert_eq!(
        controller.position(),
        position_after_release,
        "no further seek must be committed without a new release"
    );
}

#[test]
fn seek_while_paused_stays_paused() {
    let (mut controller, _handle, _dir) = active_controller("seek-paused");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller.pause();
    assert_eq!(controller.transport_state().intent, Intent::Paused);

    controller.seek_frames(44_100 * 10);
    controller.tick();

    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "T7: a seek while paused stays Paused"
    );
}

#[test]
fn seek_while_stopped_enters_paused() {
    let (mut controller, _handle, _dir) = active_controller("seek-stopped");
    controller.queue_replace(vec![track("a", 200_000)]);
    assert_eq!(controller.transport_state().intent, Intent::Stopped);

    controller.seek_frames(44_100 * 10);
    controller.tick();

    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "T6: a seek while stopped becomes Paused"
    );
}

#[test]
fn position_readout_updates_as_playback_advances() {
    let (mut controller, _handle, _dir) = active_controller("position-readout");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let before = rendered_texts(&mut controller, &mut artwork, &mut waveform);
    let position_before = controller.position();

    // Advance the audio clock by rendering real frames through the fake
    // backend, exactly like `modplayer-core`'s own streaming tests.
    let _ = controller.backend_mut().render_buffers(20_000);

    let after = rendered_texts(&mut controller, &mut artwork, &mut waveform);
    let position_after = controller.position();

    assert!(
        position_after > position_before,
        "position must advance while playing"
    );
    assert_ne!(
        before, after,
        "the rendered elapsed/remaining labels must change as the audio clock advances"
    );
}

// US3 (Phase 5, T052): a still-streaming (`Filling`) store's uncovered
// buckets render as placeholder columns; once `Complete`, none remain.
// Driven directly at the `AnalysisService`/`waveform_columns` level (like
// `preview_frame_drives_the_drag_shown_in_a_live_test_helper` below), not
// through a full `now_playing::show` render, so a plain temp-dir
// `AnalysisPaths` keeps this test independent of any on-disk cache.

#[test]
fn placeholder_columns_for_uncovered_buckets() {
    use egui::{Rect, pos2};
    use modplayer_audio_source::{DecodedStore, TrackId};
    use modplayer_core::AnalysisStatus;
    use modplayer_core::analysis::{AnalysisPaths, AnalysisService};
    use modplayer_ui::waveform::{ColumnPaint, waveform_columns};

    let dir = TempDir::new("placeholder-columns");
    let paths = AnalysisPaths::with_dir(dir.path());
    let mut service = AnalysisService::new(Some(paths));

    let id = TrackId::new("spotify:track:placeholder-columns").unwrap_or_else(|_| unreachable!());
    let level0_frames = 128u64;
    let total_buckets = 40u64;
    let len_frames = total_buckets * level0_frames;

    service.attach(id.clone(), 44_100, len_frames);
    let store = DecodedStore::new(44_100, len_frames);

    // Fill only the first half: the store stays `Filling`, so half the
    // level-0 buckets exist and half do not.
    let filled_frames = (total_buckets / 2) * level0_frames;
    let filled: Vec<f32> = (0..filled_frames)
        .flat_map(|i| {
            let s = if i % 4 < 2 { 0.6 } else { -0.6 };
            [s, s]
        })
        .collect();
    store.write_frames(0, &filled);
    service.attach_store(id.clone(), std::sync::Arc::clone(&store));

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut peaks = None;
    while std::time::Instant::now() < deadline {
        service.drain();
        if let Some(snapshot) = service.latest()
            && snapshot.status == AnalysisStatus::Partial
            && let Some(p) = snapshot.peaks.clone()
        {
            peaks = Some(p);
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let peaks = peaks.expect("expected a Partial snapshot with some peaks");

    let space = TimeSpace::new(
        Rect::from_min_max(pos2(0.0, 0.0), pos2(total_buckets as f32, 40.0)),
        0..len_frames,
        44_100,
    );
    let columns = waveform_columns(&space, &peaks, total_buckets as usize);
    assert!(
        columns[..(total_buckets / 2) as usize]
            .iter()
            .all(|c| matches!(c, ColumnPaint::Present { .. })),
        "the filled half must render present columns, got {columns:?}"
    );
    assert!(
        columns[(total_buckets / 2) as usize..]
            .iter()
            .any(|c| matches!(c, ColumnPaint::Placeholder)),
        "the unfilled half must render at least one placeholder column, got {columns:?}"
    );

    // Complete the store: no placeholder must remain.
    let rest: Vec<f32> = (filled_frames..len_frames)
        .flat_map(|i| {
            let s = if i % 4 < 2 { 0.6 } else { -0.6 };
            [s, s]
        })
        .collect();
    store.write_frames(filled_frames, &rest);
    store.set_complete(len_frames);

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut complete_peaks = None;
    while std::time::Instant::now() < deadline {
        service.drain();
        if let Some(snapshot) = service.latest()
            && snapshot.status == AnalysisStatus::Complete
            && let Some(p) = snapshot.peaks.clone()
        {
            complete_peaks = Some(p);
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let complete_peaks = complete_peaks.expect("expected a Complete snapshot with peaks");
    let columns = waveform_columns(&space, &complete_peaks, total_buckets as usize);
    assert!(
        columns
            .iter()
            .all(|c| matches!(c, ColumnPaint::Present { .. })),
        "no placeholder column must remain once Complete, got {columns:?}"
    );

    service.shutdown();
}

#[test]
fn preview_frame_drives_the_drag_shown_in_a_live_test_helper() {
    // Sanity check for the test helper itself (not app behaviour): a
    // `DragPreview` on `WaveformState` is what `preview_frame()` reports,
    // which `now_playing.rs` uses in place of the real position while
    // dragging (data-model.md §5.4).
    let mut state = WaveformState::default();
    assert_eq!(state.preview_frame(), None);
    state.drag = Some(DragPreview {
        target_frame: 999,
        origin: DragOrigin::Overview,
    });
    assert_eq!(state.preview_frame(), Some(999));
}

// T048 substitute evidence (US2, M6, contracts/ui-waveform.md §2): a
// pointer pinch/wheel zoom on the *detail* view anchors on the pointer's
// own frame, not the playhead — the distinguishing behaviour from the
// overview (whose anchor is always the playhead, already exercised
// identically by `keyboard_table_matches_pointer_results`'s `+`/`-` rows).

#[test]
fn pointer_zoom_on_detail_is_anchored_on_the_pointer_not_the_playhead() {
    let (mut controller, _handle, _dir) = active_controller("pointer-zoom-detail");
    controller.queue_replace(vec![track("a", 200_000)]);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    // First frame: establish the detail window and read its screen rect
    // from its own AccessKit node (named `waveform-detail`, distinct from
    // the overview's `transport-seek`).
    let bounds = {
        let mut output = ctx.run_ui(default_input(), |ui| {
            modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .expect("accesskit_update should be populated once enabled");
        output.drop_without_applying_deltas();
        let name = tr("waveform-detail");
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some(name.as_str()))
            .expect("the detail slider must render once a track is loaded");
        let b = node.bounds().expect("the detail slider must have bounds");
        Rect::from_min_max(
            Pos2::new(b.x0 as f32, b.y0 as f32),
            Pos2::new(b.x1 as f32, b.y1 as f32),
        )
    };

    let sample_rate = controller.source_sample_rate();
    let len_frames = (200_000u64 * u64::from(sample_rate)) / 1000;
    let before = waveform
        .detail
        .expect("the detail window must exist after the first frame");
    assert!(
        before.contains(0),
        "sanity: a fresh, never-seeked window contains the playhead (frame 0)"
    );

    // Hover near the window's left edge — far from the playhead, which
    // sits at the window's centre — and pinch/wheel-zoom in. If the anchor
    // were (wrongly) the playhead, as it is on the overview, the result
    // would equal `before.zoom_about(0, 2.0, ..)` instead.
    let pointer_x = bounds.left() + bounds.width() * 0.1;
    let space = TimeSpace::new(
        bounds,
        before.start_frame..(before.start_frame + before.width_frames),
        sample_rate,
    );
    let anchor_frame = space.frame_at(pointer_x);

    let mut input = default_input();
    input
        .events
        .push(Event::PointerMoved(Pos2::new(pointer_x, bounds.center().y)));
    input.events.push(Event::Zoom(2.0));
    let output = ctx.run_ui(input, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();

    let after = waveform
        .detail
        .expect("the detail window must survive the frame");
    let expected_pointer_anchored = before
        .zoom_about(anchor_frame, 2.0, len_frames, sample_rate)
        .suspend_follow_if_outside(0);
    let playhead_anchored = before
        .zoom_about(0, 2.0, len_frames, sample_rate)
        .suspend_follow_if_outside(0);
    assert_ne!(
        expected_pointer_anchored, playhead_anchored,
        "sanity: the two anchors must actually disagree for this test to mean anything"
    );
    assert_eq!(
        after, expected_pointer_anchored,
        "pointer zoom on the detail view must anchor on the pointer's own frame, not the \
         playhead (contracts/ui-waveform.md §2)"
    );
}

// T046 (US2, contracts/ui-waveform.md §3, FR-013): every keyboard row on
// either waveform widget produces exactly the effect its pointer
// equivalent would — the seek rows via the same `seek_frames` formula a
// click at that exact frame commits (contracts/ui-waveform.md §7's
// `click_on_overview_seeks_to_exact_frame` already pins the click side),
// and the zoom/pan/reset rows via the identical `DetailWindow` formula a
// pointer zoom/scroll would apply (the overview's own pointer zoom is
// `zoom_about(playhead, factor, ..)` — the same call `+`/`-`'s
// `zoom_step` makes).

/// One AccessKit frame's `(focus, [(id, label)])` (mirrors `render_nodes`/
/// `overview_bounds`, kept to just what `keyboard_table_matches_pointer_
/// results` needs: which node has focus, and each node's id/label so a
/// widget can be found by its accessible name across frames).
fn run_key_frame<B: modplayer_audio_io::OutputBackend, H: modplayer_audio_source::SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    key: Key,
    modifiers: Modifiers,
) -> (NodeId, Vec<(NodeId, Option<String>)>) {
    let mut input = default_input();
    // egui only updates its per-frame `InputState::modifiers` from a
    // dedicated `ModifiersChanged` event — `Event::Key`'s own `modifiers`
    // field alone does not reach it, and the `key_pressed`/`input.
    // modifiers` pair our own `input.rs` reads is driven solely by that
    // state. It also persists across frames like a held key, so every
    // call here sets it explicitly rather than relying on the previous
    // frame's value.
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
    let mut output = ctx.run_ui(input, |ui| {
        // 007, contracts/ui-actions.md §1: run the dispatcher exactly as
        // `App::ui` does before now_playing::show re-points this file's
        // `I`/`O`/`L` keyboard-table tests at the catalog (T047); every
        // key here not in the catalog (waveform seek/zoom/pan, `Tab`)
        // simply falls through untouched, as before.
        let claims = actions::claims_snapshot(&ui.ctx().clone());
        actions::clear_claims(&ui.ctx().clone());
        let scope = ScopeState {
            now_playing_shown: true,
            marker_focused: waveform.focused_marker.is_some(),
        };
        let mut shell = Shell::default();
        actions::dispatch_and_invoke(
            &ui.ctx().clone(),
            &claims,
            &scope,
            controller,
            &mut shell,
            waveform,
        );
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform)
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
        .map(|(id, node)| (*id, node.label().map(str::to_string)))
        .collect();
    (update.focus, nodes)
}

/// Repeatedly sends `Tab` (one per frame, matching egui's own per-frame
/// focus-advance model — several `Tab`s within one frame's event list only
/// count as one) until the node named `name` has focus, or panics after a
/// generous bound. Deliberately doesn't hard-code how many other
/// focusable widgets (transport buttons, the queue toggle, ...) precede it
/// — that count is an implementation detail (contracts/ui-waveform.md §3:
/// "Tab" moves focus like any other egui widget).
fn tab_focus_named<B: modplayer_audio_io::OutputBackend, H: modplayer_audio_source::SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    name: &str,
) {
    for _ in 0..40 {
        let (focus, nodes) = run_key_frame(
            ctx,
            controller,
            artwork,
            waveform,
            Key::Tab,
            Modifiers::default(),
        );
        if nodes
            .iter()
            .any(|(id, label)| *id == focus && label.as_deref() == Some(name))
        {
            return;
        }
    }
    panic!("Tab never reached the `{name}` widget within 40 presses");
}

#[test]
fn keyboard_table_matches_pointer_results() {
    // Seek rows: `←`/`→`, `Shift+←`/`Shift+→`, `Home`/`End`.
    let seek_cases: [(Key, Modifiers, u64); 4] = [
        (Key::ArrowRight, Modifiers::default(), 5 * 44_100),
        (Key::ArrowRight, Modifiers::SHIFT, (44_100 * 500) / 1000),
        (Key::Home, Modifiers::default(), 0),
        (
            Key::End,
            Modifiers::default(),
            8_820_000, /* 200s @ 44.1kHz */
        ),
    ];
    for (key, modifiers, expected_frame) in seek_cases {
        let (mut controller, handle, _dir) =
            active_controller(&format!("kbd-seek-{key:?}-{}", modifiers.alt as u8));
        controller.queue_replace(vec![track("a", 200_000)]);
        let mut artwork = ArtworkCache::new();
        let mut waveform = WaveformState::default();
        let ctx = Context::default();
        ctx.enable_accesskit();

        tab_focus_named(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &tr("transport-seek"),
        );
        let sample_rate = controller.source_sample_rate();
        assert_eq!(
            sample_rate, 44_100,
            "sanity: Connect always runs at 44.1kHz"
        );

        run_key_frame(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            key,
            modifiers,
        );

        let expected_ms = (expected_frame * 1000) / u64::from(sample_rate);
        let seeked_ms = last_seek_ms(&handle).expect("expected a SourceCommand::Seek");
        assert_eq!(
            u64::from(seeked_ms),
            expected_ms,
            "{key:?} (alt={}, shift={}) must seek to the exact contract formula",
            modifiers.alt,
            modifiers.shift
        );
    }

    // Zoom/pan/reset rows: `+`/`=`, `-`, `0`, `Alt+←`/`Alt+→`,
    // `Alt+Shift+←`/`Alt+Shift+→` — each compared against calling the same
    // `DetailWindow` method directly with the values `now_playing.rs`
    // itself would have used (playhead 0, a fresh, never-played track).
    let (mut controller, _handle, _dir) = active_controller("kbd-detail");
    controller.queue_replace(vec![track("a", 200_000)]);
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    tab_focus_named(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        &tr("transport-seek"),
    );
    let sample_rate = controller.source_sample_rate();
    let len_frames = (200_000u64 * u64::from(sample_rate)) / 1000;
    let playhead = 0u64;

    type DetailTransform = fn(DetailWindow, u64, u64, u32) -> DetailWindow;
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
    let detail_cases: [(Key, Modifiers, DetailTransform); 6] = [
        (Key::Plus, Modifiers::default(), |d, p, l, r| {
            d.zoom_step(p, true, l, r).suspend_follow_if_outside(p)
        }),
        (Key::Minus, Modifiers::default(), |d, p, l, r| {
            d.zoom_step(p, false, l, r).suspend_follow_if_outside(p)
        }),
        (Key::ArrowRight, Modifiers::ALT, |d, p, l, _r| {
            let delta = (0.1 * d.width_frames as f64).round() as i64;
            d.pan(delta, l).suspend_follow_if_outside(p)
        }),
        (Key::ArrowLeft, Modifiers::ALT, |d, p, l, _r| {
            let delta = -((0.1 * d.width_frames as f64).round() as i64);
            d.pan(delta, l).suspend_follow_if_outside(p)
        }),
        (Key::ArrowRight, alt_shift, |d, p, l, _r| {
            let delta = d.width_frames as i64;
            d.pan(delta, l).suspend_follow_if_outside(p)
        }),
        (Key::Num0, Modifiers::default(), |d, _p, l, r| d.reset(l, r)),
    ];

    for (key, modifiers, expected_transform) in detail_cases {
        let before = waveform
            .detail
            .expect("the detail window must exist once a track is loaded");
        run_key_frame(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            key,
            modifiers,
        );
        let after = waveform
            .detail
            .expect("the detail window must survive every frame");
        let expected = expected_transform(before, playhead, len_frames, sample_rate);
        assert_eq!(
            after, expected,
            "{key:?} (alt={}, shift={}) must match its pointer-equivalent DetailWindow formula",
            modifiers.alt, modifiers.shift
        );
    }

    // The same table holds with the *detail* widget focused instead of the
    // overview (contracts/ui-waveform.md §3: "either waveform focused") —
    // spot-checked with one seek row and one zoom row rather than the
    // whole table again.
    tab_focus_named(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        &tr("waveform-detail"),
    );
    let before = waveform.detail.expect("detail window must still exist");
    let playhead_now = 0u64; // no seek has happened on this controller/track yet
    run_key_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Plus,
        Modifiers::default(),
    );
    let after = waveform
        .detail
        .expect("detail window must survive the frame");
    assert_eq!(
        after,
        before
            .zoom_step(playhead_now, true, len_frames, sample_rate)
            .suspend_follow_if_outside(playhead_now),
        "`+` must zoom the shared detail window the same way when the detail widget (not \
         the overview) has focus"
    );

    // 006 (US1-US4, Phase 7 T091): marker/loop/cue rows, matched against
    // the exact controller method contracts/ui-markers.md §2 documents
    // for each key (there is no pointer gesture for these — `I`/`O`/`L`/
    // `M`/`1`-`8`/`Shift+1`-`8` are pure keyboard shortcuts) — a twin
    // controller driven directly through the API stands in for the
    // "pointer result" this test otherwise compares against. Detailed
    // per-scenario coverage (refusals, drag, nudge, ...) already lives in
    // `markers.rs`; this is the one place every marker/loop/cue row is
    // pinned against its documented equivalent from the same suite that
    // pins the seek/zoom rows.
    fn setup_marker_controller(
        label: &str,
    ) -> (
        PlaybackController<FakeBackend, ScriptedHost>,
        TestDirs,
        ArtworkCache,
        WaveformState,
        Context,
    ) {
        let (mut controller, _handle, dirs) = active_controller(label);
        controller.queue_replace(vec![track("a", 200_000)]);
        controller.play();
        controller.tick();
        let artwork = ArtworkCache::new();
        let waveform = WaveformState::default();
        let ctx = Context::default();
        ctx.enable_accesskit();
        (controller, dirs, artwork, waveform, ctx)
    }

    // `I` / `O`: create and complete the current region.
    {
        let (mut via_key, _dirs1, mut artwork, mut waveform, ctx) =
            setup_marker_controller("kbd-io-key");
        tab_focus_named(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            &tr("transport-seek"),
        );
        run_key_frame(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            Key::I,
            Modifiers::default(),
        );
        let _ = via_key.backend_mut().render_buffers(1);
        run_key_frame(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            Key::O,
            Modifiers::default(),
        );

        let (mut via_api, _dirs2, _artwork2, _waveform2, _ctx2) =
            setup_marker_controller("kbd-io-api");
        via_api
            .set_loop_a()
            .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
        let _ = via_api.backend_mut().render_buffers(1);
        via_api
            .set_loop_b()
            .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));

        let span_of = |c: &PlaybackController<FakeBackend, ScriptedHost>| {
            let markers = c.markers().expect("markers must exist");
            let region = markers
                .current_region()
                .expect("a region must exist after I/set_loop_a");
            markers
                .region(region)
                .and_then(|r| r.span(markers))
                .expect("region must be complete after O/set_loop_b")
        };
        assert_eq!(
            span_of(&via_key),
            span_of(&via_api),
            "`I`/`O` must land the region at exactly the same (a, b) as set_loop_a/set_loop_b"
        );
    }

    // `L`: toggle the current (complete) region's armed state.
    {
        let (mut via_key, _dirs1, mut artwork, mut waveform, ctx) =
            setup_marker_controller("kbd-l-key");
        via_key
            .set_loop_a()
            .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
        let _ = via_key.backend_mut().render_buffers(1);
        via_key
            .set_loop_b()
            .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
        tab_focus_named(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            &tr("transport-seek"),
        );
        run_key_frame(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            Key::L,
            Modifiers::default(),
        );
        let key_region = via_key
            .markers()
            .and_then(TrackMarkers::current_region)
            .expect("region must exist");
        let armed_via_key = via_key
            .markers()
            .and_then(|m| m.region(key_region))
            .is_some_and(|r| r.armed);

        let (mut via_api, _dirs2, _artwork2, _waveform2, _ctx2) =
            setup_marker_controller("kbd-l-api");
        via_api
            .set_loop_a()
            .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
        let _ = via_api.backend_mut().render_buffers(1);
        via_api
            .set_loop_b()
            .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
        via_api
            .toggle_current_loop()
            .unwrap_or_else(|e| unreachable!("toggle_current_loop: {e}"));
        // A fresh `TrackMarkers`' internal id counter is deterministic, so
        // the twin controller's own first region has the same `RegionId`
        // as `via_key`'s — resolved independently rather than reusing
        // `key_region` directly, so a future counter change breaks this
        // assertion visibly instead of silently comparing the wrong region.
        let api_region = via_api
            .markers()
            .and_then(TrackMarkers::current_region)
            .expect("region must exist");
        assert_eq!(
            key_region, api_region,
            "sanity: two freshly constructed controllers' first region must share an id"
        );
        let armed_via_api = via_api
            .markers()
            .and_then(|m| m.region(api_region))
            .is_some_and(|r| r.armed);

        assert!(armed_via_key, "`L` must arm the complete region");
        assert_eq!(
            armed_via_key, armed_via_api,
            "`L` must arm/disarm exactly like toggle_current_loop"
        );
    }

    // `M`: add a point marker at the playhead.
    {
        let (mut via_key, _dirs1, mut artwork, mut waveform, ctx) =
            setup_marker_controller("kbd-m-key");
        tab_focus_named(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            &tr("transport-seek"),
        );
        run_key_frame(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            Key::M,
            Modifiers::default(),
        );
        let key_count = via_key.markers().map(TrackMarkers::count).unwrap_or(0);

        let (mut via_api, _dirs2, _artwork2, _waveform2, _ctx2) =
            setup_marker_controller("kbd-m-api");
        via_api
            .add_point_marker()
            .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
        let api_count = via_api.markers().map(TrackMarkers::count).unwrap_or(0);

        assert_eq!(key_count, 1, "`M` must add exactly one point marker");
        assert_eq!(
            key_count, api_count,
            "`M` must add exactly as many markers as add_point_marker"
        );
    }

    // `Shift+1`: set cue slot 1 at the playhead; `1`: jump to it (never a
    // refusal/no-op once occupied, and never changes play/pause state).
    {
        let (mut via_key, _dirs1, mut artwork, mut waveform, ctx) =
            setup_marker_controller("kbd-cue-key");
        tab_focus_named(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            &tr("transport-seek"),
        );
        run_key_frame(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            Key::Num1,
            Modifiers::SHIFT,
        );
        let slot = CueSlot::new(1).unwrap_or_else(|| unreachable!());
        let key_cue_frame = via_key
            .markers()
            .and_then(|m| m.cue(slot))
            .map(|marker| marker.position);

        let (mut via_api, _dirs2, _artwork2, _waveform2, _ctx2) =
            setup_marker_controller("kbd-cue-api");
        via_api
            .set_cue(slot)
            .unwrap_or_else(|e| unreachable!("set_cue: {e}"));
        let api_cue_frame = via_api
            .markers()
            .and_then(|m| m.cue(slot))
            .map(|marker| marker.position);

        assert!(key_cue_frame.is_some(), "`Shift+1` must occupy cue slot 1");
        assert_eq!(
            key_cue_frame, api_cue_frame,
            "`Shift+1` must set the cue at exactly the same frame as set_cue"
        );

        let intent_before = via_key.transport_state().intent;
        run_key_frame(
            &ctx,
            &mut via_key,
            &mut artwork,
            &mut waveform,
            Key::Num1,
            Modifiers::default(),
        );
        assert_eq!(
            via_key.transport_state().intent,
            intent_before,
            "`1` must jump like jump_to_cue without changing play/pause state"
        );
    }
}

// -- Polish (Phase 7): analysis-failure rendering (EC-5.9) -----------------
// The Analysis Service's own failure bookkeeping (A7/A8: silent -> Failed
// with no peaks; a mid-stream decoder failure -> Failed with whatever
// partial peaks already folded) is pinned directly against `AnalysisService`
// in `modplayer-core/tests/analysis.rs` (T066/T067). These two tests pin
// the other half of EC-5.9: `now_playing::show`'s rendering reaction to
// each outcome, driven end to end through a real `PlaybackController` via
// `SourceEvent::DecodedStore` (mirrors `controller_streaming.rs::
// decoded_store_event_reaches_analysis_for_current_track_only`'s direct-
// emit pattern) rather than a mock of the paint step.

/// Tick `controller` until its analysis reaches `AnalysisStatus::Failed`,
/// or panic after `timeout`.
fn wait_for_failed<B: modplayer_audio_io::OutputBackend, H: modplayer_audio_source::SourceHost>(
    controller: &mut PlaybackController<B, H>,
    timeout: Duration,
) {
    use modplayer_core::AnalysisStatus;
    let start = std::time::Instant::now();
    loop {
        controller.tick();
        if controller
            .analysis()
            .is_some_and(|s| s.status == AnalysisStatus::Failed)
        {
            return;
        }
        assert!(
            start.elapsed() < timeout,
            "expected analysis to reach Failed within {timeout:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn analysis_unavailable_label_when_failed_without_peaks() {
    use modplayer_audio_source::DecodedStore;

    let (mut controller, handle, _dir) = active_controller("failed-without-peaks");
    let current = track("a", 200_000);
    let id = current.id.clone();
    controller.queue_replace(vec![current]);
    controller.play();
    controller.tick();

    // No frames ever written before the store fails: `fold_level0` folds
    // nothing, so `Failed`'s `has_any` is false and the snapshot carries no
    // peaks at all (A8's negative case, contracts/analysis-service.md).
    let sample_rate = controller.source_sample_rate();
    let len_frames = (200_000u64 * u64::from(sample_rate)) / 1000;
    let store = DecodedStore::new(sample_rate, len_frames);
    store.set_failed();
    handle.emit(modplayer_audio_source::SourceEvent::DecodedStore { track: id, store });

    wait_for_failed(&mut controller, Duration::from_secs(2));

    assert!(
        controller.analysis().is_some_and(|s| s.peaks.is_none()),
        "sanity: a store failed before any frame was written must carry no peaks"
    );

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let texts = rendered_painted_texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&tr("waveform-unavailable")),
        "expected the \"analysis unavailable\" label when Failed carries no peaks, got {texts:?}"
    );
}

#[test]
fn partial_peaks_stay_when_failed_with_peaks() {
    use modplayer_audio_source::DecodedStore;

    let (mut controller, handle, _dir) = active_controller("failed-with-peaks");
    let current = track("a", 200_000);
    let id = current.id.clone();
    controller.queue_replace(vec![current]);
    controller.play();
    controller.tick();

    // A few level-0 buckets' worth of non-silent tone folds before the
    // store fails (A8's positive case): the resulting `Failed` snapshot
    // must keep those partial peaks rather than discard them.
    let sample_rate = controller.source_sample_rate();
    let len_frames = (200_000u64 * u64::from(sample_rate)) / 1000;
    let partial_frames = 128u64 * 10;
    let store = DecodedStore::new(sample_rate, len_frames);
    let interleaved: Vec<f32> = (0..partial_frames)
        .flat_map(|i| {
            let s = if i % 4 < 2 { 0.6 } else { -0.6 };
            [s, s]
        })
        .collect();
    store.write_frames(0, &interleaved);
    store.set_failed();
    handle.emit(modplayer_audio_source::SourceEvent::DecodedStore { track: id, store });

    wait_for_failed(&mut controller, Duration::from_secs(2));

    assert!(
        controller.analysis().is_some_and(|s| s.peaks.is_some()),
        "sanity: a store with folded buckets before Failed must keep its partial peaks"
    );

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let texts = rendered_painted_texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        !texts.contains(&tr("waveform-unavailable")),
        "partial peaks must render as waveform columns, not the unavailable label: {texts:?}"
    );
}

// -- 010-transport-focus, Phase 5 (US3): the Transport panel toggle --------

/// The header's "Transport" toggle renders alongside "Queue"/"Effects"
/// (contracts/ui-transport-panel.md §1) — closed by default, opening the
/// panel's own title text once toggled.
#[test]
fn transport_toggle_beside_queue_and_effects() {
    let (mut controller, _handle, _dir) = active_controller("transport-toggle-beside");
    controller.queue_replace(vec![track("a", 200_000)]);
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let texts = rendered_texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&tr("queue-toggle")),
        "sanity: the Queue toggle must render, got {texts:?}"
    );
    assert!(
        texts.contains(&tr("effects-toggle")),
        "sanity: the Effects toggle must render, got {texts:?}"
    );
    assert!(
        texts.contains(&tr("transport-toggle")),
        "the Transport toggle must render beside Queue/Effects, got {texts:?}"
    );
    assert!(
        !texts.contains(&tr("transport-panel-title")),
        "the panel must start closed"
    );
}

/// Toggling the Transport panel open survives a track change (mirrors
/// 008's `e_and_header_toggle_panel_and_it_survives_track_change`) — its
/// open/closed state lives in egui temp memory (keyed on one `Context`
/// kept across every render below), not per-track state.
#[test]
fn transport_panel_survives_track_change() {
    let (mut controller, _handle, _dir) = active_controller("transport-panel-survives");
    controller.queue_replace(vec![track("a", 200_000), track("b", 200_000)]);
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let ctx = Context::default();
    ctx.enable_accesskit();
    let texts = |controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
                 artwork: &mut ArtworkCache,
                 waveform: &mut WaveformState| {
        let mut output = ctx.run_ui(default_input(), |ui| {
            modplayer_ui::now_playing::show(ui, controller, artwork, waveform);
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .expect("accesskit_update should be populated once enabled");
        output.drop_without_applying_deltas();
        update
            .nodes
            .iter()
            .flat_map(|(_, node)| [node.value(), node.label()])
            .filter_map(|text| text.map(str::to_string))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
    };

    let initial = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        !initial.contains(&tr("transport-panel-title")),
        "the panel starts closed"
    );

    modplayer_ui::transport_view::toggle_transport_panel(&ctx);
    let opened = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        opened.contains(&tr("transport-panel-title")),
        "toggling must open the panel, got {opened:?}"
    );

    controller.skip_forward();
    let after_track_change = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        after_track_change.contains(&tr("transport-panel-title")),
        "the panel must survive a track change, got {after_track_change:?}"
    );
}

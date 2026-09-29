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

use egui::accesskit::{NodeId, Role, Toggled};
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, SourceCommand, SourceHealth, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::actions::{ActionId, HostAction, ScopeState};
use modplayer_core::markers::{CueSlot, TrackMarkers};
use modplayer_core::plugins::PluginId;
use modplayer_core::settings::SettingsStore;
use modplayer_core::transport::Intent;
use modplayer_core::{NotRegisteredReason, NowPlayingPanel, PlaybackController, tr, tr_args};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::artwork::{ArtworkCache, ArtworkState};
use modplayer_ui::waveform::{DetailWindow, DragOrigin, DragPreview, TimeSpace, WaveformState};
use modplayer_ui::{Shell, actions};

/// 014-design-tokens-and-type-scale (US2, T022): a bare `Context::default()`
/// has none of the token `Style`'s `Name("display")` text style installed,
/// which `now_playing::show`'s current-track title now reaches —
/// panicking on layout otherwise. Install it once, exactly as
/// `App::new`/`App::update` do (mirrors `controls.rs` test's
/// identically-named helper).
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

/// As [`track`], but with an explicit `title` independent of the (short,
/// ASCII, <=64-byte) `id` used for the `TrackId` — needed by the 021
/// (021-transport-bar-and-panel-layout) bar-height/identity tests below,
/// which exercise 5- and 200-character titles (contract B3, B4).
fn track_with_title(id: &str, title: &str, duration_ms: u32) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!()),
        title,
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
    // Bundled plugins stay un-launched: `launch()` only spawns records
    // flagged `enabled`, and Key & Tempo (013) would otherwise register
    // its panel into the Now Playing dock asynchronously, shifting the
    // layout between one frame and the next under the tests here that
    // measure a widget's bounds and then click it.
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
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
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
    let ctx = fresh_ctx();
    let output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
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

/// A tall viewport for the Phase 5 (US3) card tests below: `egui::Frame`
/// only paints its background when `ui.is_rect_visible` — `rect.
/// intersects(clip_rect)` — so a panel far enough down a merely
/// `default_input`-sized (800×600) screen would silently paint no card
/// rect at all once a loaded track's heading/waveform/Markers block (this
/// file's own tallest, most variable content) pushes it past the fold,
/// even though its heading (an unconditional accesskit call, not gated on
/// visibility) still shows up. Tall enough that every one of the four
/// cards stays inside the clip rect regardless.
fn tall_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 4000.0))),
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
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
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
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    // 021-transport-bar-and-panel-layout (contract B2.4): the bar's own
    // master-volume `Slider` now sits above the waveform on screen, so
    // "the topmost `Role::Slider`" no longer picks the overview out —
    // matched by its own accessible name (`transport-seek`) instead.
    let mut best: Option<egui::accesskit::Rect> = None;
    for (_, node) in &update.nodes {
        if node.role() != Role::Slider || node.label() != Some(tr("transport-seek").as_str()) {
            continue;
        }
        if let Some(bounds) = node.bounds()
            && best.as_ref().is_none_or(|b| bounds.y0 < b.y0)
        {
            best = Some(bounds);
        }
    }
    let bounds = best.expect("Now Playing must render a `transport-seek` Role::Slider node");
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
    let ctx = fresh_ctx();
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
    let ctx = fresh_ctx();
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
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
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
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
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
    let ctx = fresh_ctx();
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
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();

    let mut drag = default_input();
    drag.events.push(Event::PointerMoved(right));
    let output = ctx.run_ui(drag, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
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
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
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
    let ctx = fresh_ctx();
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
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
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
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
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
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "release must commit the seek exactly once (T6: stopped -> Paused)"
    );
    let position_after_release = controller.position();

    let output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
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
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // First frame: establish the detail window and read its screen rect
    // from its own AccessKit node (named `waveform-detail`, distinct from
    // the overview's `transport-seek`).
    let bounds = {
        let mut output = ctx.run_ui(default_input(), |ui| {
            modplayer_ui::now_playing::show(
                ui,
                &mut controller,
                &mut artwork,
                &mut waveform,
                &mut modplayer_ui::section_memory::SectionMemory::default(),
            )
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
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
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
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
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
        let ctx = fresh_ctx();
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
    let ctx = fresh_ctx();
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
        let ctx = fresh_ctx();
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
    // 021-transport-bar-and-panel-layout (contract C2, superseding 016
    // C10): a collapsed card still renders its header — `transport-
    // panel-title` is present either way now — so "closed" is checked
    // through a body-only marker instead: `transport-policy`, only drawn
    // while `add_contents` runs (i.e. while open).
    assert!(
        texts.contains(&tr("transport-panel-title")),
        "a collapsed card still shows its header, got {texts:?}"
    );
    assert!(
        !texts.contains(&tr("transport-policy")),
        "the panel must start closed"
    );
}

/// Toggling the Transport panel open survives a track change (mirrors
/// 008's `e_and_header_toggle_panel_and_it_survives_track_change`) — its
/// open/closed state is persisted through the controller
/// (016-list-row-and-panel-components, FR-019), not per-track state.
#[test]
fn transport_panel_survives_track_change() {
    let (mut controller, _handle, _dir) = active_controller("transport-panel-survives");
    controller.queue_replace(vec![track("a", 200_000), track("b", 200_000)]);
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let texts = |controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
                 artwork: &mut ArtworkCache,
                 waveform: &mut WaveformState| {
        let mut output = ctx.run_ui(default_input(), |ui| {
            modplayer_ui::now_playing::show(
                ui,
                controller,
                artwork,
                waveform,
                &mut modplayer_ui::section_memory::SectionMemory::default(),
            );
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

    // 021-transport-bar-and-panel-layout (contract C2, superseding 016
    // C10): a collapsed card still renders its header, so "closed" is
    // checked through a body-only marker (`transport-policy`) instead.
    let initial = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        initial.contains(&tr("transport-panel-title")),
        "a collapsed card still shows its header, got {initial:?}"
    );
    assert!(
        !initial.contains(&tr("transport-policy")),
        "the panel starts closed"
    );

    modplayer_ui::transport_view::toggle_transport_panel(&mut controller);
    let opened = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        opened.contains(&tr("transport-policy")),
        "toggling must open the panel, got {opened:?}"
    );

    controller.skip_forward();
    let after_track_change = texts(&mut controller, &mut artwork, &mut waveform);
    assert!(
        after_track_change.contains(&tr("transport-policy")),
        "the panel must survive a track change, got {after_track_change:?}"
    );
}

// -- 016-list-row-and-panel-components, Phase 5 (US3): the four Now
// Playing blocks read as cards, and Effect Chain/Transport/Queue persist
// their open/closed state (contracts/panel-card.md C5/C6/C9/C10/C13, P3)
// -----------------------------------------------------------------------

/// One AccessKit node's role/label/value/bounds (mirrors
/// `effects_view.rs`'s own `AccessNode`) — the tests below need bounds (to
/// click a control-row switch, or to check a widget's on-screen rect
/// against a viewport), which `NodeInfo` above doesn't carry.
struct PanelNode {
    role: Role,
    label: Option<String>,
    value: Option<String>,
    bounds: Option<Rect>,
    /// `Toggled::True`/`False` for a switch/disclosure (021-transport-bar-
    /// and-panel-layout contracts B6, C3) — `None` for a node that isn't a
    /// toggle at all.
    toggled: Option<Toggled>,
}

impl PanelNode {
    /// `Response::fill_accesskit_node_from_widget_info` puts `Role::Label`
    /// text in `value` and every other role's in `label` (mirrors
    /// `rendered_texts`'s own doc comment).
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

/// One frame's AccessKit nodes (with bounds) and raw painted shapes,
/// gathered together so a card's fill rect and its heading node can be
/// cross-checked in the same render.
struct PanelFrame {
    nodes: Vec<PanelNode>,
    shapes: Vec<egui::Shape>,
}

fn render_panel_frame<
    B: modplayer_audio_io::OutputBackend,
    H: modplayer_audio_source::SourceHost,
>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    input: RawInput,
) -> PanelFrame {
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let shapes: Vec<egui::Shape> = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();

    let nodes = update
        .nodes
        .iter()
        .map(|(_, node)| PanelNode {
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
            toggled: node.toggled(),
        })
        .collect();
    PanelFrame { nodes, shapes }
}

/// Every `Shape::Rect` filled with `roles.surface_raised`, `radius::MD`
/// cornered and **stroked** — one per open `panel_card` (contract C2/C5).
/// `roles.surface_raised` alone isn't enough of a signal: `theme/style.rs`
/// also paints it as the default *inactive-widget* background (buttons,
/// combo boxes), just with `radius::SM` and a 1 px divider `bg_stroke` —
/// `panel_card`'s own frame draws neither (C2's "no stroke"), which is
/// what actually tells the two apart here. No `set_theme` call in this
/// file pins which of egui's two themes a headless `Context` resolves to,
/// so both the light and dark `Roles.surface_raised` count.
fn card_rects(shapes: &[egui::Shape]) -> Vec<Rect> {
    let light = modplayer_ui::theme::roles(&egui::Visuals::light()).surface_raised;
    let dark = modplayer_ui::theme::roles(&egui::Visuals::dark()).surface_raised;
    shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Rect(rect_shape)
                if (rect_shape.fill == light || rect_shape.fill == dark)
                    && rect_shape.corner_radius == modplayer_ui::theme::radius::MD
                    && rect_shape.stroke.width < 0.5 =>
            {
                Some(rect_shape.rect)
            }
            _ => None,
        })
        .collect()
}

fn find_panel_node<'a>(nodes: &'a [PanelNode], role: Role, name: &str) -> Option<&'a PanelNode> {
    nodes
        .iter()
        .find(|node| node.role == role && node.accessible_name() == Some(name))
}

/// Discard one frame's output — an untimed warm-up
/// (mirrors `library_view.rs`'s own warm-up pattern): a freshly opened
/// panel's `ScrollArea`/`Frame` sizing settles one frame after the id
/// first appears, so a layout fact this file's card/bounds tests pin
/// (a card's rect, a heading's position) is asserted against the *second*
/// render on a given `ctx`, never the first.
fn warm_up<B: modplayer_audio_io::OutputBackend, H: modplayer_audio_source::SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    input: RawInput,
) {
    let output = ctx.run_ui(input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();
}

/// Press then release the primary button at `pos`, in two separate frames
/// (mirrors `effects_view.rs`'s own `click_at`).
fn click_now_playing<
    B: modplayer_audio_io::OutputBackend,
    H: modplayer_audio_source::SourceHost,
>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    pos: Pos2,
) {
    let mut press_input = default_input();
    press_input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press_input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();

    let mut release_input = default_input();
    release_input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release_input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();
}

/// As [`click_now_playing`], but over an explicit `input` (screen size)
/// instead of the fixed 800x600 `default_input` — needed whenever a test
/// must click a control whose bounds it measured at a different viewport
/// (021-transport-bar-and-panel-layout T-B1).
fn click_now_playing_at<
    B: modplayer_audio_io::OutputBackend,
    H: modplayer_audio_source::SourceHost,
>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    input: &RawInput,
    pos: Pos2,
) {
    let mut press_input = input.clone();
    press_input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press_input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();

    let mut release_input = input.clone();
    release_input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release_input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();
}

/// As [`click_now_playing`], but captures and returns the release frame's
/// own [`PanelFrame`] — needed by the same-frame header/bar-toggle
/// consistency test (021-transport-bar-and-panel-layout, contract C3,
/// research R7: a header click's `request_discard` re-runs this same pass,
/// so the *release* frame's own output already carries the toggled state).
fn click_now_playing_capturing<
    B: modplayer_audio_io::OutputBackend,
    H: modplayer_audio_source::SourceHost,
>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    pos: Pos2,
) -> PanelFrame {
    let mut press_input = default_input();
    press_input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press_input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();

    let mut release_input = default_input();
    release_input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let mut output = ctx.run_ui(release_input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let shapes: Vec<egui::Shape> = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();

    let nodes = update
        .nodes
        .iter()
        .map(|(_, node)| PanelNode {
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
            toggled: node.toggled(),
        })
        .collect();
    PanelFrame { nodes, shapes }
}

/// C5 (contracts/panel-card.md): with a track loaded and Effect Chain/
/// Transport/Queue all open, all four Now Playing blocks (Markers, Effect
/// Chain, Transport, Queue) render through the shared `panel_card` — four
/// `roles.surface_raised`-filled rects in one frame.
#[test]
fn four_panels_render_as_cards_when_all_open() {
    let (mut controller, _handle, _dirs) = active_controller("c5-four-cards");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let frame = render_panel_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    assert_eq!(
        card_rects(&frame.shapes).len(),
        4,
        "expected one card per panel (Markers, Effect Chain, Transport, Queue)"
    );
}

/// C6 (contracts/panel-card.md, research R7): no panel renders its header
/// twice — exactly one `Role::Heading` node per panel, each with its
/// expected name.
#[test]
fn each_open_panel_has_exactly_one_heading_node() {
    let (mut controller, _handle, _dirs) = active_controller("c6-single-heading");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let frame = render_panel_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    for header in [
        tr("markers-panel"),
        tr("effects-panel-title"),
        tr("transport-panel-title"),
        tr("queue-panel-title"),
    ] {
        let matches = frame
            .nodes
            .iter()
            .filter(|node| {
                node.role == Role::Heading && node.accessible_name() == Some(header.as_str())
            })
            .count();
        assert_eq!(
            matches, 1,
            "expected exactly one Heading node named `{header}`, got {matches}"
        );
    }
}

/// T-C5 (021-transport-bar-and-panel-layout, contract C1, C5; supersedes
/// 016 C9's "no collapse/expand affordance is added to any header"):
/// every card — including Markers, which still has no *bar* toggle —
/// gains its own header disclosure button now, reachable with Tab and
/// activated with Space/Enter. The three bar switches (and their
/// `Q`/`E`/`T` shortcuts) remain, unchanged, as the Queue/Effects/
/// Transport cards' *other* open/close path.
#[test]
fn every_card_has_exactly_one_header_disclosure() {
    let (mut controller, _handle, _dirs) = active_controller("c9-disclosure");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let frame = render_panel_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    let button_names: Vec<String> = frame
        .nodes
        .iter()
        .filter(|node| node.role == Role::Button)
        .filter_map(|node| node.accessible_name().map(str::to_string))
        .collect();
    let disclosure_count = button_names
        .iter()
        .filter(|name| {
            let lower = name.to_lowercase();
            lower.contains("collaps") || lower.contains("expand")
        })
        .count();
    assert_eq!(
        disclosure_count, 4,
        "expected one collapse/expand disclosure per card (Markers, Effect \
         Chain, Transport, Queue): {button_names:?}"
    );

    // The three bar switches are still present, unchanged (FR-006, C3).
    for key in ["queue-toggle", "effects-toggle", "transport-toggle"] {
        assert!(
            button_names.contains(&tr(key)),
            "expected the `{key}` switch, got {button_names:?}"
        );
    }
}

/// T-C2 (021-transport-bar-and-panel-layout, contract C2, C4; replaces the
/// 016 C10 test "a closed panel draws nothing at all"): a closed panel now
/// draws its header (and its card frame) but none of its body nodes. With
/// no track loaded, Markers doesn't render at all (contract C4); the other
/// three panels default closed but each still contributes one card and one
/// heading — just no body content.
#[test]
fn a_closed_panel_contributes_a_header_only_card() {
    let (mut controller, _handle, _dirs) = active_controller("c10-closed-panel");
    assert!(!controller.now_playing_panel_open(NowPlayingPanel::EffectChain));
    assert!(!controller.now_playing_panel_open(NowPlayingPanel::Transport));
    assert!(!controller.now_playing_panel_open(NowPlayingPanel::Queue));

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        default_input(),
    );
    let frame = render_panel_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        default_input(),
    );

    assert_eq!(
        card_rects(&frame.shapes).len(),
        3,
        "no track is loaded (Markers absent), but the other three panels \
         each still contribute a header-only card"
    );
    for header in [
        tr("effects-panel-title"),
        tr("transport-panel-title"),
        tr("queue-panel-title"),
    ] {
        assert!(
            frame.nodes.iter().any(|node| node.role == Role::Heading
                && node.accessible_name() == Some(header.as_str())),
            "a closed panel must still expose its heading node: `{header}`"
        );
    }
    // No body content leaks through while closed (contract C2).
    for body_marker in [
        tr("effects-add-node"),
        tr("transport-policy"),
        tr("queue-shuffle"),
    ] {
        assert!(
            !frame
                .nodes
                .iter()
                .any(|node| node.accessible_name() == Some(body_marker.as_str())),
            "a collapsed card must not draw its body: `{body_marker}`"
        );
    }
}

/// C13 (contracts/panel-card.md, FR-023/FR-024, Edge Case): at a 960×640
/// viewport with Effect Chain/Transport/Queue all open, the master-volume
/// row, the peak meter and the Queue card all stay inside the viewport —
/// `effects_panel_reserved_height` must book at least the old 140.0 plus
/// the new card insets, so the 2026-09-19 clipping defect cannot recur.
#[test]
fn reserved_height_keeps_volume_meter_and_queue_card_in_a_960x640_viewport() {
    // No track queued: the waveform/Markers block (`current_track().is_
    // some()`-gated) stays out of the way, isolating what this contract
    // actually pins — the reserved-height sum below the Effect Chain
    // panel — from the unrelated, unbounded height of the waveform/
    // Markers section above it.
    let (mut controller, _handle, _dirs) = active_controller("c13-viewport");
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = RawInput {
        screen_rect: Some(viewport),
        ..Default::default()
    };
    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        input.clone(),
    );
    let frame = render_panel_frame(&ctx, &mut controller, &mut artwork, &mut waveform, input);

    let master_volume = find_panel_node(&frame.nodes, Role::Label, &tr("master-volume"))
        .and_then(|node| node.bounds)
        .expect("the master-volume caption must render");
    assert!(
        viewport.contains_rect(master_volume),
        "master-volume row must stay inside the viewport: {master_volume:?}"
    );

    let peak_meter = find_panel_node(&frame.nodes, Role::Label, &tr("peak-meter"))
        .and_then(|node| node.bounds)
        .expect("the peak-meter caption must render");
    assert!(
        viewport.contains_rect(peak_meter),
        "peak meter must stay inside the viewport: {peak_meter:?}"
    );

    let queue_heading = find_panel_node(&frame.nodes, Role::Heading, &tr("queue-panel-title"))
        .and_then(|node| node.bounds)
        .expect("the Queue panel heading must render");
    let queue_card = card_rects(&frame.shapes)
        .into_iter()
        .find(|rect| rect.y_range().contains(queue_heading.min.y))
        .expect("expected a card rect containing the Queue heading");
    assert!(
        viewport.contains_rect(queue_card),
        "the Queue card must stay inside the viewport: {queue_card:?}"
    );
}

/// P3 (contracts/panel-card.md, FR-019, US3 Scenario 5): a click on the
/// control-row switch and an invoked `HostAction::Toggle*` write through
/// the same setter (`PlaybackController::set_now_playing_panel_open`) —
/// either path persists to `settings.toml` immediately, so a fresh
/// controller built over the same store reads back the change right away.
#[test]
fn switch_click_and_host_action_toggles_persist_through_a_settings_reload() {
    let (mut controller, _handle, dirs) = active_controller("p3-persist");
    controller.queue_replace(vec![track("a", 200_000)]);
    let settings_path = dirs.0.path().join("settings.toml");

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        default_input(),
    );
    let frame = render_panel_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        default_input(),
    );
    let queue_switch = find_panel_node(&frame.nodes, Role::Button, &tr("queue-toggle"))
        .and_then(|node| node.bounds)
        .expect("the Queue switch must render");
    click_now_playing(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        queue_switch.center(),
    );

    assert!(
        controller.now_playing_panel_open(NowPlayingPanel::Queue),
        "clicking the Queue switch must open it"
    );
    let reloaded = PlaybackController::new(
        FakeBackend::new(vec![]),
        ScriptedHost::new(),
        SettingsStore::with_path(settings_path.clone()),
    );
    assert!(
        reloaded.now_playing_panel_open(NowPlayingPanel::Queue),
        "the switch's toggle must persist to settings.toml immediately"
    );

    let mut shell = Shell::default();
    for (action, panel) in [
        (HostAction::ToggleEffectChain, NowPlayingPanel::EffectChain),
        (HostAction::ToggleTransportPanel, NowPlayingPanel::Transport),
        (HostAction::ToggleQueue, NowPlayingPanel::Queue),
    ] {
        let before = controller.now_playing_panel_open(panel);
        actions::invoke(
            actions::Invocation {
                action: ActionId::Host(action),
                repeat: false,
            },
            &mut controller,
            &mut shell,
            &mut waveform,
            &ctx,
        );
        assert_ne!(
            controller.now_playing_panel_open(panel),
            before,
            "{action:?} must flip its panel"
        );
        let reloaded = PlaybackController::new(
            FakeBackend::new(vec![]),
            ScriptedHost::new(),
            SettingsStore::with_path(settings_path.clone()),
        );
        assert_eq!(
            reloaded.now_playing_panel_open(panel),
            controller.now_playing_panel_open(panel),
            "{action:?} must persist immediately, from the same setter the switch uses"
        );
    }
}

// ---------------------------------------------------------------------
// 021-transport-bar-and-panel-layout, Phase 3 (User Story 1): the pinned
// transport bar (contracts B1-B7), the single scroll region (S1, S3), and
// the collapsible cards (C3, C4, C6) — see contracts/ui-now-playing-
// layout.md.
// ---------------------------------------------------------------------

/// T-B1 (contract B1, FR-001, FR-010, SC-001): with all four panels open
/// and the scroll region seeded to its maximum offset (021 research R2's
/// "egui clamps it to the new content height on show"), every bar
/// control's AccessKit bounds still sit inside the window's content rect,
/// and a click on play/pause still changes `Intent` — the bar never
/// scrolls away and never stops responding.
#[test]
fn bar_controls_stay_in_bounds_and_clickable_after_scrolling_to_max() {
    let (mut controller, _handle, _dirs) = active_controller("b1-scrolled-to-max");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let content_rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = RawInput {
        screen_rect: Some(content_rect),
        ..Default::default()
    };

    // Warm up (unscrolled) so widget/layout state settles before measuring
    // (mirrors this file's own `warm_up` convention).
    let output = ctx.run_ui(input.clone(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();

    // Seed the scroll region's offset to an enormous value: `SectionMemory
    // ::scroll_area`'s doc comment says egui clamps a seeded offset to the
    // actual content height on show, so this lands at the true maximum
    // without needing to simulate physical wheel input (021 data-model.md
    // §5, research R2).
    let mut memory = modplayer_ui::section_memory::SectionMemory::default();
    memory.record(modplayer_ui::section_memory::ViewKey::NowPlaying, 1.0e6);
    // A different key, purely to flip `shown_last_frame` away from
    // `NowPlaying` so the seed above is actually applied (`scroll_area`
    // only seeds the first frame a view is shown again after being away).
    memory.record(modplayer_ui::section_memory::ViewKey::Search, 0.0);

    let mut output = ctx.run_ui(input.clone(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
        );
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let nodes: Vec<PanelNode> = update
        .nodes
        .iter()
        .map(|(_, node)| PanelNode {
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
            toggled: node.toggled(),
        })
        .collect();

    let bar_control_labels = [
        tr("transport-skip-back"),
        tr("transport-play"),
        tr("transport-stop"),
        tr("transport-skip-forward"),
        tr("queue-toggle"),
        tr("effects-toggle"),
        tr("transport-toggle"),
    ];
    for label in &bar_control_labels {
        let bounds = find_panel_node(&nodes, Role::Button, label)
            .and_then(|node| node.bounds)
            .unwrap_or_else(|| panic!("expected the `{label}` bar control to render"));
        assert!(
            content_rect.contains_rect(bounds),
            "`{label}` must stay inside the content rect at max scroll: {bounds:?}"
        );
    }

    // The bar still responds to a click at max scroll (FR-010).
    let play_pause = find_panel_node(&nodes, Role::Button, &tr("transport-play"))
        .and_then(|node| node.bounds)
        .expect("play/pause must render");
    assert_ne!(controller.transport_state().intent, Intent::Playing);
    click_now_playing_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        &input,
        play_pause.center(),
    );
    assert_eq!(
        controller.transport_state().intent,
        Intent::Playing,
        "play/pause must still respond to a click at max scroll"
    );
}

/// The pinned bar's own outer height (contract B3), read back from
/// `PanelState` after two frames on a fresh context (the second frame
/// mirrors this file's own `warm_up` convention, letting any layout
/// settle before the measurement is trusted).
#[allow(clippy::too_many_arguments)]
fn bar_outer_height(
    label: &str,
    title_len: Option<usize>,
    all_open: bool,
    playing: bool,
    width: f32,
    height: f32,
    scroll_max: bool,
) -> f32 {
    let (mut controller, _handle, _dirs) = active_controller(label);
    if let Some(len) = title_len {
        let title: String = "T".repeat(len);
        controller.queue_replace(vec![track_with_title("a", &title, 200_000)]);
        controller.set_now_playing_panel_open(NowPlayingPanel::Markers, all_open);
        if playing {
            controller.play();
        }
    }
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, all_open);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, all_open);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, all_open);

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, height))),
        ..Default::default()
    };

    let mut memory = modplayer_ui::section_memory::SectionMemory::default();
    if scroll_max {
        memory.record(modplayer_ui::section_memory::ViewKey::NowPlaying, 1.0e6);
        memory.record(modplayer_ui::section_memory::ViewKey::Search, 0.0);
    }

    // Warm-up frame.
    let output = ctx.run_ui(input.clone(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
        );
    });
    output.drop_without_applying_deltas();

    // Measured frame.
    let output = ctx.run_ui(input, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
        );
    });
    output.drop_without_applying_deltas();

    let bar_id = egui::Id::new("now-playing-transport-bar");
    egui::PanelState::load(&ctx, bar_id)
        .expect("the transport bar must register its PanelState after a frame")
        .size()
        .y
}

/// T-B3 (contract B3, FR-002, FR-011, SC-003): the bar's outer height
/// stays within +-1px of a baseline across window height, scroll offset,
/// panel-open combination, playback intent, title length, and the
/// no-track state — perturbed one axis at a time from the baseline
/// (960x640, no scroll, all panels closed, paused, a 5-char title).
#[test]
fn bar_height_invariant_across_window_scroll_panel_state_and_title() {
    let baseline = bar_outer_height("b3-baseline", Some(5), false, false, 960.0, 640.0, false);

    for height in [640.0, 820.0, 1200.0] {
        let measured = bar_outer_height(
            &format!("b3-height-{height}"),
            Some(5),
            false,
            false,
            960.0,
            height,
            false,
        );
        assert!(
            (measured - baseline).abs() <= 1.0,
            "window height {height}: bar height {measured} vs baseline {baseline}"
        );
    }

    for scroll_max in [false, true] {
        let measured = bar_outer_height(
            &format!("b3-scroll-{scroll_max}"),
            Some(5),
            true,
            false,
            960.0,
            640.0,
            scroll_max,
        );
        assert!(
            (measured - baseline).abs() <= 1.0,
            "scroll_max={scroll_max}: bar height {measured} vs baseline {baseline}"
        );
    }

    for all_open in [false, true] {
        let measured = bar_outer_height(
            &format!("b3-panels-{all_open}"),
            Some(5),
            all_open,
            false,
            960.0,
            640.0,
            false,
        );
        assert!(
            (measured - baseline).abs() <= 1.0,
            "all_open={all_open}: bar height {measured} vs baseline {baseline}"
        );
    }

    for playing in [false, true] {
        let measured = bar_outer_height(
            &format!("b3-intent-{playing}"),
            Some(5),
            false,
            playing,
            960.0,
            640.0,
            false,
        );
        assert!(
            (measured - baseline).abs() <= 1.0,
            "playing={playing}: bar height {measured} vs baseline {baseline}"
        );
    }

    for len in [5usize, 200] {
        let measured = bar_outer_height(
            &format!("b3-title-{len}"),
            Some(len),
            false,
            false,
            960.0,
            640.0,
            false,
        );
        assert!(
            (measured - baseline).abs() <= 1.0,
            "title_len={len}: bar height {measured} vs baseline {baseline}"
        );
    }

    let no_track = bar_outer_height("b3-no-track", None, false, false, 960.0, 640.0, false);
    assert!(
        (no_track - baseline).abs() <= 1.0,
        "no track: bar height {no_track} vs baseline {baseline}"
    );
}

/// T-B4 (contract B4, edge case "long title"): a 200-character title
/// renders on one painted line (truncated with an ellipsis, never
/// wrapped), while the identity group's AccessKit label
/// (`now-playing-bar-identity`) still carries the full, untruncated
/// title and artist.
#[test]
fn long_title_renders_one_line_with_full_text_in_the_a11y_label() {
    let (mut controller, _handle, _dirs) = active_controller("b4-long-title");
    let title = "T".repeat(200);
    controller.queue_replace(vec![track_with_title("a", &title, 200_000)]);

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 640.0))),
        ..Default::default()
    };
    let frame = render_panel_frame(&ctx, &mut controller, &mut artwork, &mut waveform, input);

    let title_galley = frame
        .shapes
        .iter()
        .find_map(|shape| match shape {
            egui::Shape::Text(text_shape) if text_shape.galley.job.text == title => {
                Some(text_shape.galley.clone())
            }
            _ => None,
        })
        .expect("the bar identity title must paint as a Text shape");
    assert_eq!(
        title_galley.rows.len(),
        1,
        "a 200-char title must render on exactly one line"
    );
    assert!(
        title_galley.elided,
        "a 200-char title must be truncated with an ellipsis, not wrapped"
    );

    let artist = "Artist".to_string();
    let full_name = tr_args(
        "now-playing-bar-identity",
        &[("title", title.clone()), ("artist", artist)],
    );
    assert!(
        frame
            .nodes
            .iter()
            .any(|node| node.accessible_name() == Some(full_name.as_str())),
        "the identity group's AccessKit label must carry the full, untruncated title/artist"
    );
}

/// T-B6 (contract B6, B2.5, FR-019): the Queue/Effects/Transport toggles
/// report `Toggled` matching their own open flag, and the toggle group
/// sits at least `space::XL` from skip forward's own rect.
#[test]
fn bar_toggles_report_toggled_and_stay_xl_from_skip_forward() {
    let (mut controller, _handle, _dirs) = active_controller("b6-toggles");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, false);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, true);

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    // Wide enough that the bar draws in one row (no wrap), so the gap
    // measured below reads the actual `XL` spacing (research R4).
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1600.0, 640.0))),
        ..Default::default()
    };
    let frame = render_panel_frame(&ctx, &mut controller, &mut artwork, &mut waveform, input);

    for (key, expected) in [
        ("queue-toggle", Toggled::True),
        ("effects-toggle", Toggled::False),
        ("transport-toggle", Toggled::True),
    ] {
        let node = find_panel_node(&frame.nodes, Role::Button, &tr(key))
            .unwrap_or_else(|| panic!("expected the `{key}` switch to render"));
        assert_eq!(
            node.toggled,
            Some(expected),
            "`{key}` must report `Toggled` matching its own open flag"
        );
    }

    let skip_forward = find_panel_node(&frame.nodes, Role::Button, &tr("transport-skip-forward"))
        .and_then(|node| node.bounds)
        .expect("skip forward must render");
    let queue_toggle = find_panel_node(&frame.nodes, Role::Button, &tr("queue-toggle"))
        .and_then(|node| node.bounds)
        .expect("the Queue toggle must render");
    let gap = queue_toggle.min.x - skip_forward.max.x;
    assert!(
        gap >= modplayer_ui::theme::space::XL - 0.5,
        "the toggle group must sit at least `space::XL` from skip forward: gap={gap}"
    );
}

/// T-S1 (contract S1, FR-003): source-level check that Now Playing builds
/// its one scroll region through `SectionMemory::scroll_area` and never a
/// raw, nested `ScrollArea`, and that `effects_panel_reserved_height` — the
/// old inner-scroll reservation this feature deletes — is gone.
#[test]
fn source_has_exactly_one_scroll_region_and_no_reserved_height_calc() {
    let now_playing_src = include_str!("../src/now_playing.rs");
    assert!(
        !now_playing_src.contains("ScrollArea"),
        "now_playing.rs must build its one scroll region through \
         `SectionMemory::scroll_area`, not a raw `ScrollArea` (contract S1)"
    );
    assert_eq!(
        now_playing_src.matches(".scroll_area(").count(),
        1,
        "now_playing.rs must contain exactly one scroll region (contract S1)"
    );

    let effects_src = include_str!("../src/effects_view.rs");
    assert!(
        !effects_src.contains("ScrollArea"),
        "the Effect Chain's own inner scroll area must be deleted (contract S1)"
    );
    assert!(
        !effects_src.contains("effects_panel_reserved_height")
            && !now_playing_src.contains("effects_panel_reserved_height"),
        "`effects_panel_reserved_height` must be deleted (contract S1)"
    );
}

/// T-S3 (contract S3, FR-005): no `Separator`/hairline is painted in the
/// scroll region — source-level for the widgets it hosts, plus a rendered
/// check that adjacent cards are separated by exactly `space::XL`.
#[test]
fn scroll_region_has_no_separator_and_cards_are_xl_spaced() {
    for src in [
        include_str!("../src/now_playing.rs"),
        include_str!("../src/markers.rs"),
        include_str!("../src/effects_view.rs"),
        include_str!("../src/transport_view.rs"),
        include_str!("../src/queue_view.rs"),
    ] {
        assert!(
            !src.contains(".separator("),
            "the scroll region must use `space::XL` gaps, not a `Separator`/hairline (contract S3)"
        );
    }

    let (mut controller, _handle, _dirs) = active_controller("s3-xl-gaps");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let frame = render_panel_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    let mut rects = card_rects(&frame.shapes);
    rects.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
    assert_eq!(
        rects.len(),
        4,
        "expected all four cards to render, got {}",
        rects.len()
    );
    // Each gap is `ui.add_space(space::XL)` plus the vertical layout's own
    // automatic `item_spacing.y` between successive widgets — a constant
    // baseline every gap in this vertical stack shares equally, so `space
    // ::XL` shows up as "at least XL, and every gap identical" rather than
    // as the literal on-screen distance.
    let gaps: Vec<f32> = rects
        .windows(2)
        .map(|pair| pair[1].min.y - pair[0].max.y)
        .collect();
    for &gap in &gaps {
        assert!(
            gap >= modplayer_ui::theme::space::XL - 0.5,
            "adjacent cards must be separated by at least `space::XL`: gap={gap}"
        );
    }
    let first = gaps[0];
    for &gap in &gaps {
        assert!(
            (gap - first).abs() <= 1.0,
            "every card gap must be the same `space::XL` spacing: {gaps:?}"
        );
    }
}

/// T-C3 (contract C3, FR-006, edge case "same frame", research R7): a
/// header disclosure click closes the Queue card and, in that SAME
/// output, the bar's own Queue toggle already reports `Toggled::False` —
/// `request_discard` re-runs the pass so the two never disagree for even
/// one frame.
#[test]
fn header_disclosure_click_and_bar_toggle_agree_in_the_same_output() {
    let (mut controller, _handle, _dirs) = active_controller("c3-same-frame");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let frame = render_panel_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    let queue_title = tr("queue-panel-title");
    let collapse_name = tr_args("panel-collapse", &[("panel", queue_title.clone())]);
    let disclosure = find_panel_node(&frame.nodes, Role::Button, &collapse_name)
        .and_then(|node| node.bounds)
        .expect("the Queue card's header disclosure must render while open");

    let after = click_now_playing_capturing(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        disclosure.center(),
    );

    assert!(
        !controller.now_playing_panel_open(NowPlayingPanel::Queue),
        "the header click must close the Queue panel"
    );
    let expand_name = tr_args("panel-expand", &[("panel", queue_title)]);
    assert!(
        after
            .nodes
            .iter()
            .any(|node| node.accessible_name() == Some(expand_name.as_str())),
        "the header disclosure must show `Expand Queue` in the same output"
    );
    let bar_toggle = find_panel_node(&after.nodes, Role::Button, &tr("queue-toggle"))
        .expect("the bar's Queue toggle must render");
    assert_eq!(
        bar_toggle.toggled,
        Some(Toggled::False),
        "the bar toggle must report `Toggled::False` in the same output as the header click"
    );
}

/// T-C4 (contract C4, FR-004): the cards render Markers, Effect Chain,
/// Transport, Queue, top to bottom; without a track, Markers is absent but
/// the other three still render in the same order.
#[test]
fn cards_render_in_markers_effects_transport_queue_order_and_presence_rules() {
    let (mut controller, _handle, _dirs) = active_controller("c4-with-track");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let frame = render_panel_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    let headings = [
        tr("markers-panel"),
        tr("effects-panel-title"),
        tr("transport-panel-title"),
        tr("queue-panel-title"),
    ];
    let mut ys = Vec::new();
    for name in &headings {
        let bounds = find_panel_node(&frame.nodes, Role::Heading, name)
            .and_then(|node| node.bounds)
            .unwrap_or_else(|| panic!("expected the `{name}` heading to render"));
        ys.push(bounds.min.y);
    }
    assert!(
        ys.windows(2).all(|w| w[0] < w[1]),
        "cards must render Markers, Effect Chain, Transport, Queue top to bottom: {ys:?}"
    );

    let (mut controller2, _handle2, _dirs2) = active_controller("c4-no-track");
    controller2.set_now_playing_panel_open(NowPlayingPanel::EffectChain, true);
    controller2.set_now_playing_panel_open(NowPlayingPanel::Transport, true);
    controller2.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx2 = fresh_ctx();
    ctx2.enable_accesskit();
    let mut artwork2 = ArtworkCache::new();
    let mut waveform2 = WaveformState::default();
    warm_up(
        &ctx2,
        &mut controller2,
        &mut artwork2,
        &mut waveform2,
        tall_input(),
    );
    let frame2 = render_panel_frame(
        &ctx2,
        &mut controller2,
        &mut artwork2,
        &mut waveform2,
        tall_input(),
    );

    assert!(
        !frame2.nodes.iter().any(|node| node.role == Role::Heading
            && node.accessible_name() == Some(tr("markers-panel").as_str())),
        "Markers must not render without a track (contract C4)"
    );
    let mut ys2 = Vec::new();
    for name in [
        tr("effects-panel-title"),
        tr("transport-panel-title"),
        tr("queue-panel-title"),
    ] {
        let bounds = find_panel_node(&frame2.nodes, Role::Heading, &name)
            .and_then(|node| node.bounds)
            .unwrap_or_else(|| panic!("expected the `{name}` heading to render"));
        ys2.push(bounds.min.y);
    }
    assert!(
        ys2.windows(2).all(|w| w[0] < w[1]),
        "the other three cards must still render top to bottom without a track: {ys2:?}"
    );
}

/// T-C6 (contract C6, FR-004): the master-volume row and the peak meter
/// render exactly once each, and only inside the pinned bar — never again
/// below it, inside the scroll region.
#[test]
fn master_volume_and_peak_meter_render_only_once_inside_the_bar() {
    let (mut controller, _handle, _dirs) = active_controller("c6-volume-in-bar");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, true);
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, true);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    warm_up(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let frame = render_panel_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    let bar_id = egui::Id::new("now-playing-transport-bar");
    let bar_state = egui::PanelState::load(&ctx, bar_id)
        .expect("the bar must register its PanelState after a frame");
    let bar_bottom = bar_state.outer_rect.bottom();

    for label in [tr("master-volume"), tr("peak-meter")] {
        // `Role::Label` only: text nodes also carry an implicit
        // `Role::TextRun` child with the same name (AccessKit's own tree
        // shape), which isn't a second occurrence of this widget.
        let matches: Vec<&PanelNode> = frame
            .nodes
            .iter()
            .filter(|node| {
                node.role == Role::Label && node.accessible_name() == Some(label.as_str())
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected exactly one `{label}` node, got {}",
            matches.len()
        );
        let bounds = matches[0]
            .bounds
            .unwrap_or_else(|| panic!("`{label}` must have bounds"));
        assert!(
            bounds.max.y <= bar_bottom + 0.5,
            "`{label}` must render inside the bar, not the scroll region: {bounds:?} vs bar_bottom={bar_bottom}"
        );
    }
}

// -- 021-transport-bar-and-panel-layout, Phase 4 (US2): a bar toggle or
// keyboard shortcut opens-and-reveals a closed panel, or collapses an open
// one, in the same interaction (contracts/ui-now-playing-layout.md R1-R6)
// -----------------------------------------------------------------------

/// The pinned bar's own outer rect's bottom edge (contract R1: "directly
/// under the bar") — the scroll region's own viewport top.
fn bar_panel_bottom(ctx: &Context) -> f32 {
    let bar_id = egui::Id::new("now-playing-transport-bar");
    egui::PanelState::load(ctx, bar_id)
        .expect("the transport bar must register its PanelState after a frame")
        .outer_rect
        .bottom()
}

/// As [`render_panel_frame`], but threads an explicit, caller-owned
/// `SectionMemory` through the render instead of a throwaway default one:
/// needed wherever a test must read the scroll region's own resulting
/// offset afterwards, which `now_playing::show` only ever exposes by
/// writing it into `memory` (`memory.offset(&ViewKey::NowPlaying)`).
fn render_panel_frame_with_memory<
    B: modplayer_audio_io::OutputBackend,
    H: modplayer_audio_source::SourceHost,
>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    memory: &mut modplayer_ui::section_memory::SectionMemory,
    input: RawInput,
) -> PanelFrame {
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(input, |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform, memory);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let shapes: Vec<egui::Shape> = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();

    let nodes = update
        .nodes
        .iter()
        .map(|(_, node)| PanelNode {
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
            toggled: node.toggled(),
        })
        .collect();
    PanelFrame { nodes, shapes }
}

/// As [`click_now_playing_capturing`], but threads an explicit
/// `SectionMemory` through both the press and release frame (see
/// [`render_panel_frame_with_memory`]'s doc comment).
fn click_now_playing_capturing_with_memory<
    B: modplayer_audio_io::OutputBackend,
    H: modplayer_audio_source::SourceHost,
>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    memory: &mut modplayer_ui::section_memory::SectionMemory,
    input: &RawInput,
    pos: Pos2,
) -> PanelFrame {
    let mut press_input = input.clone();
    press_input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press_input, |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform, memory);
    });
    output.drop_without_applying_deltas();

    let mut release_input = input.clone();
    release_input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    render_panel_frame_with_memory(ctx, controller, artwork, waveform, memory, release_input)
}

/// [`click_now_playing_capturing_with_memory`], plus one settle pass with
/// no new input at all: `Ui::scroll_to_rect`'s own offset adjustment
/// (`egui::containers::scroll_area`) is computed at the *end* of the
/// click's own release pass — already reflected in that pass's returned
/// `state.offset.y` (what [`SectionMemory::offset`] reads) — but it only
/// feeds the *next* pass's content layout, so that release pass's own
/// painted AccessKit bounds still reflect the pre-scroll positions. One
/// more repaint, with no further input, is exactly what a live app's next
/// vsync already shows by the time a person's eye registers the click
/// (contract R1: "no second input is needed" — a second *pass*, driven by
/// `ScrollArea`'s own `request_repaint`, is not a second input).
fn click_and_settle_now_playing_with_memory<
    B: modplayer_audio_io::OutputBackend,
    H: modplayer_audio_source::SourceHost,
>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    memory: &mut modplayer_ui::section_memory::SectionMemory,
    input: &RawInput,
    pos: Pos2,
) -> PanelFrame {
    click_now_playing_capturing_with_memory(ctx, controller, artwork, waveform, memory, input, pos);
    render_panel_frame_with_memory(ctx, controller, artwork, waveform, memory, input.clone())
}

/// T-R1 (contract R1, R6, FR-007, SC-002): with a closed Queue/Effects/
/// Transport panel and the scroll region at the top, clicking its bar
/// toggle opens it and, in that same pass, its heading sits inside the
/// window's content rect — no further scroll needed for a card this
/// short (the 16-node-chain test below exercises the taller-than-
/// viewport branch). The bar toggle itself reports `Toggled::True` in
/// that same output (mirrors T-C3's same-frame guarantee, now for the
/// bar → card direction).
#[test]
fn bar_toggle_opens_and_reveals_a_closed_panel_inside_the_viewport() {
    for (panel, toggle_key, heading_key) in [
        (NowPlayingPanel::Queue, "queue-toggle", "queue-panel-title"),
        (
            NowPlayingPanel::EffectChain,
            "effects-toggle",
            "effects-panel-title",
        ),
        (
            NowPlayingPanel::Transport,
            "transport-toggle",
            "transport-panel-title",
        ),
    ] {
        let (mut controller, _handle, _dirs) = active_controller(&format!("r1-{toggle_key}"));
        controller.queue_replace(vec![track("a", 200_000)]);
        assert!(
            !controller.now_playing_panel_open(panel),
            "sanity: {toggle_key}'s panel must start closed"
        );

        let ctx = fresh_ctx();
        let mut artwork = ArtworkCache::new();
        let mut waveform = WaveformState::default();
        let mut memory = modplayer_ui::section_memory::SectionMemory::default();

        render_panel_frame_with_memory(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
            default_input(),
        );
        let before = render_panel_frame_with_memory(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
            default_input(),
        );
        let toggle_rect = find_panel_node(&before.nodes, Role::Button, &tr(toggle_key))
            .and_then(|node| node.bounds)
            .unwrap_or_else(|| panic!("the {toggle_key} bar toggle must render"));

        let after = click_and_settle_now_playing_with_memory(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
            &default_input(),
            toggle_rect.center(),
        );

        assert!(
            controller.now_playing_panel_open(panel),
            "the click must open the panel"
        );
        let bar_toggle = find_panel_node(&after.nodes, Role::Button, &tr(toggle_key))
            .expect("the bar toggle must still render");
        assert_eq!(
            bar_toggle.toggled,
            Some(Toggled::True),
            "{toggle_key} must report Toggled::True in the reveal pass"
        );

        let heading = find_panel_node(&after.nodes, Role::Heading, &tr(heading_key))
            .and_then(|node| node.bounds)
            .unwrap_or_else(|| panic!("the {heading_key} heading must render once open"));
        let viewport_top = bar_panel_bottom(&ctx);
        assert!(
            heading.min.y + 0.5 >= viewport_top,
            "{heading_key}'s heading must not sit above the scroll viewport: {heading:?} vs top={viewport_top}"
        );
        assert!(
            heading.max.y <= 600.0 + 0.5,
            "{heading_key}'s heading must be inside the 800x600 content rect: {heading:?}"
        );
    }
}

/// T-R2 (contract R2, FR-007): when a panel's card already fits fully
/// inside a generously tall viewport, opening it through the bar toggle
/// doesn't move the scroll offset at all.
#[test]
fn revealing_an_already_visible_panel_leaves_the_offset_unchanged() {
    for (panel, toggle_key) in [
        (NowPlayingPanel::Queue, "queue-toggle"),
        (NowPlayingPanel::EffectChain, "effects-toggle"),
        (NowPlayingPanel::Transport, "transport-toggle"),
    ] {
        let (mut controller, _handle, _dirs) = active_controller(&format!("r2-{toggle_key}"));
        controller.queue_replace(vec![track("a", 200_000)]);

        let ctx = fresh_ctx();
        let mut artwork = ArtworkCache::new();
        let mut waveform = WaveformState::default();
        let mut memory = modplayer_ui::section_memory::SectionMemory::default();

        render_panel_frame_with_memory(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
            tall_input(),
        );
        let before = render_panel_frame_with_memory(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
            tall_input(),
        );
        let offset_before = memory
            .offset(&modplayer_ui::section_memory::ViewKey::NowPlaying)
            .unwrap_or(0.0);
        assert!(
            offset_before.abs() < 0.5,
            "sanity: a 960x4000 viewport must start unscrolled, got {offset_before}"
        );
        let toggle_rect = find_panel_node(&before.nodes, Role::Button, &tr(toggle_key))
            .and_then(|node| node.bounds)
            .unwrap_or_else(|| panic!("the {toggle_key} bar toggle must render"));

        click_now_playing_capturing_with_memory(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
            &tall_input(),
            toggle_rect.center(),
        );
        assert!(
            controller.now_playing_panel_open(panel),
            "the click must open the panel"
        );

        let offset_after = memory
            .offset(&modplayer_ui::section_memory::ViewKey::NowPlaying)
            .unwrap_or(0.0);
        assert!(
            (offset_after - offset_before).abs() < 0.5,
            "{toggle_key}: offset must stay unchanged when the card is already fully visible, {offset_before} -> {offset_after}"
        );
    }
}

/// T-R1's tall-card branch (contract R1, research R5's `reveal_align`): an
/// Effect Chain at its 16-node capacity is always taller than the
/// viewport, so opening it aligns its heading to the very top of the
/// scroll viewport — within `item_spacing.y` of the pinned bar's own
/// bottom edge — and the offset actually moves (the branch T-B1/T-R2's
/// short cards never exercise).
#[test]
fn revealing_a_taller_than_viewport_card_aligns_its_header_to_the_top() {
    let (mut controller, _handle, _dirs) = active_controller("r1-tall-chain");
    for _ in 0..16 {
        controller
            .chain_add_node(modplayer_effects::catalog::NodeKind::Gain)
            .expect("add gain node");
    }
    assert!(!controller.now_playing_panel_open(NowPlayingPanel::EffectChain));

    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let mut memory = modplayer_ui::section_memory::SectionMemory::default();

    render_panel_frame_with_memory(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        &mut memory,
        default_input(),
    );
    let before = render_panel_frame_with_memory(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        &mut memory,
        default_input(),
    );
    let offset_before = memory
        .offset(&modplayer_ui::section_memory::ViewKey::NowPlaying)
        .unwrap_or(0.0);
    let toggle_rect = find_panel_node(&before.nodes, Role::Button, &tr("effects-toggle"))
        .and_then(|node| node.bounds)
        .expect("the Effects bar toggle must render");

    let after = click_and_settle_now_playing_with_memory(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        &mut memory,
        &default_input(),
        toggle_rect.center(),
    );
    assert!(controller.now_playing_panel_open(NowPlayingPanel::EffectChain));

    let offset_after = memory
        .offset(&modplayer_ui::section_memory::ViewKey::NowPlaying)
        .unwrap_or(0.0);
    assert!(
        offset_after > offset_before + 1.0,
        "opening a 16-node chain must actually scroll: {offset_before} -> {offset_after}"
    );

    let heading = find_panel_node(&after.nodes, Role::Heading, &tr("effects-panel-title"))
        .and_then(|node| node.bounds)
        .expect("the Effect chain heading must render once open");
    let viewport_top = bar_panel_bottom(&ctx);
    // `Align::Min` aligns the *card's own outer rect* (`CardResponse::
    // rect`, contract R1/R6, T031) to the viewport's top, with egui's own
    // `item_spacing.y` gap (`scroll_area.rs`'s `Align::Min` branch)
    // between them; the heading itself sits one more `space::LG` (the
    // card's own `inner_margin`, contract C1/C2) further down, inside the
    // card. The tolerance below is exactly that stack, so this still
    // fails if the card ever stopped aligning to the top at all (e.g. a
    // regression back to `Some(None)`'s minimum-scroll branch, which
    // would leave a much larger, `viewport.span()`-sized gap).
    let tolerance = modplayer_ui::theme::space::LG + modplayer_ui::theme::space::XS + 2.0;
    assert!(
        (heading.min.y - viewport_top).abs() <= tolerance,
        "a card taller than the viewport must align its heading to the top: heading={heading:?} viewport_top={viewport_top} tolerance={tolerance}"
    );
}

/// T-R4 (contract R4, FR-008): pressing an OPEN panel's bar toggle
/// collapses it even while its card sits off screen, and the resulting
/// scroll offset is explained entirely by the shorter content's own
/// clamp — never by a reveal. Proven by comparison against a twin
/// controller whose chain starts (and stays) collapsed at the same seeded
/// scroll offset: if the collapse path ever revealed anything, its
/// resulting offset would differ from this reveal-free baseline.
#[test]
fn collapsing_an_open_off_screen_panel_never_reveals_it() {
    fn sixteen_node_controller(
        label: &str,
        start_open: bool,
    ) -> (
        PlaybackController<FakeBackend, ScriptedHost>,
        ScriptedHostHandle,
        TestDirs,
    ) {
        let (mut controller, handle, dirs) = active_controller(label);
        for _ in 0..16 {
            controller
                .chain_add_node(modplayer_effects::catalog::NodeKind::Gain)
                .expect("add gain node");
        }
        controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, start_open);
        (controller, handle, dirs)
    }

    /// Warm up, seed the scroll offset to egui's own clamped maximum (the
    /// "scrolled to max" trick this file's own T-B1 test uses), and return
    /// the resulting offset plus that pass's `PanelFrame`.
    fn scroll_to_max(
        ctx: &Context,
        controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
        artwork: &mut ArtworkCache,
        waveform: &mut WaveformState,
        memory: &mut modplayer_ui::section_memory::SectionMemory,
    ) -> (f32, PanelFrame) {
        render_panel_frame_with_memory(ctx, controller, artwork, waveform, memory, default_input());
        memory.record(modplayer_ui::section_memory::ViewKey::NowPlaying, 1.0e6);
        memory.record(modplayer_ui::section_memory::ViewKey::Search, 0.0);
        let frame = render_panel_frame_with_memory(
            ctx,
            controller,
            artwork,
            waveform,
            memory,
            default_input(),
        );
        let offset = memory
            .offset(&modplayer_ui::section_memory::ViewKey::NowPlaying)
            .expect("the seeded pass must record an offset");
        (offset, frame)
    }

    // -- Baseline: the chain starts (and stays) collapsed, scrolled to the
    // same seeded max offset — nothing ever reveals it, so its resulting
    // offset is purely the natural clamp at this (short, header-only)
    // content height.
    let (mut baseline_controller, _baseline_handle, _baseline_dirs) =
        sixteen_node_controller("r4-baseline", false);
    let baseline_ctx = fresh_ctx();
    let mut baseline_artwork = ArtworkCache::new();
    let mut baseline_waveform = WaveformState::default();
    let mut baseline_memory = modplayer_ui::section_memory::SectionMemory::default();
    let (baseline_offset, _) = scroll_to_max(
        &baseline_ctx,
        &mut baseline_controller,
        &mut baseline_artwork,
        &mut baseline_waveform,
        &mut baseline_memory,
    );

    // -- Subject: the chain starts OPEN (tall), scrolled to the same
    // seeded max offset (so its card sits off the top of the viewport),
    // then its bar toggle collapses it.
    let (mut controller, _handle, _dirs) = sixteen_node_controller("r4-subject", true);
    let ctx = fresh_ctx();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let mut memory = modplayer_ui::section_memory::SectionMemory::default();
    let (_subject_scrolled_offset, scrolled) = scroll_to_max(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        &mut memory,
    );

    let toggle_rect = find_panel_node(&scrolled.nodes, Role::Button, &tr("effects-toggle"))
        .and_then(|node| node.bounds)
        .expect("the Effects bar toggle must render");
    let heading = find_panel_node(&scrolled.nodes, Role::Heading, &tr("effects-panel-title"))
        .and_then(|node| node.bounds);
    let viewport_top = bar_panel_bottom(&ctx);
    assert!(
        heading.is_none_or(|h| h.max.y < viewport_top - 1.0),
        "sanity: the open chain's heading must already be scrolled off the top before collapsing: {heading:?} vs top={viewport_top}"
    );

    let after = click_now_playing_capturing_with_memory(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        &mut memory,
        &default_input(),
        toggle_rect.center(),
    );
    assert!(
        !controller.now_playing_panel_open(NowPlayingPanel::EffectChain),
        "the click must collapse the panel even while its card is off screen"
    );
    let bar_toggle = find_panel_node(&after.nodes, Role::Button, &tr("effects-toggle"))
        .expect("the bar toggle must still render");
    assert_eq!(
        bar_toggle.toggled,
        Some(Toggled::False),
        "the bar toggle must report Toggled::False after the collapse"
    );

    let subject_offset = memory
        .offset(&modplayer_ui::section_memory::ViewKey::NowPlaying)
        .expect("the subject's collapse pass must record an offset");
    assert!(
        (subject_offset - baseline_offset).abs() < 0.5,
        "collapsing an off-screen panel must change the offset only by the shorter-content \
         clamp, matching a chain that was never open: baseline={baseline_offset} \
         subject={subject_offset}"
    );
}

// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T065 (US1): Now Playing (contracts/ui-surface.md §1) — inline disabled
//! reasons per `ActiveState`/health, the seek slider committing once per
//! release (not continuously while dragging), and the position readout
//! updating as playback advances.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::Role;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, SourceHealth, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::transport::Intent;
use modplayer_core::{NotRegisteredReason, PlaybackController, tr};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};

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

/// A controller over a confirmed device with an `ActiveState::Active`
/// Connect registration — the baseline "healthy, transport enabled" state
/// every test below starts from and deviates from as needed.
fn active_controller(label: &str) -> (PlaybackController<FakeBackend, ScriptedHost>, TempDir) {
    let (store, dir) = fresh_store(label);
    let devices = vec![fake_device()];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), ScriptedHost::new(), store);
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, dir)
}

/// Render `now_playing::show` in a fresh headless, AccessKit-enabled
/// context and return every non-empty accessible text (`value`+`label`) the
/// frame produced — robust to whichever of the two properties a given
/// widget role stores its text under (contracts/ui-surface.md §1;
/// `egui::Response::fill_accesskit_node_from_widget_info` puts `Role::Label`
/// text in `value` and every other role's in `label`).
fn rendered_texts<B: modplayer_audio_io::OutputBackend, H: modplayer_audio_source::SourceHost>(
    controller: &mut PlaybackController<B, H>,
) -> Vec<String> {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(RawInput::default(), |ui| {
        modplayer_ui::now_playing::show(ui, controller);
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
    let texts = rendered_texts(&mut controller);
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
        let (mut controller, _dir) = active_controller(&format!("not-registered-{key}"));
        controller.set_playback_permitted(false, Some(reason));

        assert!(
            !controller.transport_enabled(),
            "{key}: transport must be disabled"
        );
        let texts = rendered_texts(&mut controller);
        assert!(
            texts.contains(&tr(key)),
            "{key}: expected that status line, got {texts:?}"
        );
    }
}

#[test]
fn source_unavailable_disables_transport_and_shows_its_status_line() {
    // Built directly (not via `active_controller`) so the test keeps a
    // `ScriptedHostHandle` to script a health transition after the
    // controller owns the host.
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
    let texts = rendered_texts(&mut controller);
    assert!(
        texts.contains(&tr("status-source-unavailable")),
        "expected the `status-source-unavailable` line, got {texts:?}"
    );
}

#[test]
fn healthy_active_state_shows_no_disabled_reason() {
    let (mut controller, _dir) = active_controller("healthy");
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
    let texts = rendered_texts(&mut controller);
    for key in every_status_key {
        assert!(
            !texts.contains(&tr(key)),
            "healthy/active must not show `{key}`, got {texts:?}"
        );
    }
}

#[test]
fn position_readout_updates_as_playback_advances() {
    let (mut controller, _dir) = active_controller("position-readout");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    let before = rendered_texts(&mut controller);
    let position_before = controller.position();

    // Advance the audio clock by rendering real frames through the fake
    // backend, exactly like `modplayer-core`'s own streaming tests.
    let _ = controller.backend_mut().render_buffers(20_000);

    let after = rendered_texts(&mut controller);
    let position_after = controller.position();

    assert!(
        position_after > position_before,
        "position must advance while playing"
    );
    assert_ne!(
        before, after,
        "the rendered position readout must change as the audio clock advances"
    );
}

/// Locate the seek slider among possibly-several `Role::Slider` nodes (the
/// master-volume widget is one too, contracts/ui-surface.md §1) — it is
/// drawn first, top-to-bottom, so it has the smallest `bounds.y0`.
fn topmost_slider_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> Rect {
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, controller);
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

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

#[test]
fn seek_slider_commits_once_per_release() {
    let (mut controller, _dir) = active_controller("seek-commit");
    controller.queue_replace(vec![track("a", 200_000)]);
    // Left `Stopped` deliberately: `position()` reads exactly 0 while
    // stopped (controller.rs), so any commit is unambiguous, and T6 (seek
    // while stopped becomes `Paused`) gives an observable state flip the
    // instant a commit happens — regardless of the slider's exact dragged
    // value (landing the seek at a specific position is already covered at
    // the controller level by
    // `seek_on_buffered_audio_lands_at_the_requested_position`).
    assert_eq!(controller.transport_state().intent, Intent::Stopped);

    let ctx = Context::default();
    ctx.enable_accesskit();

    let bounds = topmost_slider_bounds(&ctx, &mut controller);
    let left = Pos2::new(bounds.left() + 4.0, bounds.center().y);
    let right = Pos2::new(bounds.right() - 4.0, bounds.center().y);

    // Press near the slider's left edge: begins a drag, must not commit.
    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: left,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller)
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        controller.transport_state().intent,
        Intent::Stopped,
        "pressing must not commit a seek"
    );

    // Drag to the right edge while the button stays down: still no commit.
    let mut drag = default_input();
    drag.events.push(Event::PointerMoved(right));
    let output = ctx.run_ui(drag, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller)
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        controller.transport_state().intent,
        Intent::Stopped,
        "dragging must not commit a seek before release"
    );

    // Release: commits exactly once (T6: seek while stopped -> Paused).
    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: right,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller)
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "release must commit the seek exactly once"
    );
    let position_after_release = controller.position();

    // One more frame with no new pointer events: no further commit.
    let output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller)
    });
    output.drop_without_applying_deltas();
    assert_eq!(controller.transport_state().intent, Intent::Paused);
    assert_eq!(
        controller.position(),
        position_after_release,
        "no further seek must be committed without a new release"
    );
}

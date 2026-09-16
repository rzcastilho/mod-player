// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T072 (US2): the Queue panel (contracts/ui-surface.md §2) — rows render
//! in effective order with their origin/unavailable badges and a current
//! marker, and the keyboard-operable row actions mutate the controller's
//! queue.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::Role;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{PlaybackController, tr, tr_args};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-queue-view-{label}-{}-{unique}",
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

/// A controller over a confirmed device, ready for a queue to be built on
/// (mirrors `modplayer-core`'s own `controller_streaming.rs` fixture).
fn ready_controller(label: &str) -> (PlaybackController<FakeBackend, ScriptedHost>, TempDir) {
    let (store, dir) = fresh_store(label);
    let devices = vec![fake_device()];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), ScriptedHost::new(), store);
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    (controller, dir)
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

/// Every non-empty accessible text (`value`+`label`) `queue_view::show`
/// renders this frame (mirrors `now_playing.rs`'s own `rendered_texts`).
fn rendered_texts(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> Vec<String> {
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::queue_view::show(ui, controller);
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
        .collect()
}

/// The bounds of the `nth` (top-to-bottom) `Role::Button` node whose label
/// equals `label` — rows repeat the same action label once per row, so the
/// row order (current first, per `effective_order`) disambiguates which
/// one a click lands on.
fn nth_button_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    label: &str,
    nth: usize,
) -> Rect {
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::queue_view::show(ui, controller);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let mut matches: Vec<egui::accesskit::Rect> = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::Button && node.label() == Some(label))
        .filter_map(|(_, node)| node.bounds())
        .collect();
    matches.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap_or(std::cmp::Ordering::Equal));
    let bounds = matches.get(nth).unwrap_or_else(|| {
        panic!(
            "expected at least {} `{label}` button(s), found {}",
            nth + 1,
            matches.len()
        )
    });
    Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    )
}

/// Press then release the primary button at `pos`, in two separate frames
/// (mirrors `now_playing.rs`'s proven press/release pattern — a single
/// frame is not guaranteed to register `clicked()`).
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
    let output = ctx.run_ui(press, |ui| modplayer_ui::queue_view::show(ui, controller));
    output.drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| modplayer_ui::queue_view::show(ui, controller));
    output.drop_without_applying_deltas();
}

#[test]
fn rows_render_in_effective_order_with_badges_and_current_marker() {
    let (mut controller, _dir) = ready_controller("rows-order");
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    controller.queue_play_next_track(track("x"));
    let unavailable_id = TrackId::new("spotify:track:c").unwrap_or_else(|_| unreachable!());
    controller.queue().current(); // sanity: a queue exists
    let _ = unavailable_id;

    let ctx = Context::default();
    ctx.enable_accesskit();
    let texts = rendered_texts(&ctx, &mut controller);

    assert!(texts.contains(&tr_args("queue-row", &[("title", "a".to_string())])));
    assert!(texts.contains(&tr_args("queue-row", &[("title", "x".to_string())])));
    assert!(texts.contains(&tr_args("queue-row", &[("title", "b".to_string())])));
    assert!(texts.contains(&tr_args("queue-row", &[("title", "c".to_string())])));
    assert!(
        texts.contains(&tr("queue-current")),
        "expected the current-item marker, got {texts:?}"
    );
    assert!(
        texts.contains(&tr("queue-badge-play-next")),
        "expected the play-next badge on \"x\", got {texts:?}"
    );
    assert!(
        !texts.contains(&tr("queue-badge-unavailable")),
        "nothing is marked unavailable yet, got {texts:?}"
    );
}

#[test]
fn empty_queue_shows_the_empty_state() {
    let (mut controller, _dir) = ready_controller("empty");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let texts = rendered_texts(&ctx, &mut controller);
    assert!(texts.contains(&tr("queue-empty")));
}

#[test]
fn remove_button_on_the_current_row_advances_the_controllers_queue() {
    let (mut controller, _dir) = ready_controller("remove-current");
    controller.queue_replace(vec![track("a"), track("b")]);
    assert_eq!(
        controller
            .queue()
            .current()
            .map(|i| i.track.id.as_str().to_string()),
        Some("spotify:track:a".to_string())
    );

    let ctx = Context::default();
    ctx.enable_accesskit();
    // Rows render current-first, so the topmost "Remove" button belongs to
    // "a" (the current item).
    let bounds = nth_button_bounds(&ctx, &mut controller, &tr("queue-remove"), 0);
    click_at(&ctx, &mut controller, bounds.center());

    assert_eq!(
        controller
            .queue()
            .current()
            .map(|i| i.track.id.as_str().to_string()),
        Some("spotify:track:b".to_string()),
        "clicking Remove on the current row's button must advance past it"
    );
}

#[test]
fn move_down_button_reorders_the_controllers_queue() {
    let (mut controller, _dir) = ready_controller("move-down");
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    let order_before: Vec<_> = controller
        .queue()
        .effective_order()
        .iter()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(
        order_before,
        vec!["spotify:track:a", "spotify:track:b", "spotify:track:c"]
    );

    let ctx = Context::default();
    ctx.enable_accesskit();
    // Rows render current ("a") first, then "b", then "c" — the second
    // (index 1) "Move down" button belongs to "b".
    let bounds = nth_button_bounds(&ctx, &mut controller, &tr("queue-move-down"), 1);
    click_at(&ctx, &mut controller, bounds.center());

    let order_after: Vec<_> = controller
        .queue()
        .effective_order()
        .iter()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(
        order_after,
        vec!["spotify:track:a", "spotify:track:c", "spotify:track:b"],
        "\"b\" must move one slot later in the effective order"
    );
}

#[test]
fn shuffle_and_repeat_header_controls_mutate_the_controllers_queue() {
    let (mut controller, _dir) = ready_controller("header");
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    assert!(!controller.queue().shuffle_enabled());
    assert_eq!(
        controller.queue().repeat(),
        modplayer_audio_source::Repeat::Off
    );

    let ctx = Context::default();
    ctx.enable_accesskit();
    let shuffle_bounds = nth_button_bounds(&ctx, &mut controller, &tr("queue-shuffle"), 0);
    click_at(&ctx, &mut controller, shuffle_bounds.center());
    assert!(
        controller.queue().shuffle_enabled(),
        "clicking the shuffle toggle must turn shuffle on"
    );

    let repeat_bounds = nth_button_bounds(&ctx, &mut controller, &tr("queue-repeat-off"), 0);
    click_at(&ctx, &mut controller, repeat_bounds.center());
    assert_eq!(
        controller.queue().repeat(),
        modplayer_audio_source::Repeat::One,
        "clicking the repeat-cycle button must advance Off -> One"
    );
}

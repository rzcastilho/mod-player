// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Markers panel, overlay and shortcut tests (006, US1,
//! contracts/ui-markers.md).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::Role;
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::markers::{CueSlot, RepeatCount, TrackMarkers};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{Intent, LoopState, PlaybackController, tr_args};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::waveform::{TimeSpace, WaveformState};

/// How many frames a jump's landing position may sit past the cue it
/// targeted (006 US4): landing a seek requires one render call (the
/// engine drains its command queue at the start of it), which itself
/// advances playback by that render's own frame count — double this
/// suite's negotiated buffer size, so genuinely comfortable.
const JUMP_LAND_TOLERANCE_FRAMES: u64 = 512;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-markers-{label}-{}-{unique}",
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
/// new`'s one synchronous read of it (mirrors `controller_markers.rs`'s
/// own lock; research R10/R11).
static TRACK_STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Both temp dirs `active_controller` allocates (settings, track-state);
/// kept alive together so either can be dropped (cleaned up) only once the
/// test itself is done with the controller.
struct TestDirs(#[allow(dead_code)] TempDir, #[allow(dead_code)] TempDir);

/// A controller over a confirmed device, playing (mirrors `now_playing.rs`'s
/// own `active_controller`): the baseline every test below starts from.
/// Its `MODPLAYER_TRACK_STATE_DIR` is a temp dir, not the real per-user
/// one (006, contracts/marker-service.md §3) — every test here creates
/// markers/regions, and 006 US2's `sync_marker_attachment`/debounced
/// flush now run on every `dispatch`, so an unisolated controller would
/// read and write the real machine's saved track state.
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
        // Safety: narrowly scopes the mutation to the one synchronous
        // read `PlaybackController::new` does of this var, serialized
        // against every other test in this binary via the lock above.
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
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

fn key_event(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Run one Now Playing frame with `key` pressed (contracts/ui-markers.md
/// §2), on the same `ctx` across calls so egui's own focus bookkeeping
/// persists between them, as `now_playing.rs`'s own tests do.
fn press_key(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    key: Key,
) {
    let mut input = default_input();
    input.events.push(key_event(key));
    let output = ctx.run_ui(input, |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform)
    });
    output.drop_without_applying_deltas();
}

/// As [`press_key`], with an explicit modifier (e.g. `Shift+←`,
/// contracts/ui-markers.md §3).
fn press_key_with(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    key: Key,
    modifiers: Modifiers,
) {
    let mut input = default_input();
    input.events.push(Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    let output = ctx.run_ui(input, |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform)
    });
    output.drop_without_applying_deltas();
}

/// Run one Now Playing frame and return the screen-space bounds of the
/// topmost accesskit node for which `matches` holds (mirrors `now_playing.
/// rs`'s own `overview_bounds`, generalised to any role/label).
fn find_node_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    matches: impl Fn(&egui::accesskit::Node) -> bool,
) -> Rect {
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform)
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let mut best: Option<egui::accesskit::Rect> = None;
    for (_, node) in &update.nodes {
        if !matches(node) {
            continue;
        }
        if let Some(bounds) = node.bounds()
            && best.as_ref().is_none_or(|b| bounds.y0 < b.y0)
        {
            best = Some(bounds);
        }
    }
    let bounds = best.unwrap_or_else(|| unreachable!("no matching accesskit node found"));
    Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    )
}

/// A point marker's glyph hit-target bounds — the only `Role::Button`
/// whose accessible name (`marker-glyph`, contracts/ui-markers.md §1)
/// contains `"Marker"` (`role_label`'s point-kind text; case-sensitive, so
/// it never matches the panel's lowercase "markers" chrome buttons).
fn glyph_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Rect {
    find_node_bounds(ctx, controller, artwork, waveform, |node| {
        node.role() == Role::Button
            && (node.label().is_some_and(|l| l.contains("Marker"))
                || node.value().is_some_and(|v| v.contains("Marker")))
    })
}

/// The overview waveform's own bounds (the topmost `Role::Slider` node).
fn overview_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Rect {
    find_node_bounds(ctx, controller, artwork, waveform, |node| {
        node.role() == Role::Slider
    })
}

/// Every non-empty AccessKit `value`/`label` text the frame produced
/// (mirrors `now_playing.rs`'s own `rendered_texts`).
fn rendered_texts(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Vec<String> {
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, controller, artwork, waveform)
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

/// `I` then `O` (contracts/ui-markers.md §2, US1 AS1): each sets the
/// current region's `A`/`B` endpoint to the playhead frame at the moment
/// it is pressed — read straight from `RtShared::position_frames()`, the
/// engine's own frame cursor (FR-009), not a UI-derived estimate.
#[test]
fn i_then_o_creates_region_at_playhead_positions() {
    let (mut controller, _handle, _dir) = active_controller("i-then-o");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(5);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let expected_a = controller.shared().position_frames();
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::I);

    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .expect("I must create a region");
    let a_id = controller
        .markers()
        .and_then(|markers| markers.region(region))
        .and_then(|r| r.a)
        .expect("A endpoint must exist after I");
    let a = controller
        .markers()
        .and_then(|markers| markers.position_of(a_id))
        .expect("A marker must resolve");
    assert_eq!(a, expected_a);

    let _ = controller.backend_mut().render_buffers(5);
    let expected_b = controller.shared().position_frames();
    assert!(
        expected_b > expected_a,
        "sanity: playhead must have advanced"
    );

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::O);

    let markers = controller.markers().expect("markers must exist");
    let region_after = markers.region(region).expect("region must still exist");
    let (a_final, b_final) = region_after
        .span(markers)
        .expect("region must be complete after O");
    assert_eq!(a_final, expected_a);
    assert_eq!(b_final, expected_b);
}

/// `L` (contracts/ui-markers.md §2, AS7, FR-008): refuses with
/// `loop-region-incomplete` while no region/only `A` exists, refuses with
/// `loop-region-too-short`/`loop-region-incomplete` per `MarkerError`, and
/// arms/disarms a real region — each outcome mirrored into
/// `waveform.marker_status` (cleared on success).
#[test]
fn l_toggles_current_region_and_refuses_with_reason() {
    let (mut controller, _handle, _dir) = active_controller("l-toggle");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    // No region at all: refuses.
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::L);
    assert_eq!(waveform.marker_status, Some("loop-region-incomplete"));

    // `I` succeeds and clears the refusal; only `A` set: `L` still refuses.
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::I);
    assert_eq!(waveform.marker_status, None);
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::L);
    assert_eq!(waveform.marker_status, Some("loop-region-incomplete"));

    // A real-length region: `O` completes it, `L` arms it.
    let _ = controller.backend_mut().render_buffers(5);
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::O);
    assert_eq!(waveform.marker_status, None);
    let _ = controller.backend_mut().render_buffers(1);

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::L);
    assert_eq!(
        waveform.marker_status, None,
        "a real region must arm, not refuse"
    );
    let _ = controller.backend_mut().render_buffers(1);
    assert_ne!(controller.loop_status().state, LoopState::Disarmed);

    // `L` again disarms.
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::L);
    assert_eq!(waveform.marker_status, None);
    let _ = controller.backend_mut().render_buffers(1);
    assert_eq!(controller.loop_status().state, LoopState::Disarmed);
}

/// An armed, active, finite-repeat region shows `loop-wraps-remaining`
/// (contracts/ui-markers.md §4).
#[test]
fn armed_region_shows_wraps_remaining() {
    let (mut controller, _handle, _dir) = active_controller("wraps-remaining");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!("region must exist"));
    controller
        .set_loop_repeat(region, RepeatCount::times_clamped(5))
        .unwrap_or_else(|e| unreachable!("set_loop_repeat: {e}"));
    let a = {
        let markers = controller
            .markers()
            .unwrap_or_else(|| unreachable!("markers must exist"));
        markers
            .region(region)
            .unwrap_or_else(|| unreachable!("region must exist"))
            .span(markers)
            .unwrap_or_else(|| unreachable!("region must be complete"))
            .0
    };
    controller
        .arm_loop(region)
        .unwrap_or_else(|e| unreachable!("arm_loop: {e}"));
    controller.seek_frames(a);
    let _ = controller.backend_mut().render_buffers(1);

    let mut wraps = 0;
    for _ in 0..500 {
        let _ = controller.backend_mut().render_buffers(1);
        wraps = controller.shared().loop_wraps();
        if wraps >= 2 {
            break;
        }
    }
    assert!((2..5).contains(&wraps), "wraps={wraps}");

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);

    let remaining = 5u32.saturating_sub(wraps);
    let expected = tr_args("loop-wraps-remaining", &[("count", remaining.to_string())]);
    assert!(
        texts.contains(&expected),
        "expected {expected:?} among {texts:?}"
    );
}

/// An armed region whose playhead has not (yet) entered `[A, B)` shows the
/// `loop-armed-inactive` badge (contracts/ui-markers.md §4, data-model.md
/// §3's `RtShared::loop_state == 1`).
#[test]
fn armed_inactive_badge_when_state_1() {
    let (mut controller, _handle, _dir) = active_controller("armed-inactive");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!("region must exist"));
    controller
        .arm_loop(region)
        .unwrap_or_else(|e| unreachable!("arm_loop: {e}"));
    // Deliberately no seek back to `A`: the playhead has already moved past
    // `B`, so the commit lands armed-but-inactive (state 1), never active.
    let _ = controller.backend_mut().render_buffers(1);
    assert_eq!(controller.loop_status().state, LoopState::ArmedInactive);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);

    assert!(
        texts.contains(&modplayer_core::tr("loop-armed-inactive")),
        "expected the armed-inactive badge among {texts:?}"
    );
}

/// A helper to run a paint closure against a freshly allocated rect/
/// `TimeSpace` and capture the raw (pre-tessellation) shapes it produced,
/// mirroring `now_playing.rs`'s own `rendered_painted_texts` pattern.
fn painted_shapes(
    window: std::ops::Range<u64>,
    sample_rate: u32,
    paint: impl FnOnce(&egui::Painter, &TimeSpace),
) -> Vec<Shape> {
    let ctx = Context::default();
    let mut paint = Some(paint);
    let output = ctx.run_ui(default_input(), |ui| {
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(400.0, 40.0), egui::Sense::hover());
        let space = TimeSpace::new(rect, window.clone(), sample_rate);
        if let Some(paint) = paint.take() {
            paint(ui.painter(), &space);
        }
    });
    let shapes = output
        .shapes
        .iter()
        .map(|clipped| clipped.shape.clone())
        .collect();
    output.drop_without_applying_deltas();
    shapes
}

/// `markers::paint_overlay` (006, contracts/ui-markers.md §1, §5,
/// data-model.md §3): one line per marker regardless of `loop_state`, plus
/// the current region's span rendered per its style — outline-only
/// (disarmed), hatched with no rect (armed-inactive), or filled
/// (armed-active).
#[test]
fn overlay_paints_lines_and_span_states() {
    let id = TrackId::new("spotify:track:overlay").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);
    markers
        .set_loop_a(1_000)
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    markers
        .set_loop_b(5_000)
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));

    for loop_state in [0u8, 1, 2] {
        let shapes = painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
            modplayer_ui::markers::paint_overlay(painter, space, Some(&markers), loop_state, None);
        });

        let line_count = shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::LineSegment { .. }))
            .count();
        let rects: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Rect(rect_shape) => Some(rect_shape),
                _ => None,
            })
            .collect();

        assert!(
            line_count >= 2,
            "loop_state={loop_state}: expected at least the 2 marker lines, got {line_count}"
        );

        match loop_state {
            1 => {
                assert!(
                    line_count > 2,
                    "armed-inactive must add hatch lines beyond the 2 marker lines"
                );
                assert!(rects.is_empty(), "armed-inactive paints no span rect");
            }
            2 => {
                assert_eq!(line_count, 2, "only the 2 marker lines for armed-active");
                assert_eq!(rects.len(), 1);
                assert!(rects[0].fill.a() > 0, "armed-active span must be filled");
            }
            _ => {
                assert_eq!(line_count, 2, "only the 2 marker lines while disarmed");
                assert_eq!(rects.len(), 1);
                assert_eq!(
                    rects[0].fill,
                    egui::Color32::TRANSPARENT,
                    "disarmed span must be outline-only"
                );
                assert!(rects[0].stroke.width > 0.0);
            }
        }
    }
}

/// The Markers panel's empty state (contracts/ui-markers.md §4, FR-020):
/// with no markers on the current track, the row list is replaced by
/// `markers-empty` ("No markers — press I to set A").
#[test]
fn empty_state_shows_press_i_hint() {
    let (mut controller, _handle, _dir) = active_controller("empty-state");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&modplayer_core::tr("markers-empty")),
        "expected the empty-state hint among {texts:?}"
    );
}

/// "Clear all markers" (contracts/ui-markers.md §4, US2 AS6): the header
/// button is a two-step inline confirmation
/// (`markers-clear-confirm { $count }` + yes/no); `Esc` cancels it back to
/// the plain button without touching any marker, while confirming (via
/// `clear_all_markers`, exercised directly here as the "yes" path) empties
/// the model.
#[test]
fn clear_all_two_step_confirm_and_cancel() {
    let (mut controller, _handle, _dir) = active_controller("clear-all");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let count_before = controller.markers().map(TrackMarkers::count).unwrap_or(0);
    assert!(count_before > 0, "sanity: a marker must exist");

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    // Not yet confirming: the plain button, no "Clear N markers?" text.
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
    assert!(texts.contains(&modplayer_core::tr("markers-clear-all")));
    let confirm_text = tr_args(
        "markers-clear-confirm",
        &[("count", count_before.to_string())],
    );
    assert!(!texts.contains(&confirm_text));

    // Two-step confirmation showing.
    waveform.clear_confirm = true;
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&confirm_text),
        "expected {confirm_text:?} among {texts:?}"
    );

    // `Esc` cancels: back to the plain button, nothing cleared.
    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Escape,
    );
    assert!(!waveform.clear_confirm);
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        count_before,
        "Esc must not clear any marker"
    );

    // Confirming ("yes") empties the model.
    waveform.clear_confirm = true;
    controller.clear_all_markers();
    waveform.clear_confirm = false;
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        0
    );
}

/// A `clamped` marker (006 FR-018/SC-012: a saved position beyond a
/// shorter re-saved track's current length) gets an extra warning glyph on
/// its overlay line that an ordinary marker does not
/// (`paint_clamped_warning`, contracts/ui-markers.md §1).
#[test]
fn clamped_marker_shows_warning_glyph() {
    let id = TrackId::new("spotify:track:clamped").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);
    markers
        .set_loop_a(1_000)
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));

    let triangle_count = |markers: &TrackMarkers| {
        painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
            modplayer_ui::markers::paint_overlay(painter, space, Some(markers), 0, None);
        })
        .iter()
        .filter(|shape| matches!(shape, Shape::Path(_)))
        .count()
    };

    assert_eq!(triangle_count(&markers), 0, "no clamped markers yet");

    // Shrinking the track below the marker's position flags it `clamped`.
    markers.set_len_frames(500);
    assert_eq!(
        triangle_count(&markers),
        1,
        "a clamped marker must paint exactly one warning triangle"
    );
}

/// `M` (contracts/ui-markers.md §2, US3 AS8): adds a point marker at the
/// playhead with its externalised default name, kept sorted among any
/// other markers.
#[test]
fn m_creates_point_marker_with_default_name_sorted() {
    let (mut controller, _handle, _dir) = active_controller("m-creates-point");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let expected_pos = controller.shared().position_frames();
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::M);

    let markers = controller.markers().expect("M must create a marker");
    assert_eq!(markers.count(), 1);
    let marker = markers.markers().first().expect("one marker must exist");
    assert_eq!(marker.position, expected_pos);
    assert_eq!(
        marker.name,
        tr_args("marker-default-name", &[("n", "1".to_string())])
    );

    // A second `M` keeps the list sorted by (position, id).
    let _ = controller.backend_mut().render_buffers(5);
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::M);
    let markers = controller.markers().expect("markers must exist");
    assert_eq!(markers.count(), 2);
    let positions: Vec<u64> = markers.markers().iter().map(|m| m.position).collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted, "markers must stay sorted by position");
}

/// The 64-marker limit (SC-005, FR-002) refuses `M` inline, same mechanism
/// as `I`/`O`/`L`.
#[test]
fn sixty_fifth_marker_refused_inline() {
    let (mut controller, _handle, _dir) = active_controller("m-limit");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    for _ in 0..64 {
        controller
            .add_point_marker()
            .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    }
    assert_eq!(controller.markers().map(TrackMarkers::count), Some(64));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::M);
    assert_eq!(waveform.marker_status, Some("marker-limit-reached"));
    assert_eq!(controller.markers().map(TrackMarkers::count), Some(64));
}

/// While an inline rename is open, the view-level shortcuts (`I`/`O`/`L`/
/// `M`) must not fire — the rename `TextEdit` has focus, so the research
/// R17 text-field guard blocks them (FR-004a).
#[test]
fn shortcuts_inactive_while_rename_open() {
    let (mut controller, _handle, _dir) = active_controller("rename-guard");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(id),
        rename: Some((id, String::new())),
        ..Default::default()
    };
    let ctx = Context::default();
    ctx.enable_accesskit();

    let count_before = controller.markers().map(TrackMarkers::count).unwrap_or(0);
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::M);
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        count_before,
        "M must not create a marker while an inline rename is open"
    );
    assert!(
        waveform.rename.is_some(),
        "the rename itself must stay open (M must not have touched it)"
    );
}

/// The focused-marker keyboard table's arrow row (contracts/ui-markers.md
/// §3, SC-004): `←`/`→` nudge by the configured step, `Shift+←`/`Shift+→`
/// nudge by 10x it.
#[test]
fn glyph_focus_arrow_nudges_by_setting_and_shift_ten_x() {
    let (mut controller, _handle, _dir) = active_controller("nudge");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller.set_nudge_step_ms(25);

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let start = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(id),
        ..Default::default()
    };
    let ctx = Context::default();
    ctx.enable_accesskit();

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::ArrowRight,
    );
    let rate = controller.source_sample_rate().max(1);
    let step = (25u64 * u64::from(rate)) / 1000;
    let after_one = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(after_one, start + step, "plain arrow nudges by one step");

    press_key_with(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::ArrowRight,
        Modifiers::SHIFT,
    );
    let after_shift = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(
        after_shift,
        after_one + step * 10,
        "Shift nudges by 10x the step"
    );

    press_key_with(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::ArrowLeft,
        Modifiers::SHIFT,
    );
    let after_shift_back = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(after_shift_back, after_shift - step * 10);
}

/// `Delete`/`Backspace` on a focused region endpoint (contracts/
/// ui-markers.md §3, AS5): the region becomes incomplete (and, since it
/// was the armed one, disarmed) and focus returns.
#[test]
fn delete_focused_endpoint_makes_region_incomplete() {
    let (mut controller, _handle, _dir) = active_controller("delete-endpoint");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(5);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!());
    let a_id = controller
        .markers()
        .and_then(|m| m.region(region))
        .and_then(|r| r.a)
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(a_id),
        ..Default::default()
    };
    let ctx = Context::default();
    ctx.enable_accesskit();

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Delete,
    );

    let markers = controller.markers().unwrap_or_else(|| unreachable!());
    let r = markers.region(region).unwrap_or_else(|| unreachable!());
    assert!(r.a.is_none(), "the deleted endpoint must be cleared");
    assert!(!r.is_complete());
    assert_eq!(
        waveform.focused_marker, None,
        "focus must return to the detail waveform after delete"
    );
}

/// `F2`/`Enter` opens the inline rename; `Enter` commits, `Esc` cancels
/// without touching the model (contracts/ui-markers.md §3, AS1).
#[test]
fn f2_rename_commits_on_enter_cancels_on_esc() {
    let (mut controller, _handle, _dir) = active_controller("rename-commit-cancel");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(id),
        ..Default::default()
    };
    let ctx = Context::default();
    ctx.enable_accesskit();

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::F2);
    assert_eq!(
        waveform.rename.as_ref().map(|(rename_id, _)| *rename_id),
        Some(id),
        "F2 must open the inline rename for the focused marker"
    );

    if let Some((_, name)) = waveform.rename.as_mut() {
        *name = "Verse".to_string();
    }
    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Enter,
    );
    assert_eq!(waveform.rename, None, "Enter must close the rename");
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id))
            .map(|m| m.name.clone()),
        Some("Verse".to_string()),
        "Enter must commit the draft"
    );

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::F2);
    if let Some((_, name)) = waveform.rename.as_mut() {
        *name = "Should not stick".to_string();
    }
    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Escape,
    );
    assert_eq!(waveform.rename, None, "Esc must close the rename");
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id))
            .map(|m| m.name.clone()),
        Some("Verse".to_string()),
        "Esc must not commit the draft"
    );
}

/// `C` cycles the focused marker's palette colour, and the panel/overlay
/// repaint in the new colour immediately (contracts/ui-markers.md §3, AS2)
/// — both derive from the same `theme::marker_color(marker.color)` call,
/// so the model's colour is the single source of truth for both.
#[test]
fn c_cycles_palette_and_row_matches_glyph() {
    let (mut controller, _handle, _dir) = active_controller("cycle-color");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let before = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.color)
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(id),
        ..Default::default()
    };
    let ctx = Context::default();
    ctx.enable_accesskit();

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::C);

    let after = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.color)
        .unwrap_or_else(|| unreachable!());
    assert_ne!(before, after, "C must cycle the palette");

    let markers_now = controller.markers().cloned();
    let expected_color = modplayer_ui::theme::marker_color(after);
    let shapes = painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
        modplayer_ui::markers::paint_overlay(painter, space, markers_now.as_ref(), 0, None);
    });
    let has_matching_line = shapes.iter().any(|shape| {
        matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == expected_color)
    });
    assert!(
        has_matching_line,
        "the overlay line must repaint in the marker's new colour"
    );
}

/// A relative-delta drag from the overview glyph lands within 5ms of the
/// intended sample (SC-003), via the detail-view zoom-assist.
#[test]
fn drag_from_overview_zooms_detail_and_lands_within_5ms() {
    let (mut controller, _handle, _dir) = active_controller("drag-overview");
    controller.queue_replace(vec![track("a", 200_000)]);

    const SAMPLE_RATE: u32 = 44_100;
    let len_frames = (200_000u64 * u64::from(SAMPLE_RATE)) / 1000;
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let overview_rect = overview_bounds(&ctx, &mut controller, &mut artwork, &mut waveform);
    let glyph_rect = glyph_bounds(&ctx, &mut controller, &mut artwork, &mut waveform);
    let press_pos = glyph_rect.center();
    let target_x = (press_pos.x + 80.0).min(overview_rect.right() - 1.0);

    let space = TimeSpace::new(overview_rect, 0..len_frames, SAMPLE_RATE);
    let expected_frame = space.frame_at(target_x);

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: press_pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();

    let mut drag = default_input();
    drag.events
        .push(Event::PointerMoved(Pos2::new(target_x, press_pos.y)));
    let output = ctx.run_ui(drag, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    assert!(
        waveform.marker_drag.is_some(),
        "sanity: a drag must be in progress"
    );

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: Pos2::new(target_x, press_pos.y),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();

    assert_eq!(
        waveform.marker_drag, None,
        "release must commit and clear the drag"
    );
    let committed = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let margin_frames = (5 * u64::from(SAMPLE_RATE)) / 1000;
    let delta = committed.abs_diff(expected_frame);
    assert!(
        delta <= margin_frames,
        "committed {committed} vs intended {expected_frame} (delta {delta} > {margin_frames} frames / 5ms)"
    );
}

/// `Esc` while dragging a marker restores its original position and the
/// detail window it had before the drag started (contracts/ui-markers.md
/// §5).
#[test]
fn drag_esc_restores_position_and_window() {
    let (mut controller, _handle, _dir) = active_controller("drag-esc");
    controller.queue_replace(vec![track("a", 200_000)]);

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let origin = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let glyph_rect = glyph_bounds(&ctx, &mut controller, &mut artwork, &mut waveform);
    let original_detail = waveform.detail.expect("a detail window must exist by now");
    let press_pos = glyph_rect.center();
    let target_x = press_pos.x + 80.0;

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: press_pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();

    let mut drag = default_input();
    drag.events
        .push(Event::PointerMoved(Pos2::new(target_x, press_pos.y)));
    let output = ctx.run_ui(drag, |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    assert!(
        waveform.marker_drag.is_some(),
        "sanity: a drag must be in progress"
    );
    assert_ne!(
        waveform.detail,
        Some(original_detail),
        "sanity: the zoom-assist must have changed the detail window"
    );

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

    assert_eq!(waveform.marker_drag, None, "Esc must clear the drag");
    assert_eq!(
        controller.markers().and_then(|m| m.position_of(id)),
        Some(origin),
        "Esc must not move the marker"
    );
    assert_eq!(
        waveform.detail,
        Some(original_detail),
        "Esc must restore the pre-drag detail window"
    );
}

/// `Shift+1` sets slot 1's cue at the playhead and `1` jumps back to it
/// exactly, through `seek_frames` (006 US4, contracts/ui-markers.md §2,
/// AS1–AS3): the jump changes neither the position (an unrelated seek
/// away first proves it actually moved) nor whether playback is playing
/// or paused.
#[test]
fn shift_digit_sets_cue_and_digit_jumps_keeping_state() {
    let (mut controller, _handle, _dir) = active_controller("cue-set-jump");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(5);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let slot1 = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    let cue_pos = controller.shared().position_frames();
    press_key_with(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
        Modifiers::SHIFT,
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.cue(slot1))
            .map(|c| c.position),
        Some(cue_pos),
        "Shift+1 must set slot 1's cue at the playhead"
    );
    assert_eq!(waveform.marker_status, None);

    // While playing: move well away, jump back with `1`. Landing the seek
    // itself requires one render (the engine drains its command queue at
    // the start of a render call), so the observed position is `cue_pos`
    // plus that one render's own frame advance — the fixed
    // `JUMP_LAND_TOLERANCE_FRAMES` below comfortably covers it while still
    // proving the jump landed at the cue, not merely continued from
    // `moved_pos` (FR-014, "instant and precise").
    let _ = controller.backend_mut().render_buffers(10);
    let moved_pos = controller.shared().position_frames();
    assert!(
        moved_pos > cue_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "sanity: playhead must have advanced well past the cue"
    );
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
    );
    controller.tick();
    let _ = controller.backend_mut().render_buffers(1);

    let landed = controller.shared().position_frames();
    assert!(
        landed >= cue_pos && landed <= cue_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "1 must jump to (very near) the cue: landed={landed}, cue_pos={cue_pos}"
    );
    assert!(
        landed < moved_pos,
        "the jump must have moved backward, not just continued playing"
    );
    assert_eq!(
        controller.transport_state().intent,
        Intent::Playing,
        "jumping while playing must not pause"
    );

    // While paused: `1` still dispatches the same `seek_frames` (FR-014's
    // existing seek path already keeps a paused transport paused, per
    // 005 — checked here at the play/pause-state level only).
    controller.pause();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Paused);

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
    );
    controller.tick();

    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "jumping while paused must not resume playback"
    );
}

/// `1`-`8` on an empty slot is a silent no-op (006 US4, AS5,
/// contracts/ui-markers.md §2): no seek, no status, no marker.
#[test]
fn digit_on_empty_slot_is_noop() {
    let (mut controller, _handle, _dir) = active_controller("cue-empty-noop");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(5);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let before_pos = controller.shared().position_frames();
    let before_intent = controller.transport_state().intent;
    let count_before = controller.markers().map(TrackMarkers::count).unwrap_or(0);

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num2,
    );
    controller.tick();
    let _ = controller.backend_mut().render_buffers(1);

    assert_eq!(
        waveform.marker_status, None,
        "an empty-slot jump is never a refusal"
    );
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        count_before,
        "an empty-slot jump must not create a marker"
    );
    assert_eq!(
        controller.transport_state().intent,
        before_intent,
        "an empty-slot jump must not change play/pause state"
    );
    assert!(
        controller.shared().position_frames() >= before_pos,
        "no seek must have fired: the playhead only advances forward, from natural playback"
    );
}

/// `Shift+n` on an already-occupied slot is a move, not a creation, so it
/// never hits the 64-marker limit (006 US4, AS6, contracts/marker-
/// service.md §7 — mirrors `markers_model.rs::move_paths_never_hit_limit`
/// at the controller/UI layer).
#[test]
fn shift_digit_on_occupied_slot_moves_at_limit() {
    let (mut controller, _handle, _dir) = active_controller("cue-move-at-limit");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let slot1 = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    controller
        .set_cue(slot1)
        .unwrap_or_else(|e| unreachable!("set_cue: {e}"));
    for _ in 0..63 {
        controller
            .add_point_marker()
            .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    }
    assert_eq!(controller.markers().map(TrackMarkers::count), Some(64));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    ctx.enable_accesskit();

    let _ = controller.backend_mut().render_buffers(5);
    let new_pos = controller.shared().position_frames();

    press_key_with(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
        Modifiers::SHIFT,
    );

    assert_eq!(
        waveform.marker_status, None,
        "moving an occupied slot at the 64-marker cap must not refuse"
    );
    assert_eq!(
        controller.markers().map(TrackMarkers::count),
        Some(64),
        "a move creates no new marker"
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.cue(slot1))
            .map(|c| c.position),
        Some(new_pos),
        "slot 1's cue must have moved to the new playhead"
    );
}

/// FR-022 / contracts/ui-markers.md §3: reaching a glyph with `Tab` must
/// arm the focused-marker key table exactly as clicking it does. Regression
/// for the gap found driving quickstart.md's M15 — `focused_marker` was set
/// only from `clicked()`/`drag_started()`, so a keyboard-only user could
/// land on a glyph and still not nudge, rename, recolour or delete it.
#[test]
fn tab_focus_on_a_glyph_enables_the_marker_key_table() {
    let (mut controller, _handle, _dir) = active_controller("tab-focus");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller.set_nudge_step_ms(25);

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let start = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    // No `focused_marker`: focus arrives purely through egui, as `Tab` does.
    let mut waveform = WaveformState::default();
    let ctx = Context::default();

    let glyph_id = egui::Id::new(("marker-glyph", "overview", id));
    let output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    ctx.memory_mut(|memory| memory.request_focus(glyph_id));

    let output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(ui, &mut controller, &mut artwork, &mut waveform)
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        waveform.focused_marker,
        Some(id),
        "keyboard focus on the glyph selects it, like a click"
    );

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::ArrowRight,
    );
    let rate = controller.source_sample_rate().max(1);
    let step = (25u64 * u64::from(rate)) / 1000;
    assert_eq!(
        controller.markers().and_then(|m| m.position_of(id)),
        Some(start + step),
        "and the nudge row then applies without any pointer input"
    );
}

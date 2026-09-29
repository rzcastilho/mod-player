// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T040 (US2, FR-014, Constitution VIII): `TimeSpace`'s pixel<->frame
//! mapping round-trips within one pixel for arbitrary windows/rects — the
//! coordinate space every later overlay (markers, loops, plugins) attaches
//! through (data-model.md §5.3). `DetailWindow`'s own pure-method tests
//! live alongside its implementation in `src/waveform/state.rs` (T039).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::Role;
use egui::{Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, pos2};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, PeakBucket, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::plugins::PluginId;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{AnalysisStatus, PeakLevel, PlaybackController, WaveformPeaks, tr};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::section_memory::{SectionMemory, ViewKey};
use modplayer_ui::waveform::{self, TimeSpace, WaveformEvent, WaveformPaint, WaveformState};
use modplayer_ui::{layout, now_playing};
use proptest::prelude::*;

proptest! {
    #[test]
    fn time_space_round_trips_within_one_pixel(
        left in 0.0f32..10_000.0,
        width in 1.0f32..4_000.0,
        window_start in 0u64..(3_600 * 192_000),
        span in 1u64..(3_600 * 192_000),
        sample_rate in 8_000u32..192_000,
        frame_offset in 0.0f64..1.0,
    ) {
        let rect = Rect::from_min_size(pos2(left, 0.0), egui::vec2(width, 40.0));
        let window = window_start..window_start.saturating_add(span);
        let space = TimeSpace::new(rect, window.clone(), sample_rate);

        let frame = window.start + ((frame_offset * span as f64).round() as u64).min(span);
        let x = space.x_of(frame);
        let back = space.frame_at(x);
        let fpp = space.frames_per_pixel();

        prop_assert!(x.is_finite());
        prop_assert!(
            (frame.abs_diff(back) as f64) <= fpp.max(1.0),
            "frame {frame} round-tripped to {back} (fpp {fpp})"
        );
    }
}

// ---------------------------------------------------------------------
// D10.9 (018-window-sizing-and-responsive-dock, contract D8, FR-012):
// `waveform::overview`/`detail` now allocate whatever `height` they are
// given rather than a fixed constant — proving they are actually wired
// to `layout::waveform_heights`'s own result, not just that the pure
// formula (already covered by layout.rs's own D10.3) is correct.
// ---------------------------------------------------------------------

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(
            pos2(0.0, 0.0),
            egui::vec2(1200.0, 2000.0),
        )),
        ..Default::default()
    }
}

/// Runs `waveform::overview`/`detail` inside a bare `Context::run_ui` and
/// returns the rect height each widget actually allocated
/// (`WaveformResponse::space::rect`), for a given content height `h`.
fn allocated_heights(h: f32) -> (f32, f32) {
    let (overview_height, detail_height) = layout::waveform_heights(h);
    let paint_data = WaveformPaint {
        status: AnalysisStatus::Pending,
        peaks: None,
        playhead: None,
        unavailable_text: "unavailable",
        highlight: None,
        hover_suppressed: false,
    };
    let ctx = Context::default();
    let mut overview_rect_height = 0.0;
    let mut detail_rect_height = 0.0;
    let output = ctx.run_ui(default_input(), |ui| {
        let (overview_response, _event) = waveform::overview(
            ui,
            10_000,
            1_000,
            0,
            false,
            true,
            overview_height,
            &paint_data,
            &mut |_painter, _space| {},
        );
        overview_rect_height = overview_response.space.rect.height();
        let (detail_response, _event) = waveform::detail(
            ui,
            0..10_000,
            1_000,
            0,
            false,
            true,
            detail_height,
            &paint_data,
            &mut |_painter, _space| {},
        );
        detail_rect_height = detail_response.space.rect.height();
    });
    output.drop_without_applying_deltas();
    (overview_rect_height, detail_rect_height)
}

/// D10.9: for two distinct window heights, both widgets allocate exactly
/// `layout::waveform_heights(H)`'s own values — proving `now_playing.rs`'s
/// per-frame `H` capture actually reaches the widgets' allocated rects.
#[test]
fn overview_and_detail_heights_track_content_height() {
    for h in [400.0f32, 900.0f32] {
        let (expected_overview, expected_detail) = layout::waveform_heights(h);
        let (overview_rect_height, detail_rect_height) = allocated_heights(h);
        assert!(
            (overview_rect_height - expected_overview).abs() < 0.01,
            "H={h}: overview allocated {overview_rect_height}, expected {expected_overview}"
        );
        assert!(
            (detail_rect_height - expected_detail).abs() < 0.01,
            "H={h}: detail allocated {detail_rect_height}, expected {expected_detail}"
        );
    }
}

// ---------------------------------------------------------------------
// T-S2 (021-transport-bar-and-panel-layout, contract S2, FR-003, FR-011,
// SC-003): the same claim as D10.9 above, but through the real
// `now_playing::show` integration rather than a bare call to
// `waveform::overview`/`detail` — proving `layout::waveform_heights(H)`
// is still what the widgets receive once the pinned transport bar and
// the single scroll region sit around them, and that H (captured before
// either is drawn, contract S2) stays independent of the scroll
// region's own offset.
// ---------------------------------------------------------------------

/// 014-design-tokens-and-type-scale (US2, T022): installs the token
/// `Style`'s named text styles (`display`, `section`, …) that
/// `now_playing::show` reaches — mirrors every other integration test
/// file's identically named helper (e.g. `now_playing.rs`, `markers.rs`).
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
            "modplayer-ui-waveform-{label}-{}-{unique}",
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
/// new`'s one synchronous read of it (mirrors `now_playing.rs`'s own
/// lock/pattern).
static TRACK_STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Both temp dirs `active_controller` allocates (settings, track-state);
/// kept alive together so either can be dropped only once the test itself
/// is done with the controller.
struct TestDirs(#[allow(dead_code)] TempDir, #[allow(dead_code)] TempDir);

/// A controller over a confirmed device with a track already queued
/// (mirrors `now_playing.rs`'s own `active_controller`, trimmed to what
/// this file needs): the baseline every test below starts from.
fn active_controller(label: &str) -> (PlaybackController<FakeBackend, ScriptedHost>, TestDirs) {
    let (store, dir) = fresh_store(label);
    let track_state_dir = TempDir::new(&format!("{label}-track-state"));
    let host = ScriptedHost::new();
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
    // layout between the warm-up and the measured frame below.
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
    controller.queue_replace(vec![track("a", 200_000)]);
    (controller, TestDirs(dir, track_state_dir))
}

/// Runs `now_playing::show` at a `960 x height` screen, with
/// `ViewKey::NowPlaying`'s scroll offset preset to `scroll_offset`, and
/// returns the overview's and detail's own AccessKit bounds heights —
/// the same rects the widgets themselves allocated (`response.
/// widget_info` fills AccessKit bounds from the response's own rect, so
/// this reads exactly what D10.9's direct-call helper above reads via
/// `WaveformResponse::space::rect`). A warm-up frame lets the dock/bar
/// layout settle before the measured frame (this file's own convention,
/// mirroring `now_playing.rs`'s `bar_outer_height`).
fn overview_and_detail_heights_via_show(
    label: &str,
    height: f32,
    scroll_offset: f32,
) -> (f32, f32) {
    let (mut controller, _dirs) = active_controller(label);
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let mut memory = SectionMemory::default();
    if scroll_offset != 0.0 {
        memory.record(ViewKey::NowPlaying, scroll_offset);
    }
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(
            pos2(0.0, 0.0),
            egui::vec2(960.0, height),
        )),
        ..Default::default()
    };

    // Warm-up frame.
    let output = ctx.run_ui(input.clone(), |ui| {
        now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut memory,
        );
    });
    output.drop_without_applying_deltas();

    // Measured frame.
    let mut output = ctx.run_ui(input, |ui| {
        now_playing::show(
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

    let overview_name = tr("transport-seek");
    let detail_name = tr("waveform-detail");
    let mut overview_height = None;
    let mut detail_height = None;
    for (_, node) in &update.nodes {
        if node.role() != Role::Slider {
            continue;
        }
        let Some(bounds) = node.bounds() else {
            continue;
        };
        let height = (bounds.y1 - bounds.y0) as f32;
        if node.label() == Some(overview_name.as_str()) {
            overview_height = Some(height);
        } else if node.label() == Some(detail_name.as_str()) {
            detail_height = Some(height);
        }
    }
    (
        overview_height.expect("the overview slider must render once a track is loaded"),
        detail_height.expect("the detail slider must render once a track is loaded"),
    )
}

/// T-S2: overview and detail heights at H ∈ {640, 820, 1200} equal
/// `layout::waveform_heights(H)` through the real `now_playing::show`
/// integration, and stay pinned to H regardless of the scroll region's
/// own offset — H is captured before the bar or scroll area are drawn
/// (contract S2), so scrolling the region must not perturb it.
#[test]
fn overview_and_detail_heights_track_window_height_independent_of_scroll_via_show() {
    for h in [640.0f32, 820.0f32, 1200.0f32] {
        let (expected_overview, expected_detail) = layout::waveform_heights(h);
        let (overview_h, detail_h) =
            overview_and_detail_heights_via_show(&format!("s2-h-{h}"), h, 0.0);
        assert!(
            (overview_h - expected_overview).abs() < 1.0,
            "H={h}: overview allocated {overview_h}, expected {expected_overview}"
        );
        assert!(
            (detail_h - expected_detail).abs() < 1.0,
            "H={h}: detail allocated {detail_h}, expected {expected_detail}"
        );
    }

    // Independent of scroll offset: at a fixed H, a non-zero (even
    // past-max, clamped) scroll offset must not change either height.
    let h = 820.0f32;
    let (expected_overview, expected_detail) = layout::waveform_heights(h);
    for scroll_offset in [0.0f32, 1.0e6] {
        let (overview_h, detail_h) = overview_and_detail_heights_via_show(
            &format!("s2-scroll-{scroll_offset}"),
            h,
            scroll_offset,
        );
        assert!(
            (overview_h - expected_overview).abs() < 1.0,
            "scroll_offset={scroll_offset}: overview allocated {overview_h}, expected {expected_overview}"
        );
        assert!(
            (detail_h - expected_detail).abs() < 1.0,
            "scroll_offset={scroll_offset}: detail allocated {detail_h}, expected {expected_detail}"
        );
    }
}

// ---------------------------------------------------------------------
// T012 (022-waveform-legibility, US1, contracts/ui-waveform-legibility.md
// WL3, FR-003, FR-019): the playhead paints two `line_segment`s — a
// `PLAYHEAD_CASING_WIDTH` casing in `playhead_casing`, then a
// `PLAYHEAD_CORE_WIDTH` core in `playhead_core` — topmost on both views
// (above the fill and the `overlays` hook). Written first and failing:
// `paint::playhead` still draws 005/006's single `strong_text_color`
// stroke.
// ---------------------------------------------------------------------

enum PlayheadView {
    Overview,
    Detail,
}

/// Runs `waveform::overview`/`detail` with a playhead mid-track, an
/// `overlays` hook that paints one translucent rect standing in for 006's
/// loop shading / marker lines (WL1's layer 4, not otherwise exercised by
/// this file), and returns every shape the frame painted in paint order —
/// the same order `output.shapes` records, where a later entry paints
/// over an earlier one for overlapping geometry (mirrors `tests/
/// markers.rs`'s `painted_shapes` helper).
fn playhead_shape_capture(view: PlayheadView) -> Vec<Shape> {
    let paint_data = WaveformPaint {
        status: AnalysisStatus::Pending,
        peaks: None,
        playhead: Some(5_000),
        unavailable_text: "unavailable",
        highlight: None,
        hover_suppressed: false,
    };
    let ctx = fresh_ctx();
    // `theme::roles` reads `visuals.dark_mode`; pin the light theme so the
    // shapes below compare against `LIGHT_WAVEFORM` deterministically,
    // independent of the harness's default `Context` theme.
    ctx.set_theme(egui::ThemePreference::from(egui::Theme::Light));
    let mut overlays = |painter: &egui::Painter, space: &TimeSpace| {
        painter.rect_filled(
            space.rect,
            0.0,
            Color32::from_rgba_unmultiplied(255, 0, 0, 64),
        );
    };
    let output = ctx.run_ui(default_input(), |ui| match view {
        PlayheadView::Overview => {
            let _ = waveform::overview(
                ui,
                10_000,
                1_000,
                5_000,
                false,
                true,
                64.0,
                &paint_data,
                &mut overlays,
            );
        }
        PlayheadView::Detail => {
            let _ = waveform::detail(
                ui,
                0..10_000,
                1_000,
                5_000,
                false,
                true,
                120.0,
                &paint_data,
                &mut overlays,
            );
        }
    });
    let shapes = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();
    shapes
}

/// WL3: exactly two `line_segment`s at the playhead x, same points (full
/// rect height) — casing first at `PLAYHEAD_CASING_WIDTH` in
/// `playhead_casing`, core second at `PLAYHEAD_CORE_WIDTH` in
/// `playhead_core` — on both the overview and the detail view.
#[test]
fn playhead_paints_casing_then_core_on_both_views() {
    use modplayer_ui::theme::tokens::LIGHT;
    use modplayer_ui::theme::waveform::{
        PLAYHEAD_CASING_WIDTH, PLAYHEAD_CORE_WIDTH, waveform_roles,
    };

    let roles = waveform_roles(&LIGHT);
    for view in [PlayheadView::Overview, PlayheadView::Detail] {
        let shapes = playhead_shape_capture(view);
        let lines: Vec<_> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::LineSegment { points, stroke } => Some((*points, *stroke)),
                _ => None,
            })
            .collect();
        assert_eq!(
            lines.len(),
            2,
            "expected exactly the casing and core line segments, got {lines:?}"
        );
        let (casing_points, casing_stroke) = lines[0];
        let (core_points, core_stroke) = lines[1];
        assert_eq!(casing_stroke.width, PLAYHEAD_CASING_WIDTH);
        assert_eq!(casing_stroke.color, roles.playhead_casing);
        assert_eq!(core_stroke.width, PLAYHEAD_CORE_WIDTH);
        assert_eq!(core_stroke.color, roles.playhead_core);
        assert_eq!(
            casing_points, core_points,
            "casing and core sit at the same x, spanning the full rect height"
        );
    }
}

/// WL1: the playhead is always topmost — it must paint after (over) every
/// other shape the frame produced, on both views.
#[test]
fn playhead_paints_topmost_above_fill_and_overlays() {
    for view in [PlayheadView::Overview, PlayheadView::Detail] {
        let shapes = playhead_shape_capture(view);
        let line_indices: Vec<usize> = shapes
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s, Shape::LineSegment { .. }))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            line_indices.len(),
            2,
            "playhead must paint two line segments"
        );
        let last_other_index = shapes
            .iter()
            .enumerate()
            .filter(|(_, s)| !matches!(s, Shape::LineSegment { .. }))
            .map(|(i, _)| i)
            .max()
            .expect("the fill/overlay shapes must be present ahead of the playhead");
        assert!(
            line_indices[0] > last_other_index,
            "the playhead's casing must paint after every fill/overlay shape"
        );
    }
}

// ---------------------------------------------------------------------
// T018 (022-waveform-legibility, US3, contracts/ui-waveform-legibility.md
// WL5, data-model.md §8): the hover scrub indicator — a stateless 1px
// line + `m:ss.mmm` label present only on the hovered view, only while
// `enabled` and not `hover_suppressed`; follows the pointer; never emits
// a `WaveformEvent`; never changes the AccessKit node. Written first and
// failing: `waveform::hover` does not exist yet.
// ---------------------------------------------------------------------

fn hover_paint_data(hover_suppressed: bool) -> WaveformPaint<'static> {
    WaveformPaint {
        status: AnalysisStatus::Pending,
        peaks: None,
        playhead: Some(5_000),
        unavailable_text: "unavailable",
        highlight: None,
        hover_suppressed,
    }
}

fn input_with_pointer(pointer: Pos2) -> RawInput {
    let mut input = default_input();
    input.events.push(Event::PointerMoved(pointer));
    input
}

/// The hover line's own stroke width ([`theme::waveform::
/// HOVER_LINE_WIDTH`]) is distinct from the playhead's (2px core / 4px
/// casing), so filtering on it unambiguously picks the hover line out of
/// a frame that also painted the playhead.
fn hover_line_shapes(shapes: &[Shape]) -> Vec<(Pos2, Pos2, Color32)> {
    use modplayer_ui::theme::waveform::HOVER_LINE_WIDTH;
    shapes
        .iter()
        .filter_map(|s| match s {
            // Width alone also matches other, unrelated 1px-stroked
            // shapes — `theme::divider`'s horizontal rule, an unfocused
            // marker line, or (through the full `now_playing::show`
            // integration) the master peak meter's `-6dB`/`0dB` scale
            // ticks (`widgets/peak_meter.rs::paint_mark`, ~18px tall).
            // The hover line is additionally vertical and spans a full
            // waveform rect's height — at least the 64px overview
            // minimum (018-window-sizing-and-responsive-dock), well
            // above any of those.
            Shape::LineSegment { points, stroke }
                if stroke.width == HOVER_LINE_WIDTH
                    && points[0].x == points[1].x
                    && (points[0].y - points[1].y).abs() >= 40.0 =>
            {
                Some((points[0], points[1], stroke.color))
            }
            _ => None,
        })
        .collect()
}

/// Runs `waveform::overview` as the frame's only widget — `Context::
/// run_ui`'s root `Ui` covers the whole viewport with no margin, so it
/// lands at `rect (0,0)-(1200,64)` for `default_input()`'s 1200-wide
/// screen — and returns the shapes it painted plus the `WaveformEvent`,
/// if any.
fn overview_frame(
    pointer: Option<Pos2>,
    enabled: bool,
    hover_suppressed: bool,
) -> (Vec<Shape>, Option<WaveformEvent>) {
    let paint_data = hover_paint_data(hover_suppressed);
    let ctx = fresh_ctx();
    let run = |input: RawInput| {
        let mut event_out = None;
        let output = ctx.run_ui(input, |ui| {
            let (_response, event) = waveform::overview(
                ui,
                10_000,
                1_000,
                5_000,
                false,
                enabled,
                64.0,
                &paint_data,
                &mut |_painter, _space| {},
            );
            event_out = event;
        });
        let shapes: Vec<Shape> = output.shapes.iter().map(|c| c.shape.clone()).collect();
        output.drop_without_applying_deltas();
        (shapes, event_out)
    };
    // A warm-up pass with no pointer event first: egui resolves a
    // widget's `hovered()`/`hover_pos()` against the interaction snapshot
    // its *previous* pass left behind (this file's other hover-adjacent
    // tests, e.g. `widgets/controls.rs`'s `button_hover_and_press_track_
    // across_passes`, follow the same two-pass convention), so a bare
    // pointer-move on a Context's very first pass never registers.
    let _ = run(default_input());
    run(pointer.map_or_else(default_input, input_with_pointer))
}

#[test]
fn hover_present_only_while_enabled_and_unsuppressed() {
    // Inside the overview's own (0,0)-(1200,64) rect.
    let pointer = pos2(600.0, 30.0);

    let (shapes, _) = overview_frame(Some(pointer), true, false);
    assert_eq!(
        hover_line_shapes(&shapes).len(),
        1,
        "enabled, hovered, unsuppressed: the hover line must paint"
    );

    let (shapes, _) = overview_frame(Some(pointer), false, false);
    assert!(
        hover_line_shapes(&shapes).is_empty(),
        "disabled: no hover line even while the pointer sits over the rect"
    );

    let (shapes, _) = overview_frame(Some(pointer), true, true);
    assert!(
        hover_line_shapes(&shapes).is_empty(),
        "hover_suppressed: no hover line, e.g. during a drag"
    );

    let (shapes, _) = overview_frame(None, true, false);
    assert!(
        hover_line_shapes(&shapes).is_empty(),
        "no pointer over the widget: no hover line"
    );
}

/// WL5: "Only the hovered view shows it; the other view does not
/// mirror." Both views drawn in the same frame; the pointer sits inside
/// the overview's own rect (`y < 64`) and therefore outside the detail's,
/// which starts below it (egui's default item spacing pages the cursor
/// down between the two `allocate_exact_size` calls).
#[test]
fn hover_shows_only_on_the_hovered_view() {
    let pointer = pos2(600.0, 30.0);
    let overview_paint = hover_paint_data(false);
    let detail_paint = hover_paint_data(false);
    let ctx = fresh_ctx();
    let run = |input: RawInput| {
        let output = ctx.run_ui(input, |ui| {
            let _ = waveform::overview(
                ui,
                10_000,
                1_000,
                5_000,
                false,
                true,
                64.0,
                &overview_paint,
                &mut |_painter, _space| {},
            );
            let _ = waveform::detail(
                ui,
                0..10_000,
                1_000,
                5_000,
                false,
                true,
                120.0,
                &detail_paint,
                &mut |_painter, _space| {},
            );
        });
        let shapes: Vec<Shape> = output.shapes.iter().map(|c| c.shape.clone()).collect();
        output.drop_without_applying_deltas();
        shapes
    };
    let _ = run(default_input()); // warm-up: see `overview_frame`'s own note.
    let shapes = run(input_with_pointer(pointer));

    assert_eq!(
        hover_line_shapes(&shapes).len(),
        1,
        "exactly one hover line across both views, on the hovered one"
    );
}

#[test]
fn hover_follows_the_pointer() {
    let (shapes_left, _) = overview_frame(Some(pos2(100.0, 30.0)), true, false);
    let (shapes_right, _) = overview_frame(Some(pos2(1_000.0, 30.0)), true, false);
    let left_x = hover_line_shapes(&shapes_left)[0].0.x;
    let right_x = hover_line_shapes(&shapes_right)[0].0.x;
    assert!(
        left_x < right_x,
        "hover line must track the pointer: left={left_x}, right={right_x}"
    );
}

#[test]
fn hover_never_returns_a_waveform_event() {
    let (_shapes, event) = overview_frame(Some(pos2(600.0, 30.0)), true, false);
    assert_eq!(
        event, None,
        "merely hovering (no click/drag/key) must never seek or preview"
    );
}

struct SliderAccessKitSnapshot {
    value: Option<String>,
    description: Option<String>,
}

/// The `transport-seek` slider's own AccessKit node for a single
/// `waveform::overview` frame (FR-016/FR-019: hovering must never change
/// it).
fn overview_accesskit_snapshot(pointer: Option<Pos2>) -> SliderAccessKitSnapshot {
    let paint_data = hover_paint_data(false);
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut warm_up = ctx.run_ui(default_input(), |ui| {
        let _ = waveform::overview(
            ui,
            10_000,
            1_000,
            5_000,
            false,
            true,
            64.0,
            &paint_data,
            &mut |_painter, _space| {},
        );
    });
    warm_up.platform_output.accesskit_update.take();
    warm_up.drop_without_applying_deltas();

    let input = pointer.map_or_else(default_input, input_with_pointer);
    let mut output = ctx.run_ui(input, |ui| {
        let _ = waveform::overview(
            ui,
            10_000,
            1_000,
            5_000,
            false,
            true,
            64.0,
            &paint_data,
            &mut |_painter, _space| {},
        );
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    let name = tr("transport-seek");
    for (_, node) in &update.nodes {
        if node.role() == Role::Slider && node.label() == Some(name.as_str()) {
            return SliderAccessKitSnapshot {
                value: node.value().map(str::to_string),
                description: node.description().map(str::to_string),
            };
        }
    }
    panic!("expected the transport-seek slider's AccessKit node");
}

#[test]
fn hover_does_not_change_the_accesskit_node() {
    let without_hover = overview_accesskit_snapshot(None);
    let with_hover = overview_accesskit_snapshot(Some(pos2(600.0, 30.0)));
    assert_eq!(
        without_hover.value, with_hover.value,
        "the slider's value text must be byte-identical with and without hover"
    );
    assert_eq!(
        without_hover.description, with_hover.description,
        "the slider's description must be byte-identical with and without hover"
    );
}

/// WL5's suppression contract, through the real `now_playing::show`
/// integration: hover stays hidden for the whole of a 005 seek-drag, and
/// reappears the next time the pointer moves once the drag has committed
/// on release.
#[test]
fn hover_suppressed_during_a_seek_drag_then_reappears_after_release() {
    let (mut controller, _dirs) = active_controller("hover-drag-suppress");
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let bounds = {
        let mut output = ctx.run_ui(default_input(), |ui| {
            now_playing::show(
                ui,
                &mut controller,
                &mut artwork,
                &mut waveform,
                &mut SectionMemory::default(),
            );
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .expect("accesskit_update should be populated once enabled");
        output.drop_without_applying_deltas();
        let name = tr("transport-seek");
        update
            .nodes
            .iter()
            .find_map(|(_, node)| {
                if node.role() == Role::Slider && node.label() == Some(name.as_str()) {
                    let b = node.bounds()?;
                    Some(Rect::from_min_max(
                        pos2(b.x0 as f32, b.y0 as f32),
                        pos2(b.x1 as f32, b.y1 as f32),
                    ))
                } else {
                    None
                }
            })
            .expect("the overview slider must render once a track is loaded")
    };
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
        now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();

    let mut drag_move = default_input();
    drag_move.events.push(Event::PointerMoved(right));
    let output = ctx.run_ui(drag_move, |ui| {
        now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();
    assert!(
        waveform.drag.is_some(),
        "sanity: a drag preview must be active after this move"
    );

    // `waveform.drag` above was only just set (from this same frame's
    // `WaveformEvent::Preview`), too late to have suppressed this frame's
    // own hover paint — one more frame, still dragging, actually observes
    // the suppression.
    let mut drag_move_again = default_input();
    drag_move_again.events.push(Event::PointerMoved(right));
    let output = ctx.run_ui(drag_move_again, |ui| {
        now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut SectionMemory::default(),
        );
    });
    let shapes: Vec<Shape> = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();
    assert!(
        waveform.drag.is_some(),
        "sanity: the drag preview must still be active"
    );
    assert!(
        hover_line_shapes(&shapes).is_empty(),
        "hover must stay suppressed while a seek-drag is in progress"
    );

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: right,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| {
        now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut SectionMemory::default(),
        );
    });
    output.drop_without_applying_deltas();
    assert!(waveform.drag.is_none(), "sanity: the drag must have ended");

    let mut mv = default_input();
    mv.events.push(Event::PointerMoved(right));
    let output = ctx.run_ui(mv, |ui| {
        now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut SectionMemory::default(),
        );
    });
    let shapes: Vec<Shape> = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();
    assert_eq!(
        hover_line_shapes(&shapes).len(),
        1,
        "hover must reappear once the drag has ended and the pointer moves again"
    );
}

proptest! {
    /// data-model.md §8's validation: `label_rect ⊆ space.rect` for every
    /// pointer x, at both of this feature's supported minimum waveform
    /// heights (018-window-sizing-and-responsive-dock: overview 64px /
    /// detail 120px).
    #[test]
    fn hover_label_rect_always_inside_the_rect_at_minimum_heights(
        width in 150.0f32..4_000.0,
        overview in proptest::bool::ANY,
        x_fraction in 0.0f32..=1.0,
    ) {
        let height: f32 = if overview { 64.0 } else { 120.0 };
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(width, height));
        let space = TimeSpace::new(rect, 0..10_000, 44_100);
        let x = rect.left() + x_fraction * rect.width();
        let indicator = waveform::hover_indicator(
            &space,
            Some(pos2(x, height / 2.0)),
            false,
            egui::vec2(60.0, 18.0),
        )
        .expect("pointer present, not suppressed");
        prop_assert!(
            rect.contains_rect(indicator.label_rect),
            "label_rect {:?} escapes rect {:?} (width={width}, height={height}, x={x})",
            indicator.label_rect,
            rect
        );
    }
}

// ---------------------------------------------------------------------
// T024 (022-waveform-legibility, US4, contracts/ui-waveform-legibility.md
// WL2, SC-005, SC-006): each present column paints an outer peak bar and
// an inner ±RMS average band, in `played_*` tones left of the displayed
// playhead and `unplayed_*` tones right of it. One bucket per pixel (
// `frames_per_bucket = 1`, one `PeakLevel`) keeps each column's frame
// range exactly one caller-specified `(min, max, rms)` triple, so every
// assertion below can address a column by its pixel index. Written first
// and failing: `ColumnPaint::Present` has no `rms` field yet, and
// `paint_columns` neither reads `WaveformPaint::playhead` nor the four
// `played_*`/`unplayed_*` tokens.
// ---------------------------------------------------------------------

/// One `PeakBucket` per pixel column, all present — `waveform_columns`
/// then folds bucket `i` straight into column `i` (`frames_per_pixel ==
/// frames_per_bucket == 1`), so a test can dial in exact per-column
/// `(min, max, rms)` triples.
fn one_bucket_per_pixel_peaks(specs: &[(i8, i8, u8)]) -> WaveformPeaks {
    let buckets: Vec<PeakBucket> = specs
        .iter()
        .map(|&(min, max, rms)| PeakBucket { min, max, rms })
        .collect();
    let len = buckets.len() as u64;
    WaveformPeaks {
        sample_rate: 44_100,
        len_frames: len,
        levels: vec![PeakLevel::full(1, buckets)],
    }
}

/// Same as [`one_bucket_per_pixel_peaks`], but the buckets at `absent`
/// indices are left unmarked — their columns fold to `ColumnPaint::
/// Placeholder` regardless of the `(min, max, rms)` filler value at that
/// index.
fn one_bucket_per_pixel_peaks_with_gaps(specs: &[(i8, i8, u8)], absent: &[usize]) -> WaveformPeaks {
    let buckets: Vec<PeakBucket> = specs
        .iter()
        .map(|&(min, max, rms)| PeakBucket { min, max, rms })
        .collect();
    let len = buckets.len() as u64;
    let present = (0..buckets.len()).filter(|i| !absent.contains(i));
    WaveformPeaks {
        sample_rate: 44_100,
        len_frames: len,
        levels: vec![PeakLevel::partial(1, buckets, present)],
    }
}

/// Runs `paint::paint` directly (not the full `overview`/`detail` widget —
/// this story touches only column fill, not input/AccessKit) over a fresh
/// `width x height` rect at `(0, 0)`, window `0..width` (one frame per
/// pixel, matching [`one_bucket_per_pixel_peaks`]'s one-bucket-per-pixel
/// levels), and returns every shape painted.
fn column_shape_capture(
    peaks: &WaveformPeaks,
    playhead: Option<u64>,
    width: usize,
    height: f32,
) -> Vec<Shape> {
    let paint_data = WaveformPaint {
        status: AnalysisStatus::Complete,
        peaks: Some(peaks),
        playhead,
        unavailable_text: "unavailable",
        highlight: None,
        hover_suppressed: false,
    };
    let ctx = fresh_ctx();
    // Pin the light theme, same reasoning as `playhead_shape_capture`:
    // deterministic comparison against `LIGHT_WAVEFORM`'s own tokens.
    ctx.set_theme(egui::ThemePreference::from(egui::Theme::Light));
    let output = ctx.run_ui(default_input(), |ui| {
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(width as f32, height), egui::Sense::hover());
        let space = TimeSpace::new(rect, 0..(width as u64), 44_100);
        waveform::paint::paint(ui.painter(), &space, ui.visuals(), &paint_data);
    });
    let shapes = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();
    shapes
}

/// Every filled rect exactly one pixel wide, left edge at `x0` — i.e. the
/// rect(s) a single column painted (background/highlight rects span the
/// whole width and never match a one-pixel-wide `x0`).
fn rects_at_column(shapes: &[Shape], x0: f32) -> Vec<(Rect, Color32)> {
    shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Rect(r) if (r.rect.left() - x0).abs() < 0.01 && r.rect.width() <= 1.01 => {
                Some((r.rect, r.fill))
            }
            _ => None,
        })
        .collect()
}

/// WL2/SC-006: shape capture shows at least two distinct fill colours left
/// vs right of the playhead x — every column left of the playhead paints
/// `played_*` tones, every column at/right of it paints `unplayed_*`
/// tones, and the two tone pairs share no colour.
#[test]
fn two_tone_columns_distinct_left_and_right_of_playhead() {
    let width = 20usize;
    let specs: Vec<(i8, i8, u8)> = (0..width).map(|_| (-60i8, 60i8, 40u8)).collect();
    let peaks = one_bucket_per_pixel_peaks(&specs);
    let shapes = column_shape_capture(&peaks, Some(10), width, 40.0);

    let mut left_colors = std::collections::HashSet::new();
    let mut right_colors = std::collections::HashSet::new();
    for col in 0..width {
        let x0 = col as f32;
        let played = x0 + 0.5 < 10.0;
        for (_, color) in rects_at_column(&shapes, x0) {
            if played {
                left_colors.insert(color.to_array());
            } else {
                right_colors.insert(color.to_array());
            }
        }
    }
    assert!(!left_colors.is_empty() && !right_colors.is_empty());
    assert!(
        left_colors.is_disjoint(&right_colors),
        "played tones {left_colors:?} must share no colour with unplayed tones {right_colors:?}"
    );
}

/// WL2: a column with `rms > 0` emits exactly two rects — the outer peak
/// bar and the inner average band — both drawn from its side's own tone
/// pair (`played_peak`/`played_average` left of the playhead,
/// `unplayed_peak`/`unplayed_average` right of it).
#[test]
fn column_with_rms_emits_peak_and_average_rects_in_sides_tone_pair() {
    use modplayer_ui::theme::tokens::LIGHT;
    use modplayer_ui::theme::waveform::waveform_roles;

    let roles = waveform_roles(&LIGHT);
    let width = 20usize;
    let mut specs: Vec<(i8, i8, u8)> = vec![(-5, 5, 0); width];
    specs[3] = (-40, 40, 30); // played side (col_left 3 + 0.5 < playhead x 10)
    specs[15] = (-40, 40, 30); // unplayed side
    let peaks = one_bucket_per_pixel_peaks(&specs);
    let shapes = column_shape_capture(&peaks, Some(10), width, 40.0);

    let played: std::collections::HashSet<_> = rects_at_column(&shapes, 3.0)
        .into_iter()
        .map(|(_, c)| c.to_array())
        .collect();
    assert_eq!(
        played,
        [
            roles.played_peak.to_array(),
            roles.played_average.to_array()
        ]
        .into_iter()
        .collect(),
        "played column must emit exactly the played_peak + played_average pair"
    );

    let unplayed: std::collections::HashSet<_> = rects_at_column(&shapes, 15.0)
        .into_iter()
        .map(|(_, c)| c.to_array())
        .collect();
    assert_eq!(
        unplayed,
        [
            roles.unplayed_peak.to_array(),
            roles.unplayed_average.to_array()
        ]
        .into_iter()
        .collect(),
        "unplayed column must emit exactly the unplayed_peak + unplayed_average pair"
    );
}

/// WL2: `rms == 0` collapses the average band to zero height — under the
/// "drawn only if >= 1px tall" rule, it is omitted entirely, leaving only
/// the peak rect.
#[test]
fn average_band_omitted_when_under_one_pixel() {
    use modplayer_ui::theme::tokens::LIGHT;
    use modplayer_ui::theme::waveform::waveform_roles;

    let roles = waveform_roles(&LIGHT);
    let width = 20usize;
    let mut specs: Vec<(i8, i8, u8)> = vec![(-5, 5, 0); width];
    specs[5] = (-40, 40, 0);
    let peaks = one_bucket_per_pixel_peaks(&specs);
    let shapes = column_shape_capture(&peaks, Some(10), width, 40.0);

    let rects = rects_at_column(&shapes, 5.0);
    assert_eq!(
        rects.len(),
        1,
        "rms=0 must omit the average band: expected only the peak rect, got {rects:?}"
    );
    assert_eq!(rects[0].1, roles.played_peak);
}

/// WL2: the average band is clipped to the peak rect — a saturated `rms`
/// on a narrow peak must not paint outside the peak's own bounds.
#[test]
fn average_band_clipped_to_the_peak_rect() {
    use modplayer_ui::theme::tokens::LIGHT;
    use modplayer_ui::theme::waveform::waveform_roles;

    let roles = waveform_roles(&LIGHT);
    let width = 20usize;
    let mut specs: Vec<(i8, i8, u8)> = vec![(-5, 5, 0); width];
    specs[7] = (-10, 10, 127); // rms saturated far beyond this narrow peak
    let peaks = one_bucket_per_pixel_peaks(&specs);
    let shapes = column_shape_capture(&peaks, Some(10), width, 40.0);

    let rects = rects_at_column(&shapes, 7.0);
    assert_eq!(
        rects.len(),
        2,
        "expected peak + average rects, got {rects:?}"
    );
    let (peak_rect, _) = rects
        .iter()
        .find(|(_, c)| *c == roles.played_peak)
        .expect("peak rect present");
    let (avg_rect, _) = rects
        .iter()
        .find(|(_, c)| *c == roles.played_average)
        .expect("average rect present");
    assert!(
        avg_rect.top() >= peak_rect.top() - 0.01,
        "average band top {} must not rise above the peak rect top {}",
        avg_rect.top(),
        peak_rect.top()
    );
    assert!(
        avg_rect.bottom() <= peak_rect.bottom() + 0.01,
        "average band bottom {} must not exceed the peak rect bottom {}",
        avg_rect.bottom(),
        peak_rect.bottom()
    );
}

/// WL2: placeholder columns (an absent bucket) keep 005's unchanged band —
/// no played/unplayed split, on either side of the playhead.
#[test]
fn placeholder_columns_ignore_the_played_unplayed_split() {
    use modplayer_ui::theme::tokens::LIGHT;
    use modplayer_ui::theme::waveform::waveform_roles;

    let roles = waveform_roles(&LIGHT);
    let width = 20usize;
    let specs: Vec<(i8, i8, u8)> = (0..width).map(|_| (-40i8, 40i8, 30u8)).collect();
    let peaks = one_bucket_per_pixel_peaks_with_gaps(&specs, &[2, 17]);
    let shapes = column_shape_capture(&peaks, Some(10), width, 40.0);

    let left_placeholder = rects_at_column(&shapes, 2.0);
    let right_placeholder = rects_at_column(&shapes, 17.0);
    assert_eq!(
        left_placeholder.len(),
        1,
        "placeholder paints one band rect"
    );
    assert_eq!(
        right_placeholder.len(),
        1,
        "placeholder paints one band rect"
    );
    assert_eq!(
        left_placeholder[0].1, right_placeholder[0].1,
        "placeholder colour must not depend on the played/unplayed side"
    );
    assert_ne!(left_placeholder[0].1, roles.played_peak);
    assert_ne!(left_placeholder[0].1, roles.unplayed_peak);
}

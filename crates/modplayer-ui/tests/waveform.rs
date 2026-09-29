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
use egui::{Context, RawInput, Rect, pos2};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::plugins::PluginId;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{AnalysisStatus, PlaybackController, tr};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::section_memory::{SectionMemory, ViewKey};
use modplayer_ui::waveform::{self, TimeSpace, WaveformPaint, WaveformState};
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

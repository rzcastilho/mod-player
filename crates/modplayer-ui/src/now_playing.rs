// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Now Playing screen (contracts/ui-surface.md §1, extended by
//! 005-now-playing-waveform's contracts/ui-waveform.md §1): artwork,
//! title/artists/album, a status line, the full transport (play/pause,
//! stop, skip back/forward), elapsed/remaining labels either side of a
//! whole-track waveform overview (replacing 003's seek slider), the
//! master-volume/peak-meter widgets (unchanged from 001), and inline
//! disabled reasons.

use std::time::Duration;

use egui::{Button, Id, RichText, Ui, Vec2};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{SourceHealth, SourceHost};
use modplayer_core::{ActiveState, AnalysisStatus, Intent, PlaybackController, tr, tr_args};

use crate::artwork::{ArtworkCache, ArtworkState};
use crate::effects_view;
use crate::markers;
use crate::plugin_overlays::{self, ViewKind};
use crate::plugin_panels;
use crate::queue_view;
use crate::theme;
use crate::transport_view;
use crate::waveform::{
    self, DetailWindow, DragOrigin, DragPreview, WaveformEvent, WaveformPaint, WaveformState,
};
use crate::widgets::initials::initials_placeholder;
use crate::widgets::{peak_meter, volume};

/// The artwork square's side length (matches `rows.rs`'s row artwork).
const ARTWORK_SIZE: f32 = 96.0;

/// Height kept free below the Effect Chain panel's scroll area for the
/// master volume row, the peak meter and the Queue toggle's panel header,
/// so an open panel never pushes them off the bottom of the window.
const EFFECTS_PANEL_RESERVED_HEIGHT: f32 = 140.0;

/// Persists the Queue panel's open/closed state across frames in egui's
/// own per-viewer memory (ui-surface.md §2: reached from Now Playing via a
/// toggle, no dedicated nav-rail section).
fn queue_panel_open_id() -> Id {
    Id::new("now-playing-queue-panel-open")
}

/// `Q` (007, `HostAction::ToggleQueue`, contracts/ui-actions.md §3):
/// flips the same egui temp-memory flag the header's "Queue" toggle
/// button reads/writes, so a keyboard toggle and a click stay in sync.
pub fn toggle_queue_panel(ctx: &egui::Context) {
    let id = queue_panel_open_id();
    ctx.memory_mut(|memory| {
        let open = memory.data.get_temp::<bool>(id).unwrap_or(false);
        memory.data.insert_temp(id, !open);
    });
}

/// Draw the Now Playing screen, applying any transport/volume/seek change
/// directly to `controller`. `waveform` is the session's own waveform
/// widget state (drag preview, detail window — `App`-owned, like
/// `artwork`).
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) {
    // A track change drops any in-flight drag preview from the previous
    // track (data-model.md §5.2) and re-centres the detail window on the
    // new track's playhead, keeping the previous width if any
    // (contracts/ui-waveform.md §5) — `show_waveform` applies the latter
    // once it has computed `playhead_frame`/`len_frames`.
    let current_id = controller.current_track().map(|item| item.track.id.clone());
    let track_changed = current_id != waveform.last_track;
    if track_changed {
        waveform.drag = None;
        waveform.last_track = current_id;
    }

    // 011-plugin-ui-contributions, contracts/ui-panels.md L1: the docked
    // column must draw first, exactly like `app.rs`'s own nav rail
    // (`Panel::left`), so the rest of this content correctly sees the
    // narrower remaining width once at least one panel is docked.
    plugin_panels::show_dock(ui, controller);

    show_heading(ui, controller, artwork);
    show_status_line(ui, controller);
    show_transfer_banner(ui, controller);

    let available = controller.transport_enabled();

    let queue_id = queue_panel_open_id();
    let mut queue_open = ui
        .memory(|memory| memory.data.get_temp::<bool>(queue_id))
        .unwrap_or(false);
    let effects_id = effects_view::panel_open_id();
    let mut effects_open = ui
        .memory(|memory| memory.data.get_temp::<bool>(effects_id))
        .unwrap_or(false);
    let transport_id = transport_view::panel_open_id();
    let mut transport_open = ui
        .memory(|memory| memory.data.get_temp::<bool>(transport_id))
        .unwrap_or(false);

    ui.horizontal(|ui| {
        let playing = controller.transport_state().intent == Intent::Playing;
        let play_pause_key = if playing {
            "transport-pause"
        } else {
            "transport-play"
        };
        if ui
            .add_enabled(available, Button::new(tr(play_pause_key)))
            .clicked()
        {
            if playing {
                controller.pause();
            } else {
                controller.play();
            }
        }

        if ui
            .add_enabled(available, Button::new(tr("transport-stop")))
            .clicked()
        {
            controller.stop();
        }

        if ui
            .add_enabled(available, Button::new(tr("transport-skip-back")))
            .clicked()
        {
            controller.skip_back();
        }

        if ui
            .add_enabled(available, Button::new(tr("transport-skip-forward")))
            .clicked()
        {
            controller.skip_forward();
        }

        if ui
            .selectable_label(queue_open, tr("queue-toggle"))
            .clicked()
        {
            queue_open = !queue_open;
        }

        if ui
            .selectable_label(effects_open, tr("effects-toggle"))
            .clicked()
        {
            effects_open = !effects_open;
        }

        if ui
            .selectable_label(transport_open, tr("transport-toggle"))
            .clicked()
        {
            transport_open = !transport_open;
        }
    });
    ui.memory_mut(|memory| memory.data.insert_temp(queue_id, queue_open));
    ui.memory_mut(|memory| memory.data.insert_temp(effects_id, effects_open));
    ui.memory_mut(|memory| memory.data.insert_temp(transport_id, transport_open));

    if controller.current_track().is_some() {
        show_waveform(ui, controller, waveform, available, track_changed);
        markers::panel(ui, controller, waveform);
        markers::handle_focused_marker_keys(ui, controller, waveform);
    }

    // 008, contracts/ui-effect-chain.md §1: drawn after the waveform/
    // markers block and before the Queue panel, regardless of whether a
    // track is loaded (an empty chain with 0 % figures is valid).
    if effects_open {
        ui.add_space(theme::space::XL);
        // The panel scrolls on its own: a full 16-node chain (or two
        // equalizers) is taller than the window, and without this the
        // "Add node…" row, master volume and the Queue panel fell off the
        // bottom with no way to reach them (2026-09-19 manual walk, M8/
        // M10). Capped so the controls below it stay on screen.
        let max_height = (ui.available_height() - EFFECTS_PANEL_RESERVED_HEIGHT).max(160.0);
        egui::ScrollArea::vertical()
            .id_salt("now-playing-effect-chain-scroll")
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                effects_view::show(ui, controller);
                effects_view::handle_focused_handle_keys(ui, controller);
            });
    }

    // 010-transport-focus, contracts/ui-transport-panel.md §1: drawn after
    // the Effect Chain panel (if open) and before the Queue panel.
    if transport_open {
        ui.add_space(theme::space::XL);
        transport_view::show(ui, controller);
    }

    if let Some(new_volume) = volume::master_volume(ui, controller.master_volume()) {
        controller.set_master_volume(new_volume);
    }

    let peak = controller.shared().peak();
    let ceiling_db = controller.ceiling().db();
    peak_meter::peak_meter(ui, peak, ceiling_db);

    if queue_open {
        ui.add_space(theme::space::XL);
        queue_view::show(ui, controller);
    }

    // contracts/ui-panels.md L2: floated panels last (position-independent
    // overlay windows; drawn after all other Now Playing content).
    plugin_panels::show_floated_windows(ui.ctx(), controller);

    // Position advances at the audio-clock rate (>= 60 Hz, FR-006/SC-003)
    // independent of egui's own input-driven repaint cadence; keep the
    // screen repainting while playing so the readout/waveform playhead
    // stay live even with no mouse/keyboard activity.
    if controller.transport_state().intent == Intent::Playing {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

fn show_heading<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
) {
    match controller.current_track() {
        Some(item) => {
            ui.horizontal(|ui| {
                draw_artwork(
                    ui,
                    artwork,
                    item.track.artwork_url.as_deref(),
                    artwork_name(item),
                );
                ui.vertical(|ui| {
                    // 014-design-tokens-and-type-scale (US2, T022): the
                    // Now Playing track title is the app's one `display`-
                    // role surface (data-model.md §6), a size up from the
                    // `title` role `ui.heading()` would otherwise give it.
                    ui.label(
                        RichText::new(tr_args(
                            "now-playing-title",
                            &[("title", item.track.title.clone())],
                        ))
                        .text_style(theme::text::DISPLAY.clone()),
                    );
                    if !item.track.artists.is_empty() {
                        ui.label(tr_args(
                            "now-playing-artist",
                            &[("artist", item.track.artists.join(", "))],
                        ));
                    }
                    if let Some(album) = &item.track.album {
                        ui.label(tr_args("now-playing-album", &[("album", album.clone())]));
                    }
                });
            });
        }
        None => {
            ui.heading(tr("now-playing-empty"));
            ui.label(tr("now-playing-pick-a-track"));
        }
    }
}

/// The name [`initials_placeholder`] derives from on a `Failed`/no-URL
/// artwork (contracts/ui-waveform.md §1: "initials placeholder(album
/// title, 96)") — the track's own title when it has no album (mirrors
/// `rows.rs`'s `artwork_name`).
fn artwork_name(item: &modplayer_core::QueueItem) -> &str {
    item.track.album.as_deref().unwrap_or(&item.track.title)
}

/// Draw the 96px artwork square: the decoded texture when `Ready`, a
/// neutral square while `Loading`, and the initials placeholder on
/// `Failed` or no URL at all (contracts/ui-waveform.md §1, FR-020).
fn draw_artwork(ui: &mut Ui, artwork: &mut ArtworkCache, url: Option<&str>, name: &str) {
    let state = url.map(|url| artwork.get(ui.ctx(), url));
    match state {
        Some(ArtworkState::Ready(texture_id)) => {
            ui.add(egui::Image::from_texture((
                texture_id,
                Vec2::splat(ARTWORK_SIZE),
            )));
        }
        Some(ArtworkState::Loading) => {
            let (rect, _response) =
                ui.allocate_exact_size(Vec2::splat(ARTWORK_SIZE), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                ui.painter()
                    .rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            }
        }
        Some(ArtworkState::Failed) | None => {
            initials_placeholder(ui, name, ARTWORK_SIZE);
        }
    }
}

/// The one-line status under the heading (contracts/ui-surface.md §1):
/// empty when playing and healthy, otherwise the disabled reason,
/// `Reconnecting…` (US4 T20 — a soft status: the transport stays enabled
/// and audio keeps playing from the buffer while transient), or
/// `buffering`.
fn show_status_line<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &PlaybackController<B, H>,
) {
    if let Some(reason) = controller.disabled_reason() {
        ui.label(tr(reason));
        return;
    }
    if matches!(
        controller.transport_state().health,
        SourceHealth::Transient { .. }
    ) {
        ui.label(tr("status-reconnecting"));
        return;
    }
    if controller.transport_state().buffering {
        ui.label(tr("status-buffering"));
    }
}

/// "Playing on <device>" banner + **Play here** button (contracts/
/// ui-surface.md §1, FR-016/018/019): shown only while
/// `ActiveState::Inactive`; a no-op render otherwise.
fn show_transfer_banner<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    let device = match controller.active_state() {
        ActiveState::Inactive { other_device } => other_device
            .clone()
            .unwrap_or_else(|| tr("banner-unknown-device")),
        _ => return,
    };
    ui.horizontal(|ui| {
        ui.label(tr_args("banner-playing-elsewhere", &[("device", device)]));
        if ui.button(tr("banner-play-here")).clicked() {
            controller.play_here();
        }
    });
}

/// Elapsed label, the waveform overview (with the detail window
/// highlighted), the remaining label, and the waveform detail
/// (contracts/ui-waveform.md §1) — only called with a current track
/// (`show`'s empty-state branch skips this whole block, FR-015).
fn show_waveform<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
    available: bool,
    track_changed: bool,
) {
    // A Connect track always plays at 44.1 kHz; `source_sample_rate()`
    // only reports it once a `TrackStarted`/`BecameActive` has actually
    // reached the engine, so fall back to it here rather than divide by
    // zero for the interval between `queue_replace` and first playback.
    let sample_rate = match controller.source_sample_rate() {
        0 => 44_100,
        rate => rate,
    };
    let track_len_ms = controller
        .transport_state()
        .track_len_ms
        .or_else(|| {
            controller
                .current_track()
                .map(|item| item.track.duration_ms)
        })
        .unwrap_or(0);
    let len_frames = (u64::from(track_len_ms) * u64::from(sample_rate)) / 1000;

    let playhead_frame = match waveform.preview_frame() {
        Some(frame) => frame,
        None => duration_to_frames(controller.position(), sample_rate),
    }
    .min(len_frames);
    let playing = controller.transport_state().intent == Intent::Playing;

    // Follow application order (contracts/ui-waveform.md §5), applied
    // before either widget paints so both show the same, already-updated
    // window this frame.
    let mut detail = match waveform.detail {
        Some(previous) if track_changed => previous.recenter(playhead_frame, len_frames),
        Some(previous) => previous,
        None => DetailWindow::initial(playhead_frame, len_frames, sample_rate),
    };
    if waveform.drag.is_none() {
        detail = detail.follow_playhead(playhead_frame, playing, len_frames);
    }

    // Cloned (Arc pointer copies, not the peak data itself) rather than
    // borrowed, so this doesn't tie a `&controller` borrow across the
    // `apply_waveform_event` calls below (each of which needs `&mut
    // controller` for a committed seek).
    let snapshot = controller.analysis().cloned();
    let status = snapshot
        .as_ref()
        .map(|snapshot| snapshot.status)
        .unwrap_or(AnalysisStatus::Pending);
    let peaks = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.peaks.clone());
    let peaks = peaks.as_deref();
    let unavailable_text = tr("waveform-unavailable");

    ui.label(theme::mono_text(tr_args(
        "time-elapsed",
        &[("time", format_mmss_frames(playhead_frame, sample_rate))],
    )));

    let previewing = waveform.drag.is_some();
    let markers_snapshot = controller.markers().cloned();
    let loop_state = controller.shared().loop_state();
    let focused_marker = waveform.focused_marker;
    // 011-plugin-ui-contributions (US3, FR-014/FR-015): every `Active`
    // plugin's overlay layers, read once for both waveforms this frame —
    // painting them costs zero plugin calls (O10, SC-003).
    let overlay_layers = controller.plugin_overlays();
    markers::lane(
        ui,
        "overview",
        0..len_frames,
        sample_rate,
        len_frames,
        markers_snapshot.as_ref(),
        controller,
        waveform,
        &mut detail,
    );
    let overview_paint = WaveformPaint {
        status,
        peaks,
        playhead: Some(playhead_frame),
        unavailable_text: &unavailable_text,
        highlight: Some(detail.start_frame..(detail.start_frame + detail.width_frames)),
    };
    // US3 T089 (O7): reserve the 16px glyph/label lane directly above the
    // overview's own rect — `plugin_overlays::paint` (below) paints into
    // exactly this strip, mirroring `markers::lane`'s own reserved-height
    // precedent rather than a second widget call.
    ui.add_space(plugin_overlays::LANE_HEIGHT);
    let (_overview_response, overview_event) = waveform::overview(
        ui,
        len_frames,
        sample_rate,
        playhead_frame,
        previewing,
        available,
        &overview_paint,
        &mut |painter, space| {
            markers::paint_overlay(
                painter,
                space,
                markers_snapshot.as_ref(),
                loop_state,
                focused_marker,
            );
            // O5: after markers/loop, before the playhead (paint::paint
            // already ran, `waveform::overview`'s own body calls
            // `paint::playhead` right after this closure returns).
            plugin_overlays::paint(
                painter,
                space,
                len_frames,
                ViewKind::Overview,
                &overlay_layers,
            );
        },
    );
    if let Some(event) = overview_event {
        apply_waveform_event(
            event,
            controller,
            waveform,
            &mut detail,
            len_frames,
            sample_rate,
            playhead_frame,
            DragOrigin::Overview,
        );
    }

    let remaining_frames = len_frames.saturating_sub(playhead_frame);
    ui.label(theme::mono_text(tr_args(
        "time-remaining",
        &[("time", format_mmss_frames(remaining_frames, sample_rate))],
    )));

    let detail_window = detail.start_frame..(detail.start_frame + detail.width_frames);
    markers::lane(
        ui,
        "detail",
        detail_window.clone(),
        sample_rate,
        len_frames,
        markers_snapshot.as_ref(),
        controller,
        waveform,
        &mut detail,
    );
    let detail_window = detail.start_frame..(detail.start_frame + detail.width_frames);
    let detail_paint = WaveformPaint {
        status,
        peaks,
        playhead: Some(playhead_frame),
        unavailable_text: &unavailable_text,
        highlight: None,
    };
    // US3 T089: the detail view's own lane (O7 applies to both views).
    ui.add_space(plugin_overlays::LANE_HEIGHT);
    let (_detail_response, detail_event) = waveform::detail(
        ui,
        detail_window,
        sample_rate,
        playhead_frame,
        previewing,
        available,
        &detail_paint,
        &mut |painter, space| {
            markers::paint_overlay(
                painter,
                space,
                markers_snapshot.as_ref(),
                loop_state,
                focused_marker,
            );
            plugin_overlays::paint(
                painter,
                space,
                len_frames,
                ViewKind::Detail,
                &overlay_layers,
            );
        },
    );
    if let Some(event) = detail_event {
        apply_waveform_event(
            event,
            controller,
            waveform,
            &mut detail,
            len_frames,
            sample_rate,
            playhead_frame,
            DragOrigin::Detail,
        );
    }

    waveform.detail = Some(detail);
}

/// Apply one [`WaveformEvent`] from either waveform widget
/// (contracts/ui-waveform.md §2-3, §5): `Preview`/`Commit`/`CancelDrag`
/// touch `waveform.drag` and the controller exactly as US1 did; the
/// zoom/pan/reset rows (new in US2) always target the single shared
/// `detail` window regardless of which widget produced them, since the
/// overview has no zoom of its own (contracts/ui-waveform.md §2's "on the
/// overview the anchor is the playhead").
#[allow(clippy::too_many_arguments)]
fn apply_waveform_event<B: OutputBackend, H: SourceHost>(
    event: WaveformEvent,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
    detail: &mut DetailWindow,
    len_frames: u64,
    sample_rate: u32,
    playhead_frame: u64,
    origin: DragOrigin,
) {
    match event {
        WaveformEvent::Preview(frame) => {
            waveform.drag = Some(DragPreview {
                target_frame: frame,
                origin,
            });
        }
        WaveformEvent::Commit(frame) => {
            waveform.drag = None;
            controller.seek_frames(frame);
            if !detail.contains(frame) {
                *detail = detail.recenter(frame, len_frames);
            }
        }
        WaveformEvent::CancelDrag => {
            waveform.drag = None;
        }
        WaveformEvent::Zoom {
            anchor_frame,
            factor,
        } => {
            *detail = detail
                .zoom_about(anchor_frame, factor, len_frames, sample_rate)
                .suspend_follow_if_outside(playhead_frame);
        }
        WaveformEvent::Pan { delta_frames } => {
            *detail = detail
                .pan(delta_frames, len_frames)
                .suspend_follow_if_outside(playhead_frame);
        }
        WaveformEvent::ZoomStep { zoom_in } => {
            *detail = detail
                .zoom_step(playhead_frame, zoom_in, len_frames, sample_rate)
                .suspend_follow_if_outside(playhead_frame);
        }
        WaveformEvent::PanStep { fraction } => {
            let delta_frames = (fraction * detail.width_frames as f64).round() as i64;
            *detail = detail
                .pan(delta_frames, len_frames)
                .suspend_follow_if_outside(playhead_frame);
        }
        WaveformEvent::Reset => {
            *detail = detail.reset(len_frames, sample_rate);
        }
    }
}

/// `duration.as_secs_f64() * sample_rate`, rounded — the inverse of
/// `waveform`'s own frame-to-duration conversions, used only for the
/// playhead's frame position (display precision; the real seek path is
/// always `seek_frames` with an already-integral frame).
fn duration_to_frames(duration: Duration, sample_rate: u32) -> u64 {
    (duration.as_secs_f64() * f64::from(sample_rate.max(1))).round() as u64
}

/// `"m:ss"` for a frame count at `sample_rate` (no leading-zero minutes;
/// contracts/ui-waveform.md's `time-elapsed`/`time-remaining`).
fn format_mmss_frames(frame: u64, sample_rate: u32) -> String {
    format_mmss(Duration::from_secs_f64(
        frame as f64 / f64::from(sample_rate.max(1)),
    ))
}

/// `"m:ss"` (no leading-zero minutes; contracts/ui-surface.md's former
/// seek-slider text, now `time-elapsed`/`time-remaining`).
fn format_mmss(duration: Duration) -> String {
    let total = duration.as_secs();
    format!("{}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_mmss_pads_seconds_to_two_digits() {
        assert_eq!(format_mmss(Duration::from_secs(5)), "0:05");
        assert_eq!(format_mmss(Duration::from_secs(65)), "1:05");
        assert_eq!(format_mmss(Duration::from_secs(3661)), "61:01");
    }

    #[test]
    fn format_mmss_frames_matches_duration_based_formatting() {
        assert_eq!(format_mmss_frames(5 * 44_100, 44_100), "0:05");
        assert_eq!(format_mmss_frames(0, 44_100), "0:00");
    }

    #[test]
    fn duration_to_frames_round_trips_whole_seconds() {
        assert_eq!(duration_to_frames(Duration::from_secs(1), 44_100), 44_100);
        assert_eq!(duration_to_frames(Duration::ZERO, 44_100), 0);
    }
}

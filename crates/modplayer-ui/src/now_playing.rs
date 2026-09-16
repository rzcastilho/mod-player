// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Now Playing screen (contracts/ui-surface.md §1): track title/artist,
//! a status line, the full transport (play/pause, stop, skip back/forward,
//! seek slider, position readout), the master-volume/peak-meter widgets
//! (unchanged from 001), and inline disabled reasons.

use std::time::Duration;

use egui::{Button, Id, Key, Modifiers, Slider, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{SourceHealth, SourceHost};
use modplayer_core::{ActiveState, Intent, PlaybackController, tr, tr_args};

use crate::queue_view;
use crate::widgets::{peak_meter, volume};

/// `Left`/`Right` seek step (contracts/ui-surface.md §1).
const SEEK_STEP: Duration = Duration::from_secs(5);

/// Persists the Queue panel's open/closed state across frames in egui's
/// own per-viewer memory (ui-surface.md §2: reached from Now Playing via a
/// toggle, no dedicated nav-rail section).
fn queue_panel_open_id() -> Id {
    Id::new("now-playing-queue-panel-open")
}

/// Draw the Now Playing screen, applying any transport/volume change
/// directly to `controller`.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    show_heading(ui, controller);
    show_status_line(ui, controller);
    show_transfer_banner(ui, controller);

    let available = controller.transport_enabled();

    let queue_id = queue_panel_open_id();
    let mut queue_open = ui
        .memory(|memory| memory.data.get_temp::<bool>(queue_id))
        .unwrap_or(false);
    if ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Q)) {
        queue_open = !queue_open;
    }

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
    });
    ui.memory_mut(|memory| memory.data.insert_temp(queue_id, queue_open));

    show_seek_slider(ui, controller, available);

    if let Some(new_volume) = volume::master_volume(ui, controller.master_volume()) {
        controller.set_master_volume(new_volume);
    }

    let peak = controller.shared().peak();
    let ceiling_db = controller.ceiling().db();
    peak_meter::peak_meter(ui, peak, ceiling_db);

    if queue_open {
        ui.separator();
        queue_view::show(ui, controller);
    }

    // Position advances at the audio-clock rate (>= 60 Hz, FR-006/SC-003)
    // independent of egui's own input-driven repaint cadence; keep the
    // screen repainting while playing so the readout/slider stay live even
    // with no mouse/keyboard activity.
    if controller.transport_state().intent == Intent::Playing {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

fn show_heading<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &PlaybackController<B, H>,
) {
    match controller.current_track() {
        Some(item) => {
            ui.heading(tr_args(
                "now-playing-title",
                &[("title", item.track.title.clone())],
            ));
            if !item.track.artists.is_empty() {
                ui.label(tr_args(
                    "now-playing-artist",
                    &[("artist", item.track.artists.join(", "))],
                ));
            }
        }
        None => {
            ui.heading(tr("now-playing-empty"));
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

fn show_seek_slider<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    available: bool,
) {
    let duration = controller
        .transport_state()
        .track_len_ms
        .map(|ms| Duration::from_millis(u64::from(ms)))
        .unwrap_or(Duration::ZERO);
    let duration_secs = duration.as_secs_f64().max(0.001);
    let mut position_secs = controller.position().as_secs_f64().min(duration_secs);

    ui.horizontal(|ui| {
        ui.label(tr_args(
            "transport-position",
            &[
                ("position", format_mmss(controller.position())),
                ("duration", format_mmss(duration)),
            ],
        ));
        let slider_text = format!(
            "{} / {}",
            format_mmss(Duration::from_secs_f64(position_secs)),
            format_mmss(duration)
        );
        let response = ui.add_enabled(
            available,
            Slider::new(&mut position_secs, 0.0..=duration_secs)
                .show_value(false)
                .text(slider_text),
        );

        let mut commit = response.drag_stopped();
        if response.has_focus() {
            ui.input(|input| {
                if input.key_pressed(Key::ArrowLeft) {
                    position_secs = (position_secs - SEEK_STEP.as_secs_f64()).max(0.0);
                    commit = true;
                } else if input.key_pressed(Key::ArrowRight) {
                    position_secs = (position_secs + SEEK_STEP.as_secs_f64()).min(duration_secs);
                    commit = true;
                } else if input.key_pressed(Key::Home) {
                    position_secs = 0.0;
                    commit = true;
                } else if input.key_pressed(Key::End) {
                    position_secs = duration_secs;
                    commit = true;
                }
            });
        }
        if commit {
            controller.seek(Duration::from_secs_f64(position_secs));
        }
    });
}

/// `"m:ss"` (no leading-zero minutes; contracts/ui-surface.md's
/// `transport-position`/seek-slider text).
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
}

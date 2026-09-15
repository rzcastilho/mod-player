// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Now Playing screen (contracts/ui-surface.md): track title,
//! Play/Pause toggle, Stop, the master-volume and peak-meter widgets, and
//! an inline disabled-reason when there is no output device
//! (`PlaybackController::transport_available() == false`).

use egui::{Button, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_core::{PlaybackController, tr};
use modplayer_engine::Transport;

use crate::widgets::{peak_meter, volume};

/// Draw the Now Playing screen, applying any transport/volume change
/// directly to `controller`.
pub fn show<B: OutputBackend>(ui: &mut Ui, controller: &mut PlaybackController<B>) {
    ui.heading(tr("synthetic-track-title"));

    let available = controller.transport_available();
    if !available {
        ui.label(tr("transport-disabled-no-device"));
    }

    ui.horizontal(|ui| {
        let playing = controller.transport() == Transport::Playing;
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
    });

    if let Some(new_volume) = volume::master_volume(ui, controller.master_volume()) {
        controller.set_master_volume(new_volume);
    }

    let peak = controller.shared().peak();
    let ceiling_db = controller.ceiling().db();
    peak_meter::peak_meter(ui, peak, ceiling_db);
}

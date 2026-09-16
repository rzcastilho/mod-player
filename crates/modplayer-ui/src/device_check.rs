// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Device Check screen (contracts/ui-surface.md, data-model.md §6.3):
//! shown on first launch (no confirmed device) and identically from
//! Settings › Audio › "Test output device" (T051). Device
//! enumeration/preview/persistence all live on `PlaybackController`; this
//! screen only holds the local radio/combo selection state and translates
//! clicks into controller calls.

use egui::Ui;
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::{PlaybackController, tr};
use modplayer_engine::{BufferPreset, DeviceId, NegotiatedBuffer, SampleRate};

/// The three device-buffer presets shown in the combo, in display order
/// (contracts/ui-surface.md).
const PRESETS: [BufferPreset; 3] = [
    BufferPreset::Performance,
    BufferPreset::Balanced,
    BufferPreset::Safe,
];

/// Local UI state for one Device Check screen instance: which device is
/// highlighted in the radio list and which preset is chosen in the combo.
pub struct DeviceCheckScreen {
    selected: Option<DeviceId>,
    preset: BufferPreset,
}

impl DeviceCheckScreen {
    /// Start a screen, preselecting `initial_device` (typically the
    /// controller's currently active/previewed device) and `initial_preset`.
    pub fn new(initial_device: Option<DeviceId>, initial_preset: BufferPreset) -> Self {
        Self {
            selected: initial_device,
            preset: initial_preset,
        }
    }

    /// Draw the screen and apply the user's actions directly to
    /// `controller`. Returns `true` once the screen should close ("Yes" or
    /// "Skip for now" was chosen this frame).
    pub fn show<B: OutputBackend, H: SourceHost>(
        &mut self,
        ui: &mut Ui,
        controller: &mut PlaybackController<B, H>,
    ) -> bool {
        let devices = controller.backend().devices().unwrap_or_default();

        // Empty state (zero devices): only Skip (contracts/ui-surface.md).
        if devices.is_empty() {
            ui.label(tr("device-check-no-devices"));
            if ui.button(tr("device-check-skip")).clicked() {
                controller.skip_device_check();
                return true;
            }
            return false;
        }

        if self
            .selected
            .as_ref()
            .is_none_or(|selected| !devices.iter().any(|d| &d.id == selected))
        {
            self.selected = devices
                .iter()
                .find(|d| d.is_default)
                .or_else(|| devices.first())
                .map(|d| d.id.clone());
        }

        let device_rate = controller
            .active_device()
            .map(|d| d.negotiated.device_rate)
            .unwrap_or_default();

        ui.label(tr("device-check-device-list"));
        let mut newly_selected = None;
        for device in &devices {
            let is_selected = self.selected.as_ref() == Some(&device.id);
            if ui.radio(is_selected, device.name.clone()).clicked() && !is_selected {
                newly_selected = Some(device.id.clone());
            }
        }

        // Selecting a preset here only updates the label shown
        // (`self.preset`); it takes effect once "Yes" confirms it and
        // reopens the stream (contracts/ui-surface.md) — no stream rebuild
        // on every combo change in this phase.
        egui::ComboBox::from_id_salt("device-check-buffer-preset")
            .selected_text(preset_label(self.preset, device_rate))
            .show_ui(ui, |ui| {
                for preset in PRESETS {
                    ui.selectable_value(
                        &mut self.preset,
                        preset,
                        preset_label(preset, device_rate),
                    );
                }
            });

        if let Some(device) = newly_selected {
            self.selected = Some(device.clone());
            controller.preview_device(device);
        }

        if ui.button(tr("device-check-play-tone")).clicked() {
            controller.play_test_tone();
        }

        ui.label(tr("device-check-question"));
        let mut close = false;
        ui.horizontal(|ui| {
            if ui.button(tr("device-check-yes")).clicked() {
                if let Some(device) = self.selected.clone() {
                    controller.confirm_device(device, self.preset);
                }
                close = true;
            }
            if ui.button(tr("device-check-no")).clicked()
                && let Some(device) = self.selected.clone()
            {
                controller.preview_device(device);
            }
            if ui.button(tr("device-check-skip")).clicked() {
                controller.skip_device_check();
                close = true;
            }
        });

        close
    }
}

/// `"{preset} (~{ms} ms)"`, `ms` rounded to one decimal; raw frame counts
/// never appear (contracts/ui-surface.md).
fn preset_label(preset: BufferPreset, device_rate: SampleRate) -> String {
    let key = match preset {
        BufferPreset::Performance => "buffer-preset-performance",
        BufferPreset::Balanced => "buffer-preset-balanced",
        BufferPreset::Safe => "buffer-preset-safe",
    };
    let negotiated = NegotiatedBuffer {
        preset,
        frames: preset.requested_frames(),
        device_rate,
    };
    format!("{} (~{:.1} ms)", tr(key), negotiated.latency_ms())
}

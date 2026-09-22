// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Audio settings screen (contracts/ui-surface.md's Audio-category
//! `SettingDescriptor`s, T091): output-device combo, buffer-preset combo
//! with its latency suffix, the limiter-ceiling slider, the safe-volume
//! checkbox + cap slider, and the "Test output device" button that reopens
//! Device Check identically to first launch (T051).
//!
//! `output_device`/`buffer_preset`/`limiter_ceiling` already live in
//! `PlaybackController`'s shadow state and change through its existing
//! public methods (`confirm_device`, `set_ceiling`); `safe_volume` does not
//! (it only affects the startup clamp, FR-011, and has no controller
//! setter), so it is read/written straight through
//! `PlaybackController::settings_store()` using the same reload-mutate-save
//! pattern `controller.rs`'s own `persist_settings` uses internally.
//!
//! `focus` names the descriptor id, if any, a settings-search result asked
//! to land on this frame (`settings/mod.rs`'s `SettingsScreen`, T090); the
//! matching widget claims keyboard focus that frame.

use egui::{ComboBox, Slider, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::{AudioSettings, PlaybackController, Severity, tr};
use modplayer_engine::{
    BufferPreset, CeilingDb, DeviceId, NegotiatedBuffer, SafeVolume, SampleRate, VolumePercent,
};

use crate::device_check::DeviceCheckScreen;
use crate::theme;

/// The three device-buffer presets shown in the combo, in display order
/// (mirrors `device_check.rs`'s own `PRESETS`).
const PRESETS: [BufferPreset; 3] = [
    BufferPreset::Performance,
    BufferPreset::Balanced,
    BufferPreset::Safe,
];

/// Draw the "Test output device" button. Returns a fresh `DeviceCheckScreen`
/// — preselecting the controller's currently active device and preset,
/// exactly as first launch does — the frame it is clicked.
pub fn test_output_device_button<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &PlaybackController<B, H>,
) -> Option<DeviceCheckScreen> {
    if ui.button(tr("setting-test-output-device")).clicked() {
        Some(DeviceCheckScreen::new(
            controller.active_device().map(|d| d.id.clone()),
            controller.preset(),
        ))
    } else {
        None
    }
}

/// Draw the full Audio settings screen, applying every change directly to
/// `controller` (or, for safe-volume, straight to its settings store via
/// `cached`). Returns a fresh `DeviceCheckScreen` the frame "Test output
/// device" is clicked.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    cached: &mut AudioSettings,
    focus: Option<&str>,
) -> Option<DeviceCheckScreen> {
    let devices = controller.backend().devices().unwrap_or_default();
    let device_rate = controller
        .active_device()
        .map(|d| d.negotiated.device_rate)
        .unwrap_or_default();

    ui.label(tr("setting-output-device"));
    // FR-006, U2: field-description prose, capped at the 72-character
    // measure (research R17).
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("setting-output-device-desc"));
    });
    let current_device = controller.preferred_device().cloned();
    let current_label = current_device
        .as_ref()
        .and_then(|id| devices.iter().find(|d| &d.id == id))
        .map(|d| d.name.clone())
        .unwrap_or_default();
    let mut newly_selected: Option<DeviceId> = None;
    let output_device_response = ComboBox::from_id_salt("audio.output_device")
        .selected_text(current_label)
        .show_ui(ui, |ui| {
            for device in &devices {
                let is_selected = current_device.as_ref() == Some(&device.id);
                if ui
                    .selectable_label(is_selected, device.name.clone())
                    .clicked()
                    && !is_selected
                {
                    newly_selected = Some(device.id.clone());
                }
            }
        })
        .response;
    if focus == Some("audio.output_device") {
        output_device_response.request_focus();
    }
    if let Some(device) = newly_selected {
        let preset = controller.preset();
        controller.confirm_device(device, preset);
    }

    ui.label(tr("setting-buffer-preset"));
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("setting-buffer-preset-desc"));
    });
    let mut preset = controller.preset();
    let previous_preset = preset;
    // 014-design-tokens-and-type-scale (US3, T044): the buffer preset's
    // latency figure is a numeric readout — `mono` so its digits line up
    // with every other numeric readout's column.
    let buffer_preset_response = ComboBox::from_id_salt("audio.buffer_preset")
        .selected_text(theme::mono_text(preset_label(preset, device_rate)))
        .show_ui(ui, |ui| {
            for candidate in PRESETS {
                ui.selectable_value(
                    &mut preset,
                    candidate,
                    theme::mono_text(preset_label(candidate, device_rate)),
                );
            }
        })
        .response;
    if focus == Some("audio.buffer_preset") {
        buffer_preset_response.request_focus();
    }
    if preset != previous_preset
        && let Some(device) = controller.preferred_device().cloned()
    {
        controller.confirm_device(device, preset);
    }

    ui.label(tr("setting-limiter-ceiling"));
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("setting-limiter-ceiling-desc"));
    });
    let mut ceiling_db = f64::from(controller.ceiling().db());
    let ceiling_response = ui.add(
        Slider::new(
            &mut ceiling_db,
            f64::from(CeilingDb::MIN)..=f64::from(CeilingDb::MAX),
        )
        .step_by(0.1),
    );
    if focus == Some("audio.limiter_ceiling") {
        ceiling_response.request_focus();
    }
    if ceiling_response.changed() {
        controller.set_ceiling(CeilingDb::new(ceiling_db as f32));
    }

    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("setting-safe-volume-desc"));
    });
    let mut safe_volume_enabled = cached.safe_volume.enabled;
    let enabled_response = ui.checkbox(&mut safe_volume_enabled, tr("setting-safe-volume"));
    if focus == Some("audio.safe_volume_enabled") {
        enabled_response.request_focus();
    }
    if enabled_response.changed() {
        persist_safe_volume(
            controller,
            cached,
            SafeVolume {
                enabled: safe_volume_enabled,
                cap: cached.safe_volume.cap,
            },
        );
    }

    ui.label(tr("setting-safe-volume-cap"));
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("setting-safe-volume-cap-desc"));
    });
    let mut cap = f64::from(cached.safe_volume.cap.value());
    let cap_response = ui.add(Slider::new(&mut cap, 0.0..=100.0).step_by(1.0));
    if focus == Some("audio.safe_volume_cap") {
        cap_response.request_focus();
    }
    if cap_response.changed() {
        persist_safe_volume(
            controller,
            cached,
            SafeVolume {
                enabled: cached.safe_volume.enabled,
                cap: VolumePercent::new(cap.round().clamp(0.0, 100.0) as u8),
            },
        );
    }

    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("setting-test-output-device-desc"));
    });
    let test_button_response = ui.button(tr("setting-test-output-device"));
    if focus == Some("audio.test_output_device") {
        test_button_response.request_focus();
    }
    if test_button_response.clicked() {
        return Some(DeviceCheckScreen::new(
            controller.active_device().map(|d| d.id.clone()),
            controller.preset(),
        ));
    }

    None
}

/// Reload the current settings, overwrite only `safe_volume`, and save —
/// mirroring `controller.rs`'s own `persist_settings` (reload-mutate-save,
/// so a concurrent change to any other field is not clobbered), reporting a
/// `settings-save-failed` warning on failure, and refreshing `cached` from
/// what was actually saved.
fn persist_safe_volume<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
    cached: &mut AudioSettings,
    safe_volume: SafeVolume,
) {
    let mut settings = controller.settings_store().load().settings;
    settings.safe_volume = safe_volume;
    if controller.settings_store().save(&settings).is_err() {
        controller
            .notifications_mut()
            .raise(Severity::Warning, "settings-save-failed");
    }
    *cached = settings;
}

/// `"{preset} (~{ms} ms)"`, `ms` rounded to one decimal (mirrors
/// `device_check.rs`'s own `preset_label`; raw frame counts never appear
/// outside Settings › Developer, contracts/ui-surface.md).
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

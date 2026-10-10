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

use std::ops::RangeInclusive;

use egui::{ComboBox, Slider, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::{AudioSettings, PlaybackController, Severity, tr};
use modplayer_engine::{
    BufferPreset, CeilingDb, DeviceId, NegotiatedBuffer, SafeVolume, SampleRate, VolumePercent,
};

use crate::device_check::DeviceCheckScreen;
use crate::settings::field::{self, FieldOutput, FieldSpec, ResetState, Unit};
use crate::theme::{self, tokens::space};
use crate::widgets::controls::{SwitchKind, panel_card, switch};

/// The three device-buffer presets shown in the combo, in display order
/// (mirrors `device_check.rs`'s own `PRESETS`).
const PRESETS: [BufferPreset; 3] = [
    BufferPreset::Performance,
    BufferPreset::Balanced,
    BufferPreset::Safe,
];

/// The safe-volume switch: a checkbox-kind host switch that paints its own
/// label (so the field row does not draw it twice).
fn safe_volume_switch(ui: &mut Ui, on: &mut bool) -> egui::Response {
    switch(ui, SwitchKind::Checkbox, on, &tr("setting-safe-volume"))
}

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

/// The safe-volume cap's bounds: what the slider clamps with and what the
/// range caption shows (research R3). `VolumePercent` itself caps at 100.
pub(crate) const SAFE_VOLUME_CAP_RANGE: RangeInclusive<u8> = 0..=100;

/// Give `output`'s control its visible label as accessible name.
fn labelled(output: &FieldOutput) {
    if let Some(id) = output.label_id {
        output.control.clone().labelled_by(id);
    }
}

/// Draw the full Audio settings screen as two cards — "Output" and "Level
/// protection" (028, contracts F1) — applying every change directly to
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
    let mut test_screen = None;
    // Reset compares against the single source of defaults (research R4).
    let defaults = AudioSettings::default();

    panel_card(ui, &tr("settings-group-output"), |ui| {
        let current_device = controller.preferred_device().cloned();
        let current_label = current_device
            .as_ref()
            .and_then(|id| devices.iter().find(|d| &d.id == id))
            .map(|d| d.name.clone())
            .unwrap_or_default();
        let mut newly_selected: Option<DeviceId> = None;
        let mut spec = FieldSpec::new("audio.output_device", tr("setting-output-device"));
        spec.help = Some(tr("setting-output-device-desc"));
        let output = field::row(ui, &spec, |ui| {
            ComboBox::from_id_salt("audio.output_device")
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
                .response
        });
        labelled(&output);
        if focus == Some("audio.output_device") {
            output.control.request_focus();
        }
        if let Some(device) = newly_selected {
            let preset = controller.preset();
            controller.confirm_device(device, preset);
        }
        ui.add_space(space::MD);

        let mut preset = controller.preset();
        let previous_preset = preset;
        let mut spec = FieldSpec::new("audio.buffer_preset", tr("setting-buffer-preset"));
        spec.help = Some(tr("setting-buffer-preset-desc"));
        if controller.preferred_device().is_some() {
            spec.reset = ResetState::compute(&previous_preset, &defaults.buffer_preset);
        }
        // 014-design-tokens-and-type-scale (US3, T044): the buffer preset's
        // latency figure is a numeric readout — `mono` so its digits line up
        // with every other numeric readout's column.
        let output = field::row(ui, &spec, |ui| {
            ComboBox::from_id_salt("audio.buffer_preset")
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
                .response
        });
        labelled(&output);
        if focus == Some("audio.buffer_preset") {
            output.control.request_focus();
        }
        if output.reset_clicked {
            preset = defaults.buffer_preset;
        }
        if preset != previous_preset
            && let Some(device) = controller.preferred_device().cloned()
        {
            controller.confirm_device(device, preset);
        }
        ui.add_space(space::MD);

        let mut spec = FieldSpec::new("audio.test_output_device", tr("setting-test-output-device"));
        spec.help = Some(tr("setting-test-output-device-desc"));
        spec.label_in_control = true;
        let output = field::row(ui, &spec, |ui| ui.button(tr("setting-test-output-device")));
        if focus == Some("audio.test_output_device") {
            output.control.request_focus();
        }
        if output.control.clicked() {
            test_screen = Some(DeviceCheckScreen::new(
                controller.active_device().map(|d| d.id.clone()),
                controller.preset(),
            ));
        }
    });
    ui.add_space(space::LG);

    panel_card(ui, &tr("settings-group-level-protection"), |ui| {
        let mut ceiling_db = f64::from(controller.ceiling().db());
        let mut spec = FieldSpec::new("audio.limiter_ceiling", tr("setting-limiter-ceiling"));
        spec.help = Some(tr("setting-limiter-ceiling-desc"));
        spec.reset = ResetState::compute(&controller.ceiling(), &defaults.limiter_ceiling_db);
        spec.range = Some(field::format_range(
            Unit::Dbfs,
            f64::from(CeilingDb::MIN),
            f64::from(CeilingDb::MAX),
        ));
        let output = field::row(ui, &spec, |ui| {
            ui.add(
                Slider::new(
                    &mut ceiling_db,
                    f64::from(CeilingDb::MIN)..=f64::from(CeilingDb::MAX),
                )
                .step_by(0.1)
                .custom_formatter(|v, _| field::format_value(Unit::Dbfs, v))
                .custom_parser(|text| field::parse_value(Unit::Dbfs, text)),
            )
        });
        labelled(&output);
        if focus == Some("audio.limiter_ceiling") {
            output.control.request_focus();
        }
        if output.reset_clicked {
            controller.set_ceiling(defaults.limiter_ceiling_db);
        } else if output.control.changed() {
            controller.set_ceiling(CeilingDb::new(ceiling_db as f32));
        }
        ui.add_space(space::MD);

        let mut safe_volume_enabled = cached.safe_volume.enabled;
        let mut spec = FieldSpec::new("audio.safe_volume_enabled", tr("setting-safe-volume"));
        spec.help = Some(tr("setting-safe-volume-desc"));
        spec.reset =
            ResetState::compute(&cached.safe_volume.enabled, &defaults.safe_volume.enabled);
        spec.label_in_control = true;
        let output = field::row(ui, &spec, |ui| {
            safe_volume_switch(ui, &mut safe_volume_enabled)
        });
        if focus == Some("audio.safe_volume_enabled") {
            output.control.request_focus();
        }
        if output.reset_clicked {
            persist_safe_volume(
                controller,
                cached,
                SafeVolume {
                    enabled: defaults.safe_volume.enabled,
                    cap: cached.safe_volume.cap,
                },
            );
        } else if output.control.changed() {
            persist_safe_volume(
                controller,
                cached,
                SafeVolume {
                    enabled: safe_volume_enabled,
                    cap: cached.safe_volume.cap,
                },
            );
        }
        ui.add_space(space::MD);

        let mut cap = f64::from(cached.safe_volume.cap.value());
        let (cap_min, cap_max) = (
            f64::from(*SAFE_VOLUME_CAP_RANGE.start()),
            f64::from(*SAFE_VOLUME_CAP_RANGE.end()),
        );
        let mut spec = FieldSpec::new("audio.safe_volume_cap", tr("setting-safe-volume-cap"));
        spec.help = Some(tr("setting-safe-volume-cap-desc"));
        spec.range = Some(field::format_range(Unit::Percent, cap_min, cap_max));
        spec.reset = ResetState::compute(&cached.safe_volume.cap, &defaults.safe_volume.cap);
        let output = field::row(ui, &spec, |ui| {
            ui.add(
                Slider::new(&mut cap, cap_min..=cap_max)
                    .step_by(1.0)
                    .custom_formatter(|v, _| field::format_value(Unit::Percent, v))
                    .custom_parser(|text| field::parse_value(Unit::Percent, text)),
            )
        });
        labelled(&output);
        if focus == Some("audio.safe_volume_cap") {
            output.control.request_focus();
        }
        if output.reset_clicked {
            persist_safe_volume(
                controller,
                cached,
                SafeVolume {
                    enabled: cached.safe_volume.enabled,
                    cap: defaults.safe_volume.cap,
                },
            );
        } else if output.control.changed() {
            persist_safe_volume(
                controller,
                cached,
                SafeVolume {
                    enabled: cached.safe_volume.enabled,
                    cap: VolumePercent::new(cap.round().clamp(cap_min, cap_max) as u8),
                },
            );
        }
    });

    test_screen
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

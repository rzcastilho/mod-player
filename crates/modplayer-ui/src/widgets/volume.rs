// SPDX-License-Identifier: MIT OR Apache-2.0

//! The master-volume widget (contracts/ui-surface.md `master-volume`): a
//! 0-100 % slider with a dB readout. Arrow keys step ±1 (egui's `Slider`
//! native keyboard handling with `step_by(1.0)`); PgUp/PgDn step ±10
//! (handled here — egui's `Slider` has no built-in page-key binding).
//! Accessible value is `"{pct} %, {db} dB"`.
//!
//! This widget touches no controller/engine state itself: it returns the
//! new value (if any) for the caller to apply via
//! `PlaybackController::set_master_volume`.

use egui::{Key, Slider, Ui};
use modplayer_core::tr;
use modplayer_engine::VolumePercent;

/// `PgUp`/`PgDn` step size (contracts/ui-surface.md).
const PAGE_STEP: u8 = 10;

/// Draw the `master-volume` caption and slider for `current`. Returns
/// `Some(new_value)` when the user changed it this frame (drag, arrow
/// keys, or PgUp/PgDn), `None` otherwise.
pub fn master_volume(ui: &mut Ui, current: VolumePercent) -> Option<VolumePercent> {
    let mut pct = f64::from(current.value());
    let readout = format_readout(current);

    let mut new_value = None;
    ui.horizontal(|ui| {
        ui.label(tr("master-volume"));
        let response = ui.add(
            Slider::new(&mut pct, 0.0..=100.0)
                .step_by(1.0)
                .text(readout),
        );

        new_value = response
            .changed()
            .then(|| VolumePercent::new(pct.round().clamp(0.0, 100.0) as u8));

        if response.has_focus() {
            let (page_up, page_down) =
                ui.input(|i| (i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown)));
            if page_up {
                new_value = Some(VolumePercent::new(
                    current.value().saturating_add(PAGE_STEP),
                ));
            } else if page_down {
                new_value = Some(VolumePercent::new(
                    current.value().saturating_sub(PAGE_STEP),
                ));
            }
        }
    });

    new_value
}

/// `"{pct} %, {db} dB"` (contracts/ui-surface.md's accessible value, also
/// used as the slider's own visible/accessible text; Rust's float
/// formatting already renders `f32::NEG_INFINITY` as `-inf`, so 0 % needs
/// no special case).
fn format_readout(volume: VolumePercent) -> String {
    format!("{} %, {:.1} dB", volume.value(), volume.to_db())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readout_formats_percent_and_db() {
        assert_eq!(format_readout(VolumePercent::new(100)), "100 %, 0.0 dB");
        assert_eq!(format_readout(VolumePercent::new(0)), "0 %, -inf dB");
    }
}

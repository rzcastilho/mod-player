// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Settings › Developer screen (contracts/ui-surface.md's
//! `developer.buffer_frames` / `developer.raise_notification`
//! descriptors): a read-only buffer-frame readout and three buttons that
//! raise a sample notification of each severity, so the notification area
//! (T080) can be exercised without waiting for a real device event.
//! Registered as a searchable `SettingDescriptor` and wired into the
//! category-list Settings shell in US5 (T087-T090); this phase only builds
//! the screen itself.

use egui::Ui;
use modplayer_audio_io::OutputBackend;
use modplayer_core::{PlaybackController, Severity, tr};

/// Draw the raw buffer-frame readout and the "raise sample notification"
/// buttons, applying any click directly to `controller`'s notification
/// center.
pub fn show<B: OutputBackend>(ui: &mut Ui, controller: &mut PlaybackController<B>) {
    ui.label(tr("setting-buffer-frames"));
    ui.label(tr("setting-buffer-frames-desc"));
    ui.label(buffer_frames_readout(controller));

    ui.label(tr("setting-raise-notification"));
    ui.label(tr("setting-raise-notification-desc"));
    ui.horizontal(|ui| {
        if ui.button(tr("severity-critical")).clicked() {
            controller
                .notifications_mut()
                .raise(Severity::Critical, "sample-notification-critical");
        }
        if ui.button(tr("severity-warning")).clicked() {
            controller
                .notifications_mut()
                .raise(Severity::Warning, "sample-notification-warning");
        }
        if ui.button(tr("severity-info")).clicked() {
            controller
                .notifications_mut()
                .raise(Severity::Info, "sample-notification-info");
        }
    });
}

/// `"requested N / negotiated M frames"` (contracts/ui-surface.md) — the
/// one place in the UI raw frame counts are shown (developer-only, unlike
/// Device Check's rounded-ms preset labels).
fn buffer_frames_readout<B: OutputBackend>(controller: &PlaybackController<B>) -> String {
    let requested = controller.preset().requested_frames().frames();
    let negotiated = controller.shared().negotiated_frames();
    format!("requested {requested} / negotiated {negotiated} frames")
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_audio_io::FakeBackend;
    use modplayer_core::SettingsStore;

    fn fresh_store() -> SettingsStore {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-developer-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        SettingsStore::with_path(dir.join("settings.toml"))
    }

    #[test]
    fn readout_shows_requested_and_zero_negotiated_before_any_stream() {
        let controller = PlaybackController::new(FakeBackend::new(vec![]), fresh_store());
        assert_eq!(
            buffer_frames_readout(&controller),
            "requested 256 / negotiated 0 frames"
        );
    }
}

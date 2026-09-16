// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Settings › Playback screen (contracts/ui-surface.md §3, FR-001,
//! T062): a single device-name field, committed on Enter/blur through
//! `PlaybackController::set_device_name`, with an inline "too long" error
//! and an empty field restoring the default name.

use egui::{TextEdit, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::{PlaybackController, tr};

/// Owned across frames (mirrors `AboutScreen`/`DeveloperScreen`'s own
/// sub-state) so a draft edit survives repaint and the last-known effective
/// name — used as the field's placeholder, and to avoid recomputing it
/// (`device_name()` shells out to `hostname` when no custom name is set)
/// on every frame — is cached rather than read fresh each frame.
pub struct PlaybackScreen {
    draft: String,
    effective_name: String,
    too_long: bool,
}

impl PlaybackScreen {
    /// Seed the draft and placeholder from `controller`'s current effective
    /// device name (custom or default) once, when the Settings screen is
    /// constructed.
    pub fn new<B: OutputBackend, H: SourceHost>(controller: &PlaybackController<B, H>) -> Self {
        let effective_name = controller.device_name();
        Self {
            draft: effective_name.clone(),
            effective_name,
            too_long: false,
        }
    }
}

/// Draw the device-name field, committing on Enter or losing focus
/// (contracts/ui-surface.md §3: "commits on Enter/blur via
/// `set_device_name`; inline error `setting-device-name-too-long`; empty
/// restores the default and the field shows the default as placeholder").
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut PlaybackScreen,
    focus: Option<&str>,
) {
    let name_label = ui.label(tr("setting-device-name"));
    ui.label(tr("setting-device-name-hint"));

    let response = ui
        .add(TextEdit::singleline(&mut screen.draft).hint_text(screen.effective_name.clone()))
        .labelled_by(name_label.id);
    if focus == Some("playback.device_name") {
        response.request_focus();
    }
    if response.lost_focus() {
        match controller.set_device_name(&screen.draft) {
            Ok(()) => {
                screen.too_long = false;
                screen.effective_name = controller.device_name();
                // An empty/whitespace commit restores the default; show it
                // in the field itself (not just the placeholder) so the
                // effective name is never hidden behind blank text.
                screen.draft = screen.effective_name.clone();
            }
            Err(_) => {
                screen.too_long = true;
            }
        }
    }
    if screen.too_long {
        ui.label(tr("setting-device-name-too-long"));
    }
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
            "modplayer-ui-playback-settings-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        SettingsStore::with_path(dir.join("settings.toml"))
    }

    #[test]
    fn new_seeds_the_draft_from_the_current_effective_name() {
        let controller = PlaybackController::new(
            FakeBackend::new(vec![]),
            modplayer_audio_source_synthetic::SyntheticHost::new(44_100),
            fresh_store(),
        );
        let screen = PlaybackScreen::new(&controller);
        assert_eq!(screen.draft, controller.device_name());
        assert_eq!(screen.effective_name, controller.device_name());
        assert!(!screen.too_long);
    }
}

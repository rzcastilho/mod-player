// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Appearance settings screen (contracts/ui-surface.md's
//! `appearance.theme` descriptor, T092): a Theme combo (System/Light/Dark).
//!
//! Theme is not part of `PlaybackController`'s shadow state — it is applied
//! once at app startup and has no controller setter (`controller.rs`'s own
//! doc comment: "the Appearance settings screen that changes it lands in
//! US5") — so, like `audio.rs`'s safe-volume controls, a change is
//! persisted straight through `PlaybackController::settings_store()`'s
//! reload-mutate-save pattern, and applied live via `crate::theme::apply`
//! (contracts/ui-surface.md "Theme": "applied ... immediately on change").

use egui::{ComboBox, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_core::{AudioSettings, PlaybackController, Severity, tr};
use modplayer_engine::Theme;

/// The three theme options, in display order.
const THEMES: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

/// Draw the Appearance settings screen, persisting and live-applying any
/// theme change. `cached` is the Settings screen's cached settings snapshot
/// (`settings/mod.rs`'s `SettingsScreen`, T090), kept in sync with whatever
/// was last saved. `focus` is `Some("appearance.theme")` the frame a
/// settings-search result asks to land here.
pub fn show<B: OutputBackend>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B>,
    cached: &mut AudioSettings,
    focus: Option<&str>,
) {
    ui.label(tr("setting-theme"));
    ui.label(tr("setting-theme-desc"));

    let mut theme = cached.theme;
    let previous = theme;
    let response = ComboBox::from_id_salt("appearance.theme")
        .selected_text(theme_label(theme))
        .show_ui(ui, |ui| {
            for candidate in THEMES {
                ui.selectable_value(&mut theme, candidate, theme_label(candidate));
            }
        })
        .response;
    if focus == Some("appearance.theme") {
        response.request_focus();
    }

    if theme != previous {
        let mut settings = controller.settings_store().load().settings;
        settings.theme = theme;
        if controller.settings_store().save(&settings).is_err() {
            controller
                .notifications_mut()
                .raise(Severity::Warning, "settings-save-failed");
        }
        *cached = settings;
        crate::theme::apply(ui.ctx(), theme);
    }
}

fn theme_label(theme: Theme) -> String {
    match theme {
        Theme::System => tr("setting-theme-system"),
        Theme::Light => tr("setting-theme-light"),
        Theme::Dark => tr("setting-theme-dark"),
    }
}

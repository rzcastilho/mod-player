// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Settings screens (contracts/ui-surface.md "Settings", T090, T094): a
//! fixed-order category list, a per-keystroke search box (`Ctrl/Cmd+F`)
//! over `modplayer_core::settings_registry`, results rendered as
//! `"{category} › {title}"`, and the eleven category screens — working
//! screens for Audio (T091), Appearance (T092), Language (T093) and
//! Developer (T083); placeholder content for the rest.

pub mod appearance;
pub mod audio;
pub mod developer;
pub mod language;

use egui::{Id, Key, TextEdit, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_core::settings_registry::{self, SettingsCategory};
use modplayer_core::{AudioSettings, PlaybackController, tr};

use crate::device_check::DeviceCheckScreen;

const SEARCH_BOX_ID: &str = "settings-search-box";

/// UI-only state for the whole Settings screen: the selected category, the
/// live search query, which descriptor id (if any) a search result asked
/// to focus this frame, and a cache of the settings fields no controller
/// shadow state exists for yet (safe-volume, theme — `audio.rs`/
/// `appearance.rs` read/write them straight through
/// `PlaybackController::settings_store()` rather than through a controller
/// setter).
pub struct SettingsScreen {
    category: SettingsCategory,
    search_query: String,
    focus_target: Option<&'static str>,
    cached_settings: AudioSettings,
}

impl SettingsScreen {
    /// Open the Settings screen on its first (fixed-order) category,
    /// loading the settings-store snapshot `audio.rs`/`appearance.rs` cache
    /// locally.
    pub fn new<B: OutputBackend>(controller: &PlaybackController<B>) -> Self {
        Self {
            category: SettingsCategory::ALL[0],
            search_query: String::new(),
            focus_target: None,
            cached_settings: controller.settings_store().load().settings,
        }
    }
}

/// Draw the Settings screen (category list, search box and results, then
/// the selected category's content), applying every change directly to
/// `controller`. Returns a fresh `DeviceCheckScreen` the frame the Audio
/// category's "Test output device" button is clicked.
pub fn show<B: OutputBackend>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B>,
    screen: &mut SettingsScreen,
) -> Option<DeviceCheckScreen> {
    let search_id = Id::new(SEARCH_BOX_ID);
    let focus_search_box = ui.input(|input| input.modifiers.command && input.key_pressed(Key::F));
    if focus_search_box {
        ui.memory_mut(|memory| memory.request_focus(search_id));
    }

    ui.horizontal(|ui| {
        ui.label(tr("settings-search"));
        ui.add(TextEdit::singleline(&mut screen.search_query).id(search_id));
    });

    if !screen.search_query.is_empty() {
        for descriptor in settings_registry::search(&screen.search_query) {
            let label = format!(
                "{} › {}",
                tr(descriptor.category.label_key()),
                tr(descriptor.title_key)
            );
            let response = ui.selectable_label(false, label);
            let activated_by_enter =
                response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
            if response.clicked() || activated_by_enter {
                screen.category = descriptor.category;
                screen.focus_target = Some(descriptor.id);
            }
        }
        ui.separator();
    }

    ui.horizontal_wrapped(|ui| {
        for category in SettingsCategory::ALL {
            if ui
                .selectable_label(screen.category == category, tr(category.label_key()))
                .clicked()
            {
                screen.category = category;
            }
        }
    });
    ui.separator();

    let focus = screen.focus_target.take();
    match screen.category {
        SettingsCategory::Audio => audio::show(ui, controller, &mut screen.cached_settings, focus),
        SettingsCategory::Appearance => {
            appearance::show(ui, controller, &mut screen.cached_settings, focus);
            None
        }
        SettingsCategory::Language => {
            language::show(ui, focus);
            None
        }
        SettingsCategory::Developer => {
            developer::show(ui, controller);
            None
        }
        SettingsCategory::Account
        | SettingsCategory::Playback
        | SettingsCategory::Controls
        | SettingsCategory::Plugins
        | SettingsCategory::Offline
        | SettingsCategory::PrivacyDiagnostics
        | SettingsCategory::About => {
            ui.label(tr("placeholder-settings-category"));
            None
        }
    }
}

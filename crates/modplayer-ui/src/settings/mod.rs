// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Settings screens (contracts/ui-surface.md "Settings", T090, T094): a
//! fixed-order category list, a per-keystroke search box (`Ctrl/Cmd+F`)
//! over `modplayer_core::settings_registry`, results rendered as
//! `"{category} › {title}"`, and the eleven category screens — working
//! screens for Audio (T091), Appearance (T092), Language (T093) and
//! Developer (T083); placeholder content for the rest.

pub mod about;
pub mod account;
pub mod appearance;
pub mod audio;
pub mod controls;
pub mod developer;
pub mod language;
pub mod plugins;

pub mod playback;

use egui::{Id, Key, TextEdit, Ui};
use modplayer_account::{AccountEvent, AccountService};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::settings_registry::{self, SettingsCategory};
use modplayer_core::{AudioSettings, PlaybackController, PluginId, tr};

use crate::actions::{self, Claim};
use crate::device_check::DeviceCheckScreen;
use crate::settings::about::AboutScreen;
use crate::settings::controls::ControlsScreen;

const SEARCH_BOX_ID: &str = "settings-search-box";

/// UI-only state for the whole Settings screen: the selected category, the
/// live search query, which descriptor id (if any) a search result asked
/// to focus this frame, which plugin-settings field (if any) a plugin
/// search hit asked to open and focus this frame (US4 T103, contracts/
/// overlays-settings-notify.md S6 — kept separate from `focus_target`
/// since a plugin field's id is a plugin-declared `String`, not one of
/// `settings_registry::DESCRIPTORS`' own `&'static str`s), and a cache of
/// the settings fields no controller shadow state exists for yet
/// (safe-volume, theme — `audio.rs`/`appearance.rs` read/write them
/// straight through `PlaybackController::settings_store()` rather than
/// through a controller setter).
pub struct SettingsScreen {
    category: SettingsCategory,
    search_query: String,
    focus_target: Option<&'static str>,
    plugin_focus: Option<(PluginId, String)>,
    cached_settings: AudioSettings,
    about: AboutScreen,
    playback: playback::PlaybackScreen,
    controls: ControlsScreen,
    plugins: plugins::PluginsScreen,
}

impl SettingsScreen {
    /// Open the Settings screen on its first (fixed-order) category,
    /// loading the settings-store snapshot `audio.rs`/`appearance.rs` cache
    /// locally.
    pub fn new<B: OutputBackend, H: SourceHost>(controller: &PlaybackController<B, H>) -> Self {
        Self {
            category: SettingsCategory::ALL[0],
            search_query: String::new(),
            focus_target: None,
            plugin_focus: None,
            cached_settings: controller.settings_store().load().settings,
            about: AboutScreen::default(),
            playback: playback::PlaybackScreen::new(controller),
            controls: ControlsScreen::default(),
            plugins: plugins::PluginsScreen::new(),
        }
    }
}

/// Draw the Settings screen (category list, search box and results, then
/// the selected category's content), applying every change directly to
/// `controller`. Returns a fresh `DeviceCheckScreen` the frame the Audio
/// category's "Test output device" button is clicked, plus any
/// `AccountEvent`s the Account category's Sign in/Sign out commands raised
/// this frame (US3 T085/T086/T087) for the caller to map to notifications/
/// screens.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    account: &mut AccountService,
    screen: &mut SettingsScreen,
) -> (Option<DeviceCheckScreen>, Vec<AccountEvent>) {
    let search_id = Id::new(SEARCH_BOX_ID);

    ui.horizontal(|ui| {
        ui.label(tr("settings-search"));
        let response = ui.add(TextEdit::singleline(&mut screen.search_query).id(search_id));
        // 007, contracts/ui-actions.md §2. This screen's own `Ctrl/Cmd+F`
        // focus-request handler is removed (research R10): it has been
        // unreachable since 004 made `Ctrl/Cmd+F` an app-wide jump to
        // Search (the shell switched sections before Settings could see
        // the key), so nothing observable changes.
        actions::register_claim(ui.ctx(), response.id, Claim::TextLike);
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

        // US4 T103 (contracts/overlays-settings-notify.md S6): every
        // visible plugin settings field is searchable too, as its own
        // "Plugins › <plugin> › <label>" hit alongside the fixed
        // `DESCRIPTORS` results above.
        let plugin_views = controller.plugin_settings_views();
        for hit in settings_registry::search_plugin_settings(&screen.search_query, &plugin_views) {
            let response = ui.selectable_label(false, hit.path.clone());
            let activated_by_enter =
                response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
            if response.clicked() || activated_by_enter {
                screen.category = SettingsCategory::Plugins;
                screen.plugin_focus = Some((hit.plugin, hit.field_id));
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
        SettingsCategory::Audio => (
            audio::show(ui, controller, &mut screen.cached_settings, focus),
            Vec::new(),
        ),
        SettingsCategory::Appearance => {
            appearance::show(ui, controller, &mut screen.cached_settings, focus);
            (None, Vec::new())
        }
        SettingsCategory::Language => {
            language::show(ui, focus);
            (None, Vec::new())
        }
        SettingsCategory::Developer => {
            developer::show(ui, controller);
            (None, Vec::new())
        }
        SettingsCategory::Account => (None, account::show(ui, account)),
        SettingsCategory::About => {
            about::show(ui, &mut screen.about);
            (None, Vec::new())
        }
        SettingsCategory::Playback => {
            playback::show(ui, controller, &mut screen.playback, focus);
            (None, Vec::new())
        }
        SettingsCategory::Controls => {
            controls::show(ui, controller, &mut screen.controls, focus);
            (None, Vec::new())
        }
        SettingsCategory::Plugins => {
            let plugin_focus = screen.plugin_focus.take();
            let field_focus = plugin_focus.as_ref().map(|(p, f)| (*p, f.as_str()));
            plugins::show(ui, controller, &mut screen.plugins, field_focus);
            (None, Vec::new())
        }
        SettingsCategory::Offline | SettingsCategory::PrivacyDiagnostics => {
            ui.label(tr("placeholder-settings-category"));
            (None, Vec::new())
        }
    }
}

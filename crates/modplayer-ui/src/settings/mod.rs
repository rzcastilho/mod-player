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
pub mod category_row;
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
use crate::section_memory::{SectionMemory, ViewKey};
use crate::settings::about::AboutScreen;
use crate::settings::category_row::CategoryRowState;
use crate::settings::controls::ControlsScreen;
use crate::theme;

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
    /// 020-shell-navigation-and-gates (US2): the category row's own
    /// partition/menu state (contracts/settings-category-row.md).
    row: CategoryRowState,
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
            row: CategoryRowState::default(),
        }
    }

    /// Reset the selected category back to the first, fixed-order category
    /// (020-shell-navigation-and-gates, US3-AS4, contracts/section-
    /// memory.md M4): called on sign-out/revocation, alongside
    /// `SectionMemory::reset` and the other section sub-views, via
    /// `app::reset_session_ui`. Leaves the search query, focus targets and
    /// every category screen's own state untouched — only the *selected*
    /// category is part of US3-AS4's "every section starts at its default
    /// view".
    pub fn reset_category(&mut self) {
        self.category = SettingsCategory::ALL[0];
    }

    /// The currently selected category (read-only): changed only by
    /// selecting a category in the row/menu, a search-result click, or
    /// [`Self::reset_category`]. 020-shell-navigation-and-gates,
    /// contracts/section-memory.md M4: lets a caller (`app::
    /// reset_session_ui`'s own tests) observe the reset without reaching
    /// into a private field.
    #[must_use]
    pub const fn category(&self) -> SettingsCategory {
        self.category
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
    memory: &mut SectionMemory,
) -> (Option<DeviceCheckScreen>, Vec<AccountEvent>) {
    show_header(ui, controller, screen);
    show_content(ui, controller, account, screen, memory)
}

/// The search box, its live results, and the category row (020-shell-
/// navigation-and-gates, US3, contracts/section-memory.md: "the search box
/// and the category row stay fixed at the top" — FR-010, the selected
/// category must always be visible). Never scrolls.
fn show_header<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut SettingsScreen,
) {
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
        ui.add_space(theme::space::XL);
    }

    // 014-design-tokens-and-type-scale (US2, T032): the category list is
    // this screen's "section list entries" (data-model.md §6 "settings
    // groups" -> `theme::section_label`) — there is no separate "Settings"
    // screen heading anywhere in this slice (audited: `show` goes straight
    // from the search box to this list, and `app.rs`/`shell.rs` draw no
    // per-section title above it either, T031), so `title` has nothing to
    // apply to here.
    // 020-shell-navigation-and-gates (US2, FR-008-FR-012): the row never
    // wraps — whatever does not fit at the current width collapses into a
    // keyboard-operable "More" menu instead (contracts/settings-category-
    // row.md), replacing the old `ui.horizontal_wrapped` block.
    if let Some(category) = category_row::show(ui, screen.category, &mut screen.row) {
        screen.category = category;
    }
    ui.add_space(theme::space::XL);
}

/// The selected category's own body (020-shell-navigation-and-gates, US3,
/// contracts/section-memory.md): wrapped in `memory.scroll_area(&ViewKey::
/// Settings(category))` so it retains its own scroll offset across a round
/// trip, keyed per category — the search box and row above never scroll.
fn show_content<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    account: &mut AccountService,
    screen: &mut SettingsScreen,
    memory: &mut SectionMemory,
) -> (Option<DeviceCheckScreen>, Vec<AccountEvent>) {
    let focus = screen.focus_target.take();
    let key = ViewKey::Settings(screen.category);
    let scroll = memory.scroll_area(&key);
    let output = scroll.show(ui, |ui| match screen.category {
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
    });
    memory.record(key, output.state.offset.y);
    output.inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_screen() -> SettingsScreen {
        use modplayer_audio_io::FakeBackend;
        use modplayer_audio_source_synthetic::ScriptedHost;
        use modplayer_core::settings::SettingsStore;
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-settings-reset-category-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let store = SettingsStore::with_path(dir.join("settings.toml"));
        let controller =
            PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
        SettingsScreen::new(&controller)
    }

    /// 020-shell-navigation-and-gates (US3, contracts/section-memory.md
    /// M4): `reset_category` sets the selected category back to
    /// `SettingsCategory::ALL[0]`, even from a category that isn't it.
    #[test]
    fn reset_category_returns_to_the_first_fixed_order_category() {
        let mut screen = fresh_screen();
        assert_eq!(screen.category(), SettingsCategory::ALL[0], "test setup");

        screen.category = SettingsCategory::Audio;
        assert_ne!(screen.category(), SettingsCategory::ALL[0], "test setup");

        screen.reset_category();
        assert_eq!(screen.category(), SettingsCategory::ALL[0]);
    }
}

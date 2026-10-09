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
pub mod field;
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
    /// 028-settings-fields-and-account (data-model §3.5): the defaults
    /// snapshot per-field Reset compares against (research R4).
    #[allow(dead_code, reason = "read by the Reset wiring in a later phase")]
    defaults: AudioSettings,
    /// The search-result field highlight, if any (data-model §3.4).
    highlight: Option<FieldHighlight>,
    /// Focus deferred to the next frame (after a Reset), merged with
    /// `focus_target` when the category screen is drawn.
    pending_focus: Option<&'static str>,
}

/// A transient outline on a search-result field (data-model §3.4):
/// created when a result is chosen, armed on the next frame, and cleared
/// after 3 s or on the first key/pointer press.
#[derive(Debug, Clone, PartialEq)]
struct FieldHighlight {
    /// Descriptor id of the chosen result.
    id: &'static str,
    /// `ctx.input(|i| i.time)` when armed.
    started_at: f64,
    /// `false` on the selecting frame; `true` from the next frame on.
    armed: bool,
}

/// How long a search-result highlight stays (F19).
const HIGHLIGHT_SECONDS: f64 = 3.0;

impl FieldHighlight {
    /// A highlight created on the selecting frame (unarmed).
    fn new(id: &'static str) -> Self {
        Self {
            id,
            started_at: 0.0,
            armed: false,
        }
    }

    /// Advance one frame. The first call arms (and starts the 3 s clock)
    /// without looking at input, so the press that chose the result does
    /// not clear it. Later calls return `false` once 3 s have passed or a
    /// key / pointer button was pressed.
    fn advance(&mut self, now: f64, key_pressed: bool, pointer_pressed: bool) -> bool {
        if !self.armed {
            self.armed = true;
            self.started_at = now;
            return true;
        }
        !(now - self.started_at >= HIGHLIGHT_SECONDS || key_pressed || pointer_pressed)
    }
}

impl SettingsScreen {
    /// A descriptor search result was chosen: select its category, ask its
    /// control to take focus, and (for the five owned categories) start a
    /// highlight. Controls and Developer results only focus (F20).
    fn select_result(&mut self, category: SettingsCategory, id: &'static str) {
        self.category = category;
        self.focus_target = Some(id);
        self.highlight = matches!(
            category,
            SettingsCategory::Audio
                | SettingsCategory::Playback
                | SettingsCategory::Appearance
                | SettingsCategory::Language
                | SettingsCategory::Account
        )
        .then(|| FieldHighlight::new(id));
    }

    /// Select `category` by any means other than a search result; drops the
    /// highlight when the category actually changes.
    fn change_category(&mut self, category: SettingsCategory) {
        if self.category != category {
            self.highlight = None;
        }
        self.category = category;
    }

    /// Advance the highlight for this frame and publish it for the field
    /// builder; schedules a repaint so expiry needs no input.
    fn tick_highlight(&mut self, ctx: &egui::Context) {
        let (now, key, pointer) = ctx.input(|i| {
            let key = i
                .events
                .iter()
                .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }));
            let pointer = i
                .events
                .iter()
                .any(|e| matches!(e, egui::Event::PointerButton { pressed: true, .. }));
            (i.time, key, pointer)
        });
        if let Some(h) = self.highlight.as_mut()
            && !h.advance(now, key, pointer)
        {
            self.highlight = None;
        }
        match &self.highlight {
            Some(h) => {
                field::set_highlight_target(ctx, Some((h.id, h.armed)));
                if h.armed {
                    let remaining = (HIGHLIGHT_SECONDS - (now - h.started_at)).max(0.0);
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining));
                } else {
                    ctx.request_repaint();
                }
            }
            None => field::set_highlight_target(ctx, None),
        }
    }

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
            defaults: AudioSettings::default(),
            highlight: None,
            pending_focus: None,
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
        self.change_category(SettingsCategory::ALL[0]);
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
    screen.tick_highlight(ui.ctx());
    show_header(ui, controller, screen);
    // A result chosen this frame is unarmed; publish it before the
    // category body draws so the field outlines on the same frame.
    if let Some(h) = &screen.highlight {
        field::set_highlight_target(ui.ctx(), Some((h.id, h.armed)));
    }
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
                screen.select_result(descriptor.category, descriptor.id);
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
                screen.change_category(SettingsCategory::Plugins);
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
        screen.change_category(category);
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
    let focus = screen
        .focus_target
        .take()
        .or_else(|| screen.pending_focus.take());
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
        SettingsCategory::Account => (None, account::show(ui, account, focus)),
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
        category @ (SettingsCategory::Offline | SettingsCategory::PrivacyDiagnostics) => {
            unavailable_category(ui, category);
            (None, Vec::new())
        }
    });
    memory.record(key, output.state.offset.y);
    output.inner
}

/// Body of a category that has no settings yet (F21): a card titled with the
/// category name and one wrapped "not available yet" sentence. Status comes
/// from [`SettingsCategory::is_available`], so the body disappears when a
/// later feature adds descriptors.
fn unavailable_category(ui: &mut Ui, category: SettingsCategory) {
    let sentence_key = match category {
        SettingsCategory::Offline => "settings-unavailable-offline",
        _ => "settings-unavailable-privacy-diagnostics",
    };
    crate::widgets::controls::panel_card(ui, &tr(category.label_key()), |ui| {
        ui.add(egui::Label::new(tr(sentence_key)).wrap());
    });
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::disallowed_methods)]
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

    /// How long a search-result highlight lasts (data-model §3.4).
    #[test]
    fn highlight_is_unarmed_on_the_selecting_frame_then_armed() {
        let mut h = FieldHighlight::new("markers.nudge_step_ms");
        assert!(!h.armed, "unarmed when created on the selecting frame");
        // A key press on the selecting frame (the Enter that chose it)
        // must not clear it: the first advance only arms.
        assert!(h.advance(10.0, true, true));
        assert!(h.armed);
        assert!((h.started_at - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn highlight_clears_after_three_seconds() {
        let mut h = FieldHighlight::new("x");
        assert!(h.advance(10.0, false, false));
        assert!(h.advance(12.9, false, false));
        assert!(!h.advance(13.0, false, false));
    }

    #[test]
    fn highlight_clears_on_key_press_once_armed() {
        let mut h = FieldHighlight::new("x");
        assert!(h.advance(0.0, false, false));
        assert!(!h.advance(0.1, true, false));
    }

    #[test]
    fn highlight_clears_on_pointer_press_once_armed() {
        let mut h = FieldHighlight::new("x");
        assert!(h.advance(0.0, false, false));
        assert!(!h.advance(0.1, false, true));
    }

    #[test]
    fn highlight_clears_on_category_change_and_restarts_on_new_result() {
        let mut screen = fresh_screen();
        screen.select_result(SettingsCategory::Playback, "markers.nudge_step_ms");
        assert_eq!(
            screen.highlight,
            Some(FieldHighlight::new("markers.nudge_step_ms"))
        );
        screen.highlight.as_mut().expect("set").armed = true;

        // A new result restarts: new id, unarmed.
        screen.select_result(SettingsCategory::Audio, "audio.limiter_ceiling");
        assert_eq!(
            screen.highlight,
            Some(FieldHighlight::new("audio.limiter_ceiling"))
        );

        // Changing category by other means clears.
        screen.change_category(SettingsCategory::About);
        assert_eq!(screen.highlight, None);
    }

    #[test]
    fn controls_and_developer_results_never_highlight() {
        let mut screen = fresh_screen();
        screen.select_result(SettingsCategory::Controls, "controls.keybindings");
        assert_eq!(screen.highlight, None);
        assert_eq!(screen.focus_target, Some("controls.keybindings"));
        screen.select_result(SettingsCategory::Developer, "developer.buffer_frames");
        assert_eq!(screen.highlight, None);
    }

    /// F21: Offline and Privacy & diagnostics draw a titled card plus the
    /// per-category "not available yet" sentence, never the generic
    /// placeholder.
    #[test]
    fn unavailable_categories_show_a_header_and_their_own_sentence() {
        use egui::accesskit::Role;
        for (category, sentence_key) in [
            (SettingsCategory::Offline, "settings-unavailable-offline"),
            (
                SettingsCategory::PrivacyDiagnostics,
                "settings-unavailable-privacy-diagnostics",
            ),
        ] {
            let ctx = egui::Context::default();
            crate::theme::apply_tokens(&ctx);
            ctx.enable_accesskit();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                unavailable_category(ui, category);
            });
            let update = output
                .platform_output
                .accesskit_update
                .take()
                .expect("accesskit update");
            output.drop_without_applying_deltas();
            let labels: Vec<(Role, String)> = update
                .nodes
                .iter()
                .filter_map(|(_, n)| {
                    n.label()
                        .or_else(|| n.value())
                        .map(|l| (n.role(), l.to_string()))
                })
                .collect();
            assert!(
                labels.contains(&(Role::Heading, tr(category.label_key()))),
                "{category:?}: {labels:?}"
            );
            assert!(
                labels.iter().any(|(_, l)| *l == tr(sentence_key)),
                "{category:?}: {labels:?}"
            );
            assert!(
                !labels
                    .iter()
                    .any(|(_, l)| *l == tr("placeholder-settings-category")),
                "{category:?}: {labels:?}"
            );
        }
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

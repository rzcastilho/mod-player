// SPDX-License-Identifier: MIT OR Apache-2.0

//! The eframe `App` entrypoint (contracts/ui-surface.md "Main window"):
//! wires an injected `PlaybackController` to the shell's navigation,
//! notification area, and theme. The binary (`crates/modplayer`, T096)
//! constructs the concrete `CpalBackend`-backed controller, calls
//! `controller.launch()`, and hands it to `App::new`.

use std::time::{Duration, Instant};

use egui::{Align2, Area, CentralPanel, Id, Panel, Ui, vec2};
use modplayer_audio_io::OutputBackend;
use modplayer_core::PlaybackController;

use crate::device_check::DeviceCheckScreen;
use crate::settings::SettingsScreen;
use crate::shell::{Section, Shell};
use crate::{notifications, now_playing, settings, theme};

/// Upper bound between UI frames while the app is running (≈ 30 Hz).
const REPAINT_INTERVAL: Duration = Duration::from_millis(33);

/// The whole application: the playback controller plus the app-shell UI
/// state (selected nav section, the Settings screen's own state, an open
/// Device Check overlay if any).
pub struct App<B: OutputBackend> {
    controller: PlaybackController<B>,
    shell: Shell,
    settings: SettingsScreen,
    device_check: Option<DeviceCheckScreen>,
}

impl<B: OutputBackend> App<B> {
    /// Construct the app around an already-`launch()`ed `controller`.
    /// Applies the persisted theme to `cc.egui_ctx` before returning, so it
    /// is set before `ui()` ever paints (contracts/ui-surface.md "Theme").
    /// Opens Device Check immediately when the controller says it must be
    /// shown (no confirmed device yet, or zero devices at launch).
    pub fn new(cc: &eframe::CreationContext<'_>, controller: PlaybackController<B>) -> Self {
        theme::apply(&cc.egui_ctx, controller.theme());
        let device_check = controller.should_show_device_check().then(|| {
            DeviceCheckScreen::new(
                controller.active_device().map(|d| d.id.clone()),
                controller.preset(),
            )
        });
        let settings = SettingsScreen::new(&controller);
        Self {
            controller,
            shell: Shell::default(),
            settings,
            device_check,
        }
    }
}

impl<B: OutputBackend> eframe::App for App<B> {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // Retry any pending commands, drain device events (US3), and age
        // out Info notifications — once per frame.
        self.controller.tick();
        self.controller.notifications_mut().tick(Instant::now());

        let ctx = ui.ctx().clone();
        // Repaint at ≥ 30 Hz (plan.md "Device watcher thread"): the peak
        // meter, device events and Info-notification expiry are all polled
        // from this method, so the UI must keep ticking without input.
        ctx.request_repaint_after(REPAINT_INTERVAL);
        self.shell.handle_shortcuts(&ctx);

        Panel::left(Id::new("shell-nav-rail")).show(ui, |ui| {
            self.shell.nav_rail(ui);
        });

        // Top-right, newest-first, non-modal — never blocks navigation or
        // playback (contracts/ui-surface.md).
        let dismissed = Area::new(Id::new("shell-notifications"))
            .anchor(Align2::RIGHT_TOP, vec2(-8.0, 8.0))
            .show(&ctx, |ui| {
                notifications::show(ui, self.controller.notifications())
            })
            .inner;
        if let Some(id) = dismissed {
            self.controller.notifications_mut().dismiss(id);
        }

        CentralPanel::default().show(ui, |ui| {
            if let Some(mut screen) = self.device_check.take() {
                if !screen.show(ui, &mut self.controller) {
                    self.device_check = Some(screen);
                }
                return;
            }

            match self.shell.section {
                Section::Library => crate::shell::library_placeholder(ui),
                Section::NowPlaying => now_playing::show(ui, &mut self.controller),
                Section::Plugins => crate::shell::plugins_placeholder(ui),
                Section::Settings => {
                    if let Some(screen) =
                        settings::show(ui, &mut self.controller, &mut self.settings)
                    {
                        self.device_check = Some(screen);
                    }
                }
            }
        });
    }
}

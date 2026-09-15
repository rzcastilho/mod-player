// SPDX-License-Identifier: MIT OR Apache-2.0

//! The eframe `App` entrypoint (contracts/ui-surface.md "Main window"):
//! wires an injected `PlaybackController` to the shell's navigation,
//! notification area, and theme, and an injected `AccountService` to the
//! 002-first-launch-and-sign-in launch gate
//! (`modplayer_account::launch_flow::next_step`, data-model.md §2.4). The
//! binary (`crates/modplayer`, T032/T096) constructs both, calls
//! `controller.launch()`/`account.launch()`, and hands them to `App::new`.
//!
//! The launch gate is evaluated every frame (design note 8): whenever it
//! is not `Main`, the central panel shows that gate instead of the normal
//! shell. `Welcome` renders the real US1 screen (`welcome.rs`,
//! `privacy_notice.rs`); `SignIn` still renders a placeholder naming the
//! step until US2 (Phase 4) lands its screen.

use std::time::{Duration, Instant};

use egui::{Align2, Area, CentralPanel, Id, OpenUrl, Panel, Ui, vec2};
use modplayer_account::{AccountEvent, AccountService, LaunchStep, SessionState, Tier, next_step};
use modplayer_audio_io::OutputBackend;
use modplayer_core::{NotificationAction, PlaybackController, Severity, tr};

use crate::device_check::DeviceCheckScreen;
use crate::settings::SettingsScreen;
use crate::shell::{Section, Shell};
use crate::sign_in::{self, SignInScreen, TierResult};
use crate::welcome::{self, WelcomeScreen};
use crate::{notifications, now_playing, settings, theme};

/// Upper bound between UI frames while the app is running (≈ 30 Hz).
const REPAINT_INTERVAL: Duration = Duration::from_millis(33);

/// The whole application: the playback controller plus the app-shell UI
/// state (selected nav section, the Settings screen's own state, an open
/// Device Check overlay if any), and the account service that gates
/// everything ahead of it.
pub struct App<B: OutputBackend> {
    controller: PlaybackController<B>,
    account: AccountService,
    /// Cached from `settings.disclosure` at construction (data-model.md
    /// §1.1). Only the welcome screen's acknowledge action
    /// (`show_welcome`) updates it afterwards, the same frame it records
    /// the acknowledgement, so `launch_step()` advances without a restart.
    disclosure_acknowledged_version: u32,
    /// The Welcome/Decline/Privacy-Notice screen's own sub-view state
    /// (US1). Constructed once and reused across frames like
    /// `device_check`, even though it is only ever shown before the first
    /// acknowledgement.
    welcome: WelcomeScreen,
    /// The Sign-in step's own sub-view state (US2), constructed once and
    /// reused across frames like `welcome`/`device_check`.
    sign_in: SignInScreen,
    shell: Shell,
    settings: SettingsScreen,
    /// The Device Check overlay, when open — either the launch gate's
    /// (`should_show_device_check()` at launch/after sign-in) or a
    /// Settings-triggered "Test output device" preview (independent of the
    /// launch gate: `settings::show` hands one back regardless of
    /// confirmation state).
    device_check: Option<DeviceCheckScreen>,
}

impl<B: OutputBackend> App<B> {
    /// Construct the app around an already-`launch()`ed `controller` and
    /// `account`. Applies the persisted theme to `cc.egui_ctx` before
    /// returning, so it is set before `ui()` ever paints
    /// (contracts/ui-surface.md "Theme").
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mut controller: PlaybackController<B>,
        account: AccountService,
    ) -> Self {
        theme::apply(&cc.egui_ctx, controller.theme());
        let disclosure_acknowledged_version = controller
            .settings_store()
            .load()
            .settings
            .disclosure
            .map(|ack| ack.acknowledged_version)
            .unwrap_or(0);
        let device_check = controller.should_show_device_check().then(|| {
            DeviceCheckScreen::new(
                controller.active_device().map(|d| d.id.clone()),
                controller.preset(),
            )
        });
        let settings = SettingsScreen::new(&controller);

        // FR-017 (US4, T100): a session found at launch whose secure store
        // failed to read (`SessionState::StoreUnreadable`) raises a
        // Warning up front, so it's visible even before the sign-in step
        // (which renders the same message inline, T096) is ever painted.
        if matches!(account.state(), SessionState::StoreUnreadable) {
            controller.notifications_mut().raise_with_args(
                Severity::Warning,
                "store-unreadable",
                vec![("store", tr(account.secure().platform_name_key()))],
            );
        }

        Self {
            controller,
            account,
            disclosure_acknowledged_version,
            welcome: WelcomeScreen::default(),
            sign_in: SignInScreen::default(),
            shell: Shell::default(),
            settings,
            device_check,
        }
    }

    /// The launch step for this frame (data-model.md §2.4), combining the
    /// disclosure acknowledgement, the account session, and 001's device
    /// confirmation state.
    fn launch_step(&self) -> LaunchStep {
        next_step(
            self.disclosure_acknowledged_version,
            self.account.state(),
            self.account.tier(),
            self.controller.should_show_device_check(),
        )
    }
}

impl<B: OutputBackend> eframe::App for App<B> {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // Retry any pending commands, drain device events (US3 of 001),
        // and age out Info notifications — once per frame.
        self.controller.tick();
        self.controller.notifications_mut().tick(Instant::now());

        let ctx = ui.ctx().clone();
        // Repaint at ≥ 30 Hz (plan.md "Device watcher thread"): the peak
        // meter, device events and Info-notification expiry are all polled
        // from this method, so the UI must keep ticking without input.
        ctx.request_repaint_after(REPAINT_INTERVAL);
        self.shell.handle_shortcuts(&ctx);

        // Drain account worker events and map them to notifications/
        // screens (US2 T071, design note 4: "notifications are the UI's
        // job" — `modplayer-account` stays free of `modplayer-core`).
        let account_events = self.account.tick();
        for event in account_events {
            self.handle_account_event(&ctx, event);
        }

        Panel::left(Id::new("shell-nav-rail")).show(ui, |ui| {
            self.shell.nav_rail(ui);
        });

        // Top-right, newest-first, non-modal — never blocks navigation or
        // playback (contracts/ui-surface.md).
        let interaction = Area::new(Id::new("shell-notifications"))
            .anchor(Align2::RIGHT_TOP, vec2(-8.0, 8.0))
            .show(&ctx, |ui| {
                notifications::show(ui, self.controller.notifications())
            })
            .inner;
        if let Some(id) = interaction.dismissed {
            self.controller.notifications_mut().dismiss(id);
        }
        // A notification action button click (e.g. `signin-again`'s "Sign
        // in", US2 T071): every `NotificationAction` variant currently
        // means "start a fresh sign-in attempt".
        if let Some((id, NotificationAction::SignIn)) = interaction.action_clicked {
            self.controller.notifications_mut().dismiss(id);
            for event in self.account.start_sign_in() {
                self.handle_account_event(&ctx, event);
            }
        }

        let step = self.launch_step();

        CentralPanel::default().show(ui, |ui| match step {
            LaunchStep::Welcome => self.show_welcome(ui),
            LaunchStep::SignIn => self.show_sign_in(ui),
            LaunchStep::DeviceCheck => self.show_launch_gate_device_check(ui),
            LaunchStep::Main => self.show_main(ui),
        });
    }
}

impl<B: OutputBackend> App<B> {
    /// The Welcome/Decline/Privacy-Notice launch gate (US1, contracts/
    /// ui-surface.md "Welcome"): draws whichever sub-view `self.welcome`
    /// is on, and updates the cached acknowledged version the moment
    /// acknowledgement is recorded so `launch_step()` advances past
    /// `Welcome` this same frame (data-model.md §2.4).
    fn show_welcome(&mut self, ui: &mut Ui) {
        if let Some(welcome::WelcomeOutcome::Acknowledged) =
            self.welcome.show(ui, &mut self.controller)
        {
            self.disclosure_acknowledged_version = modplayer_account::DISCLOSURE_BUNDLE_VERSION;
        }
    }

    /// The Sign-in step launch gate (US2, contracts/ui-surface.md
    /// "Sign-in step"): draws whichever sign-in sub-state `self.sign_in`
    /// (plus `self.account`'s `SessionState`) resolves to, reacting to any
    /// event a command invoked this frame raised synchronously (opening
    /// the browser is `App`'s job, design note 10).
    fn show_sign_in(&mut self, ui: &mut Ui) {
        let events = sign_in::show(ui, &mut self.account, &mut self.sign_in);
        let ctx = ui.ctx().clone();
        for event in events {
            self.handle_account_event(&ctx, event);
        }
    }

    /// Map one `AccountEvent` to a notification/screen update (US2 T071,
    /// design note 4; US3 T086; US4 T100).
    fn handle_account_event(&mut self, ctx: &egui::Context, event: AccountEvent) {
        match event {
            AccountEvent::BrowserUrlReady(url) => {
                ctx.open_url(OpenUrl::new_tab(url));
            }
            AccountEvent::StoreUnavailable { .. } | AccountEvent::SignInFailed(_) => {
                // Rendered inline on the sign-in step itself
                // (contracts/ui-surface.md); no separate notification.
            }
            AccountEvent::Authorized => {
                // The "Checking your account" sub-state renders directly
                // from `SessionState::Checking`; no notification needed.
            }
            AccountEvent::TierChecked(tier) => match tier {
                Tier::Premium => {}
                Tier::Free => self.sign_in.set_tier_result(TierResult::Free),
                Tier::Unknown => self.sign_in.set_tier_result(TierResult::Unknown),
            },
            AccountEvent::TierCheckFailed => {
                self.sign_in.set_tier_result(TierResult::Unknown);
            }
            AccountEvent::RefreshFailing => {
                self.controller.notifications_mut().raise_with_action(
                    Severity::Warning,
                    "signin-again",
                    NotificationAction::SignIn,
                );
            }
            AccountEvent::RefreshRecovered => {
                self.controller
                    .notifications_mut()
                    .dismiss_by_key("signin-again");
            }
            AccountEvent::SignedOut { categories } => {
                let joined = categories
                    .iter()
                    .map(|key| tr(key))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.controller.notifications_mut().raise_with_args(
                    Severity::Info,
                    "signed-out",
                    vec![("categories", joined)],
                );
            }
            AccountEvent::SignOutIncomplete { category } => {
                self.controller.notifications_mut().raise_with_args(
                    Severity::Warning,
                    "signout-incomplete",
                    vec![("category", tr(category))],
                );
            }
            AccountEvent::SessionExpired => {
                self.controller.notifications_mut().raise_with_action(
                    Severity::Critical,
                    "session-expired",
                    NotificationAction::SignIn,
                );
            }
            AccountEvent::SessionRevoked => {
                self.controller.notifications_mut().raise_with_action(
                    Severity::Critical,
                    "session-revoked",
                    NotificationAction::SignIn,
                );
            }
        }
    }

    /// The launch gate's own Device Check (data-model.md §2.4: `Active`/
    /// `Expired`, `Premium`, not yet confirmed) — reuses whatever screen
    /// `App::new`/a prior frame built, or builds a fresh one, mirroring
    /// 001's `launch()`-time construction.
    fn show_launch_gate_device_check(&mut self, ui: &mut Ui) {
        let mut screen = self.device_check.take().unwrap_or_else(|| {
            DeviceCheckScreen::new(
                self.controller.active_device().map(|d| d.id.clone()),
                self.controller.preset(),
            )
        });
        if !screen.show(ui, &mut self.controller) {
            self.device_check = Some(screen);
        }
    }

    /// The normal app shell (001's nav rail sections), including a
    /// Settings-triggered Device Check preview overlay if one is open —
    /// independent of the launch gate (`settings::show`'s "Test output
    /// device" hands one back regardless of confirmation state).
    fn show_main(&mut self, ui: &mut Ui) {
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
                let (device_check, events) = settings::show(
                    ui,
                    &mut self.controller,
                    &mut self.account,
                    &mut self.settings,
                );
                if let Some(screen) = device_check {
                    self.device_check = Some(screen);
                }
                let ctx = ui.ctx().clone();
                for event in events {
                    self.handle_account_event(&ctx, event);
                }
            }
        }
    }
}

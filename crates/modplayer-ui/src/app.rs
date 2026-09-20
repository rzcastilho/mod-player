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

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align2, Area, CentralPanel, Id, OpenUrl, Panel, Ui, vec2};
use modplayer_account::{
    AccountEvent, AccountService, LaunchStep, ReadOutcome, RequestId, SessionState, Tier,
    UPGRADE_URL, next_step,
};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::actions::ScopeState;
use modplayer_core::{
    ActiveState, Intent, NotRegisteredReason, NotificationAction, PlaybackController,
    STATUS_PAGE_URL, Severity, tr,
};

use crate::actions;
use crate::artwork::ArtworkCache;
use crate::detail_view::{self, DetailOutcome, DetailTarget};
use crate::device_check::DeviceCheckScreen;
use crate::library_view::{self, LibraryOutcome};
use crate::settings::SettingsScreen;
use crate::shell::{Section, Shell};
use crate::sign_in::{self, SignInScreen, TierResult};
use crate::ticker::Ticker;
use crate::waveform::WaveformState;
use crate::welcome::{self, WelcomeScreen};
use crate::{notifications, now_playing, plugins_view, search_view, settings, theme};

/// Upper bound between UI frames while the app is running (≈ 30 Hz).
const REPAINT_INTERVAL: Duration = Duration::from_millis(33);

/// The whole application: the playback controller plus the app-shell UI
/// state (selected nav section, the Settings screen's own state, an open
/// Device Check overlay if any), and the account service that gates
/// everything ahead of it.
pub struct App<B: OutputBackend, H: SourceHost> {
    controller: PlaybackController<B, H>,
    account: AccountService,
    /// Background repaint thread (research R12, FR-008/SC-010): keeps
    /// `tick()` running while minimized/hidden/backgrounded.
    ticker: Ticker,
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
    /// The in-flight `request_playback_state()` for the transfer banner's
    /// "Playing on <device>" name (US3 T077, FR-016/019, research R3),
    /// `None` when nothing is outstanding. Re-requested every frame the
    /// controller is `Inactive` with no name yet — covers both the launch
    /// check ("another device is already active") and a reconnect after a
    /// later `BecameInactive`.
    pending_device_name_request: Option<RequestId>,
    /// Off-thread artwork fetch/decode cache (004-search-and-library-
    /// browse, contracts/ui-surface.md §6): lives for the signed-in
    /// session, cleared on sign-out (design note 8) alongside `search`/
    /// `library` state.
    artwork: ArtworkCache,
    /// The Library view's own frame-persistent state (selected tab, US2
    /// T065) — constructed once and reused like `shell`/`settings`.
    library_view: library_view::LibraryViewState,
    /// The single-level detail-navigation stack (US2 T065): `Some` while an
    /// album/playlist/artist detail is open over the Library view; nothing
    /// in this slice opens a detail view from another detail view, so one
    /// level is enough.
    library_detail: Option<detail_view::DetailTarget>,
    /// The Now Playing waveform widgets' own session state (005-now-
    /// playing-waveform, data-model.md §5.2: drag preview, detail window)
    /// — constructed once and reused like `library_view`/`settings`.
    waveform: WaveformState,
}

impl<B: OutputBackend, H: SourceHost> App<B, H> {
    /// Construct the app around an already-`launch()`ed `controller` and
    /// `account`. Applies the persisted theme to `cc.egui_ctx` before
    /// returning, so it is set before `ui()` ever paints
    /// (contracts/ui-surface.md "Theme").
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mut controller: PlaybackController<B, H>,
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

        // A relaunch resumes an already-`Active`/`Premium` session with no
        // `AccountEvent::TierChecked` of its own (that only fires during a
        // live sign-in flow, T061/contracts/ui-surface.md §6 "Launch"), so
        // the permission has to be set here too, from whatever `launch()`
        // already resolved.
        apply_account_permission(&mut controller, &account);

        // 009 US4 (T106, contracts/plugin-host-service.md §1): a plugin
        // thread's own admitted request wakes the UI promptly by calling
        // this instead of waiting for the next scheduled repaint.
        let waker_ctx = cc.egui_ctx.clone();
        controller.set_waker(Arc::new(move || waker_ctx.request_repaint()));

        let ticker = Ticker::spawn(cc.egui_ctx.clone());

        Self {
            controller,
            account,
            ticker,
            disclosure_acknowledged_version,
            welcome: WelcomeScreen::default(),
            sign_in: SignInScreen::default(),
            shell: Shell::default(),
            settings,
            device_check,
            pending_device_name_request: None,
            artwork: ArtworkCache::new(),
            library_view: library_view::LibraryViewState::default(),
            library_detail: None,
            waveform: WaveformState::default(),
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

    /// This frame's `ScopeState` (research R5, contracts/ui-actions.md
    /// §1): `now_playing_shown` follows the shell's own selected section
    /// (dispatch already only ever runs while `launch_step() == Main`
    /// with no Device Check overlay open, so no need to repeat that gate
    /// here); `marker_focused` narrows it further to a focused marker
    /// glyph/row.
    fn scope_state(&self) -> ScopeState {
        let now_playing_shown = self.shell.section == Section::NowPlaying;
        ScopeState {
            now_playing_shown,
            marker_focused: now_playing_shown && self.waveform.focused_marker.is_some(),
        }
    }
}

impl<B: OutputBackend, H: SourceHost> eframe::App for App<B, H> {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // Retry any pending commands, drain device events (US3 of 001),
        // and age out Info notifications — once per frame.
        self.controller.tick();
        self.controller.notifications_mut().tick(Instant::now());
        self.ticker
            .set_playing(self.controller.transport_state().intent == Intent::Playing);

        let ctx = ui.ctx().clone();
        // Repaint at ≥ 30 Hz (plan.md "Device watcher thread"): the peak
        // meter, device events and Info-notification expiry are all polled
        // from this method, so the UI must keep ticking without input.
        ctx.request_repaint_after(REPAINT_INTERVAL);

        // The Action & Binding dispatcher (007, contracts/ui-actions.md
        // §1): runs before any widget draws, consuming every key event it
        // resolves so no later widget can also act on it (SC-010). Reads
        // *last* frame's focus claims, then clears the accumulator for
        // this frame's fresh registrations (design note 2).
        let claims = actions::claims_snapshot(&ctx);
        actions::clear_claims(&ctx);
        if self.launch_step() == LaunchStep::Main && self.device_check.is_none() {
            let scope = self.scope_state();
            let invocations = actions::dispatch(&ctx, &claims, self.controller.actions(), &scope);
            for inv in invocations {
                actions::invoke(
                    inv,
                    &mut self.controller,
                    &mut self.shell,
                    &mut self.waveform,
                    &ctx,
                );
            }
        }

        // Drain account worker events and map them to notifications/
        // screens (US2 T071, design note 4: "notifications are the UI's
        // job" — `modplayer-account` stays free of `modplayer-core`).
        let account_events = self.account.tick();
        for event in account_events {
            self.handle_account_event(&ctx, event);
        }
        self.request_other_device_name_if_needed();

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
        // A notification action button click (US2 T071, US4 T090): route
        // by which action it was — `open_url` only ever happens here, from
        // the UI (design note 10).
        if let Some((id, action)) = interaction.action_clicked {
            match action {
                NotificationAction::SignIn => {
                    self.controller.notifications_mut().dismiss(id);
                    for event in self.account.start_sign_in() {
                        self.handle_account_event(&ctx, event);
                    }
                }
                NotificationAction::OpenStatusPage => {
                    ctx.open_url(OpenUrl::new_tab(STATUS_PAGE_URL));
                }
                NotificationAction::RetrySource => {
                    self.controller.notifications_mut().dismiss(id);
                    self.controller.retry_source();
                }
                NotificationAction::OpenUpgradePage => {
                    ctx.open_url(OpenUrl::new_tab(UPGRADE_URL));
                }
                // 009 US1 (T075)/US4 (T108): `plugin-suspended`'s Restart/
                // Disable actions.
                NotificationAction::RestartPlugin(plugin_id) => {
                    self.controller.notifications_mut().dismiss(id);
                    self.controller.plugin_restart(plugin_id);
                }
                NotificationAction::DisablePlugin(plugin_id) => {
                    self.controller.notifications_mut().dismiss(id);
                    self.controller.plugin_disable(plugin_id);
                }
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

    /// Closing the window quits (FR-008): stop, release the source's
    /// resources (deregistering the Connect device) and drop the stream —
    /// no tray/background mode. The workspace's `eframe` dependency does
    /// not enable the `glow` feature (default renderer is `wgpu`), so
    /// `on_exit` takes no context parameter.
    fn on_exit(&mut self) {
        self.controller.shutdown();
    }
}

impl<B: OutputBackend, H: SourceHost> App<B, H> {
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
            AccountEvent::TierChecked(tier) => {
                match tier {
                    Tier::Premium => {}
                    Tier::Free => self.sign_in.set_tier_result(TierResult::Free),
                    Tier::Unknown => self.sign_in.set_tier_result(TierResult::Unknown),
                }
                apply_account_permission(&mut self.controller, &self.account);
            }
            AccountEvent::TierCheckFailed => {
                // No notification: with session-sourced tier (spec Amendment
                // 2026-09-16) the public `/v1/me` check is best-effort and
                // 429s on every launch with a Keymaster token, so a failure
                // is expected, not an error. The Account panel already shows
                // "tier: Unknown / Never checked online" inline; the session
                // is the real tier authority.
                self.sign_in.set_tier_result(TierResult::Unknown);
                apply_account_permission(&mut self.controller, &self.account);
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
                // Design note 7: deregister the Connect device *before*
                // processing 002's own report of the sign-out.
                self.controller.clear_for_sign_out();
                // Design note 8 (004-search-and-library-browse): the
                // artwork cache is per-session state too.
                self.artwork = ArtworkCache::new();
                self.waveform = WaveformState::default();
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
                // Design note 7, same ordering as `SignedOut` above.
                self.controller.clear_for_sign_out();
                self.artwork = ArtworkCache::new();
                self.waveform = WaveformState::default();
                self.controller.notifications_mut().raise_with_action(
                    Severity::Critical,
                    "session-revoked",
                    NotificationAction::SignIn,
                );
            }
            AccountEvent::ReadResult { request_id, result } => {
                self.handle_read_result(request_id, &result);
            }
        }
    }

    /// Route an `AccountEvent::ReadResult` to the transfer banner's own
    /// in-flight request, updating the controller's `Inactive` device name
    /// (US3 T077). 003's "Play from account" once routed through here too;
    /// it was retired in 004-search-and-library-browse (T062) in favour of
    /// the Library view.
    fn handle_read_result(
        &mut self,
        request_id: modplayer_account::RequestId,
        result: &ReadOutcome,
    ) {
        if self.pending_device_name_request == Some(request_id) {
            self.pending_device_name_request = None;
            if let ReadOutcome::PlaybackState(outcome) = result {
                // `Ok(None)`/`Err(_)` leave the name `None`: the banner
                // falls back to a generic "another device" (Complexity
                // Tracking: "graceful fallback").
                let name = outcome
                    .as_ref()
                    .ok()
                    .and_then(|summary| summary.as_ref())
                    .and_then(|summary| summary.device_name.clone());
                self.controller.set_other_device_name(name);
            }
        }
    }

    /// Fetch the other Connect device's name (US3 T077, FR-016/019,
    /// research R3) whenever the controller is `Inactive` with no name
    /// known yet and nothing is already in flight — covers both the
    /// launch-time check ("another device is already active", ui-surface
    /// §6 "Launch") and a later reconnect after `BecameInactive`.
    fn request_other_device_name_if_needed(&mut self) {
        let needs_name = matches!(
            self.controller.active_state(),
            ActiveState::Inactive { other_device: None }
        );
        if needs_name && self.pending_device_name_request.is_none() {
            self.pending_device_name_request = Some(self.account.request_playback_state());
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
            // The library_placeholder wiring point shell.rs used to own is
            // dropped (004-search-and-library-browse, T026); Section::Library
            // now renders the real Library view, with a single-level
            // detail-navigation stack layered over it (T065).
            Section::Library => self.show_library(ui),
            Section::Search => {
                search_view::show(
                    ui,
                    &mut self.controller,
                    &mut self.artwork,
                    &mut self.shell.focus_search_requested,
                );
            }
            Section::NowPlaying => now_playing::show(
                ui,
                &mut self.controller,
                &mut self.artwork,
                &mut self.waveform,
            ),
            // 009 US4 (T106, contracts/ui-plugins.md §2): a 500 ms repaint
            // request keeps the live CPU/memory gauges moving while the
            // section is visible, on top of the global ≥ 30 Hz loop above.
            Section::Plugins => {
                ui.ctx().request_repaint_after(Duration::from_millis(500));
                plugins_view::show(ui, &mut self.controller);
            }
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

    /// The Library view, with the single-level detail-navigation stack
    /// (US2 T065) layered over it: a detail target open takes over the
    /// whole panel until **Back**; otherwise the tabbed Library view
    /// itself, whose outcomes (open a detail, or focus Search from an
    /// empty state) are handled here since they cross this view's own
    /// scope (library_view.rs's doc comment).
    fn show_library(&mut self, ui: &mut Ui) {
        if let Some(target) = self.library_detail.clone() {
            let outcome = detail_view::show(ui, &mut self.controller, &mut self.artwork, &target);
            if outcome == DetailOutcome::Back {
                self.library_detail = None;
            }
            return;
        }

        let outcome = library_view::show(
            ui,
            &mut self.controller,
            &mut self.artwork,
            &mut self.library_view,
        );
        match outcome {
            LibraryOutcome::None => {}
            LibraryOutcome::FocusSearch => {
                self.shell.section = Section::Search;
                self.shell.focus_search_requested = true;
            }
            LibraryOutcome::OpenAlbum(id) => {
                self.library_detail = Some(DetailTarget::Album(id));
            }
            LibraryOutcome::OpenPlaylist(id) => {
                self.library_detail = Some(DetailTarget::Playlist(id));
            }
            LibraryOutcome::OpenArtist(id) => {
                self.library_detail = Some(DetailTarget::Artist(id));
            }
        }
    }
}

/// `controller.set_playback_permitted(..)` from the account session's
/// current view (T061, contracts/ui-surface.md §6 "App wiring").
///
/// Session-sourced tier (spec Amendment 2026-09-16): the public `/v1/me`
/// tier check returns a blanket 429 with a Keymaster token, so tier stays
/// `Unknown` for real Premium users and can no longer gate registration.
/// Permission to *attempt* registration therefore follows from being signed
/// in; the receiver session is the tier authority — a successful
/// registration (`SourceEvent::Registered`) enables the transport, and a
/// `PremiumAccountRequired` rejection (`SourceEvent::TierRejected`) disables
/// it with "Premium required" (FR-027). A definitive `Free` (only possible
/// when a working Web API reported it) still blocks up front.
fn apply_account_permission<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
    account: &AccountService,
) {
    match permission_decision(account.state(), account.tier()) {
        None => controller.set_playback_permitted(true, None),
        Some(reason) => controller.set_playback_permitted(false, Some(reason)),
    }
}

/// Pure core of [`apply_account_permission`]: `None` permits the registration
/// attempt, `Some(reason)` disables the transport with that inline reason.
/// Session-sourced tier (spec Amendment 2026-09-16) — a signed-in account is
/// permitted to *attempt* registration whenever its tier is not a definitive
/// `Free`, because `Unknown` (the Keymaster 429 case) can no longer be
/// distinguished from `Premium` up front; the session confirms it.
fn permission_decision(state: &SessionState, tier: Tier) -> Option<NotRegisteredReason> {
    match state {
        SessionState::SignedOut { .. } => Some(NotRegisteredReason::SignedOut),
        SessionState::Active | SessionState::Expired | SessionState::Checking { .. } => {
            if tier == Tier::Free {
                Some(NotRegisteredReason::PremiumRequired)
            } else {
                None
            }
        }
        // `Authorizing`, `StoreUnreadable`: not in a state to attempt yet.
        _ => Some(NotRegisteredReason::SubscriptionNotVerified),
    }
}

#[cfg(test)]
mod permission_tests {
    use super::permission_decision;
    use modplayer_account::{SessionState, Tier};
    use modplayer_core::NotRegisteredReason;

    #[test]
    fn signed_in_unknown_tier_permits_registration() {
        // The Keymaster-429 case: real Premium users read as `Unknown`, and
        // must still be allowed to register so the session can confirm tier.
        assert_eq!(
            permission_decision(&SessionState::Active, Tier::Unknown),
            None
        );
        assert_eq!(
            permission_decision(&SessionState::Expired, Tier::Unknown),
            None
        );
        assert_eq!(
            permission_decision(&SessionState::Checking { attempt_id: 1 }, Tier::Unknown),
            None
        );
    }

    #[test]
    fn signed_in_premium_permits_registration() {
        assert_eq!(
            permission_decision(&SessionState::Active, Tier::Premium),
            None
        );
    }

    #[test]
    fn definitive_free_blocks_with_premium_required() {
        assert_eq!(
            permission_decision(&SessionState::Active, Tier::Free),
            Some(NotRegisteredReason::PremiumRequired)
        );
    }

    #[test]
    fn signed_out_blocks_with_signed_out_reason() {
        assert_eq!(
            permission_decision(&SessionState::SignedOut { note: None }, Tier::Unknown),
            Some(NotRegisteredReason::SignedOut)
        );
    }

    #[test]
    fn not_yet_attemptable_states_block_as_not_verified() {
        assert_eq!(
            permission_decision(&SessionState::StoreUnreadable, Tier::Unknown),
            Some(NotRegisteredReason::SubscriptionNotVerified)
        );
        assert_eq!(
            permission_decision(
                &SessionState::Authorizing {
                    attempt_id: 1,
                    resumed: false
                },
                Tier::Unknown
            ),
            Some(NotRegisteredReason::SubscriptionNotVerified)
        );
    }
}

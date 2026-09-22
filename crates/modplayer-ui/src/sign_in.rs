// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sign-in step sub-states (contracts/ui-surface.md "Sign-in step"):
//! renders `SessionState`'s sign-in sub-states (`SignedOut`,
//! `StoreUnreadable`, `Authorizing`, `Checking`) plus one transient overlay
//! that is UI-only state, not part of `SessionState` itself — the
//! tier-result view (shown once after a tier check resolves to Free or
//! Unknown, before the launch gate proceeds). No `TextEdit` exists
//! anywhere on this screen (FR-007: no password field).
//!
//! `SessionState::StoreUnreadable` (a session found at launch whose secure
//! store failed to read, US4 T096) shares its rendering with the
//! `start_sign_in` probe-failure message (`screen.store_unavailable`) —
//! same `{ $store }`/remedy/Retry layout — but its Retry calls
//! `AccountService::retry_store_read` instead of `start_sign_in`, so
//! recovering never opens a browser or starts a fresh attempt (quickstart
//! M6: "unlock + Retry → signed in, nothing was cleared").

use egui::{Spinner, Ui, WidgetInfo, WidgetType};
use modplayer_account::{AccountEvent, AccountService, SessionState, SignInNote, UPGRADE_URL};
use modplayer_core::{tr, tr_args};

use crate::theme;

/// The transient tier-result sub-view (contracts/ui-surface.md: shown once
/// after `TierChecked`/`TierCheckFailed`). Premium shows nothing — the
/// flow advances immediately — so only these two outcomes pause here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierResult {
    Free,
    Unknown,
}

/// Local UI-only state for the sign-in step. `App` owns one instance for
/// the launch gate's lifetime (mirroring `WelcomeScreen`/
/// `DeviceCheckScreen`).
#[derive(Debug, Default)]
pub struct SignInScreen {
    /// Set when a `start_sign_in` probe fails; cleared by pressing Retry.
    store_unavailable: Option<&'static str>,
    /// Set by `App`'s event mapping (`app.rs`) when a `TierChecked`/
    /// `TierCheckFailed` event resolves to Free/Unknown; cleared here by
    /// pressing Continue (or Retry, which re-triggers the check).
    pub tier_result: Option<TierResult>,
}

impl SignInScreen {
    /// Show the tier-result overlay (`App`'s event mapping calls this;
    /// never called with a Premium result — Premium shows nothing).
    pub fn set_tier_result(&mut self, result: TierResult) {
        self.tier_result = Some(result);
    }
}

/// Draw the current sign-in sub-state, applying every command directly to
/// `account`. Returns any events raised by a command this frame invoked
/// synchronously (`start_sign_in`'s `BrowserUrlReady`/`StoreUnavailable`,
/// or a synthesised `BrowserUrlReady` for "Open the browser again") for
/// `App` to react to — only `App` calls `ctx.open_url` (design note 10).
pub fn show(
    ui: &mut Ui,
    account: &mut AccountService,
    screen: &mut SignInScreen,
) -> Vec<AccountEvent> {
    if let Some(result) = screen.tier_result {
        show_tier_result(ui, account, screen, result);
        return Vec::new();
    }

    if let Some(store_name_key) = screen.store_unavailable {
        return show_store_unavailable(ui, account, screen, store_name_key);
    }

    match account.state() {
        SessionState::SignedOut { note } => {
            let note = *note;
            show_signed_out(ui, account, screen, note)
        }
        SessionState::StoreUnreadable => show_store_unreadable(ui, account),
        SessionState::Authorizing { resumed, .. } => {
            let resumed = *resumed;
            show_authorizing(ui, account, resumed)
        }
        SessionState::Checking { .. } => {
            show_checking(ui);
            Vec::new()
        }
        SessionState::Active | SessionState::Expired => Vec::new(),
    }
}

fn show_signed_out(
    ui: &mut Ui,
    account: &mut AccountService,
    screen: &mut SignInScreen,
    note: Option<SignInNote>,
) -> Vec<AccountEvent> {
    ui.heading(tr("signin-title"));
    ui.label(tr("signin-explanation"));
    if let Some(note) = note {
        ui.label(tr(note_key(note)));
    }
    let button_key = if note.is_some() {
        "signin-retry"
    } else {
        "signin-start"
    };
    if ui.button(tr(button_key)).clicked() {
        return start_sign_in(account, screen);
    }
    Vec::new()
}

fn note_key(note: SignInNote) -> &'static str {
    match note {
        SignInNote::Cancelled => "signin-note-cancelled",
        SignInNote::TimedOut => "signin-note-timed-out",
        SignInNote::ServiceError => "signin-note-service-error",
        SignInNote::PreviousDidNotFinish => "signin-note-previous-unfinished",
        SignInNote::Revoked => "signin-note-revoked",
        // `SessionState::StoreUnreadable` (a session found at launch whose
        // secure store failed to read) gets its own dedicated remediation
        // view (`show_store_unreadable`), never this generic note line —
        // this arm exists only so the match stays exhaustive.
        SignInNote::StoreUnreadable => "signin-note-service-error",
    }
}

fn start_sign_in(account: &mut AccountService, screen: &mut SignInScreen) -> Vec<AccountEvent> {
    let events = account.start_sign_in();
    for event in &events {
        if let AccountEvent::StoreUnavailable { store_name_key } = event {
            screen.store_unavailable = Some(store_name_key);
        }
    }
    events
}

fn show_store_unavailable(
    ui: &mut Ui,
    account: &mut AccountService,
    screen: &mut SignInScreen,
    store_name_key: &'static str,
) -> Vec<AccountEvent> {
    show_store_message(ui, store_name_key);
    if ui.button(tr("signin-retry")).clicked() {
        screen.store_unavailable = None;
        return start_sign_in(account, screen);
    }
    Vec::new()
}

/// `SessionState::StoreUnreadable` (contracts/account-session.md
/// "Construction": a session found at launch whose secure store failed to
/// read, FR-017). Shares `show_store_message`'s layout with the
/// `start_sign_in` probe-failure view above, but Retry re-reads the store
/// in place (`retry_store_read`) instead of starting a fresh sign-in
/// attempt — the session is still potentially valid, just unreadable right
/// now (quickstart M6).
fn show_store_unreadable(ui: &mut Ui, account: &mut AccountService) -> Vec<AccountEvent> {
    let store_name_key = account.secure().platform_name_key();
    show_store_message(ui, store_name_key);
    if ui.button(tr("signin-retry")).clicked() {
        return account.retry_store_read();
    }
    Vec::new()
}

/// The shared "can't reach the store" message: `{ $store } = tr(platform_
/// name_key)` plus one platform-specific remediation line (contracts/
/// ui-surface.md: "`signin-store-remedy-<platform>`").
fn show_store_message(ui: &mut Ui, store_name_key: &'static str) {
    ui.heading(tr("signin-title"));
    // FR-006, U2: the store-unreadable explanation is prose, capped at the
    // 72-character measure (research R17).
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr_args(
            "signin-store-unavailable",
            &[("store", tr(store_name_key))],
        ));
        ui.label(tr(remedy_key(store_name_key)));
    });
}

/// The platform-specific remediation line (contracts/ui-surface.md:
/// "`signin-store-remedy-<platform>`").
fn remedy_key(store_name_key: &str) -> &'static str {
    match store_name_key {
        "store-name-keychain" => "signin-store-remedy-keychain",
        "store-name-credential-manager" => "signin-store-remedy-credential-manager",
        _ => "signin-store-remedy-secret-service",
    }
}

fn show_authorizing(ui: &mut Ui, account: &mut AccountService, resumed: bool) -> Vec<AccountEvent> {
    ui.heading(tr("signin-waiting"));
    if resumed {
        ui.label(tr("signin-waiting-resumed"));
    }

    let mut events = Vec::new();
    ui.horizontal(|ui| {
        if ui.button(tr("signin-open-again")).clicked()
            && let Some(url) = account.browser_url()
        {
            events.push(AccountEvent::BrowserUrlReady(url));
        }
        if ui.button(tr("signin-cancel")).clicked() {
            account.cancel_sign_in();
        }
    });
    events
}

fn show_checking(ui: &mut Ui) {
    ui.heading(tr("signin-checking"));
    let response = ui.add(Spinner::new());
    let accessible_name = tr("signin-checking");
    response.widget_info(|| {
        WidgetInfo::labeled(WidgetType::ProgressIndicator, true, accessible_name.clone())
    });
}

fn show_tier_result(
    ui: &mut Ui,
    account: &mut AccountService,
    screen: &mut SignInScreen,
    result: TierResult,
) {
    match result {
        TierResult::Free => {
            ui.heading(tr("tier-free-title"));
            ui.label(tr("tier-free-explanation"));
            ui.horizontal(|ui| {
                if ui.button(tr("tier-open-upgrade")).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(UPGRADE_URL));
                }
                if ui.button(tr("tier-continue")).clicked() {
                    screen.tier_result = None;
                }
            });
        }
        TierResult::Unknown => {
            ui.heading(tr("tier-unknown-title"));
            ui.label(tr("tier-unknown-explanation"));
            ui.horizontal(|ui| {
                if ui.button(tr("tier-retry")).clicked() {
                    screen.tier_result = None;
                    account.recheck_tier();
                }
                if ui.button(tr("tier-continue")).clicked() {
                    screen.tier_result = None;
                }
            });
        }
    }
}
